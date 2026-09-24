//! The `EMBEDDING_GENERATE` job handler — a port of v4's
//! `lib/background-jobs/handlers/embedding-generate.ts` (P4.6BL, dogfood
//! finding #35). Generates an embedding for a single entity — one of the four
//! entity types `MEMORY` / `CONVERSATION_CHUNK` / `HELP_DOC` / `MOUNT_CHUNK` —
//! using the configured embedding profile, and records the outcome on the
//! entity's `embedding_status` row.
//!
//! ## The permanent-error classifier is load-bearing — do not remove it
//!
//! v4's own doc comment records the motivating incident: tens of thousands of
//! DEAD `EMBEDDING_GENERATE` rows accumulated from **deterministic** failures
//! (over-context / NaN / dimension-mismatch inputs) being retried three times
//! each, forever. [`is_permanent_embedding_error`] marks such jobs failed on
//! their `embedding_status` row and lets the JOB complete (never retried, never
//! DEAD); transient errors ("fetch failed", timeouts, connection resets)
//! deliberately do NOT match, so they still retry to `maxAttempts` → DEAD.
//! The [`preflight_skip_reason`] guards catch the two deterministically
//! unembeddable shapes (empty/whitespace-only and oversize input) BEFORE the
//! provider call for the same reason — for MEMORY, CONVERSATION_CHUNK and
//! MOUNT_CHUNK. **Not for HELP_DOC** since v4 `492771aff` (bug 168): a help
//! doc embeds by SECTION and averages them, so an oversize page is exactly what
//! that path exists for; its empty case is `Skipping empty entity`.
//!
//! ## v4 quirks reproduced deliberately
//!
//!   - **The missing-entity handling is asymmetric.** MEMORY, HELP_DOC and
//!     MOUNT_CHUNK mark the status row failed (`'<X> not found'`);
//!     CONVERSATION_CHUNK just logs and returns with **no** `markAsFailed`.
//!   - **MEMORY embeds the plain `` `${summary}\n\n${content}` `` concat** —
//!     NOT the anchor-aware `build_memory_embedding_text` the gate's create
//!     path uses. v4's handler predates the episodic anchors and was never
//!     updated; the re-embed path therefore drops the anchor line, and v5
//!     matches.
//!   - **`markAsEmbedded`/`markAsFailed` UPSERT** (v4 `a5d6cee5`, Bug 7 —
//!     re-ported in P4.D25). They used to be find-then-update and returned
//!     `null` silently when the triple had no row, which — once v4's
//!     enqueue-time upserts went away — was *every* newly-minted entity, so no
//!     outcome was ever recorded. Both now mint the row, which is why the
//!     handler threads the job's `userId` down to every mark site.
//!
//! ## v4 side-effects that map to no-ops in v5
//!
//!   - `getVectorStoreManager().unloadStore(characterId)` (v4 :182) — v4 caches
//!     per-character vector stores in memory and must drop the cache after a
//!     direct-to-DB write. v5 loads [`crate::db::vector_store`] stores fresh
//!     per operation (no manager, no cache), so there is nothing to unload.
//!   - `invalidateMountPoint(chunk.mountPointId)` (v4 :446) — v4's mount-chunk
//!     search reads an in-memory cache; v5's reads the DB directly (the
//!     documented no-op seam, see `photos/save_image_to_album.rs`).

use serde_json::Value;

use crate::db::embedding_status::EmbeddingStatusRepository;
use crate::db::runtime::Db;
use crate::jsstr::{js_trim, utf16_len};
use crate::model::embedding::{EmbeddingPriority, EmbeddingProvider};

/// v4 `EMBEDDING_MAX_CHARS` (`lib/embedding/embedding-service.ts:74`) — the
/// oversize pre-flight cap, in JS string length (UTF-16 code units). The single
/// most load-bearing constant for avoiding the DEAD-row incident.
pub const EMBEDDING_MAX_CHARS: usize = 128 * 1024;

/// The decoded `EMBEDDING_GENERATE` payload (v4 `EmbeddingGeneratePayload`,
/// `lib/background-jobs/queue-service.ts`). v4 performs a bare cast — no
/// validation — so every field decodes leniently here.
#[derive(Debug, Clone)]
pub struct EmbeddingGeneratePayload {
    /// `None` when absent — the dispatch then reaches v4's
    /// `Unsupported entity type: undefined` throw.
    pub entity_type: Option<String>,
    pub entity_id: String,
    /// Memories only. The MEMORY branch deliberately IGNORES it (v4 reads
    /// `memory.characterId` from the row); carried for payload fidelity.
    pub character_id: Option<String>,
    pub profile_id: Option<String>,
    /// Conversation chunks only — log context, never a lookup key.
    pub chat_id: Option<String>,
}

impl EmbeddingGeneratePayload {
    /// Decode from the raw job payload JSON (v4's unchecked
    /// `job.payload as EmbeddingGeneratePayload`).
    pub fn from_json(payload: &Value) -> Self {
        let get = |k: &str| payload.get(k).and_then(Value::as_str).map(str::to_string);
        Self {
            entity_type: get("entityType"),
            entity_id: get("entityId").unwrap_or_default(),
            character_id: get("characterId"),
            profile_id: get("profileId"),
            chat_id: get("chatId"),
        }
    }
}

