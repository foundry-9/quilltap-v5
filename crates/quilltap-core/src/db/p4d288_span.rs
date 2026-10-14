//! The `83334b9a5` round's twelve boot ensures as ONE chain — P4.D288 (C1-2),
//! v4 `migrations/scripts/index.ts:881-903` at the pin, in v4's registration
//! order (which IS v4's run order): 1 superseded-at, 2 scenario source, 3
//! episode watermark, 4 recompute, 5 rezone, 6 superseded-by index, 7 settings
//! move, 8 vector retire, 9 cursors, 10 drop INTER jobs, 11 tfidf column, 12
//! drop housekeeping / consolidation jobs.
//!
//! ONE home so the host's boot ([`quilltap-host`]'s `seed_built_ins`, after
//! `add-memory-tiers-v1` and BEFORE the P4.160 index-family backfill, which
//! would otherwise make `idx_memories_supersededById` ahead of slot 6) and the
//! vintage mirror-boot in the harness run the SAME chain — a hand-copied list
//! drifted once already (the mirror-boot lacked slots 6 / 10 / 12).
//!
//! Each ensure is FATAL on failure (v4's migrations are not `resumable`, so a
//! failure stops v4's boot): the first `Err` returns. Each logs v5's ONE INFO
//! (`target: "quilltap::boot"`) when it did anything and is silent on a no-op
//! boot; the runner's own lines and `PRETTY_LABELS` are NO-PORT.
//!
//! `host_fallback` is the host's calendar NAME (`HostConfig::tz` — the
//! `QUILLTAP_TIMEZONE` / `TZ`-reconciled system zone, P4.D289 LR-3's
//! `server_tz`), the rezone's rung under the instance zone (O-8).

use rusqlite::Connection;

use super::memory_rewrite_heals::LedgeredHealOutcome;
use super::DbError;

