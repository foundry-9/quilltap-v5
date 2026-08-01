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
/// cell: `enabled` defaults `true`, `retentionDays` defaults `30`. A NULL cell
/// (or an absent key) is v4's `undefined`, which the outer
/// `.default({ enabled: true, verboseMode: false, retentionDays: 30 })` fills in
/// — which is also why the handler's `?? 30` is unreachable in v4 and is ported
/// as the equivalent default rather than as a second fallback.
fn llm_logging_bag(settings: &Value) -> (bool, f64) {
    let bag = settings.get("llmLoggingSettings");
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
}
