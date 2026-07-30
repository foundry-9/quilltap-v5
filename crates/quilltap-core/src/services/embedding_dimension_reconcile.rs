//! Embedding dimension reconciliation (startup self-heal) — a port of v4's
//! `lib/startup/reconcile-embedding-dimensions.ts` (P4.d27 / v4 `7391404e`),
//! wired at boot immediately after the render reconcile (v4
//! `instrumentation.ts` "PHASE 3.7", right behind "PHASE 3.6").
//!
//! There is exactly one embedding standard per instance: the vectors the default
//! embedding profile produces. This pass runs on every boot and makes the stored
//! corpus conform to it, across every embedding-bearing store:
//!
//!   - `memories`            (main — re-embedded via reindex)
//!   - `vector_entries`      (main — non-conforming rows DELETED here; they are
//!                            derived data, already invisible to vector search,
//!                            and the memory re-embed recreates them)
//!   - `vector_indices`      (main — meta `dimensions` snapped to target)
//!   - `conversation_chunks` (main — live chats re-embedded; STALE chats'
//!                            non-conforming embeddings NULLed to the cold-tier
//!                            state the stale sweep produces, so the Salon reopen
//!                            path re-embeds on demand)
//!   - `help_docs`           (main — re-embedded via reindex)
//!   - `doc_mount_chunks`    (the mount-index partition — see the ⚠ below)
//!
//! How this state arises: historically, switching WHICH profile was the default
//! did not trigger a re-embed (only editing an already-default profile's
//! provider/model did), so a TF-IDF-era corpus could survive under a neural
//! default indefinitely — stored 258-d vectors silently skipped by every cosine
//! scan, and warned about on every housekeeping pass. This pass is the recurring
//! safety net for corpora already in that state, for kills mid-reindex, and for
//! any future writer that slips a non-conforming vector in.
//!
//! Cheap when clean: one COUNT per table (format-aware SQL on the blob header,
//! [`EMBEDDING_DIM_SQL`]), no rows hydrated. When non-conforming vectors ARE
//! found, the repair is ENQUEUED as a `mismatched-dim` `EMBEDDING_REINDEX_ALL`
//! rather than run inline, so a big backlog cannot block the loading screen.
//! Rows marked FAILED for the default profile are excluded from the "needs work"
//! count — they are deterministic failures and would otherwise re-enqueue a
//! reindex on every boot with no progress.
//!
//! Skipped entirely for a BUILTIN (TF-IDF) default: its dimension is the fitted
//! vocabulary size, which the refit pipeline owns.
//!
//! ## ⚠ The mount-chunk count is DEAD CODE IN v4, and reproduced as such
//!
//! v4's `countNonconformingMountChunks` opens with
//! `tableExists(mainDb, 'doc_mount_points')` and its comment says "mount point
//! config lives in the main DB". **It does not** — `doc_mount_points` is a
//! mount-index table (v4's own repository logs "Failed to ensure
//! doc_mount_points table in mount index database", and v5's `fresh_schema.json`,
//! re-dumped from v4's `generateDDL`, lists it under `mountIndex`). So on every
//! real instance that guard is false and the function returns 0 before it ever
//! reaches the mount-index handle. v4's unit test does not catch this because it
//! creates `doc_mount_points` in its *main* test database.
//!
//! This port reproduces v4's operation order exactly, so `mismatched.mountChunks`
//! is 0 on any real instance here too — and the differential proves it over a
//! corpus that deliberately contains non-conforming chunks on an ENABLED mount.
//! The counting logic below is nonetheless a faithful port and is exercised by
//! this module's own unit test (which, like v4's, puts `doc_mount_points` in the
//! main connection). **Do not "fix" this without a ruling**: it is v4 behavior,
//! and the reindex handler's phase 4 — which reads mount points correctly — heals
//! those chunks whenever a reindex runs for any other reason.
//!
//! ## v5 divergences, all documented
//!
//!   - **`getVectorStoreManager().unloadAll()` is a no-op here.** v4 drops its
//!     in-memory store cache after a delete/snap so the next search reloads; v5
//!     has no store cache (the standing documented no-op, the
//!     `embedding_reindex_job` precedent).
//!   - **Synchronous, not fire-and-forget.** v4 runs this phase as a detached
//!     promise so a large backlog cannot delay readiness; v5's boot seeds are
//!     synchronous on a joined thread. The COUNT-only fast path bounds the cost
//!     on a conforming corpus and the repair is enqueued, never run inline, so
//!     the worst case is bounded by the scans — which is what v5's existing
//!     render reconcile already accepts.
//!   - **The enqueue writes the job row directly** on the held connection
//!     (`enqueue_embedding_reindex_all` is async and this runs inside
//!     `write_blocking`), matching the render reconcile's precedent. The row it
//!     writes is byte-identical to that helper's: priority −1, `maxAttempts` 3.
//!
//! Runs on the writer's main connection, like v5's other boot repairs — so every
//! shared helper is the `_conn` twin. Never throws into the startup path.

use rusqlite::{params, Connection};
use serde_json::json;

