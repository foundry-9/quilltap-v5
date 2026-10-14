//! The two DATA-rewrite boot ensures of the `83334b9a5` span — P4.D288 (C1-2
//! slots 4 and 5, R-E), porting v4
//! `migrations/scripts/recompute-reinforced-importance-v1.ts` (`e0ac7493a`,
//! bug 216) and `migrations/scripts/rezone-legacy-memory-occurred-at-v1.ts`
//! (`c2795733f`, bug 212 / PR 90) over P4.D289's ONE formula
//! ([`crate::memory_scoring::reinforced_importance`]) and ONE calendar
//! ([`crate::memory_time::calendar`]).
//!
//! **R-E — ledgered and STAMPED.** v4's `shouldRun` for both fires on ANY
//! instance carrying the columns, so neither can be structure-gated: each runs
//! only while `migrations_state` holds no row for its id (EITHER app's — v4 or
//! v5 — counts), and on success stamps v4's EXACT row
//! ([`stamp_completed`]: `id`, the run's `completedAt`, `itemsAffected`, v4's
//! `message` bytes) so v4's runner never re-runs it on the instance v5 leaves
//! behind. `shouldRun` false (no `memories` table, or a column absent) is
//! [`LedgeredHealOutcome::NotNeeded`] — nothing written, nothing stamped, as
//! v4's runner records nothing.
//!
//! Both read every candidate up front and rewrite in batches of 500, each its
//! own savepoint (v4's per-batch `db.transaction`); the stamp follows the last
//! batch. v4's `reportProgress` (a CLI progress bar) has no port, and — the
//! slot-3 precedent ([`super::memory_episode_watermark_repair`]) — neither have
//! the migrations' own lines (the INFO before and after, the per-batch DEBUG,
//! the failure ERROR): a failure is FATAL to boot through the caller's `?`, and
//! the span runner ([`super::p4d288_span`]) logs v5's ONE boot INFO when a heal
//! ran, the rezone's carrying the zone it used (O-8).
//!
//! **`NotNeeded` (a recorded departure from the order's two-arm spelling).**
//! v4's runner records NOTHING when `shouldRun` is false
//! (`migrations/index.ts:164-167`), so "no `memories` table / a column absent"
//! is a third state, distinct from both "already recorded" and "ran" —
//! unreachable on a provisioned instance, kept honest rather than folded into
//! either arm.

use rusqlite::Connection;

use super::migrations_ledger::{ensure_migrations_tables, ledger_has, stamp_completed};
use super::schema_probe::{column_exists, in_savepoint, table_exists};
use super::DbError;

/// v4 `recompute-reinforced-importance-v1`'s id.
pub const RECOMPUTE_MIGRATION_ID: &str = "recompute-reinforced-importance-v1";
/// v4 `rezone-legacy-memory-occurred-at-v1`'s id.
pub const REZONE_MIGRATION_ID: &str = "rezone-legacy-memory-occurred-at-v1";

/// v4's `BATCH_SIZE`.
const BATCH_SIZE: usize = 500;
/// v4's recompute `EPSILON` — a stored value within it is left alone.
const EPSILON: f64 = 1e-9;
/// v4's rezone `DAY_MS`.
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// One recompute candidate: `(id, importance, reinforcementCount,
/// reinforcedImportance)` — v4's `MemoryImportanceRow`.
type ImportanceRow = (String, Option<f64>, Option<f64>, Option<f64>);

/// What a ledgered heal did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgeredHealOutcome {
    /// `migrations_state` already records the id (v4 or v5 ran it).
    AlreadyCompleted,
    /// v4's `shouldRun` is false on this instance — nothing written or stamped.
    NotNeeded,
    /// The rewrite ran and was stamped: `rewritten` of `total` candidates.
    Ran { rewritten: usize, total: usize },
}

/// Run `work` in 500-row batches, each its own savepoint.
fn apply_batches(
    conn: &Connection,
    savepoint: &str,
    changes: &[(String, rusqlite::types::Value)],
    sql: &str,
) -> Result<(), DbError> {
    for batch in changes.chunks(BATCH_SIZE) {
        in_savepoint(conn, savepoint, |c| {
            let mut stmt = c.prepare(sql)?;
            for (id, value) in batch {
                stmt.execute(rusqlite::params![value, id])?;
            }
            Ok(())
        })?;
    }
    Ok(())
}