/// v4 `isPermanentEmbeddingError` — whether an embedding failure is
/// deterministic (retrying the exact same input will fail again). Lowercases
/// the message; permanent on any substring hit. `"fetch failed"` / timeouts /
/// connection resets deliberately do NOT match, so they still retry.
pub fn is_permanent_embedding_error(message: &str) -> bool {
    let m = message.to_lowercase();
    m.contains("nan")
        || m.contains("non-finite")
        || m.contains("exceeds the context length")
        || m.contains("maximum context length")
        || m.contains("cannot embed empty input")
        || m.contains("dimension mismatch")
}

/// v4 `skipIfOversize`'s two pre-flight guards, as a pure reason: `Some(reason)`
/// when the text is deterministically unembeddable (the caller marks the status
/// row failed and bails WITHOUT throwing, so the queue never retries it), `None`
/// to proceed. Lengths are JS string lengths (UTF-16 code units); the trim is
/// JS `String.prototype.trim`.
pub fn preflight_skip_reason(text: &str) -> Option<String> {
    if js_trim(text).is_empty() {
        return Some("Empty input — nothing to embed".to_string());
    }
    let len = utf16_len(text);
    if len <= EMBEDDING_MAX_CHARS {
        return None;
    }
    Some(format!(
        "Oversize: {len} chars exceeds {EMBEDDING_MAX_CHARS}-char cap"
    ))
}

/// Handle an `EMBEDDING_GENERATE` job (v4 `handleEmbeddingGenerate`).
///
/// `Ok(())` completes the job — including the guard-skip, permanent-error and
/// missing-entity arms (v4 `return`s there; the queue never retries).
/// `Err(message)` fails the job — the runner marks it FAILED with the ported
/// backoff, retrying to `maxAttempts` (3) → DEAD (v4 `throw`).
pub async fn handle_embedding_generate<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    user_id: &str,
    payload: &EmbeddingGeneratePayload,
) -> Result<(), String> {
    match payload.entity_type.as_deref() {
        Some("HELP_DOC") => help_doc_branch(db, embedding, user_id, payload).await,
        Some("CONVERSATION_CHUNK") => {
            conversation_chunk_branch(db, embedding, user_id, payload).await
        }
        Some("MOUNT_CHUNK") => mount_chunk_branch(db, embedding, user_id, payload).await,
        Some("MEMORY") => memory_branch(db, embedding, user_id, payload).await,
        // v4: `throw new Error(`Unsupported entity type: ${payload.entityType}`)`
        // — an absent field renders as JS `undefined`.
        other => Err(format!(
            "Unsupported entity type: {}",
            other.unwrap_or("undefined")
        )),
    }
}

/// Stringify a [`crate::db::DbError`] for the catch path (v4's
/// `error instanceof Error ? error.message : String(error)` — DB failures are
/// not oracle-pinned, only the provider/not-found/guard strings are).
fn db_str(e: crate::db::DbError) -> String {
    format!("{e}")
}

/// `markAsFailed` through the writer (main partition). Since v4 `a5d6cee5` the
/// repo UPSERTS, so the job's `user_id` rides along to mint the row when the
/// triple has none — v4 passes `job.userId` at all thirteen of its call sites,
/// which v5 consolidates into this one.
async fn mark_failed(
    db: &Db,
    entity_type: &'static str,
    entity_id: &str,
    profile_id: &str,
    error: &str,
    user_id: &str,
) -> Result<bool, crate::db::DbError> {
    let (eid, pid, msg, uid) = (
        entity_id.to_string(),
        profile_id.to_string(),
        error.to_string(),
        user_id.to_string(),
    );
    db.write(move |ws| {
        EmbeddingStatusRepository::new(ws.main().connection()).mark_as_failed(
            entity_type,
            &eid,
            &pid,
            &msg,
            &uid,
        )
    })
    .await
}

/// `markAsEmbedded` through the writer (main partition). Upserts — see
/// [`mark_failed`] for why `user_id` is threaded.
async fn mark_embedded(
    db: &Db,
    entity_type: &'static str,
    entity_id: &str,
    profile_id: &str,
    user_id: &str,
) -> Result<bool, crate::db::DbError> {
    let (eid, pid, uid) = (
        entity_id.to_string(),
        profile_id.to_string(),
        user_id.to_string(),
    );
    db.write(move |ws| {
        EmbeddingStatusRepository::new(ws.main().connection()).mark_as_embedded(
            entity_type,
            &eid,
            &pid,
            &uid,
        )
    })
    .await
}

/// The shared catch block (v4's four identical `catch (error)` arms): mark the
/// status row failed FIRST, then classify — permanent completes the job (warn),
/// transient re-throws (error → retry → DEAD).
async fn catch_arm(
    db: &Db,
    entity_type: &'static str,
    entity_id: &str,
    profile_id: &str,
    message: String,
    user_id: &str,
) -> Result<(), String> {
    mark_failed(db, entity_type, entity_id, profile_id, &message, user_id)
        .await
        .map_err(db_str)?;
    if is_permanent_embedding_error(&message) {
        tracing::warn!(
            target: "quilltap::jobs",
            entity_type,
            entity_id,
            error = %message,
            "[EmbeddingGenerate] Permanent embedding error — marked failed, skipping retry",
        );
        return Ok(());
    }
    tracing::error!(
        target: "quilltap::jobs",
        entity_type,
        entity_id,
        error = %message,
        "[EmbeddingGenerate] Failed to generate embedding",
    );
    Err(message)
}