use super::maintenance::is_stale_conn;
use super::queue_service::{resolve_stale_chat_days_conn, retention_cutoff_iso};
use crate::clock::iso_to_ms;
use crate::db::background_jobs::CreateOptions;
use crate::db::background_jobs::{BackgroundJobsRepository, BjCreate};
use crate::db::DbError;
use crate::embedding_blob::EMBEDDING_DIM_SQL;

/// Why the pass did nothing (v4 `skippedReason`).
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum SkippedReason {
    /// No profile has `isDefault = 1`.
    NoProfile,
    /// The default is BUILTIN — its dimension is the fitted vocabulary size.
    BuiltinProfile,
    /// The default profile declares neither `truncateToDimensions` nor
    /// `dimensions` (or declares one ≤ 0).
    NoFixedDim,
    /// The database handle was unavailable (v4's `getRawDatabase()` → null).
    DbUnavailable,
}

impl SkippedReason {
    /// v4's string form, which the differential compares.
    pub fn as_str(self) -> &'static str {
        match self {
            SkippedReason::NoProfile => "no-profile",
            SkippedReason::BuiltinProfile => "builtin-profile",
            SkippedReason::NoFixedDim => "no-fixed-dim",
            SkippedReason::DbUnavailable => "db-unavailable",
        }
    }
}

/// Recoverable non-conforming rows found, per table (v4 `mismatched`).
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub struct MismatchedCounts {
    pub memories: usize,
    pub conversation_chunks: usize,
    pub help_docs: usize,
    /// ⚠ Structurally 0 on any real instance — see the module doc.
    pub mount_chunks: usize,
}

impl MismatchedCounts {
    fn total(self) -> usize {
        self.memories + self.conversation_chunks + self.help_docs + self.mount_chunks
    }
}

/// What one reconciliation pass did (v4 `EmbeddingDimensionReconcileResult`).
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub struct DimensionReconcileResult {
    /// Target dimension enforced, or `None` when the pass was skipped.
    pub target_dimensions: Option<usize>,
    /// Why the pass was skipped, if it was.
    pub skipped_reason: Option<SkippedReason>,
    /// Non-conforming `vector_entries` rows deleted.
    pub vector_entries_deleted: usize,
    /// `vector_indices` meta rows whose dimensions were snapped to target.
    pub vector_index_meta_fixed: usize,
    /// Stale-chat conversation chunks whose non-conforming embeddings were NULLed.
    pub stale_chunk_embeddings_cleared: usize,
    pub mismatched: MismatchedCounts,
    /// Whether a `mismatched-dim` reindex was enqueued.
    pub reindex_enqueued: bool,
}

impl DimensionReconcileResult {
    fn skipped(reason: SkippedReason) -> Self {
        Self {
            skipped_reason: Some(reason),
            ..Default::default()
        }
    }
}

/// v4's `NONCONFORMING` WHERE fragment: embedding present and not at the target
/// dimension. The single `?` binds the target dim.
fn nonconforming() -> String {
    format!("embedding IS NOT NULL AND {EMBEDDING_DIM_SQL} != ?")
}

/// v4's `NOT_FAILED(table)` exclusion fragment — rows already marked FAILED for
/// the default profile are deterministic failures (oversize, over-context) that
/// must not keep re-triggering reindexes. The two `?` bind entityType then
/// profileId, in that order.
fn not_failed(table: &str) -> String {
    format!(
        "NOT EXISTS (\n  SELECT 1 FROM \"embedding_status\" es\n  \
         WHERE es.\"entityType\" = ?\n    \
         AND es.\"entityId\" = \"{table}\".\"id\"\n    \
         AND es.\"profileId\" = ?\n    \
         AND es.\"status\" = 'FAILED'\n)"
    )
}

/// v4's `tableExists` — a lazily-created v4 instance legitimately lacks tables
/// (the P4.9G3 lesson), and every count below is guarded by it.
fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT name FROM sqlite_master WHERE type='table' AND name = ?",
        params![name],
        |r| r.get::<_, String>(0),
    )
    .is_ok()
}

/// v4's `countNonconforming` — one COUNT with the FAILED exclusion. For memories
/// and help docs a NULL embedding is ALSO non-conforming (nothing else re-embeds
/// it); conversation chunks are NOT counted when NULL (that is the deliberate
/// cold-tier state, healed on reopen), and mount chunks' NULLs belong to the
/// mount scan pipeline.
fn count_nonconforming(
    conn: &Connection,
    table: &str,
    target_dim: usize,
    entity_type: &str,
    profile_id: &str,
    include_null: bool,
) -> Result<usize, DbError> {
    let nc = nonconforming();
    let where_clause = if include_null {
        format!("(embedding IS NULL OR {nc})")
    } else {
        nc
    };
    let sql = format!(
        "SELECT COUNT(*) AS n FROM \"{table}\" WHERE {where_clause} AND {}",
        not_failed(table)
    );
    let n: i64 = conn.query_row(
        &sql,
        params![target_dim as i64, entity_type, profile_id],
        |r| r.get(0),
    )?;
    Ok(n as usize)
}

/// The default embedding profile's four reconcile-relevant columns, read straight
/// off the row (v4 does the same: this runs before some subsystems are up).
struct DefaultProfile {
    id: String,
    user_id: String,
    provider: String,
    dimensions: Option<i64>,
    truncate_to_dimensions: Option<i64>,
}

