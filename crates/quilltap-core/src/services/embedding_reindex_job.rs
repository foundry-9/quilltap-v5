//! The `EMBEDDING_REINDEX_ALL` job handler (P4.6BM) — a port of v4's
//! `lib/background-jobs/handlers/embedding-reindex.ts`
//! (`handleEmbeddingReindexAll`).
//!
//! A full embedding swap: re-embed help docs, then character memories, then
//! conversation chunks, then document mount chunks, by batch-inserting one
//! `EMBEDDING_GENERATE` job per row. v4 fires it after a TF-IDF vocabulary refit,
//! after an embedding provider/model change **or a different profile becoming
//! default**, from the manual "Re-embed Everything" button, and (since
//! P4.d27 / v4 `7391404e`) whenever the boot dimension reconcile finds
//! non-conforming vectors.
//!
//! ## Before this handler existed, its jobs died
//!
//! v5 already MINTS these jobs: `services::embedding_refit_job` enqueues one
//! whenever a BUILTIN refit runs with `triggerReindex`, and `EMBEDDING_REFIT`
//! *is* registered. Every such job retried three times and went DEAD — the
//! dogfood-#35 shape again.
//!
//! ## Scopes
//!
//!   - `all` (the default when the payload omits `scope`): wipe and re-embed
//!     everything. Cancels in-flight `EMBEDDING_GENERATE` jobs, resets every
//!     `embedding_status` row for the profile, deletes each character's vector
//!     index (so a dimension change can take), and clears every help-doc
//!     embedding first.
//!   - `mismatched-dim`: re-embed only rows whose stored vector length differs
//!     from the profile's target dim (`truncateToDimensions ?? dimensions`).
//!     Deletes nothing and cancels nothing. A BUILTIN profile has neither field
//!     and no fixed-dim concept, so partial scope THROWS for it.
//!
//! ## v4 behavior reproduced deliberately
//!
//!   - **The try/catch asymmetry is real and load-bearing.** Phases 1 (help
//!     docs), 3 (conversation chunks) and 4 (mount chunks) are each wrapped and
//!     CONTINUE on failure; phase 2 (memories) is NOT. Do not "tidy" this into
//!     matching arms. ⚠ Precisely stated (corrected at the 5cc76688-round
//!     unification): in v4 even the uncaught region cannot actually FAIL the
//!     job — every repo call in it is `safeQuery`-wrapped or caught
//!     (`cancelByType`, `markAllPendingByProfileId`, `findByCharacterId`,
//!     `deleteStore`, `createBatch` all fail soft), so only the two explicit
//!     throws (profile-not-found, BUILTIN-partial-scope) end a v4 run. v5
//!     PROPAGATES a `DbError` in that region to a Failed (and so retried) job —
//!     a deliberate divergence in the conservative direction, reachable only
//!     through infra-level DB faults no differential can inject; the fan-out
//!     read alone is fail-soft (matching v4's observable arm the fixture pins).
//!   - **Flat priority 0, `maxAttempts` 3.** NOT `EMBEDDING_ENTITY_PRIORITIES` —
//!     these records are built by hand, unlike `enqueue_embedding_generate`'s
//!     (10 for MEMORY/CONVERSATION_CHUNK) and unlike the boot repair's.
//!   - **No dedupe anywhere on this path.** `create_batch` bypasses
//!     `find_pending_for_entity`, so a partial-scope run can stack jobs on top
//!     of pending ones; only full scope's `cancel_by_type` controls collisions.
//!     Reproduced, not improved.
//!   - **`BATCH_SIZE = 200`** slices (v4 stays well under SQLite's 999-variable
//!     limit).
//!   - The payload is a bare cast, not Zod — both fields decode leniently.
//!
//! ## The three exclusions (P4.d27 / v4 `7391404e`)
//!
//!   - **STALE (cold-tiered) chats are skipped in phase 3.** The stale-chat sweep
//!     deliberately clears their chunk embeddings and the Salon reopen path
//!     re-embeds on demand with the then-current profile; re-embedding them here
//!     would pay provider calls for chats nobody is reading, and the next sweep
//!     would clear the result anyway. Same shared `isStale` gate as the sweeps.
//!   - **FAILED entities are skipped in `mismatched-dim` scope**, per entity type,
//!     via [`crate::db::embedding_status::EmbeddingStatusRepository::list_failed_entity_ids`].
//!     Those are deterministic failures (oversize, over-context, NaN inputs), and
//!     re-enqueueing them on every reconcile would re-pay for the same guaranteed
//!     error. The sets load LAZILY, so `all` scope pays nothing.
//!   - **Phase 4 walks ENABLED mount points only** — a disabled mount is not
//!     searchable, and its scan pipeline re-embeds when it is re-enabled.
//!
//! The memory fan-out enumerates character ids from the MEMORIES table
//! (`find_distinct_character_ids`), not from the characters repository:
//! `characters.findByUserId` silently drops a character whose vault is
//! unavailable, and a dropped character's memories would then be left permanently
//! un-re-embedded. Memories are the ground truth here.
//!
//! ## v4 side-effects that map to no-ops in v5
//!
//!   - `invalidateAll()` on the mount-chunk cache (v4 :103-104): v4's mount-chunk
//!     search reads an in-memory cache; v5's reads the DB directly (the standing
//!     documented no-op, see `embedding_generate_job`'s header).
//!   - `vectorStoreManager.deleteStore(id)` drops an in-memory store AND calls
//!     `deleteByCharacterId`; v5 has no store cache, so only the DB half
//!     ([`crate::db::vector_indices::VectorIndicesRepository::delete_by_character_id`])
//!     is needed — and it already existed.
//!
//! ## The help-doc file walk is a host seam
//!
//! v4's `syncHelpDocs` walks `process.cwd()/help` itself. v5's port
//! ([`super::help_doc_sync::sync_help_docs`]) takes an already-walked file list
//! because the walk belongs to the host — so this handler takes it too, and the
//! host registration supplies `quilltap_host::files_store::load_help_source_files`.
//! That closes the "no production caller" deferral standing on that module.

