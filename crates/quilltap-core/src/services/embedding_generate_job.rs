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
//! provider call for the same reason.
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
// HELP_DOC (v4 :320–400)
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

async fn help_doc_try<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    user_id: &str,
    payload: &EmbeddingGeneratePayload,
    profile_id: &str,
    doc: &crate::db::help_docs::HelpDocRow,
) -> Result<(), String> {
    let text = format!("{}\n\n{}", doc.title, doc.content);
    if guard_skip(
        db,
        "HELP_DOC",
        &payload.entity_id,
        profile_id,
        &text,
        user_id,
    )
    .await?
    .is_some()
    {
        return Ok(());
    }
    let result = embedding
        .generate_embedding_for_user(
            &text,
            user_id,
            payload.profile_id.as_deref(),
            EmbeddingPriority::Background,
        )
        .await
        .map_err(|e| e.message)?;

    let (vec, did) = (result.embedding.clone(), doc.id.clone());
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

    // Section-level vectors, in the SAME job as the whole-document one (v4
    // `24633026`). v4's *why*, carried forward: doing it here rather than
    // through a HELP_DOC_CHUNK entity type of its own keeps one unit of work
    // per document — the reindex enqueue, the `embedding_status` bookkeeping
    // and the dimension reconcile all continue to count `help_docs` rows, and
    // chunks can never carry a dimension the parent doc doesn't, because they
    // are always written together.
    let chunks_embedded = embed_help_doc_chunks(db, embedding, user_id, payload, doc).await;

    mark_embedded(db, "HELP_DOC", &payload.entity_id, profile_id, user_id)
        .await
        .map_err(db_str)?;
    tracing::info!(
        target: "quilltap::jobs",
        doc_id = %doc.id,
        title = %doc.title,
        dimensions = result.dimensions,
        chunks_embedded,
        "[EmbeddingGenerate] Help doc embedding generated",
    );
    Ok(())
}

/// v4 `embedHelpDocChunks` (`embedding-generate.ts:323`, new at `24633026`) —
/// embed every section chunk of a help document that still lacks a vector.
/// Returns the number embedded on this pass.
///
/// **Chunks that already carry an embedding are skipped**, which makes a retry
/// of a partially-completed job cheap: the rows are recreated with null
/// embeddings whenever the doc's content changes, and a full reindex clears
/// them, so a populated embedding is always current for its text.
///
/// **A single chunk's failure is logged and skipped, never thrown.** The
/// document's own embedding has already been stored by the caller, so the doc
/// stays findable at whole-document granularity; throwing here would fail a job
/// whose main work succeeded, and the next sync or reindex retries the
/// stragglers. The outer read is wrapped for the same reason — hence `()` in
/// place of a `Result`, matching v4's swallow.
async fn embed_help_doc_chunks<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    user_id: &str,
    payload: &EmbeddingGeneratePayload,
    doc: &crate::db::help_docs::HelpDocRow,
) -> usize {
    let mut embedded = 0usize;

    let doc_id = doc.id.clone();
    let chunks = match db.read_main(move |conn| {
        crate::db::help_doc_chunks::HelpDocChunksRepository::new(conn).find_by_doc_id(&doc_id)
    }) {
        Ok(chunks) => chunks,
        Err(e) => {
            tracing::warn!(
                target: "quilltap::jobs",
                doc_id = %doc.id,
                error = %e,
                "[EmbeddingGenerate] Could not embed help doc chunks",
            );
            return embedded;
        }
    };

    for chunk in &chunks {
        // v4: `if (chunk.embedding && chunk.embedding.length > 0) continue`.
        if !chunk.embedding.is_empty() {
            continue;
        }

        let text = crate::services::help_doc_chunking::help_chunk_embedding_text(
            &doc.title,
            chunk.heading.as_deref(),
            &chunk.content,
        );
        // v4's `text.trim().length === 0` guard. It is effectively unreachable
        // in production — the composed text always leads with the document
        // title, and `extractTitle` falls back to the title-cased filename, so
        // a real doc's title is never empty. Carried anyway because v4 carries
        // it. No corpus row can exercise it (which is why none tries);
        // `help_doc_chunking::tests::composed_text_is_blank_only_when_every_part_is`
        // pins the reachability claim itself.
        if crate::jsstr::js_trim(&text).is_empty() {
            continue;
        }

        match embedding
            .generate_embedding_for_user(
                &text,
                user_id,
                payload.profile_id.as_deref(),
                EmbeddingPriority::Background,
            )
            .await
        {
            Ok(result) => {
                let (vec, cid) = (result.embedding.clone(), chunk.id.clone());
                let written = db
                    .write(move |ws| {
                        let now = crate::clock::now_iso();
                        crate::db::help_doc_chunks::HelpDocChunksRepository::new(
                            ws.main().connection(),
                        )
                        .update_embedding(&cid, &vec, &now)
                    })
                    .await;
                match written {
                    // v4's no-fallback `safeQuery` logs and RETHROWS, so a hard
                    // DB error lands in the per-chunk catch (the Err arm below)
                    // and is NOT counted — but a no-row-matched update merely
                    // returns null in v4, so a row that vanished mid-job still
                    // increments `embedded` on both sides (hence `Ok(_)`, not
                    // `Ok(true)`).
                    Ok(_) => embedded += 1,
                    Err(e) => tracing::warn!(
                        target: "quilltap::jobs",
                        doc_id = %doc.id,
                        chunk_id = %chunk.id,
                        error = %e,
                        "[EmbeddingGenerate] Help doc chunk embedding write failed",
                    ),
                }
            }
            Err(e) => tracing::warn!(
                target: "quilltap::jobs",
                doc_id = %doc.id,
                chunk_id = %chunk.id,
                chunk_index = chunk.chunk_index,
                error = %e.message,
                "[EmbeddingGenerate] Help doc chunk embedding failed — skipping chunk",
            ),
        }
    }

    embedded
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
}