/// The guard step shared by all four try-bodies: `Some(())` when the entity was
/// skipped (status marked failed; the caller completes), `None` to proceed.
async fn guard_skip(
    db: &Db,
    entity_type: &'static str,
    entity_id: &str,
    profile_id: &str,
    text: &str,
    user_id: &str,
) -> Result<Option<()>, String> {
    let Some(reason) = preflight_skip_reason(text) else {
        return Ok(None);
    };
    tracing::warn!(
        target: "quilltap::jobs",
        entity_type,
        entity_id,
        reason = %reason,
        "[EmbeddingGenerate] Skipping deterministically unembeddable entity",
    );
    mark_failed(db, entity_type, entity_id, profile_id, &reason, user_id)
        .await
        .map_err(db_str)?;
    Ok(Some(()))
}

// ============================================================================
// MEMORY (v4 :122–228)
// ============================================================================

async fn memory_branch<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    user_id: &str,
    payload: &EmbeddingGeneratePayload,
) -> Result<(), String> {
    let pid = payload.profile_id.clone().unwrap_or_default();
    let eid = payload.entity_id.clone();
    let memory = db
        .read_main(move |conn| crate::db::memories_read::find_by_id(conn, &eid))
        .map_err(db_str)?;
    let Some(memory) = memory else {
        tracing::warn!(
            target: "quilltap::jobs",
            memory_id = %payload.entity_id,
            "[EmbeddingGenerate] Memory not found",
        );
        mark_failed(
            db,
            "MEMORY",
            &payload.entity_id,
            &pid,
            "Memory not found",
            user_id,
        )
        .await
        .map_err(db_str)?;
        return Ok(());
    };

    let summary = memory
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let content = memory
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let character_id = memory
        .get("characterId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    // v4 :142 — the plain concat, NOT build_memory_embedding_text (see the
    // module doc).
    let text = format!("{summary}\n\n{content}");

    match memory_try(db, embedding, user_id, payload, &pid, &character_id, &text).await {
        Ok(()) => Ok(()),
        Err(msg) => catch_arm(db, "MEMORY", &payload.entity_id, &pid, msg, user_id).await,
    }
}

async fn memory_try<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    user_id: &str,
    payload: &EmbeddingGeneratePayload,
    profile_id: &str,
    character_id: &str,
    text: &str,
) -> Result<(), String> {
    if guard_skip(db, "MEMORY", &payload.entity_id, profile_id, text, user_id)
        .await?
        .is_some()
    {
        return Ok(());
    }
    let result = embedding
        .generate_embedding_for_user(
            text,
            user_id,
            payload.profile_id.as_deref(),
            EmbeddingPriority::Background,
        )
        .await
        .map_err(|e| e.message)?;

    // v4 :157 `updateForCharacter(characterId, id, { embedding })` — a `null`
    // return (vanished / ownership mismatch) is silent in v4; ignore the flag.
    let (vec, dims) = (result.embedding.clone(), result.dimensions as f64);
    let (cid, mid) = (character_id.to_string(), payload.entity_id.clone());
    db.write(move |ws| {
        let patch = crate::db::memories::MemUpdate {
            embedding: Some(Some(vec.clone())),
            ..Default::default()
        };
        crate::db::memories::MemoriesRepository::new(ws.main().connection())
            .update_for_character(&cid, &mid, &patch)
            .map(|_| ())
    })
    .await
    .map_err(db_str)?;

    // v4 :167–178 — write directly to the vector_indices tables instead of
    // loading the full in-memory store (v4's comment: loading a 12k-vector
    // store to insert one row would cost hundreds of MB of heap).
    let (vec, cid, mid) = (
        result.embedding.clone(),
        character_id.to_string(),
        payload.entity_id.clone(),
    );
    db.write(move |ws| {
        let repo = crate::db::vector_indices::VectorIndicesRepository::new(ws.main().connection());
        if repo.entry_exists(&mid)? {
            repo.update_entry_embedding(&mid, Some(&vec))?;
        } else {
            repo.add_entry(&crate::db::vector_indices::VectorEntryInput {
                id: mid.clone(),
                character_id: cid.clone(),
                embedding: Some(vec.clone()),
            })?;
        }
        repo.save_meta(&cid, dims)
    })
    .await
    .map_err(db_str)?;

    // v4 :182 `unloadStore(characterId)` — no-op in v5 (no cached store
    // manager; see the module doc).

    mark_embedded(db, "MEMORY", &payload.entity_id, profile_id, user_id)
        .await
        .map_err(db_str)?;
    tracing::info!(
        target: "quilltap::jobs",
        memory_id = %payload.entity_id,
        character_id = %character_id,
        dimensions = result.dimensions,
        "[EmbeddingGenerate] Embedding generated successfully",
    );
    Ok(())
}

// ============================================================================
// CONVERSATION_CHUNK (v4 :235–313)
// ============================================================================

async fn conversation_chunk_branch<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    user_id: &str,
    payload: &EmbeddingGeneratePayload,
) -> Result<(), String> {
    let pid = payload.profile_id.clone().unwrap_or_default();
    let eid = payload.entity_id.clone();
    let chunk = db
        .read_main(move |conn| {
            crate::db::conversation_chunks::ConversationChunksRepository::new(conn)
                .find_row_by_id(&eid)
        })
        .map_err(db_str)?;
    let Some(chunk) = chunk else {
        // ⚠ The v4 asymmetry: this branch just logs and returns — NO
        // markAsFailed (v4 :242–249), unlike the other three entity types.
        tracing::warn!(
            target: "quilltap::jobs",
            chunk_id = %payload.entity_id,
            chat_id = payload.chat_id.as_deref().unwrap_or_default(),
            "[EmbeddingGenerate] Conversation chunk not found",
        );
        return Ok(());
    };

    match conversation_chunk_try(db, embedding, user_id, payload, &pid, &chunk).await {
        Ok(()) => Ok(()),
        Err(msg) => {
            catch_arm(
                db,
                "CONVERSATION_CHUNK",
                &payload.entity_id,
                &pid,
                msg,
                user_id,
            )
            .await
        }
    }
}

