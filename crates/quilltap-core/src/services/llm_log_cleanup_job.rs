//! The `LLM_LOG_CLEANUP` job handler — a port of v4's
//! `lib/background-jobs/handlers/llm-log-cleanup.ts` (P4.24, dogfood finding
//! #40). Deletes a user's LLM debug logs older than their configured retention
//! window.
//!
//! ## Why this exists at all
//!
//! `LLM_LOG_CLEANUP` was the LAST type in `KNOWN_JOB_TYPES` with no registered
//! handler. The enqueue path has been live the whole time — on the daily cadence
//! *and* immediately at boot — so every start-up minted a job that burned three
//! attempts against the "recognized but not yet available" arm and died. On the
//! real instance the retention window was being maintained by v4 instead (a
//! 416 MB llm-logs partition holding a 7-day window at ~1,080 rows/day); the day
//! v5 is the only app, that number stops being bounded.
//!
//! ## The five arms, in v4's order — two of them look like bugs and are not
//!
//!   1. Payload is `{ userId, retentionDays? }`.
//!   2. `retentionDays` absent → read `chatSettings.findByUserId`. **A missing
//!      settings row bails here** (warn + return; the job COMPLETES). Otherwise
//!      `llmLoggingSettings?.retentionDays ?? 30`.
//!   3. `retentionDays <= 0` → return. This is where "0 means keep forever"
//!      lives — deliberately NOT in the repository, whose own guard is `< 0`.
//!   4. **A SECOND settings fetch runs unconditionally** — even when the payload
//!      carried `retentionDays` and step 2 never ran. `chatSettings && !enabled`
//!      → return. ⚠️ Note the asymmetry against step 2: here a MISSING settings
//!      row does **not** bail, because the `chatSettings &&` short-circuits and
//!      cleanup proceeds. Carried faithfully; the differential pins both halves.
//!   5. `cleanupOldLogs(userId, retentionDays)`; on error v4 logs and
//!      **rethrows**, so the job runner records the failure and retries.
//!
//! v4's `logger.warn`/`info`/`error` lines are `tracing` events here: log output
//! is operator output, outside the differential contract (the P4.18 ruling).
//!
//! ## Two clock inputs, one inherited modeling boundary
//!
//! `now_ms` + an IANA `tz` are parameters rather than ambient reads (the
//! [`crate::day_references`] seam) because the retention cutoff is LOCAL
//! calendar-day arithmetic — see
//! [`crate::db::llm_logs::llm_log_retention_cutoff_iso`].
//!
//! The boundary: v4's `findByUserId` Zod-parses the whole row and its
//! `findOneByFilter` uses the `null`-fallback `safeQuery` overload, so a row
//! whose `llmLoggingSettings` cell is *malformed* (a wrong-typed field, not just
//! absent) reads back as `null` and takes step 2's missing-settings arm. v5's
//! `find_by_user_id` is not a Zod validator — it marshals what is stored — so a
//! malformed cell here degrades to the schema defaults instead. That is the
//! repo-wide modeling choice v5 made long before this handler, not something
//! this port introduced; every shape a v4 `create` can write agrees on both
//! sides, which is what the fixture exercises. Recorded rather than widened.

use serde_json::Value;

use crate::db::runtime::Db;

/// v4 `LLMLogCleanupPayload` — `{ userId, retentionDays? }`. The `userId` field
/// is ignored by v4's handler, which reads `job.userId` (the row's own value)
/// throughout; only the override is decoded.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LlmLogCleanupPayload {
    /// `payload.retentionDays`. `None` is JS `undefined` — the ONLY value that
    /// sends step 2 to the settings row. Any present value is coerced with JS
    /// `Number()` semantics, which keeps the downstream comparisons faithful:
    /// a `null` becomes `0` and returns at step 3 exactly as `null <= 0` does,
    /// and a non-numeric string becomes `NaN`, fails `<= 0`, and reaches the
    /// repository's unrepresentable-cutoff error exactly as v4's `Invalid Date`
    /// does.
    pub retention_days: Option<f64>,
}

impl LlmLogCleanupPayload {
    pub fn from_json(payload: &Value) -> Self {
        Self {
            retention_days: payload
                .get("retentionDays")
                .map(crate::pascal::js_value::to_number),
        }
    }
}