fn find_default_profile(conn: &Connection) -> Result<Option<DefaultProfile>, DbError> {
    conn.query_row(
        "SELECT id, userId, provider, dimensions, truncateToDimensions \
         FROM embedding_profiles WHERE isDefault = 1 LIMIT 1",
        [],
        |r| {
            Ok(DefaultProfile {
                id: r.get(0)?,
                user_id: r.get(1)?,
                provider: r.get(2)?,
                // REAL-affinity columns in v5's schema; read leniently.
                dimensions: r.get::<_, Option<f64>>(3)?.map(|v| v as i64),
                truncate_to_dimensions: r.get::<_, Option<f64>>(4)?.map(|v| v as i64),
            })
        },
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other.into()),
    })
}

/// Reconcile every stored embedding against the default profile's dimension
/// (v4 `reconcileEmbeddingDimensions`). **Never fails the boot** — any error is
/// logged and reported as an all-zero result with no `skipped_reason`, exactly
/// as v4's outer try/catch does.
///
/// `main` is the writer's main connection; `mount` is the mount-index connection
/// when the instance has that partition. `now_ms` is the injected clock (the
/// staleness window's origin).
pub fn reconcile_embedding_dimensions(
    main: &Connection,
    mount: Option<&Connection>,
    now_ms: i64,
) -> DimensionReconcileResult {
    match run_reconcile(main, mount, now_ms) {
        Ok(result) => result,
        Err(e) => {
            tracing::error!(
                target: "quilltap::boot",
                error = %e,
                "Embedding dimension reconcile failed",
            );
            DimensionReconcileResult::default()
        }
    }
}

fn run_reconcile(
    main: &Connection,
    mount: Option<&Connection>,
    now_ms: i64,
) -> Result<DimensionReconcileResult, DbError> {
    // v4's `db-unavailable` arm is `getRawDatabase()` returning null. v5 is handed
    // a live connection, so the analogous "the corpus is not there" condition is a
    // main partition with no `embedding_profiles` table at all.
    if !table_exists(main, "embedding_profiles") {
        return Ok(DimensionReconcileResult::skipped(
            SkippedReason::DbUnavailable,
        ));
    }

    let Some(profile) = find_default_profile(main)? else {
        return Ok(DimensionReconcileResult::skipped(SkippedReason::NoProfile));
    };
    if profile.provider == "BUILTIN" {
        return Ok(DimensionReconcileResult::skipped(
            SkippedReason::BuiltinProfile,
        ));
    }

    let target_dim = profile.truncate_to_dimensions.or(profile.dimensions);
    let Some(target_dim) = target_dim.filter(|d| *d > 0) else {
        tracing::warn!(
            target: "quilltap::boot",
            profile_id = %profile.id,
            provider = %profile.provider,
            "Default embedding profile has no fixed dimension; cannot enforce conformance",
        );
        return Ok(DimensionReconcileResult::skipped(SkippedReason::NoFixedDim));
    };
    let target_dim = target_dim as usize;

    let mut result = DimensionReconcileResult {
        target_dimensions: Some(target_dim),
        ..Default::default()
    };

    // ---- vector_entries: delete non-conforming rows outright. They are pure
    // derived data (the memory row keeps the source text), every search path
    // already skips them by length, and the memory re-embed recreates them.
    // NOTE: no FAILED exclusion and no user scoping — v4 deletes unconditionally.
    if table_exists(main, "vector_entries") {
        result.vector_entries_deleted = main.execute(
            &format!("DELETE FROM \"vector_entries\" WHERE {}", nonconforming()),
            params![target_dim as i64],
        )?;
    }

    // ---- vector_indices: snap meta dimensions to the standard so freshly
    // re-embedded vectors are accepted and search validates against the truth.
    if table_exists(main, "vector_indices") {
        result.vector_index_meta_fixed = main.execute(
            "UPDATE \"vector_indices\" SET dimensions = ? WHERE dimensions != ?",
            params![target_dim as i64, target_dim as i64],
        )?;
    }

    // v4 drops its cached in-memory stores here when either changed. v5 has no
    // store cache — a documented no-op (see the module doc).

    // ---- conversation_chunks on STALE chats: converge to the cold-tier state
    // (NULL embedding) instead of paying to re-embed chats nobody is reading.
    if table_exists(main, "conversation_chunks") && table_exists(main, "chats") {
        result.stale_chunk_embeddings_cleared =
            clear_stale_chat_nonconforming_chunks(main, target_dim, now_ms)?;
    }

    // ---- Count what still needs re-embedding (recoverable rows only).
    if table_exists(main, "memories") {
        // The `characterId IS NOT NULL` guard mirrors the reindex fan-out (which
        // enumerates characters from `memories.characterId`) — a row it cannot
        // reach must not re-trigger the sweep every boot.
        let sql = format!(
            "SELECT COUNT(*) AS n FROM \"memories\"\n         \
             WHERE (embedding IS NULL OR {})\n           \
             AND \"characterId\" IS NOT NULL\n           \
             AND {}",
            nonconforming(),
            not_failed("memories")
        );
        let n: i64 = main.query_row(
            &sql,
            params![target_dim as i64, "MEMORY", profile.id.as_str()],
            |r| r.get(0),
        )?;
        result.mismatched.memories = n as usize;
    }
    if table_exists(main, "conversation_chunks") && table_exists(main, "chats") {
        result.mismatched.conversation_chunks =
            count_nonconforming_live_chunks(main, target_dim, &profile.id)?;
    }
    if table_exists(main, "help_docs") {
        result.mismatched.help_docs =
            count_nonconforming(main, "help_docs", target_dim, "HELP_DOC", &profile.id, true)?;
    }
    result.mismatched.mount_chunks =
        count_nonconforming_mount_chunks(main, mount, target_dim, &profile.id);

    if result.mismatched.total() > 0 {
        result.reindex_enqueued = enqueue_mismatched_reindex(main, &profile.user_id, &profile.id)?;
    }

    let touched_anything = result.mismatched.total() > 0
        || result.vector_entries_deleted > 0
        || result.vector_index_meta_fixed > 0
        || result.stale_chunk_embeddings_cleared > 0;

    // v4's two-arm log shape: a full INFO line when anything was touched, a DEBUG
    // line otherwise. (The healthy outcome must still say something at debug —
    // that is the dogfood-finding shape the render reconcile's gate had.)
    if touched_anything {
        tracing::info!(
            target: "quilltap::boot",
            profile_id = %profile.id,
            target_dimensions = target_dim,
            vector_entries_deleted = result.vector_entries_deleted,
            vector_index_meta_fixed = result.vector_index_meta_fixed,
            stale_chunk_embeddings_cleared = result.stale_chunk_embeddings_cleared,
            memories = result.mismatched.memories,
            conversation_chunks = result.mismatched.conversation_chunks,
            help_docs = result.mismatched.help_docs,
            mount_chunks = result.mismatched.mount_chunks,
            reindex_enqueued = result.reindex_enqueued,
            "Embedding dimension reconcile found non-conforming vectors",
        );
    } else {
        tracing::debug!(
            target: "quilltap::boot",
            profile_id = %profile.id,
            target_dimensions = target_dim,
            "Embedding dimension reconcile: corpus conforms",
        );
    }

    Ok(result)
}