async fn conversation_chunk_try<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    user_id: &str,
    payload: &EmbeddingGeneratePayload,
    profile_id: &str,
    chunk: &crate::db::conversation_chunks::CcChunkRow,
) -> Result<(), String> {
    if guard_skip(
        db,
        "CONVERSATION_CHUNK",
        &payload.entity_id,
        profile_id,
        &chunk.content,
        user_id,
    )
    .await?
    .is_some()
    {
        return Ok(());
    }
    let result = embedding
        .generate_embedding_for_user(
            &chunk.content,
            user_id,
            payload.profile_id.as_deref(),
            EmbeddingPriority::Background,
        )
        .await
        .map_err(|e| e.message)?;

    let (vec, cid) = (result.embedding.clone(), chunk.id.clone());
    let updated = db
        .write(move |ws| {
            let now = crate::clock::now_iso();
            crate::db::conversation_chunks::ConversationChunksRepository::new(
                ws.main().connection(),
            )
            .update_embedding(&cid, &vec, &now)
        })
        .await
        .map_err(db_str)?;
    if !updated {
        // v4 `updateEmbedding` throws here; the message lands in the catch.
        return Err(format!(
            "Chunk not found for embedding update: {}",
            chunk.id
        ));
    }

    mark_embedded(
        db,
        "CONVERSATION_CHUNK",
        &payload.entity_id,
        profile_id,
        user_id,
    )
    .await
    .map_err(db_str)?;
    tracing::info!(
        target: "quilltap::jobs",
        chunk_id = %chunk.id,
        chat_id = payload.chat_id.as_deref().unwrap_or_default(),
        interchange_index = chunk.interchange_index,
        dimensions = result.dimensions,
        "[EmbeddingGenerate] Conversation chunk embedding generated",
    );
    Ok(())
}

// ============================================================================
// HELP_DOC (v4 :324–558, rewritten at `492771aff`, bug 168)
// ============================================================================

async fn help_doc_branch<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    user_id: &str,
    payload: &EmbeddingGeneratePayload,
) -> Result<(), String> {
    let pid = payload.profile_id.clone().unwrap_or_default();
    let eid = payload.entity_id.clone();
    let doc = db
        .read_main(move |conn| crate::db::help_docs::HelpDocsRepository::new(conn).find_by_id(&eid))
        .map_err(db_str)?;
    let Some(doc) = doc else {
        tracing::warn!(
            target: "quilltap::jobs",
            doc_id = %payload.entity_id,
            "[EmbeddingGenerate] Help doc not found",
        );
        mark_failed(
            db,
            "HELP_DOC",
            &payload.entity_id,
            &pid,
            "Help doc not found",
            user_id,
        )
        .await
        .map_err(db_str)?;
        return Ok(());
    };

    match help_doc_try(db, embedding, user_id, payload, &pid, &doc).await {
        Ok(()) => Ok(()),
        Err(msg) => catch_arm(db, "HELP_DOC", &payload.entity_id, &pid, msg, user_id).await,
    }
}

/// v4 `handleHelpDocEmbedding`'s try body (`:484-527`).
///
/// **The document's own vector is the normalised mean of its section vectors**
/// ([`crate::embedding_vector::average_embeddings`]), never an embedding of the
/// whole text. v4's *why*, carried forward: a help page can run past any
/// provider's input ceiling — `chat-settings.md` passed OpenAI's 8,192 tokens
/// and was left with no vector at all, and so invisible to `help_search` (bug
/// 168; this port's dogfood #120, which found the same five pages unembedded) —
/// while a section never can.
///
/// **No `guard_skip` for HELP_DOC** (v4 dropped `skipIfOversize` here; MEMORY,
/// CONVERSATION_CHUNK and MOUNT_CHUNK keep it): an oversize page is exactly
/// what the section path exists for, and the empty case has its own arm below.
async fn help_doc_try<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    user_id: &str,
    payload: &EmbeddingGeneratePayload,
    profile_id: &str,
    doc: &crate::db::help_docs::HelpDocRow,
) -> Result<(), String> {
    let sections = embed_help_doc_sections(db, embedding, user_id, payload, doc).await?;
    // The width filter above makes the differing-dimension refusal
    // unreachable; were it reached, v4's throw lands in the same catch.
    let doc_embedding =
        crate::embedding_vector::average_embeddings(&sections.vectors).map_err(|e| e.message())?;

    let Some(doc_embedding) = doc_embedding else {
        // v4's *why*: no section had any text — the same deterministic dead end
        // as an empty memory, so it is marked failed without a retry.
        tracing::warn!(
            target: "quilltap::jobs",
            context = "handleEmbeddingGenerate",
            entityType = "HELP_DOC",
            entityId = doc.id.as_str(),
            title = doc.title.as_str(),
            "[EmbeddingGenerate] Skipping empty entity",
        );
        mark_failed(
            db,
            "HELP_DOC",
            &payload.entity_id,
            profile_id,
            "Empty input — nothing to embed",
            user_id,
        )
        .await
        .map_err(db_str)?;
        return Ok(());
    };

    let (vec, did) = (doc_embedding.clone(), doc.id.clone());
    let updated = db
        .write(move |ws| {
            let now = crate::clock::now_iso();
            crate::db::help_docs::HelpDocsRepository::new(ws.main().connection())
                .update_embedding(&did, &vec, &now)
        })
        .await
        .map_err(db_str)?;
    if !updated {
        return Err(format!(
            "Help doc not found for embedding update: {}",
            doc.id
        ));
    }

    mark_embedded(db, "HELP_DOC", &payload.entity_id, profile_id, user_id)
        .await
        .map_err(db_str)?;
    tracing::info!(
        target: "quilltap::jobs",
        context = "handleEmbeddingGenerate",
        docId = doc.id.as_str(),
        title = doc.title.as_str(),
        dimensions = doc_embedding.len(),
        sectionsAveraged = sections.vectors.len(),
        sectionsEmbedded = sections.embedded,
        sectionsReused = sections.reused,
        sectionsFailed = sections.failed,
        "[EmbeddingGenerate] Help doc embedding generated",
    );
    Ok(())
}