/// v4 `LLMLoggingSettingsSchema` materialized over a stored `llmLoggingSettings`
/// cell — `(enabled, retentionDays)`, defaulting `true` / `30`.
///
/// A NULL cell (or an absent key) is v4's `undefined`, which the outer
/// `.default({ enabled: true, verboseMode: false, retentionDays: 30 })` fills in.
/// That is why the handler's `?? 30` is unreachable in v4 and is ported as the
/// equivalent default rather than as a second fallback — and it is not a
/// harmless detail: the **enqueuer** consults the same bag, so reading a NULL
/// cell as "not configured" silently drops that user from the daily sweep
/// forever. Both callers go through this one function for exactly that reason.
///
/// One boundary this does not reproduce (recorded, not widened): a *malformed*
/// bag — a wrong-typed field, not an absent one — makes v4's Zod parse throw,
/// which its `safeQuery` turns into `null` for `findByUserId` and into an empty
/// list for `findAll`. Here it degrades to the defaults instead. No v4 write
/// produces such a cell.
pub fn llm_logging_settings_defaults(bag: Option<&Value>) -> (bool, f64) {
    let enabled = bag
        .and_then(|b| b.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let retention_days = bag
        .and_then(|b| b.get("retentionDays"))
        .and_then(Value::as_f64)
        .unwrap_or(30.0);
    (enabled, retention_days)
}

/// [`llm_logging_settings_defaults`] over a whole `chat_settings` object.
fn llm_logging_bag(settings: &Value) -> (bool, f64) {
    llm_logging_settings_defaults(settings.get("llmLoggingSettings"))
}

/// v4's `findByUserId` under its `null`-fallback `safeQuery`: a read failure is
/// indistinguishable from "no settings row".
fn read_settings(db: &Db, user_id: &str) -> Option<Value> {
    let uid = user_id.to_string();
    db.read_main(move |conn| crate::db::chat_settings::find_by_user_id(conn, &uid))
        .ok()
        .flatten()
}

/// v4 `handleLLMLogCleanup(job)`. `Ok(())` completes the job; `Err(message)` is
/// v4's rethrow, which the job runner records as a failure.
pub async fn handle_llm_log_cleanup(
    db: &Db,
    user_id: &str,
    payload: &LlmLogCleanupPayload,
    now_ms: i64,
    tz: &str,
) -> Result<(), String> {
    // 1–2. The override, or the user's configured window.
    let retention_days = match payload.retention_days {
        Some(days) => days,
        None => {
            let Some(settings) = read_settings(db, user_id) else {
                tracing::warn!(user_id, "[LLMLogCleanup] Chat settings not found, skipping");
                return Ok(());
            };
            llm_logging_bag(&settings).1
        }
    };

    // 3. "0 (or less) means keep forever."
    if retention_days <= 0.0 {
        return Ok(());
    }

    // 3½. P4.D331 (v4 `e4a60ad9e`, `:53-58`): the Log's entries are pruned on
    //     the same window BEFORE the logging-enabled return below, so they go
    //     even when LLM logging itself is switched off. A failed DELETE is the
    //     repository's fallback (0); a `NaN` window throws `Invalid time value`
    //     out of the prune — v4's sequence: the job fails here, before the
    //     second settings read. v4's DEBUG carries `jobId`; v5's handler has no
    //     job id (the existing `Cleanup completed` omits it too — recorded).
    let tz_owned = tz.to_string();
    let deleted_entry_count = db
        .write(move |writers| {
            let guard = crate::db::log_entries::llm_logs_guard_sentence(writers.llm_logs_state());
            let conn = match (writers.llm_logs(), guard) {
                (Some(w), None) => Ok(w.connection()),
                (_, sentence) => Err(sentence.unwrap_or("LLM logs database is in degraded mode")),
            };
            crate::db::log_entries::cleanup_old_entries_on(conn, retention_days, now_ms, &tz_owned)
        })
        .await
        .map_err(|e| crate::db::fallback::error_text(&e))?;
    tracing::debug!(
        deleted_entry_count,
        retention_days,
        "[LLMLogCleanup] Log entries pruned"
    );

    // 4. The unconditional second fetch. A MISSING row does not bail here —
    //    v4's `chatSettings && !enabled` short-circuits and cleanup proceeds.
    if let Some(settings) = read_settings(db, user_id) {
        if !llm_logging_bag(&settings).0 {
            return Ok(());
        }
    }

    // 5. The delete, on the llm-logs partition.
    let uid = user_id.to_string();
    let tz = tz.to_string();
    let deleted = db
        .write(move |writers| {
            let Some(writer) = writers.llm_logs() else {
                return Err(crate::db::DbError::PartitionUnavailable(
                    crate::write_partition::WriteDbTarget::LlmLogs,
                ));
            };
            writer
                .llm_logs()
                .cleanup_old_logs(&uid, retention_days, now_ms, &tz)
        })
        .await
        .map_err(|e| e.to_string())?;

    tracing::info!(
        user_id,
        retention_days,
        deleted_count = deleted,
        "[LLMLogCleanup] Cleanup completed"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn payload_absent_field_is_undefined_not_zero() {
        assert_eq!(
            LlmLogCleanupPayload::from_json(&json!({ "userId": "u" })),
            LlmLogCleanupPayload {
                retention_days: None
            }
        );
    }

    /// The three present-but-odd shapes, each landing where JS lands: a number
    /// passes through, a `null` coerces to 0 (and so returns at step 3, exactly
    /// as `null <= 0` does), and a non-numeric string is `NaN` (which fails
    /// `<= 0` and reaches the repository's Invalid-Date arm).
    #[test]
    fn payload_present_values_use_js_number_semantics() {
        assert_eq!(
            LlmLogCleanupPayload::from_json(&json!({ "retentionDays": 7 })).retention_days,
            Some(7.0)
        );
        assert_eq!(
            LlmLogCleanupPayload::from_json(&json!({ "retentionDays": null })).retention_days,
            Some(0.0)
        );
        assert_eq!(
            LlmLogCleanupPayload::from_json(&json!({ "retentionDays": "5" })).retention_days,
            Some(5.0)
        );
        assert!(
            LlmLogCleanupPayload::from_json(&json!({ "retentionDays": "x" }))
                .retention_days
                .is_some_and(f64::is_nan)
        );
    }

    /// The Zod defaults, including the NULL cell that makes the handler's
    /// `?? 30` unreachable in v4.
    #[test]
    fn llm_logging_bag_materializes_zod_defaults() {
        assert_eq!(
            llm_logging_bag(&json!({ "llmLoggingSettings": Value::Null })),
            (true, 30.0)
        );
        assert_eq!(llm_logging_bag(&json!({})), (true, 30.0));
        assert_eq!(
            llm_logging_bag(&json!({ "llmLoggingSettings": { "enabled": false } })),
            (false, 30.0)
        );
        assert_eq!(
            llm_logging_bag(
                &json!({ "llmLoggingSettings": { "enabled": true, "retentionDays": 7 } })
            ),
            (true, 7.0)
        );
    }

    /// P4.D331: the Log's prune runs on the job's window — the payload
    /// override here — before step 4, and logs v4's DEBUG `[LLMLogCleanup] Log
    /// entries pruned {deletedEntryCount, retentionDays}` on the CALLER thread
    /// (after the write), so the thread-scoped rig pins it. A `<= 0` window
    /// returns before it: no prune, no line.
    #[test]
    fn the_log_prune_runs_on_the_window_and_logs_its_count() {
        use crate::db::runtime::{Db, DbPaths};
        use crate::db::Writer;
        const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main.db");
        let logs = dir.path().join("logs.db");
        drop(Writer::open_writable(&main, PEPPER).unwrap());
        {
            let w =
                Writer::open_writable_in(crate::db::table_shape::Partition::LlmLogs, &logs, PEPPER)
                    .unwrap();
            let mut failures = crate::db::table_shape::EnsureFailures::default();
            crate::db::table_shape::create_missing_structural_tables(
                None,
                Some(w.connection()),
                &mut failures,
            );
            assert!(failures.is_empty(), "{failures:?}");
            let repo = crate::db::log_entries::LogEntriesRepository::new(w.connection());
            for (id, at) in [
                (
                    "11111111-1111-4111-8111-000000000001",
                    "2026-03-07T00:00:00.000Z",
                ),
                (
                    "11111111-1111-4111-8111-000000000002",
                    "2026-03-14T00:00:00.000Z",
                ),
            ] {
                repo.create(
                    "a line",
                    "22222222-2222-4222-8222-222222222222",
                    None,
                    Some(&crate::db::log_entries::LogEntryCreateOptions {
                        id: Some(id.into()),
                        created_at: Some(at.into()),
                        updated_at: Some(at.into()),
                    }),
                )
                .unwrap();
            }
        }
        let db = Db::open(
            DbPaths {
                main,
                mount_index: None,
                llm_logs: Some(logs),
            },
            PEPPER,
        )
        .unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let now_ms = crate::clock::iso_to_ms("2026-03-15T17:00:00.000Z").unwrap();
        let run = |days: Option<f64>| {
            crate::test_support::captured_with(|| {
                rt.block_on(handle_llm_log_cleanup(
                    &db,
                    "33333333-3333-4333-8333-333333333333",
                    &LlmLogCleanupPayload {
                        retention_days: days,
                    },
                    now_ms,
                    "UTC",
                ))
            })
        };
        let (_, zero) = run(Some(0.0));
        assert!(
            !zero.iter().any(|l| l.contains("Log entries pruned")),
            "{zero:#?}"
        );
        let (_, lines) = run(Some(7.0));
        let pruned: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("Log entries pruned"))
            .collect();
        assert_eq!(
            pruned,
            ["DEBUG quilltap_core::services::llm_log_cleanup_job [LLMLogCleanup] Log entries pruned deleted_entry_count=1 retention_days=7"],
            "{lines:#?}"
        );
        let left: i64 = db
            .read_llm_logs(|c| {
                Ok(c.query_row("SELECT count(*) FROM log_entries", [], |r| r.get(0))?)
            })
            .unwrap();
        assert_eq!(left, 1);
    }
}