/// v4's `countNonconformingLiveChunks` — non-conforming chunks the reindex
/// handler can actually reach: the chat must still exist (an orphaned chunk would
/// otherwise re-trigger a futile reindex on every boot). Stale chats'
/// non-conforming chunks were already NULLed by
/// [`clear_stale_chat_nonconforming_chunks`], so they no longer match.
fn count_nonconforming_live_chunks(
    conn: &Connection,
    target_dim: usize,
    profile_id: &str,
) -> Result<usize, DbError> {
    let sql = format!(
        "SELECT COUNT(*) AS n FROM \"conversation_chunks\"\n       \
         WHERE {}\n         \
         AND \"chatId\" IN (SELECT \"id\" FROM \"chats\")\n         \
         AND {}",
        nonconforming(),
        not_failed("conversation_chunks")
    );
    let n: i64 = conn.query_row(
        &sql,
        params![target_dim as i64, "CONVERSATION_CHUNK", profile_id],
        |r| r.get(0),
    )?;
    Ok(n as usize)
}

/// v4's `clearStaleChatNonconformingChunks` — NULL non-conforming chunk
/// embeddings on stale chats. Staleness is decided by the same shared gate the
/// maintenance sweeps use, evaluated per CANDIDATE chat — only chats that
/// actually hold a non-conforming chunk are examined, so this is a no-op scan on
/// a conforming corpus.
fn clear_stale_chat_nonconforming_chunks(
    conn: &Connection,
    target_dim: usize,
    now_ms: i64,
) -> Result<usize, DbError> {
    let candidate_ids: Vec<String> = {
        let sql = format!(
            "SELECT DISTINCT \"chatId\" AS chatId FROM \"conversation_chunks\"\n         \
             WHERE \"chatId\" IS NOT NULL AND {}",
            nonconforming()
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![target_dim as i64], |r| r.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    if candidate_ids.is_empty() {
        return Ok(0);
    }

    // v4 hydrates each candidate's `{id, updatedAt}` and drops the ones whose
    // chat row is gone (an orphan chunk's chat can never be stale — there is
    // nothing to date it by).
    let mut candidates: Vec<(String, Option<String>)> = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT \"id\" AS id, \"updatedAt\" AS updatedAt FROM \"chats\" WHERE \"id\" = ?",
        )?;
        for id in &candidate_ids {
            let found = stmt
                .query_row(params![id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
                })
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })?;
            if let Some(row) = found {
                candidates.push(row);
            }
        }
    }

    let cutoff_ms = iso_to_ms(&retention_cutoff_iso(
        resolve_stale_chat_days_conn(conn),
        now_ms,
    ))
    .unwrap_or(now_ms);

    let clear_sql = format!(
        "UPDATE \"conversation_chunks\" SET embedding = NULL WHERE \"chatId\" = ? AND {}",
        nonconforming()
    );
    let mut cleared = 0usize;
    for (chat_id, updated_at) in candidates {
        // v4 narrows `isStale` to `Pick<ChatMetadata,'id'|'updatedAt'>` and passes
        // `updatedAt ?? ''`.
        let chat = json!({ "id": chat_id, "updatedAt": updated_at.unwrap_or_default() });
        if is_stale_conn(conn, &chat, cutoff_ms)? {
            cleared += conn.execute(&clear_sql, params![chat_id, target_dim as i64])?;
        }
    }
    Ok(cleared)
}