/// One section of a help doc as the embedding pass sees it (v4
/// `HelpDocSection`, `:324-331`).
struct HelpDocSection {
    /// The stored chunk row id; `None` for a slice made on the fly (no rows).
    id: Option<String>,
    chunk_index: f64,
    heading: Option<String>,
    content: String,
    embedding: Option<Vec<f32>>,
}

/// What [`embed_help_doc_sections`] hands back (v4's `{vectors, embedded,
/// reused, failed}`).
struct SectionsOutcome {
    vectors: Vec<Vec<f32>>,
    embedded: usize,
    reused: usize,
    failed: usize,
}

/// v4 `embedHelpDocSections` (`embedding-generate.ts:354-449`) — give every
/// section of a help document a vector, and return them.
///
/// v4's *why*, carried forward:
///   - Sections are the stored `help_doc_chunks` rows. When a doc has none yet
///     (a sync whose slicing has not landed) it is sliced HERE, in memory, so
///     the document still gets a vector; those slices are never persisted —
///     the next sync or reconcile writes the rows.
///   - A stored vector is reused, which makes a retry cheap: rows are recreated
///     with null embeddings whenever the doc's content changes and a full
///     reindex clears them, so a populated vector is current for its text. A
///     stored vector whose WIDTH differs from a freshly generated one belongs
///     to an earlier profile and is re-embedded rather than averaged in.
///   - A single section's failure is logged and skipped; the rest still stand
///     for the document. If every section fails, the last error is returned so
///     the job's permanent/transient handling decides what happens next.
///   - Vectors are collected in memory rather than re-read (in v4's job child
///     the writes are buffered and a read would not see them).
async fn embed_help_doc_sections<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    user_id: &str,
    payload: &EmbeddingGeneratePayload,
    doc: &crate::db::help_docs::HelpDocRow,
) -> Result<SectionsOutcome, String> {
    // v4's `findByDocId` is a FALLBACK `safeQuery`: a failing read logs its
    // ERROR and answers `[]` — which then takes the in-memory slice below.
    let doc_id = doc.id.clone();
    let stored = match db.read_main(move |conn| {
        crate::db::help_doc_chunks::HelpDocChunksRepository::new(conn).find_by_doc_id(&doc_id)
    }) {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(
                target: "quilltap::db",
                docId = doc.id.as_str(),
                error = %e,
                "Error finding help doc chunks by doc",
            );
            Vec::new()
        }
    };
    let stored_rows = stored.len();
    let mut sections: Vec<HelpDocSection> = if stored.is_empty() {
        crate::services::help_doc_chunking::build_help_doc_chunks(&doc.content)
            .into_iter()
            .map(|draft| HelpDocSection {
                id: None,
                chunk_index: draft.chunk_index as f64,
                heading: draft.heading,
                content: draft.content,
                embedding: None,
            })
            .collect()
    } else {
        stored
            .into_iter()
            .map(|chunk| HelpDocSection {
                id: Some(chunk.id),
                chunk_index: chunk.chunk_index,
                heading: chunk.heading,
                content: chunk.content,
                // v4: `chunk.embedding && chunk.embedding.length > 0 ? … : null`.
                embedding: (!chunk.embedding.is_empty()).then_some(chunk.embedding),
            })
            .collect()
    };

    tracing::debug!(
        target: "quilltap::jobs",
        context = "handleEmbeddingGenerate",
        docId = doc.id.as_str(),
        sections = sections.len(),
        storedRows = stored_rows,
        "[EmbeddingGenerate] Embedding help doc sections",
    );

    let mut tally = SectionTally::default();

    // `:415-420` — every embedding-less section, in chunk order.
    let mut reused_at_start: Vec<bool> = sections.iter().map(|s| s.embedding.is_some()).collect();
    for section in sections.iter_mut() {
        if section.embedding.is_none() {
            tally.record(embed_section(db, embedding, user_id, payload, doc, section).await);
        }
    }

    // `:422-433` — settle on the current profile's width: that of the first
    // FRESH vector (evaluated after the first pass), else of the first reused
    // one. A vector of any other width is re-embedded (once).
    let fresh = sections
        .iter()
        .enumerate()
        .find(|(i, s)| s.embedding.is_some() && !reused_at_start[*i])
        .map(|(_, s)| s);
    let width = fresh
        .or_else(|| sections.iter().find(|s| s.embedding.is_some()))
        .and_then(|s| s.embedding.as_ref())
        .map(Vec::len);
    if let Some(width) = width {
        for (i, section) in sections.iter_mut().enumerate() {
            if section.embedding.as_ref().is_some_and(|v| v.len() != width) {
                reused_at_start[i] = false;
                tally.record(embed_section(db, embedding, user_id, payload, doc, section).await);
            }
        }
    }

    // `:435-437` — the vectors of the settled width, in chunk order.
    let vectors: Vec<Vec<f32>> = sections
        .iter()
        .filter_map(|s| s.embedding.as_ref())
        .filter(|v| Some(v.len()) == width)
        .cloned()
        .collect();

    // `:439-441` — throw only when nothing survived AND something failed; zero
    // sections or all-blank text is the empty arm, not an error.
    if vectors.is_empty() {
        if let Some(message) = tally.last_error {
            return Err(message);
        }
    }

    let reused = sections
        .iter()
        .enumerate()
        .filter(|(i, s)| reused_at_start[*i] && s.embedding.is_some())
        .count();
    Ok(SectionsOutcome {
        vectors,
        embedded: tally.embedded,
        reused,
        failed: tally.failed,
    })
}

