//! When a `MEMORY_MAINTENANCE` job is enqueued — the watermark half: v4
//! `lib/memory/maintenance/triggers.ts` (`104e986c7`, at the `83334b9a5` pin).
//!
//! v4's why, carried: "Called after admission inserts a row (`admitPrepared`)
//! … It measures the character (two COUNTs, debounced per character) and asks
//! the ledger's `shouldEnqueue` which phases are due; the daily sweep lives in
//! `scheduler.ts`."
//!
//! **The ONE caller is admission's after-insert site** (v4 `admit.ts:403-412`
//! — after an admission that INSERTED a row, any source, backfill included):
//! `services/admission/admit.rs`'s site is the UNIFIER's to wire (§R.10(h) —
//! P4.D297's `HANDOFF:`), with v4's catch DEBUG `Maintenance watermark check
//! skipped after admission {characterId, error}`.
//!
//! **R-L — the commit-hook half is NOT ported.** v4's
//! `maybeEnqueueMaintenanceAfterCommit` / `characterIdsFromMemoryWrites` find
//! the characters a committed job batch created memories for; at the pin every
//! reachable job batch yields none (the census
//! `maintenance_commit_hook_unreachable_census` pins the four `memories.create(`
//! callers: admission's parent-only apply, consolidation's digest create inside
//! a `MEMORY_MAINTENANCE` batch — excluded by the hook's own type gate —, the
//! restore and the `.qtap` import, which are not jobs). P4.D271 R-B's mark
//! log and its `WATERMARK_ROWID_WINDOW` table retire with it; a future v4
//! buffered create reddens the census.

use serde_json::{json, Map, Value};

use super::json_text;
use super::ledger::{note_enqueued, should_enqueue, take_watermark_check, WatermarkReading};
use super::types::MaintenanceReason;
use crate::db::runtime::Db;
use crate::db::{characters_read, memories_read, DbError};
use crate::memory_settings::resolve_memory_settings;
use crate::services::queue_service::enqueue_memory_maintenance;

const MODULE: &str = "memory:maintenance:triggers";

/// v4 `maybeEnqueueMaintenance(characterIds, nowMs)` (`triggers.ts:45-95`):
/// "watermark check for a set of characters; enqueues one run per character
/// whose watermarks say something is due. Never throws. Returns the
/// characters a run was enqueued for." The debounce first ("most calls — one
/// per inserted row — stop here, before any read"); then the instance-wide
/// switches (both off → nothing); per due id `countCapBreakdown(id).demotable`
/// and, only when consolidation is on, `countUnconsideredHot(id)`. Any failure
/// is WARN `Maintenance watermark check failed (non-fatal) {characterIds,
/// error}`.
pub async fn maybe_enqueue_maintenance(
    db: &Db,
    character_ids: &[String],
    now_ms: f64,
) -> Vec<String> {
    let mut enqueued: Vec<String> = Vec::new();
    let due: Vec<String> = character_ids
        .iter()
        .filter(|id| take_watermark_check(id, now_ms))
        .cloned()
        .collect();
    if due.is_empty() {
        return enqueued;
    }
    if let Err(error) = check(db, &due, now_ms, &mut enqueued).await {
        tracing::warn!(
            module = MODULE,
            characterIdsJson = %json_text(&json!(character_ids)),
            error = %crate::db::fallback::error_text(&error),
            "Maintenance watermark check failed (non-fatal)"
        );
    }
    enqueued
}

/// The body of v4's `try`.
async fn check(
    db: &Db,
    due: &[String],
    now_ms: f64,
    enqueued: &mut Vec<String>,
) -> Result<(), DbError> {
    let settings = db.read_main(|c| Ok(resolve_memory_settings(c, None)))?;
    let maintenance_enabled = settings.maintenance.settings.enabled;
    let consolidation_enabled = settings.maintenance.consolidation.enabled;
    if !maintenance_enabled && !consolidation_enabled {
        return Ok(());
    }
    for character_id in due {
        let cid = character_id.clone();
        let demotable = db
            .read_main(|c| memories_read::count_cap_breakdown(c, &cid))?
            .demotable;
        let unconsidered = if consolidation_enabled {
            db.read_main(|c| memories_read::count_unconsidered_hot(c, &cid))?
        } else {
            0
        };
        let reading = WatermarkReading {
            demotable,
            cap: settings.maintenance.cap_for(character_id),
            unconsidered,
            consolidation_watermark: settings.maintenance.consolidation.watermark,
            maintenance_enabled,
            consolidation_enabled,
        };
        let Some(phases) = should_enqueue(
            db,
            character_id,
            MaintenanceReason::Watermark,
            Some(&reading),
            now_ms,
        ) else {
            tracing::debug!(
                module = MODULE,
                characterId = %character_id,
                demotable,
                unconsidered,
                "Maintenance watermarks not due"
            );
            continue;
        };
        let character = db.read_main(|c| Ok(characters_read::find_by_id_raw_or_none(c, &cid)))?;
        let Some(character) = character.filter(|c| !crate::api::characters::is_archived(c)) else {
            continue;
        };
        let user_id = character
            .get("userId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mut payload = Map::new();
        payload.insert("characterId".into(), json!(character_id));
        payload.insert("reason".into(), json!("watermark"));
        payload.insert(
            "phases".into(),
            json!(phases.iter().map(|p| p.as_str()).collect::<Vec<_>>()),
        );
        let job_id = enqueue_memory_maintenance(db, &user_id, payload, None).await?;
        // "The id is always a waiting job carrying these phases (a running
        // job never absorbs a request), so the throttles may start now."
        note_enqueued(character_id, &phases, now_ms);
        enqueued.push(character_id.clone());
        tracing::info!(
            module = MODULE,
            characterId = %character_id,
            phasesJson = %json_text(&json!(phases.iter().map(|p| p.as_str()).collect::<Vec<_>>())),
            demotable,
            unconsidered,
            jobId = %job_id,
            "Maintenance watermark reached; run enqueued"
        );
    }
    Ok(())
}