/// Run the twelve span ensures, in v4's order, on the main partition.
pub fn run_span_ensures(main: &Connection, host_fallback: &str) -> Result<(), DbError> {
    let now = crate::clock::now_iso();
    // Slot 1 — `add-memory-superseded-at-v1`.
    let superseded_at = super::memory_superseded_at_repair::ensure_memory_superseded_at(main)?;
    if !superseded_at.is_noop() {
        tracing::info!(
            target: "quilltap::boot",
            migrationId = super::memory_superseded_at_repair::MIGRATION_ID,
            columnAdded = superseded_at.column_added,
            backfilledRows = superseded_at.backfilled_rows,
            "Fitted the memory supersession clock"
        );
    }
    // Slot 2 — `add-chat-scenario-source-v1`.
    let scenario_columns = super::chats_scenario_source_repair::ensure_chats_scenario_source(main)?;
    if !scenario_columns.is_empty() {
        tracing::info!(
            target: "quilltap::boot",
            migrationId = super::chats_scenario_source_repair::MIGRATION_ID,
            columnsAdded = scenario_columns.len(),
            "Added the chat scenario source columns"
        );
    }
    // Slot 3 — `add-episode-extraction-watermark-v1` (O-3's gate; STAMPED).
    if let Some(episode) =
        super::memory_episode_watermark_repair::ensure_memory_episode_watermark(main)?
    {
        tracing::info!(
            target: "quilltap::boot",
            migrationId = super::memory_episode_watermark_repair::MIGRATION_ID,
            columnAdded = episode.column_added,
            candidates = episode.candidates,
            backfilled = episode.backfilled,
            "Backfilled the episode watermarks from summary anchors"
        );
    }
    // Slots 3 + 9 on an instance where the chain is already SETTLED without
    // v4's ledger (a v5-provisioned one): O-3's gate runs nothing, so the two
    // rows v4's chain would write here are STAMPED (unit 13 — R-E's stamp rule).
    if let Some(settled) = super::memory_extraction_cursors_repair::stamp_settled_chain(main)? {
        tracing::info!(
            target: "quilltap::boot",
            episodeMigrationId = super::memory_episode_watermark_repair::MIGRATION_ID,
            cursorsMigrationId = super::memory_extraction_cursors_repair::MIGRATION_ID,
            episodeBackfilled = settled.episode_backfilled,
            cursorsCarried = settled.cursors_carried,
            "Recorded the settled extraction-cursor chain"
        );
    }
    // Slots 4 / 5 — the two STAMPED data rewrites (R-E).
    if let LedgeredHealOutcome::Ran { rewritten, total } =
        super::memory_rewrite_heals::recompute_reinforced_importance(main, &now)?
    {
        tracing::info!(
            target: "quilltap::boot",
            migrationId = super::memory_rewrite_heals::RECOMPUTE_MIGRATION_ID,
            rewritten,
            total,
            "Recomputed reinforced importance"
        );
    }
    if let LedgeredHealOutcome::Ran { rewritten, total } =
        super::memory_rewrite_heals::rezone_legacy_memory_occurred_at(main, host_fallback, &now)?
    {
        let time_zone =
            super::memory_rewrite_heals::instance_memory_time_zone(main, host_fallback)?;
        tracing::info!(
            target: "quilltap::boot",
            migrationId = super::memory_rewrite_heals::REZONE_MIGRATION_ID,
            timeZone = %time_zone,
            rewritten,
            candidates = total,
            "Re-stamped legacy UTC-midnight memory dates"
        );
    }
    // Slot 6 — `add-memories-superseded-by-index-v1`.
    if super::memories_superseded_by_index::ensure_memories_superseded_by_index(main)? {
        tracing::info!(
            target: "quilltap::boot",
            migrationId = super::memories_superseded_by_index::MIGRATION_ID,
            "Created idx_memories_supersededById"
        );
    }
    // Slot 7 — `move-memory-maintenance-settings-v1` (atomic).
    let moved =
        super::memory_maintenance_settings_move::ensure_memory_maintenance_settings_move(main)?;
    if moved.ran {
        tracing::info!(
            target: "quilltap::boot",
            migrationId = super::memory_maintenance_settings_move::MIGRATION_ID,
            copied = moved.copied,
            droppedColumns = moved.dropped_columns.len(),
            droppedInstanceKey = moved.dropped_instance_key,
            "Moved the memory maintenance settings"
        );
    }
    // Slot 8 — `retire-memory-vector-entries-v1` (O-1: wired once no
    // compiled path reads or writes the mirror; per-page savepoints).
    if let Some(retired) = super::memory_vector_mirror_retire::retire_memory_vector_mirror(main)? {
        tracing::info!(
            target: "quilltap::boot",
            migrationId = super::memory_vector_mirror_retire::MIGRATION_ID,
            mirrorRows = retired.mirror_rows,
            copied = retired.copied,
            alreadyOnRow = retired.already_on_row,
            orphaned = retired.orphaned,
            unusable = retired.unusable,
            tablesDropped = retired.tables_dropped,
            "Retired the memory vector mirror"
        );
    }
    // Slot 9 — `add-memory-extraction-cursors-v1` (R-C text, R-D order, L-8; STAMPED).
    if let Some(cursors) =
        super::memory_extraction_cursors_repair::ensure_memory_extraction_cursors(main)?
    {
        tracing::info!(
            target: "quilltap::boot",
            migrationId = super::memory_extraction_cursors_repair::MIGRATION_ID,
            tableCreated = cursors.table_created,
            backfilled = cursors.backfilled,
            droppedColumns = cursors.dropped_columns.len(),
            "Moved the fold-pass watermarks into extraction cursors"
        );
    }
    // v4's repository first touch: the cursor table's `createdAt` index.
    super::memory_extraction_cursors_repair::ensure_cursor_created_at_index(main)?;
    // Slot 10 — `drop-inter-character-memory-jobs-v1`.
    let inter = super::retired_job_rows_purge::purge_inter_character_memory_jobs(main)?;
    if inter > 0 {
        tracing::info!(
            target: "quilltap::boot",
            migrationId =
                super::retired_job_rows_purge::INTER_CHARACTER_MIGRATION_ID,
            deleted = inter,
            "Deleted retired INTER_CHARACTER_MEMORY jobs"
        );
    }
    // Slot 11 — `add-tfidf-text-shape-version-v1`.
    if super::tfidf_text_shape_repair::ensure_tfidf_text_shape_version(main)? {
        tracing::info!(
            target: "quilltap::boot",
            migrationId = super::tfidf_text_shape_repair::MIGRATION_ID,
            "Added tfidf_vocabularies.textShapeVersion"
        );
    }
    // Slot 12 — `drop-housekeeping-consolidation-jobs-v1`.
    let hk = super::retired_job_rows_purge::purge_housekeeping_consolidation_jobs(main)?;
    if hk > 0 {
        tracing::info!(
            target: "quilltap::boot",
            migrationId = super::retired_job_rows_purge::
                HOUSEKEEPING_CONSOLIDATION_MIGRATION_ID,
            deleted = hk,
            "Deleted retired housekeeping and consolidation jobs"
        );
    }
    Ok(())
}