/// `embedSection`'s counters (v4's closure-scoped `embedded` / `failed` /
/// `lastError`).
#[derive(Default)]
struct SectionTally {
    embedded: usize,
    failed: usize,
    last_error: Option<String>,
}

impl SectionTally {
    /// `None` = the silent blank-text return, counted nowhere.
    fn record(&mut self, outcome: Option<Result<(), String>>) {
        match outcome {
            Some(Ok(())) => self.embedded += 1,
            Some(Err(message)) => {
                self.failed += 1;
                self.last_error = Some(message);
            }
            None => {}
        }
    }
}

/// v4's `embedSection` (`:383-413`). ONE catch covers the provider call AND the
/// chunk write: a failure of either counts `failed`, becomes the last error,
/// and nulls the section IN MEMORY only (a stored row keeps whatever it had —
/// for a width re-embed, the OLD wrong-width vector). v5's separate `Help doc
/// chunk embedding write failed` WARN had no v4 twin and folds in here. A blank
/// composed text returns silently (`None`), counted nowhere.
async fn embed_section<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    user_id: &str,
    payload: &EmbeddingGeneratePayload,
    doc: &crate::db::help_docs::HelpDocRow,
    section: &mut HelpDocSection,
) -> Option<Result<(), String>> {
    let text = crate::services::help_doc_chunking::help_chunk_embedding_text(
        &doc.title,
        section.heading.as_deref(),
        &section.content,
    );
    if crate::jsstr::js_trim(&text).is_empty() {
        return None;
    }
    let attempt: Result<(), String> = async {
        let result = embedding
            .generate_embedding_for_user(
                &text,
                user_id,
                payload.profile_id.as_deref(),
                EmbeddingPriority::Background,
            )
            .await
            .map_err(|e| e.message)?;
        section.embedding = Some(result.embedding.clone());
        if let Some(id) = section.id.clone() {
            // A no-row-matched update returns null in v4 (no throw), so only a
            // hard DB error lands in the catch.
            let vec = result.embedding;
            db.write(move |ws| {
                let now = crate::clock::now_iso();
                crate::db::help_doc_chunks::HelpDocChunksRepository::new(ws.main().connection())
                    .update_embedding(&id, &vec, &now)
            })
            .await
            .map_err(db_str)?;
        }
        Ok(())
    }
    .await;
    Some(match attempt {
        Ok(()) => Ok(()),
        Err(message) => {
            section.embedding = None;
            tracing::warn!(
                target: "quilltap::jobs",
                context = "handleEmbeddingGenerate",
                docId = doc.id.as_str(),
                chunkId = section.id.as_deref(),
                chunkIndex = section.chunk_index,
                error = message.as_str(),
                "[EmbeddingGenerate] Help doc section embedding failed — skipping section",
            );
            Err(message)
        }
    })
}

// ============================================================================
// MOUNT_CHUNK (v4 :407–490)
// ============================================================================

async fn mount_chunk_branch<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    user_id: &str,
    payload: &EmbeddingGeneratePayload,
) -> Result<(), String> {
    let pid = payload.profile_id.clone().unwrap_or_default();
    let eid = payload.entity_id.clone();
    let chunk = db
        .read_mount_index(move |conn| {
            crate::db::doc_mount_chunks::DocMountChunksRepository::new(conn).find_row_by_id(&eid)
        })
        .map_err(db_str)?;
    let Some(chunk) = chunk else {
        tracing::warn!(
            target: "quilltap::jobs",
            chunk_id = %payload.entity_id,
            "[EmbeddingGenerate] Mount chunk not found",
        );
        mark_failed(
            db,
            "MOUNT_CHUNK",
            &payload.entity_id,
            &pid,
            "Mount chunk not found",
            user_id,
        )
        .await
        .map_err(db_str)?;
        return Ok(());
    };

    match mount_chunk_try(db, embedding, user_id, payload, &pid, &chunk).await {
        Ok(()) => Ok(()),
        Err(msg) => catch_arm(db, "MOUNT_CHUNK", &payload.entity_id, &pid, msg, user_id).await,
    }
}