/// v4 `recomputeReinforcedImportanceMigration` (slot 4): rewrite
/// `memories.reinforcedImportance` to `reinforced_importance(importance ??
/// 0.5, reinforcementCount ?? 1)` wherever the stored value is NULL or off by
/// more than 1e-9. `now` is the stamp's `completedAt`.
pub fn recompute_reinforced_importance(
    conn: &Connection,
    now: &str,
) -> Result<LedgeredHealOutcome, DbError> {
    ensure_migrations_tables(conn)?;
    if ledger_has(conn, RECOMPUTE_MIGRATION_ID)? {
        return Ok(LedgeredHealOutcome::AlreadyCompleted);
    }
    if !table_exists(conn, "memories")? {
        return Ok(LedgeredHealOutcome::NotNeeded);
    }
    for column in ["reinforcedImportance", "reinforcementCount", "importance"] {
        if !column_exists(conn, "memories", column)? {
            return Ok(LedgeredHealOutcome::NotNeeded);
        }
    }
    let rows: Vec<ImportanceRow> = {
        let mut stmt = conn.prepare(
            "SELECT id, importance, reinforcementCount, reinforcedImportance FROM memories",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    let total = rows.len();
    let mut rewritten = 0usize;
    for slice in rows.chunks(BATCH_SIZE) {
        let changes: Vec<(String, rusqlite::types::Value)> = slice
            .iter()
            .filter_map(|(id, importance, count, stored)| {
                let value = crate::memory_scoring::reinforced_importance(
                    importance.unwrap_or(0.5),
                    count.unwrap_or(1.0),
                );
                match stored {
                    Some(s) if (s - value).abs() <= EPSILON => None,
                    _ => Some((id.clone(), rusqlite::types::Value::Real(value))),
                }
            })
            .collect();
        apply_batches(
            conn,
            "recompute_reinforced_importance",
            &changes,
            "UPDATE memories SET reinforcedImportance = ?1 WHERE id = ?2",
        )?;
        rewritten += changes.len();
    }
    stamp_completed(
        conn,
        RECOMPUTE_MIGRATION_ID,
        now,
        rewritten as i64,
        &format!("Recomputed reinforced importance on {rewritten} of {total} memories"),
    )?;
    Ok(LedgeredHealOutcome::Ran { rewritten, total })
}

/// v4 `resolveInstanceTimeZone` over the heal's own connection (O-8): the
/// FIRST `chat_settings.timezone` (rowid order) that `is_valid_time_zone`
/// accepts — only when the table and column exist — then `host_fallback` (a
/// valid `QUILLTAP_TIMEZONE`, else the host's system zone, as the caller
/// resolves it), then `UTC`.
pub fn instance_memory_time_zone(
    conn: &Connection,
    host_fallback: &str,
) -> Result<String, DbError> {
    let mut configured: Option<String> = None;
    if table_exists(conn, "chat_settings")? && column_exists(conn, "chat_settings", "timezone")? {
        let mut stmt =
            conn.prepare("SELECT timezone FROM chat_settings WHERE timezone IS NOT NULL")?;
        let zones = stmt.query_map([], |r| r.get::<_, Option<String>>(0))?;
        for zone in zones {
            if let Some(z) = zone? {
                if crate::memory_time::calendar::is_valid_time_zone(Some(&z)) {
                    configured = Some(z);
                    break;
                }
            }
        }
    }
    Ok(crate::memory_time::calendar::memory_time_zone(
        configured.as_deref(),
        host_fallback,
    ))
}

/// v4 `rezonedValue(occurredAt, timeZone)`: local midnight of `occurred_at`'s
/// UTC date in `tz`, or `None` when it does not parse (V8's whole
/// `Date.parse`), is not exactly a UTC midnight (`ms % DAY_MS` — JS `%` keeps
/// the dividend's sign, so a pre-1970 midnight reads zero too) or the
/// re-stamped value equals the stored one.
fn rezoned_value(occurred_at: &str, tz: &str) -> Option<String> {
    let ms = crate::episodic::js_date_parse_ms(occurred_at)?;
    if ms % DAY_MS != 0 {
        return None;
    }
    let date = crate::memory_time::calendar::js_iso_string(ms as f64)?[..10].to_string();
    let local = crate::memory_time::calendar::zoned_day_bounds_iso(&date, tz)?.from;
    (local != occurred_at).then_some(local)
}

/// v4 `rezoneLegacyMemoryOccurredAtMigration` (slot 5): re-stamp every
/// `memories.occurredAt` that is EXACTLY a UTC midnight at local midnight of
/// the same calendar date in the instance's memory zone
/// ([`instance_memory_time_zone`]). Candidates are narrowed in SQL (`LIKE
/// '%T00:00:00%'`), the exact test is on the parsed instant. `now` is the
/// stamp's `completedAt`.
pub fn rezone_legacy_memory_occurred_at(
    conn: &Connection,
    host_fallback: &str,
    now: &str,
) -> Result<LedgeredHealOutcome, DbError> {
    ensure_migrations_tables(conn)?;
    if ledger_has(conn, REZONE_MIGRATION_ID)? {
        return Ok(LedgeredHealOutcome::AlreadyCompleted);
    }
    if !table_exists(conn, "memories")? || !column_exists(conn, "memories", "occurredAt")? {
        return Ok(LedgeredHealOutcome::NotNeeded);
    }
    let time_zone = instance_memory_time_zone(conn, host_fallback)?;
    let rows: Vec<(String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id, occurredAt FROM memories WHERE occurredAt IS NOT NULL AND occurredAt LIKE '%T00:00:00%'",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    let total = rows.len();
    let mut rewritten = 0usize;
    for slice in rows.chunks(BATCH_SIZE) {
        let changes: Vec<(String, rusqlite::types::Value)> = slice
            .iter()
            .filter_map(|(id, occurred_at)| {
                rezoned_value(occurred_at, &time_zone)
                    .map(|v| (id.clone(), rusqlite::types::Value::Text(v)))
            })
            .collect();
        apply_batches(
            conn,
            "rezone_legacy_memory_occurred_at",
            &changes,
            "UPDATE memories SET occurredAt = ?1 WHERE id = ?2",
        )?;
        rewritten += changes.len();
    }
    stamp_completed(
        conn,
        REZONE_MIGRATION_ID,
        now,
        rewritten as i64,
        &format!("Re-stamped {rewritten} legacy memory date(s) at local midnight in {time_zone}"),
    )?;
    Ok(LedgeredHealOutcome::Ran { rewritten, total })
}