/// v4's `countNonconformingMountChunks`, operation-for-operation — INCLUDING the
/// `doc_mount_points`-on-the-MAIN-connection guard that makes it return 0 on every
/// real instance. See the module doc's ⚠ section; the whole body is fail-soft to 0
/// (v4 wraps it in try/catch and warns).
fn count_nonconforming_mount_chunks(
    main: &Connection,
    mount: Option<&Connection>,
    target_dim: usize,
    profile_id: &str,
) -> usize {
    let attempt = || -> Result<usize, DbError> {
        if !table_exists(main, "doc_mount_points") {
            return Ok(0);
        }
        let enabled_ids: Vec<String> = {
            let mut stmt = main.prepare("SELECT id FROM doc_mount_points WHERE enabled = 1")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        if enabled_ids.is_empty() {
            return Ok(0);
        }
        let Some(mount) = mount else {
            return Ok(0);
        };
        if !table_exists(mount, "doc_mount_chunks") {
            return Ok(0);
        }

        // FAILED-status rows live in the main DB, so the exclusion set is pulled
        // out and applied IN MEMORY (it is small: only deterministic failures).
        let failed = crate::db::embedding_status::EmbeddingStatusRepository::new(main)
            .list_failed_entity_ids("MOUNT_CHUNK", profile_id);

        let placeholders = enabled_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT id FROM \"doc_mount_chunks\"\n         \
             WHERE {} AND \"mountPointId\" IN ({placeholders})",
            nonconforming()
        );
        let mut stmt = mount.prepare(&sql)?;
        let mut binds: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(target_dim as i64)];
        for id in &enabled_ids {
            binds.push(Box::new(id.clone()));
        }
        let bind_refs: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(bind_refs.as_slice(), |r| r.get::<_, String>(0))?;
        let mut n = 0usize;
        for row in rows {
            if !failed.contains(&row?) {
                n += 1;
            }
        }
        Ok(n)
    };
    match attempt() {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!(
                target: "quilltap::boot",
                error = %e,
                "Mount-chunk dimension check skipped",
            );
            0
        }
    }
}