async fn mount_chunk_try<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    user_id: &str,
    payload: &EmbeddingGeneratePayload,
    profile_id: &str,
    chunk: &crate::db::doc_mount_chunks::ChunkRow,
) -> Result<(), String> {
    if guard_skip(
        db,
        "MOUNT_CHUNK",
        &payload.entity_id,
        profile_id,
        &chunk.content,
        user_id,
    )
    .await?
    .is_some()
    {
        return Ok(());
    }
    let result = embedding
        .generate_embedding_for_user(
            &chunk.content,
            user_id,
            payload.profile_id.as_deref(),
            EmbeddingPriority::Background,
        )
        .await
        .map_err(|e| e.message)?;

    let (vec, cid) = (result.embedding.clone(), chunk.id.clone());
    let updated = db
        .write(move |ws| {
            let mount = ws
                .mount_index()
                .ok_or_else(|| {
                    crate::db::DbError::Internal(
                        "mount-chunk embedding requires the mount-index database".to_string(),
                    )
                })?
                .connection();
            let now = crate::clock::now_iso();
            crate::db::doc_mount_chunks::DocMountChunksRepository::new(mount)
                .update_embedding(&cid, &vec, &now)
        })
        .await
        .map_err(db_str)?;
    if !updated {
        return Err(format!(
            "Doc mount chunk not found for embedding update: {}",
            chunk.id
        ));
    }

    // v4 :446 `invalidateMountPoint(chunk.mountPointId)` — no-op in v5 (no
    // in-memory mount-chunk cache; see the module doc).

    mark_embedded(db, "MOUNT_CHUNK", &payload.entity_id, profile_id, user_id)
        .await
        .map_err(db_str)?;
    tracing::info!(
        target: "quilltap::jobs",
        chunk_id = %chunk.id,
        mount_point_id = %chunk.mount_point_id,
        dimensions = result.dimensions,
        "[EmbeddingGenerate] Mount chunk embedding generated",
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The classifier's six substrings hit, case-insensitively; the transient
    /// shapes deliberately miss (v4's doc comment names them).
    #[test]
    fn permanent_classifier_matches_v4() {
        for permanent in [
            "Embedding contains NaN values",
            "non-finite value in vector",
            "This model's maximum context length is 8192 tokens",
            "input exceeds the context length",
            "Cannot embed empty input",
            "Dimension mismatch: expected 1536, got 768",
        ] {
            assert!(is_permanent_embedding_error(permanent), "{permanent}");
        }
        for transient in ["fetch failed", "connect ETIMEDOUT", "socket hang up"] {
            assert!(!is_permanent_embedding_error(transient), "{transient}");
        }
    }

    /// The two pre-flight guards: empty/whitespace, the cap boundary (a text of
    /// exactly EMBEDDING_MAX_CHARS passes — v4 is `>`, not `>=`), and the
    /// oversize reason's exact wording. Lengths are UTF-16 code units.
    #[test]
    fn preflight_guards() {
        assert_eq!(
            preflight_skip_reason(""),
            Some("Empty input — nothing to embed".to_string())
        );
        assert_eq!(
            preflight_skip_reason(" \t\n"),
            Some("Empty input — nothing to embed".to_string())
        );
        let at_cap = "a".repeat(EMBEDDING_MAX_CHARS);
        assert_eq!(preflight_skip_reason(&at_cap), None);
        let over = "a".repeat(EMBEDDING_MAX_CHARS + 1);
        assert_eq!(
            preflight_skip_reason(&over),
            Some(format!(
                "Oversize: {} chars exceeds {EMBEDDING_MAX_CHARS}-char cap",
                EMBEDDING_MAX_CHARS + 1
            ))
        );
        // 🎈 (U+1F388) is 2 UTF-16 units: half the cap in balloons + one ASCII
        // char tips it over — the JS-length rule, not chars() or bytes.
        let mut tricky = "🎈".repeat(EMBEDDING_MAX_CHARS / 2);
        assert_eq!(preflight_skip_reason(&tricky), None);
        tricky.push('x');
        assert!(preflight_skip_reason(&tricky).is_some());
    }

    /// The lenient payload decode: absent fields → None/empty (v4's bare cast).
    #[test]
    fn payload_decode_is_lenient() {
        let p = EmbeddingGeneratePayload::from_json(&serde_json::json!({
            "entityType": "MEMORY",
            "entityId": "m1",
            "characterId": "c1",
            "profileId": "p1",
        }));
        assert_eq!(p.entity_type.as_deref(), Some("MEMORY"));
        assert_eq!(p.entity_id, "m1");
        assert_eq!(p.chat_id, None);

        let empty = EmbeddingGeneratePayload::from_json(&serde_json::json!({}));
        assert_eq!(empty.entity_type, None);
        assert_eq!(empty.entity_id, "");
    }

    // ---- P4.D222: the HELP_DOC section pass's log lines, capture-pinned ----

    use crate::model::embedding::CannedEmbeddingProvider;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const DOC: &str = "d0000000-0000-4000-8000-000000000001";

    fn fresh_db(dir: &std::path::Path) -> Db {
        let data = dir.join("data");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        Db::open(
            crate::db::runtime::DbPaths {
                main: data.join("quilltap.db"),
                mount_index: None,
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap()
    }

    /// A doc titled `T` with `content`, and stored sections `(index, heading,
    /// content)` with NULL vectors.
    fn plant(db: &Db, content: &str, sections: &[(i64, &str, &str)]) {
        let (content, sections): (String, Vec<(i64, String, String)>) = (
            content.to_string(),
            sections
                .iter()
                .map(|(i, h, c)| (*i, h.to_string(), c.to_string()))
                .collect(),
        );
        db.write_blocking(move |ws| {
            let c = ws.main().connection();
            c.execute(
                "INSERT INTO help_docs (id, title, path, url, content, contentHash, embedding, \
                 createdAt, updatedAt) VALUES (?1, 'T', 'help/t.md', '/t', ?2, 'h', NULL, 'x', 'x')",
                rusqlite::params![DOC, content],
            )?;
            for (i, h, body) in &sections {
                c.execute(
                    "INSERT INTO help_doc_chunks (id, docId, chunkIndex, heading, content, \
                     embedding, createdAt, updatedAt) VALUES (?1, ?2, ?3, ?4, ?5, NULL, 'x', 'x')",
                    rusqlite::params![format!("c{i}"), DOC, i, h, body],
                )?;
            }
            Ok(())
        })
        .unwrap();
    }

    fn run(db: &Db, embedding: &CannedEmbeddingProvider) -> (Result<(), String>, Vec<String>) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let payload = EmbeddingGeneratePayload::from_json(&serde_json::json!({
            "entityType": "HELP_DOC", "entityId": DOC, "profileId": "p"
        }));
        crate::test_support::captured_with(|| {
            rt.block_on(handle_embedding_generate(db, embedding, "u", &payload))
        })
    }

    /// The four lines v4 `492771aff` retired must never fire again.
    fn assert_retired_lines_silent(lines: &[String]) {
        for gone in [
            "Could not embed help doc chunks",
            "Help doc chunk embedding failed — skipping chunk",
            "Help doc chunk embedding write failed",
            "chunks_embedded",
        ] {
            assert!(!lines.iter().any(|l| l.contains(gone)), "{gone}: {lines:?}");
        }
    }

    #[test]
    fn the_section_pass_logs_v4s_debug_warn_and_info() {
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(dir.path());
        plant(&db, "unused", &[(0, "A", "one"), (1, "B", "two")]);
        let provider = CannedEmbeddingProvider::new()
            .with_failure_for("T \u{203a} A\n\none", "boom")
            .with_vector("T \u{203a} B\n\ntwo", vec![0.0, 1.0]);
        let (outcome, lines) = run(&db, &provider);
        outcome.unwrap();
        assert_eq!(
            lines,
            vec![
                format!(
                    "DEBUG quilltap::jobs [EmbeddingGenerate] Embedding help doc sections \
                     context=handleEmbeddingGenerate docId={DOC} sections=2 storedRows=2"
                ),
                format!(
                    "WARN quilltap::jobs [EmbeddingGenerate] Help doc section embedding failed — \
                     skipping section context=handleEmbeddingGenerate docId={DOC} chunkId=c0 \
                     chunkIndex=0 error=boom"
                ),
                format!(
                    "INFO quilltap::jobs [EmbeddingGenerate] Help doc embedding generated \
                     context=handleEmbeddingGenerate docId={DOC} title=T dimensions=2 \
                     sectionsAveraged=1 sectionsEmbedded=1 sectionsReused=0 sectionsFailed=1"
                ),
            ]
        );
        assert_retired_lines_silent(&lines);
    }

    /// An in-memory slice (no stored rows) has no id: the WARN carries no
    /// `chunkId` at all (v4 logs `undefined`, which the JSON drops). Every
    /// section failing throws the last error into the catch arm.
    #[test]
    fn a_slice_warn_has_no_chunk_id_and_all_failing_throws() {
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(dir.path());
        plant(&db, "## Only\n\nbody", &[]);
        let slices = crate::services::help_doc_chunking::build_help_doc_chunks("## Only\n\nbody");
        assert_eq!(slices.len(), 1);
        let text = crate::services::help_doc_chunking::help_chunk_embedding_text(
            "T",
            slices[0].heading.as_deref(),
            &slices[0].content,
        );
        let provider = CannedEmbeddingProvider::new().with_failure_for(text, "down");
        let (outcome, lines) = run(&db, &provider);
        assert_eq!(
            outcome,
            Err("down".to_string()),
            "transient: the job retries"
        );
        let warn = lines
            .iter()
            .find(|l| l.contains("Help doc section embedding failed"))
            .expect("the section WARN");
        assert!(!warn.contains("chunkId"), "{warn}");
        assert!(
            warn.contains("docId=") && warn.contains(" chunkIndex=0 error=down"),
            "{warn}"
        );
        assert!(
            lines.iter().any(|l| l.contains("storedRows=0")),
            "{lines:?}"
        );
        assert_retired_lines_silent(&lines);
    }

    /// Nothing to average → v4's WARN `Skipping empty entity` and a FAILED
    /// status, no retry — and HELP_DOC no longer takes the oversize guard (a
    /// page past `EMBEDDING_MAX_CHARS` embeds by section).
    #[test]
    fn empty_is_skipped_and_oversize_is_not_guarded() {
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(dir.path());
        plant(&db, "", &[]);
        let (outcome, lines) = run(&db, &CannedEmbeddingProvider::new());
        outcome.unwrap();
        assert!(
            lines.contains(&format!(
                "WARN quilltap::jobs [EmbeddingGenerate] Skipping empty entity \
                 context=handleEmbeddingGenerate entityType=HELP_DOC entityId={DOC} title=T"
            )),
            "{lines:?}"
        );
        let status: (String, String) = db
            .read_main(|c| {
                Ok(c.query_row(
                    "SELECT status, error FROM embedding_status WHERE entityId = ?1",
                    [DOC],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?)
            })
            .unwrap();
        assert_eq!(
            status,
            ("FAILED".into(), "Empty input — nothing to embed".into())
        );

        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(dir.path());
        plant(
            &db,
            &"x".repeat(EMBEDDING_MAX_CHARS + 1),
            &[(0, "S", "short")],
        );
        let provider =
            CannedEmbeddingProvider::new().with_vector("T \u{203a} S\n\nshort", vec![1.0]);
        let (outcome, lines) = run(&db, &provider);
        outcome.unwrap();
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Skipping deterministically unembeddable entity")),
            "HELP_DOC must not take the oversize guard: {lines:?}"
        );
        assert!(lines
            .iter()
            .any(|l| l.contains("Help doc embedding generated")));
    }
}