use serde_json::{json, Value};
use uuid::Uuid;

use std::collections::HashSet;

use super::help_doc_sync::{sync_help_docs, HelpSourceFile};
use crate::db::background_jobs::{BackgroundJobsRepository, BjCreate};
use crate::db::runtime::Db;
use crate::db::{chats_read, embedding_profiles, memories_read, DbError};

/// v4 `BATCH_SIZE` — max jobs per batch insert.
const BATCH_SIZE: usize = 200;

/// The decoded `EMBEDDING_REINDEX_ALL` payload (v4
/// `EmbeddingReindexAllPayload`). v4 performs a bare `as` cast — no validation.
#[derive(Debug, Clone)]
pub struct EmbeddingReindexAllPayload {
    pub profile_id: String,
    /// `None` renders as JS `undefined`, which v4's `?? 'all'` turns into full
    /// scope. Any other string is carried verbatim: v4 only ever COMPARES it to
    /// `'all'` and `'mismatched-dim'`, so an unrecognized value behaves as
    /// "neither" (no wipe, no dim filter) rather than being rejected.
    pub scope: Option<String>,
}

impl EmbeddingReindexAllPayload {
    pub fn from_json(payload: &Value) -> Self {
        Self {
            profile_id: payload
                .get("profileId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            scope: payload
                .get("scope")
                .and_then(Value::as_str)
                .map(str::to_string),
        }
    }
}

/// v4 `embeddingMatchesDim` — true iff the stored vector's length equals the
/// target dim. Null / empty (dim 0) is a MISMATCH, so such rows re-embed.
fn embedding_matches_dim(dim: usize, target_dim: usize) -> bool {
    dim != 0 && dim == target_dim
}

/// v4's `failedIdsFor(entityType)` — the deterministically-FAILED entity ids to
/// exclude, LAZILY per entity type and ONLY in `mismatched-dim` scope (`all` scope
/// pays nothing, exactly as v4's ternary does).
///
/// v4's repo read is `safeQuery`-wrapped to an empty set; a read-pool failure here
/// resolves the same way, so an unavailable exclusion set can only cause a
/// re-attempt, never a dropped row.
fn failed_ids_for(db: &Db, partial: bool, entity_type: &str, profile_id: &str) -> HashSet<String> {
    if !partial {
        return HashSet::new();
    }
    let (et, pid) = (entity_type.to_string(), profile_id.to_string());
    db.read_main(move |conn| {
        Ok(
            crate::db::embedding_status::EmbeddingStatusRepository::new(conn)
                .list_failed_entity_ids(&et, &pid),
        )
    })
    .unwrap_or_default()
}

/// The per-phase tallies that reach v4's completion log. Counting is the only
/// thing they do — no branch reads them — but v4 logs them and the boot reconcile
/// is now a caller whose progress is otherwise invisible.
#[derive(Default)]
struct Counts {
    enqueued: usize,
    /// Rows skipped because their stored vector already matches the target dim.
    dim_matched: usize,
    /// Rows skipped because they are FAILED for this profile.
    failed_skipped: usize,
    /// Chats skipped whole because they are stale (phase 3 only).
    stale_chats_skipped: usize,
}

/// Build the `EMBEDDING_GENERATE` record v4's `buildJobRecord` builds: PENDING,
/// **priority 0**, `maxAttempts` 3, `scheduledAt` = now.
fn build_job_record(user_id: &str, payload: Value, now_iso: &str) -> BjCreate {
    BjCreate {
        user_id: user_id.to_string(),
        job_type: "EMBEDDING_GENERATE".to_string(),
        status: Some("PENDING".to_string()),
        payload,
        priority: 0.0,
        attempts: 0.0,
        max_attempts: 3.0,
        last_error: None,
        scheduled_at: now_iso.to_string(),
        started_at: None,
        completed_at: None,
    }
}

/// Handle an `EMBEDDING_REINDEX_ALL` job (v4 `handleEmbeddingReindexAll`).
///
/// `help_files` is the host's help-tree walk (see the module doc); `now_iso` is
/// the injected clock for the minted rows and the two full-scope column stamps.
/// `Err(message)` fails the job with v4's exact throw string.
pub async fn handle_embedding_reindex_all(
    db: &Db,
    user_id: &str,
    payload: &EmbeddingReindexAllPayload,
    help_files: &[HelpSourceFile],
    now_iso: &str,
) -> Result<(), String> {
    let scope = payload.scope.as_deref().unwrap_or("all");

    // v4 :82-85 — a missing profile fails the job.
    let profile_id = payload.profile_id.clone();
    let profile = db
        .read_main(move |conn| embedding_profiles::find_by_id(conn, &profile_id))
        .map_err(|e| format!("{e}"))?;
    let Some(profile) = profile else {
        return Err(format!(
            "Embedding profile not found: {}",
            payload.profile_id
        ));
    };

    // v4 :91-99 — partial scope needs a target dim; BUILTIN has none.
    let mut target_dim: usize = 0;
    if scope == "mismatched-dim" {
        let d = profile
            .truncate_to_dimensions
            .or(profile.dimensions)
            .unwrap_or(0.0);
        if d <= 0.0 {
            return Err(format!(
                "Cannot run mismatched-dim reindex: profile {} has no truncateToDimensions or dimensions set. Use scope='all' instead.",
                profile.id
            ));
        }
        target_dim = d as usize;
    }
    let partial = scope == "mismatched-dim";

    // v4 :103-104 `invalidateAll()` on the mount-chunk cache — a no-op in v5
    // (see the module doc).

    // v4 :110-120 — full scope only.
    if scope == "all" {
        let (pid, now) = (payload.profile_id.clone(), now_iso.to_string());
        db.write(move |ws| {
            let main = ws.main().connection();
            BackgroundJobsRepository::new(main).cancel_by_type("EMBEDDING_GENERATE", Some(&now))?;
            // v4 keeps the count in a dead local; the CALL is the effect.
            crate::db::embedding_status::EmbeddingStatusRepository::new(main)
                .mark_all_pending_by_profile_id(&pid, &now)?;
            Ok(())
        })
        .await
        .map_err(|e| format!("{e}"))?;
    }

    let mut job_records: Vec<BjCreate> = Vec::new();
    let mut help = Counts::default();
    let mut mem = Counts::default();
    let mut chunk = Counts::default();
    let mut mount = Counts::default();

    // ── Phase 1: help docs (v4 :134-175). Whole phase caught — continue on
    //    failure.
    match phase_help_docs(
        db, user_id, payload, help_files, now_iso, partial, target_dim,
    )
    .await
    {
        Ok((mut records, counts)) => {
            job_records.append(&mut records);
            help = counts;
        }
        Err(e) => tracing::error!(
            target: "quilltap::jobs",
            error = %e,
            "[EmbeddingReindexAll] Failed to process help docs",
        ),
    }

    // ── Phase 2: character memories (v4 :180-210). DELIBERATELY NOT caught —
    //    a failure here fails the job, unlike phases 1, 3 and 4.
    //
    //    The fan-out reads the MEMORIES table, not the characters repository (v4
    //    `7391404e`): `characters.findByUserId` silently drops a character whose
    //    vault is unavailable, and that character's memories would then never be
    //    re-embedded. See the module doc.
    // The read itself is fail-soft (v4's `safeQuery`); an unavailable read pool
    // resolves the same way, so this phase's no-catch rule still cannot turn a
    // transient read failure into a failed job.
    let character_ids = db
        .read_main(|conn| Ok(memories_read::find_distinct_character_ids(conn)))
        .unwrap_or_default();

    if scope == "all" {
        for character_id in &character_ids {
            let cid = character_id.clone();
            db.write(move |ws| {
                crate::db::vector_indices::VectorIndicesRepository::new(ws.main().connection())
                    .delete_by_character_id(&cid)
                    .map(|_| ())
            })
            .await
            .map_err(|e| format!("{e}"))?;
        }
    }

    let failed_memory_ids = failed_ids_for(db, partial, "MEMORY", &payload.profile_id);
    for character_id in &character_ids {
        let cid = character_id.clone();
        let memories = db
            .read_main(move |conn| memories_read::find_by_character_id(conn, &cid))
            .map_err(|e| format!("{e}"))?;
        for memory in &memories {
            if partial && embedding_matches_dim(memory_embedding_dim(memory), target_dim) {
                mem.dim_matched += 1;
                continue;
            }
            let Some(id) = memory.get("id").and_then(Value::as_str) else {
                continue;
            };
            if failed_memory_ids.contains(id) {
                mem.failed_skipped += 1;
                continue;
            }
            job_records.push(build_job_record(
                user_id,
                json!({
                    "entityType": "MEMORY",
                    "entityId": id,
                    // v4 reads `memory.characterId`, not the loop variable.
                    "characterId": memory.get("characterId").and_then(Value::as_str),
                    "profileId": payload.profile_id,
                }),
                now_iso,
            ));
            mem.enqueued += 1;
        }
    }

    // ── Phase 3: conversation chunks (v4 :215-240). Caught, like phase 1.
    match phase_conversation_chunks(db, user_id, payload, now_iso, partial, target_dim).await {
        Ok((mut records, counts)) => {
            job_records.append(&mut records);
            chunk = counts;
        }
        Err(e) => tracing::error!(
            target: "quilltap::jobs",
            error = %e,
            "[EmbeddingReindexAll] Failed to process conversation chunks",
        ),
    }

    // ── Phase 4: document mount chunks (v4 :299-330, NEW in `7391404e`). Caught,
    //    like phases 1 and 3.
    match phase_mount_chunks(db, user_id, payload, now_iso, partial, target_dim).await {
        Ok((mut records, counts)) => {
            job_records.append(&mut records);
            mount = counts;
        }
        Err(e) => tracing::error!(
            target: "quilltap::jobs",
            error = %e,
            "[EmbeddingReindexAll] Failed to process document mount chunks",
        ),
    }

    // ── Batch insert (v4 :245-255): slices of BATCH_SIZE, ids minted per record.
    for chunk in job_records.chunks(BATCH_SIZE) {
        let batch: Vec<BjCreate> = chunk.to_vec();
        let ids: Vec<String> = (0..batch.len())
            .map(|_| Uuid::new_v4().to_string())
            .collect();
        let now = now_iso.to_string();
        db.write(move |ws| {
            BackgroundJobsRepository::new(ws.main().connection())
                .create_batch(&batch, &ids, &now)
                .map(|_| ())
        })
        .await
        .map_err(|e| format!("{e}"))?;
    }

    // v4 :258 `ensureProcessorRunning()`.
    crate::services::queue_service::wake_processor();

    // v4's completion log (:352-369). The boot dimension reconcile is now a
    // caller whose progress is otherwise invisible, so this is the one place the
    // per-phase tallies surface.
    tracing::info!(
        target: "quilltap::jobs",
        profile_id = %payload.profile_id,
        scope = %scope,
        target_dim = target_dim,
        help_doc_count = help.enqueued,
        memory_count = mem.enqueued,
        chunk_count = chunk.enqueued,
        mount_chunk_count = mount.enqueued,
        help_docs_skipped = help.dim_matched,
        memories_skipped = mem.dim_matched,
        chunks_skipped = chunk.dim_matched,
        mount_chunks_skipped = mount.dim_matched,
        stale_chats_skipped = chunk.stale_chats_skipped,
        failed_skipped = help.failed_skipped + mem.failed_skipped
            + chunk.failed_skipped + mount.failed_skipped,
        total_enqueued = job_records.len(),
        "[EmbeddingReindexAll] Reindex jobs enqueued",
    );

    Ok(())
}

/// v4's phase 1 body, lifted so its `try`/`catch` is one call site.
async fn phase_help_docs(
    db: &Db,
    user_id: &str,
    payload: &EmbeddingReindexAllPayload,
    help_files: &[HelpSourceFile],
    now_iso: &str,
    partial: bool,
    target_dim: usize,
) -> Result<(Vec<BjCreate>, Counts), DbError> {
    // v4 syncs from disk FIRST, then reads the table, then (full scope) clears.
    let files = help_files.to_vec();
    db.write(move |ws| {
        sync_help_docs(ws.main().connection(), &files);
        Ok(())
    })
    .await?;

    let docs = db.read_main(|conn| {
        crate::db::help_docs::HelpDocsRepository::new(conn).find_all_with_embedding_dims()
    })?;

    // v4 :153 gates the wipe on `scope === 'all'` EXACTLY — not on "not
    // partial". An unrecognized scope string wipes nothing in v4 (and the
    // payload doc above promises the same), so compare against the literal.
    if payload.scope.as_deref().unwrap_or("all") == "all" {
        let now = now_iso.to_string();
        db.write(move |ws| {
            crate::db::help_docs::HelpDocsRepository::new(ws.main().connection())
                .clear_all_embeddings(&now)
                .map(|_| ())
        })
        .await?;
    }

    // v4 loads the FAILED set after the sync + wipe, before the loop.
    let failed = failed_ids_for(db, partial, "HELP_DOC", &payload.profile_id);
    let mut out = Vec::new();
    let mut counts = Counts::default();
    for (id, dim) in docs {
        if partial && embedding_matches_dim(dim, target_dim) {
            counts.dim_matched += 1;
            continue;
        }
        if failed.contains(&id) {
            counts.failed_skipped += 1;
            continue;
        }
        out.push(build_job_record(
            user_id,
            json!({
                "entityType": "HELP_DOC",
                "entityId": id,
                "profileId": payload.profile_id,
            }),
            now_iso,
        ));
        counts.enqueued += 1;
    }
    Ok((out, counts))
}

/// v4's phase 3 body, lifted so its `try`/`catch` is one call site.
async fn phase_conversation_chunks(
    db: &Db,
    user_id: &str,
    payload: &EmbeddingReindexAllPayload,
    now_iso: &str,
    partial: bool,
    target_dim: usize,
) -> Result<(Vec<BjCreate>, Counts), DbError> {
    let uid = user_id.to_string();
    let chats = db.read_main(move |conn| chats_read::find_by_user_id(conn, &uid))?;
    // v4 resolves the stale window ONCE for the whole phase. This handler runs on
    // the async job runner with a live `&Db`, so the `&Db`-flavored `is_stale` is
    // the right twin here — the `_conn` twins exist for the BOOT path, which is
    // already inside a write closure (P4.D25).
    let stale_cutoff_ms = crate::clock::iso_to_ms(&super::queue_service::retention_cutoff_iso(
        super::queue_service::resolve_stale_chat_days(db),
        crate::clock::iso_to_ms(now_iso).unwrap_or(0),
    ))
    .unwrap_or(0);
    let failed = failed_ids_for(db, partial, "CONVERSATION_CHUNK", &payload.profile_id);
    let mut out = Vec::new();
    let mut counts = Counts::default();
    for chat in &chats {
        let Some(chat_id) = chat.get("id").and_then(Value::as_str) else {
            continue;
        };
        // Cold-tiered chats are healed on reopen, not here — see the module doc.
        // v4 passes the whole chat row through to the shared gate; an error
        // resolves to "not stale" inside it (v5's port keeps that), so a chat is
        // never dropped for an unreadable message history.
        if super::maintenance::is_stale(db, chat, stale_cutoff_ms)? {
            counts.stale_chats_skipped += 1;
            continue;
        }
        let cid = chat_id.to_string();
        let chunks = db.read_main(move |conn| {
            crate::db::conversation_chunks::ConversationChunksRepository::new(conn)
                .find_by_chat_id(&cid)
        })?;
        for chunk in &chunks {
            if partial && embedding_matches_dim(chunk.embedding_dim, target_dim) {
                counts.dim_matched += 1;
                continue;
            }
            if failed.contains(&chunk.id) {
                counts.failed_skipped += 1;
                continue;
            }
            out.push(build_job_record(
                user_id,
                json!({
                    "entityType": "CONVERSATION_CHUNK",
                    "entityId": chunk.id,
                    "chatId": chat_id,
                    "profileId": payload.profile_id,
                }),
                now_iso,
            ));
            counts.enqueued += 1;
        }
    }
    Ok((out, counts))
}

/// v4's NEW phase 4 body (`7391404e`), lifted so its `try`/`catch` is one call
/// site: one `EMBEDDING_GENERATE` per chunk of every ENABLED document mount.
///
/// Only enabled mounts — a disabled mount is not searchable, and its scan pipeline
/// re-embeds when it is re-enabled. Mount points and chunks both live in the
/// mount-index partition, so a main-only instance has neither and the phase is a
/// no-op (`read_mount_index` fails, the caller logs and continues — v4's own arm
/// for the same shape).
async fn phase_mount_chunks(
    db: &Db,
    user_id: &str,
    payload: &EmbeddingReindexAllPayload,
    now_iso: &str,
    partial: bool,
    target_dim: usize,
) -> Result<(Vec<BjCreate>, Counts), DbError> {
    // v4 `docMountPoints.findEnabled()`. v5's accessor carries the same SELECT
    // under a name from its first consumer (the operator doc-edit override).
    let mount_points = db.read_mount_index(|conn| {
        crate::db::doc_mount_points::DocMountPointsRepository::new(conn).find_enabled_for_docedit()
    })?;
    let failed = failed_ids_for(db, partial, "MOUNT_CHUNK", &payload.profile_id);
    let mut out = Vec::new();
    let mut counts = Counts::default();
    for mount_point in &mount_points {
        let mp = mount_point.id.clone();
        let chunks = db.read_mount_index(move |conn| {
            crate::db::doc_mount_chunks::DocMountChunksRepository::new(conn)
                .find_rows_by_mount_point_id(&mp)
        })?;
        for chunk in &chunks {
            if partial && embedding_matches_dim(chunk.embedding_dim, target_dim) {
                counts.dim_matched += 1;
                continue;
            }
            if failed.contains(&chunk.id) {
                counts.failed_skipped += 1;
                continue;
            }
            // v4's phase-4 payload carries exactly these three keys — no
            // `mountPointId`, unlike the scan pipeline's enqueue.
            out.push(build_job_record(
                user_id,
                json!({
                    "entityType": "MOUNT_CHUNK",
                    "entityId": chunk.id,
                    "profileId": payload.profile_id,
                }),
                now_iso,
            ));
            counts.enqueued += 1;
        }
    }
    Ok((out, counts))
}

/// The stored vector's length for a memory row as `memories_read` marshals it:
/// v4's `JSON.stringify(Float32Array)` object shape `{"0":v0,…}`, so the key
/// count IS `embedding.length`; a NULL embedding marshals to `null` → 0.
fn memory_embedding_dim(memory: &Value) -> usize {
    memory
        .get("embedding")
        .and_then(Value::as_object)
        .map(|o| o.len())
        .unwrap_or(0)
}

/// An `EMBEDDING_REINDEX_ALL` [`crate::services::job_runner::JobHandler`]. Needs
/// no model or wire seam — only the DB and the host's help-tree walk — so the
/// host registers it in the seam-free set.
pub struct EmbeddingReindexAllHandler {
    /// The host's help-tree walk, read once at composition (v4 re-walks per job;
    /// the tree ships with the binary and cannot change under a running
    /// process).
    pub help_files: Vec<HelpSourceFile>,
    /// `None` reads the real clock at handle time; `Some(s)` pins it.
    pub now_iso: Option<String>,
}

impl crate::services::job_runner::JobHandler for EmbeddingReindexAllHandler {
    fn handle<'a>(
        &'a self,
        db: &'a Db,
        job: &'a crate::db::background_jobs::BackgroundJob,
    ) -> crate::services::job_runner::JobFuture<'a> {
        Box::pin(async move {
            let payload_json: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
            let payload = EmbeddingReindexAllPayload::from_json(&payload_json);
            let now = self.now_iso.clone().unwrap_or_else(crate::clock::now_iso);
            match handle_embedding_reindex_all(db, &job.user_id, &payload, &self.help_files, &now)
                .await
            {
                Ok(()) => crate::services::job_runner::JobOutcome::Completed(None),
                Err(e) => crate::services::job_runner::JobOutcome::Failed(e),
            }
        })
    }
}