/// v4's `enqueueMismatchedReindex` — enqueue a `mismatched-dim`
/// `EMBEDDING_REINDEX_ALL`, deduping against one already PENDING/PROCESSING so
/// repeated boots while a backlog drains cannot stack duplicate sweeps.
///
/// v4 calls the async `enqueueEmbeddingReindexAll`; this runs inside
/// `write_blocking`, so (like the render reconcile) it writes the row directly on
/// the held connection. The row is byte-identical to that helper's.
fn enqueue_mismatched_reindex(
    main: &Connection,
    user_id: &str,
    profile_id: &str,
) -> Result<bool, DbError> {
    let repo = BackgroundJobsRepository::new(main);
    let recent = repo.find_recent_by_type("EMBEDDING_REINDEX_ALL", 10)?;
    if recent
        .iter()
        .any(|j| j.status == "PENDING" || j.status == "PROCESSING")
    {
        tracing::info!(
            target: "quilltap::boot",
            "Mismatched-dim reindex already queued; not enqueueing another",
        );
        return Ok(false);
    }

    let now = crate::clock::now_iso();
    let id = uuid::Uuid::new_v4().to_string();
    repo.create(
        &BjCreate {
            user_id: user_id.to_string(),
            job_type: "EMBEDDING_REINDEX_ALL".to_string(),
            status: Some("PENDING".to_string()),
            payload: json!({ "profileId": profile_id, "scope": "mismatched-dim" }),
            priority: -1.0,
            attempts: 0.0,
            max_attempts: 3.0,
            last_error: None,
            scheduled_at: now.clone(),
            started_at: None,
            completed_at: None,
        },
        &CreateOptions {
            id,
            created_at: now.clone(),
            updated_at: now,
        },
    )?;
    super::queue_service::ensure_processor_running();
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding_blob::{float32_to_blob, float32_to_blob_raw};

    const PROFILE_ID: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    const USER_ID: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    const TARGET_DIM: usize = 1024;
    const OLD_DIM: usize = 258;
    /// A `now` far past every seeded timestamp.
    const NOW_MS: i64 = 1_780_000_000_000; // 2026-06-08T…Z
    const FRESH_ISO: &str = "2026-06-07T00:00:00.000Z";
    const OLD_ISO: &str = "2026-01-01T00:00:00.000Z";

    fn vec_of(dim: usize) -> Vec<f32> {
        vec![0.1; dim]
    }
    /// Quantized (current-format) blob at the given dimension.
    fn good_blob(dim: usize) -> Vec<u8> {
        float32_to_blob(&vec_of(dim))
    }
    /// Legacy raw Float32 blob at the given dimension (the TF-IDF-era format).
    fn raw_blob(dim: usize) -> Vec<u8> {
        float32_to_blob_raw(&vec_of(dim))
    }

    /// The schema subset this pass reads/writes. Hand-rolled, like the render
    /// reconcile's, because a missing table is itself one of the cases.
    ///
    /// ⚠ `doc_mount_points` is created on the MAIN connection here — mirroring
    /// v4's own unit test, and the reason v4's test does not catch that the
    /// production table lives in the mount-index partition. That is exactly why
    /// this module's mount-chunk arm is exercised here and pinned at 0 in the
    /// differential.
    fn main_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE embedding_profiles (\
                id TEXT PRIMARY KEY, userId TEXT, provider TEXT, dimensions REAL, \
                truncateToDimensions REAL, isDefault INTEGER);\
             CREATE TABLE vector_entries (id TEXT PRIMARY KEY, characterId TEXT, embedding BLOB);\
             CREATE TABLE vector_indices (characterId TEXT PRIMARY KEY, dimensions REAL);\
             CREATE TABLE chats (id TEXT PRIMARY KEY, updatedAt TEXT);\
             CREATE TABLE chat_messages (\
                id TEXT PRIMARY KEY, chatId TEXT, type TEXT, role TEXT, \
                systemSender TEXT, createdAt TEXT);\
             CREATE TABLE conversation_chunks (id TEXT PRIMARY KEY, chatId TEXT, embedding BLOB);\
             CREATE TABLE memories (id TEXT PRIMARY KEY, characterId TEXT, embedding BLOB);\
             CREATE TABLE help_docs (id TEXT PRIMARY KEY, embedding BLOB);\
             CREATE TABLE embedding_status (\
                id TEXT PRIMARY KEY, entityType TEXT, entityId TEXT, profileId TEXT, \
                status TEXT);\
             CREATE TABLE doc_mount_points (id TEXT PRIMARY KEY, enabled INTEGER);\
             CREATE TABLE background_jobs (\
                id TEXT PRIMARY KEY, userId TEXT NOT NULL, type TEXT NOT NULL, \
                status TEXT NOT NULL, payload TEXT NOT NULL, priority REAL NOT NULL, \
                attempts REAL NOT NULL, maxAttempts REAL NOT NULL, lastError TEXT, \
                scheduledAt TEXT NOT NULL, startedAt TEXT, completedAt TEXT, \
                createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);",
        )
        .unwrap();
        conn
    }

    fn mount_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE doc_mount_chunks (\
                id TEXT PRIMARY KEY, mountPointId TEXT, embedding BLOB);",
        )
        .unwrap();
        conn
    }

    fn default_profile(conn: &Connection, provider: &str) {
        conn.execute(
            "INSERT INTO embedding_profiles \
               (id, userId, provider, dimensions, truncateToDimensions, isDefault) \
             VALUES (?1, ?2, ?3, ?4, NULL, 1)",
            params![PROFILE_ID, USER_ID, provider, TARGET_DIM as i64],
        )
        .unwrap();
    }

    fn entry(conn: &Connection, id: &str, blob: Vec<u8>) {
        conn.execute(
            "INSERT INTO vector_entries (id, characterId, embedding) VALUES (?1, 'char-1', ?2)",
            params![id, blob],
        )
        .unwrap();
    }

    fn failed_status(conn: &Connection, entity_type: &str, entity_id: &str) {
        conn.execute(
            "INSERT INTO embedding_status (id, entityType, entityId, profileId, status) \
             VALUES (?1, ?2, ?3, ?4, 'FAILED')",
            params![
                format!("es-{entity_id}"),
                entity_type,
                entity_id,
                PROFILE_ID
            ],
        )
        .unwrap();
    }

    fn enqueued_jobs(conn: &Connection) -> Vec<(String, String)> {
        let mut stmt = conn
            .prepare("SELECT type, payload FROM background_jobs ORDER BY rowid")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    /// v4's first test: non-conforming entries in BOTH on-disk formats are
    /// deleted, and the index meta is snapped.
    #[test]
    fn deletes_nonconforming_entries_raw_and_quantized_and_snaps_meta() {
        let conn = main_conn();
        default_profile(&conn, "OPENAI");
        entry(&conn, "e-raw-258", raw_blob(OLD_DIM));
        entry(&conn, "e-quant-258", good_blob(OLD_DIM));
        entry(&conn, "e-good", good_blob(TARGET_DIM));
        conn.execute(
            "INSERT INTO vector_indices (characterId, dimensions) VALUES ('char-1', ?1)",
            params![OLD_DIM as i64],
        )
        .unwrap();

        let r = reconcile_embedding_dimensions(&conn, None, NOW_MS);

        assert_eq!(r.skipped_reason, None);
        assert_eq!(r.target_dimensions, Some(TARGET_DIM));
        assert_eq!(r.vector_entries_deleted, 2);
        assert_eq!(r.vector_index_meta_fixed, 1);

        let remaining: Vec<String> = conn
            .prepare("SELECT id FROM vector_entries")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(remaining, vec!["e-good"]);
        // REAL affinity, like the real schema — the UPDATE binds an integer and
        // SQLite stores 1024.0, exactly as v4's JS number does.
        let meta: f64 = conn
            .query_row("SELECT dimensions FROM vector_indices", [], |r| r.get(0))
            .unwrap();
        assert_eq!(meta, TARGET_DIM as f64);
    }

    /// v4's second test: NULL and non-conforming memories count, FAILED and
    /// character-less ones do not, and a reindex is enqueued.
    #[test]
    fn counts_memories_excludes_failed_and_orphans_then_enqueues() {
        let conn = main_conn();
        default_profile(&conn, "OPENAI");
        let ins = "INSERT INTO memories (id, characterId, embedding) VALUES (?1, ?2, ?3)";
        conn.execute(ins, params!["m-old", "char-1", raw_blob(OLD_DIM)])
            .unwrap();
        conn.execute(ins, params!["m-null", "char-1", None::<Vec<u8>>])
            .unwrap();
        conn.execute(ins, params!["m-good", "char-1", good_blob(TARGET_DIM)])
            .unwrap();
        conn.execute(ins, params!["m-failed", "char-1", raw_blob(OLD_DIM)])
            .unwrap();
        conn.execute(ins, params!["m-orphan", None::<String>, raw_blob(OLD_DIM)])
            .unwrap();
        failed_status(&conn, "MEMORY", "m-failed");

        let r = reconcile_embedding_dimensions(&conn, None, NOW_MS);

        assert_eq!(r.mismatched.memories, 2, "m-old + m-null only");
        assert!(r.reindex_enqueued);
        let jobs = enqueued_jobs(&conn);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].0, "EMBEDDING_REINDEX_ALL");
        let payload: serde_json::Value = serde_json::from_str(&jobs[0].1).unwrap();
        assert_eq!(payload["profileId"], PROFILE_ID);
        assert_eq!(payload["scope"], "mismatched-dim");
    }

    /// v4's third test: stale chats' non-conforming chunks are NULLed; only
    /// live-chat chunks are counted, and an orphan chunk is counted by neither.
    #[test]
    fn nulls_stale_chunks_and_counts_only_live_ones() {
        let conn = main_conn();
        default_profile(&conn, "OPENAI");
        conn.execute(
            "INSERT INTO chats (id, updatedAt) VALUES ('stale-chat', ?1), ('live-chat', ?2)",
            params![OLD_ISO, FRESH_ISO],
        )
        .unwrap();
        let ins = "INSERT INTO conversation_chunks (id, chatId, embedding) VALUES (?1, ?2, ?3)";
        conn.execute(ins, params!["cc-stale", "stale-chat", raw_blob(OLD_DIM)])
            .unwrap();
        conn.execute(ins, params!["cc-live", "live-chat", raw_blob(OLD_DIM)])
            .unwrap();
        conn.execute(
            ins,
            params!["cc-live-good", "live-chat", good_blob(TARGET_DIM)],
        )
        .unwrap();
        conn.execute(ins, params!["cc-orphan", "gone-chat", raw_blob(OLD_DIM)])
            .unwrap();

        let r = reconcile_embedding_dimensions(&conn, None, NOW_MS);

        assert_eq!(r.stale_chunk_embeddings_cleared, 1);
        let stale: Option<Vec<u8>> = conn
            .query_row(
                "SELECT embedding FROM conversation_chunks WHERE id = 'cc-stale'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(stale.is_none(), "the stale chunk must be NULLed");
        assert_eq!(r.mismatched.conversation_chunks, 1, "cc-live only");
    }

    /// v4's fourth test — the mount-chunk COUNT, reachable only because
    /// `doc_mount_points` is on the MAIN connection here (see the module doc's ⚠).
    /// Enabled mounts only, FAILED excluded.
    #[test]
    fn counts_mount_chunks_for_enabled_mounts_only() {
        let main = main_conn();
        let mount = mount_conn();
        default_profile(&main, "OPENAI");
        main.execute(
            "INSERT INTO doc_mount_points (id, enabled) VALUES ('mp-on', 1), ('mp-off', 0)",
            [],
        )
        .unwrap();
        let ins = "INSERT INTO doc_mount_chunks (id, mountPointId, embedding) VALUES (?1, ?2, ?3)";
        mount
            .execute(ins, params!["mc-on-old", "mp-on", raw_blob(OLD_DIM)])
            .unwrap();
        mount
            .execute(ins, params!["mc-on-good", "mp-on", good_blob(TARGET_DIM)])
            .unwrap();
        mount
            .execute(ins, params!["mc-on-failed", "mp-on", raw_blob(OLD_DIM)])
            .unwrap();
        mount
            .execute(ins, params!["mc-off-old", "mp-off", raw_blob(OLD_DIM)])
            .unwrap();
        failed_status(&main, "MOUNT_CHUNK", "mc-on-failed");

        let r = reconcile_embedding_dimensions(&main, Some(&mount), NOW_MS);

        assert_eq!(r.mismatched.mount_chunks, 1, "mc-on-old only");
        assert!(r.reindex_enqueued);
    }

    /// The SAME corpus with `doc_mount_points` where it actually lives — the
    /// mount-index partition — counts ZERO. This is v4's live behavior, pinned so
    /// the divergence cannot be "fixed" by accident.
    #[test]
    fn mount_chunk_count_is_dead_when_mount_points_live_in_the_mount_partition() {
        let main = main_conn();
        let mount = mount_conn();
        default_profile(&main, "OPENAI");
        // Move the table to where production keeps it.
        main.execute_batch("DROP TABLE doc_mount_points;").unwrap();
        mount
            .execute_batch(
                "CREATE TABLE doc_mount_points (id TEXT PRIMARY KEY, enabled INTEGER);\
                 INSERT INTO doc_mount_points (id, enabled) VALUES ('mp-on', 1);",
            )
            .unwrap();
        mount
            .execute(
                "INSERT INTO doc_mount_chunks (id, mountPointId, embedding) VALUES (?1, ?2, ?3)",
                params!["mc-on-old", "mp-on", raw_blob(OLD_DIM)],
            )
            .unwrap();

        let r = reconcile_embedding_dimensions(&main, Some(&mount), NOW_MS);

        assert_eq!(
            r.mismatched.mount_chunks, 0,
            "v4 guards on the MAIN connection, so the count can never reach the chunks"
        );
        assert!(!r.reindex_enqueued, "nothing else was non-conforming");
    }

    /// v4's fifth test: a PENDING reindex among the ten most recent suppresses a
    /// second one.
    #[test]
    fn does_not_stack_a_second_reindex_while_one_is_pending() {
        let conn = main_conn();
        default_profile(&conn, "OPENAI");
        conn.execute(
            "INSERT INTO memories (id, characterId, embedding) VALUES ('m-old', 'char-1', ?1)",
            params![raw_blob(OLD_DIM)],
        )
        .unwrap();

        let first = reconcile_embedding_dimensions(&conn, None, NOW_MS);
        assert!(first.reindex_enqueued);
        let second = reconcile_embedding_dimensions(&conn, None, NOW_MS);
        assert_eq!(second.mismatched.memories, 1);
        assert!(!second.reindex_enqueued, "the dedupe must hold");
        assert_eq!(enqueued_jobs(&conn).len(), 1);
    }

    /// v4's sixth test: a BUILTIN default is a total no-op — its dimension is the
    /// fitted vocabulary size, which the refit pipeline owns.
    #[test]
    fn builtin_default_is_a_no_op() {
        let conn = main_conn();
        default_profile(&conn, "BUILTIN");
        entry(&conn, "e-old", raw_blob(OLD_DIM));

        let r = reconcile_embedding_dimensions(&conn, None, NOW_MS);

        assert_eq!(r.skipped_reason, Some(SkippedReason::BuiltinProfile));
        assert_eq!(r.target_dimensions, None);
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM vector_entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "nothing deleted");
        assert!(enqueued_jobs(&conn).is_empty());
    }

    /// v4's seventh test: a fully conforming corpus does nothing and enqueues
    /// nothing. This is the case that would pass with the whole module stubbed —
    /// which is why every other test above seeds non-conforming rows.
    #[test]
    fn conforming_corpus_does_nothing() {
        let conn = main_conn();
        default_profile(&conn, "OPENAI");
        conn.execute(
            "INSERT INTO memories (id, characterId, embedding) VALUES ('m-good', 'char-1', ?1)",
            params![good_blob(TARGET_DIM)],
        )
        .unwrap();
        entry(&conn, "e-good", good_blob(TARGET_DIM));

        let r = reconcile_embedding_dimensions(&conn, None, NOW_MS);

        assert_eq!(r.vector_entries_deleted, 0);
        assert_eq!(r.mismatched, MismatchedCounts::default());
        assert!(!r.reindex_enqueued);
        assert!(enqueued_jobs(&conn).is_empty());
    }

    /// The three skip arms the differential corpus cannot reach without emptying
    /// tables the later phases depend on.
    #[test]
    fn skip_arms() {
        // No profile at all.
        let conn = main_conn();
        assert_eq!(
            reconcile_embedding_dimensions(&conn, None, NOW_MS).skipped_reason,
            Some(SkippedReason::NoProfile)
        );

        // A non-BUILTIN default with no fixed dimension.
        let conn = main_conn();
        conn.execute(
            "INSERT INTO embedding_profiles \
               (id, userId, provider, dimensions, truncateToDimensions, isDefault) \
             VALUES (?1, ?2, 'OPENAI', NULL, NULL, 1)",
            params![PROFILE_ID, USER_ID],
        )
        .unwrap();
        assert_eq!(
            reconcile_embedding_dimensions(&conn, None, NOW_MS).skipped_reason,
            Some(SkippedReason::NoFixedDim)
        );

        // v5's analog of v4's null database handle: no corpus at all.
        let conn = Connection::open_in_memory().unwrap();
        assert_eq!(
            reconcile_embedding_dimensions(&conn, None, NOW_MS).skipped_reason,
            Some(SkippedReason::DbUnavailable)
        );
    }

    /// `truncateToDimensions` wins over `dimensions` (v4's `??`).
    #[test]
    fn truncation_wins_over_the_raw_dimension() {
        let conn = main_conn();
        conn.execute(
            "INSERT INTO embedding_profiles \
               (id, userId, provider, dimensions, truncateToDimensions, isDefault) \
             VALUES (?1, ?2, 'OPENAI', ?3, ?4, 1)",
            params![PROFILE_ID, USER_ID, TARGET_DIM as i64, 256i64],
        )
        .unwrap();
        entry(&conn, "e-256", good_blob(256));
        entry(&conn, "e-1024", good_blob(TARGET_DIM));

        let r = reconcile_embedding_dimensions(&conn, None, NOW_MS);

        assert_eq!(r.target_dimensions, Some(256));
        assert_eq!(r.vector_entries_deleted, 1, "the 1024-d entry goes");
    }
}
