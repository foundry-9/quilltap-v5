//! The scheduled danger-classification scan (P4.1d, v4
//! `lib/background-jobs/scheduled-danger-scan.ts`) — the enqueuer sweep that
//! finds unclassified chats and enqueues classification (or summary-first) jobs
//! so every chat eventually gets classified, including legacy chats created
//! before the feature existed.
//!
//! Decision tree per unclassified chat (v4's header, verbatim):
//! - Has `contextSummary` → enqueue `CHAT_DANGER_CLASSIFICATION` directly
//! - No summary, `messageCount > 50` → enqueue `CONTEXT_SUMMARY` (chaining
//!   handles classification)
//! - No summary, `messageCount <= 50` → enqueue `CHAT_DANGER_CLASSIFICATION`
//!   (the handler falls back to raw messages)
//!
//! The 10-minute cadence and the run-immediately-at-startup behavior are the
//! host driver's (`quilltap-host`); this module holds the two portable
//! decisions v4's scheduler makes:
//!
//! - [`any_user_danger_enabled`] — the "skip starting the scheduler entirely"
//!   pre-check (v4 `scheduleDangerScan`'s all-users-OFF gate; a CHECK FAILURE
//!   also skips — v4 warns and returns without starting, so the host treats
//!   `Err` as "don't start").
//! - [`run_scheduled_danger_scan`] — one sweep pass (v4
//!   `runScheduledDangerScan`).
//!
//! ## The settings read
//!
//! v4 iterates `repos.chatSettings.findAll()` and resolves each row through
//! `resolveDangerousContentSettings(settings)` (no chat — the global half
//! only). The port reads a scoped two-column SELECT (`userId`,
//! `dangerousContentSettings`) in the backend's default rowid order (matching
//! v4 `findAll`'s insertion order — the order determines the enqueue order and
//! so the minted job rows' natural order). A NULL cell parses to `None`
//! (v4's Zod default sub-object and the resolver default are the same
//! constants, so both sides resolve identically). Documented seam: a PARTIAL
//! stored sub-object (never produced by a v4 write, which materializes every
//! Zod default) would Zod-fill nested defaults in v4 but fall to the whole
//! resolver default here; the corpus keeps rows v4-written.

use serde_json::Value;

use crate::chat_predicates::is_moderation_exempt_chat_type;
use crate::db::chat_settings::DangerousContentSettings;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::services::dangerous_content::chat_override::is_classifier_on_duty;
use crate::services::dangerous_content::resolver::resolve_dangerous_content_settings;
use crate::services::queue_service::{
    enqueue_chat_danger_classification_with_priority, enqueue_context_summary,
};

/// One scoped `chat_settings` row: the user id + the parsed
/// `dangerousContentSettings` sub-object (`None` when NULL/unparseable).
pub struct DangerScanUserSettings {
    pub user_id: String,
    pub dangerous_content_settings: Option<DangerousContentSettings>,
}

/// The scoped read behind both the pre-check and the sweep: `userId` +
/// `dangerousContentSettings` for every `chat_settings` row, in rowid
/// (insertion) order — v4 `findAll`'s order.
pub fn find_all_danger_scan_settings(
    conn: &rusqlite::Connection,
) -> Result<Vec<DangerScanUserSettings>, DbError> {
    let mut stmt = conn.prepare("SELECT userId, dangerousContentSettings FROM chat_settings")?;
    let rows = stmt
        .query_map([], |row| {
            let user_id: String = row.get(0)?;
            let dcs: Option<String> = row.get(1)?;
            Ok((user_id, dcs))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .map(|(user_id, dcs)| DangerScanUserSettings {
            user_id,
            dangerous_content_settings: dcs.and_then(|t| serde_json::from_str(&t).ok()),
        })
        .collect())
}

/// Resolve one settings row's effective danger mode (the global half only — no
/// chat), v4's `resolveDangerousContentSettings(settings).settings.mode`.
fn resolved_mode(settings: &DangerScanUserSettings) -> String {
    resolve_dangerous_content_settings(settings.dangerous_content_settings.clone(), None)
        .settings
        .mode
}

/// v4 `scheduleDangerScan`'s pre-check: is danger mode non-OFF for ANY user?
/// When `false`, the host does not start the scan loop at all. A read failure
/// propagates as `Err` — v4 warns and skips starting on a check failure, so the
/// host maps `Err` to "don't start" too.
pub async fn any_user_danger_enabled(db: &Db) -> Result<bool, DbError> {
    let settings = db.read_main(find_all_danger_scan_settings)?;
    Ok(settings.iter().any(|s| resolved_mode(s) != "OFF"))
}

/// The per-chat classification gate (v4's `unclassified` filter, lifted out as
/// a pure function over the hydrated chat `Value`):
///
/// 1. Moderation-exempt chat types (Help Chat, Brahma Console) → never.
/// 2. Operator-decided chats (Vouched Safe or Uncensored) → never; nothing may
///    reclassify a chat out from under the operator.
/// 3. Never classified (`isDangerousChat == null` — absent OR JSON null) → yes.
/// 4. Classified SAFE but grown (`isDangerousChat === false` AND
///    `dangerClassifiedAtMessageCount != null` AND `(messageCount ?? 0) >
///    dangerClassifiedAtMessageCount`) → yes. Dangerous chats are sticky and
///    never re-checked.
pub fn chat_needs_classification(chat: &Value) -> bool {
    let chat_type = chat.get("chatType").and_then(Value::as_str);
    if is_moderation_exempt_chat_type(chat_type) {
        return false;
    }
    // Operator-decided chats (Vouched Safe or Uncensored) are always skipped —
    // nothing may reclassify a chat out from under the operator.
    if !is_classifier_on_duty(Some(chat)) {
        return false;
    }
    // `isDangerousChat == null` covers JS undefined (absent key — the hydrated
    // read OMITS a NULL nullable-optional) and an explicit JSON null.
    let is_dangerous = chat.get("isDangerousChat").and_then(Value::as_bool);
    let is_dangerous_present = chat
        .get("isDangerousChat")
        .map(|v| !v.is_null())
        .unwrap_or(false);
    if !is_dangerous_present {
        return true;
    }
    if is_dangerous == Some(false) {
        let classified_at = chat
            .get("dangerClassifiedAtMessageCount")
            .filter(|v| !v.is_null())
            .and_then(Value::as_f64);
        if let Some(classified_at) = classified_at {
            let message_count = chat
                .get("messageCount")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            if message_count > classified_at {
                return true;
            }
        }
    }
    false
}

/// v4's per-chat connection-profile resolution: the first participant with
/// `controlledBy !== 'user'` AND a `connectionProfileId` that still exists in
/// the user's profile set; else the first available profile; else `None`
/// (skip the chat).
pub fn resolve_scan_connection_profile_id(
    chat: &Value,
    valid_profile_ids: &std::collections::HashSet<String>,
    available_profile_ids: &[String],
) -> Option<String> {
    if let Some(participants) = chat.get("participants").and_then(Value::as_array) {
        for p in participants {
            let controlled_by = p.get("controlledBy").and_then(Value::as_str);
            if controlled_by == Some("user") {
                continue;
            }
            // JS truthiness: a present, non-empty connectionProfileId.
            let pid = p
                .get("connectionProfileId")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty());
            if let Some(pid) = pid {
                if valid_profile_ids.contains(pid) {
                    return Some(pid.to_string());
                }
            }
        }
    }
    available_profile_ids.first().cloned()
}

/// One danger-scan pass (v4 `runScheduledDangerScan`). Returns
/// `(users_processed, chats_enqueued)`.
///
/// Per user with a non-OFF resolved mode: read the user's chats, filter through
/// [`chat_needs_classification`], resolve each chat's profile, and enqueue per
/// the decision tree at priority `-2`. A per-chat enqueue failure is swallowed
/// (v4 warns and continues); a user with zero unclassified chats still counts
/// as processed. Top-level read failures propagate (v4 rethrows).
pub async fn run_scheduled_danger_scan(db: &Db) -> Result<(usize, usize), DbError> {
    let all_settings = db.read_main(find_all_danger_scan_settings)?;

    let mut users_processed = 0usize;
    let mut chats_enqueued = 0usize;

    for settings in &all_settings {
        if resolved_mode(settings) == "OFF" {
            continue;
        }

        let uid = settings.user_id.clone();
        let chats = db.read_main(move |conn| crate::db::chats_read::find_by_user_id(conn, &uid))?;

        let unclassified: Vec<&Value> = chats
            .iter()
            .filter(|c| chat_needs_classification(c))
            .collect();
        if unclassified.is_empty() {
            users_processed += 1;
            continue;
        }

        let uid = settings.user_id.clone();
        let available_profiles =
            db.read_main(move |conn| crate::db::connection_profiles::find_by_user_id(conn, &uid))?;
        let available_profile_ids: Vec<String> = available_profiles
            .iter()
            .filter_map(|p| p.get("id").and_then(Value::as_str).map(str::to_string))
            .collect();
        let valid_profile_ids: std::collections::HashSet<String> =
            available_profile_ids.iter().cloned().collect();

        for chat in &unclassified {
            let Some(connection_profile_id) = resolve_scan_connection_profile_id(
                chat,
                &valid_profile_ids,
                &available_profile_ids,
            ) else {
                continue;
            };
            let Some(chat_id) = chat.get("id").and_then(Value::as_str) else {
                continue;
            };

            // JS truthiness on `chat.contextSummary`: absent / null / empty
            // string are all falsy.
            let has_summary = chat
                .get("contextSummary")
                .and_then(Value::as_str)
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            let message_count = chat
                .get("messageCount")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);

            let enqueued = if has_summary {
                enqueue_chat_danger_classification_with_priority(
                    db,
                    &settings.user_id,
                    chat_id,
                    &connection_profile_id,
                    -2.0,
                )
                .await
            } else if message_count > 50.0 {
                enqueue_context_summary(
                    db,
                    &settings.user_id,
                    chat_id,
                    &connection_profile_id,
                    false,
                    -2.0,
                )
                .await
            } else {
                enqueue_chat_danger_classification_with_priority(
                    db,
                    &settings.user_id,
                    chat_id,
                    &connection_profile_id,
                    -2.0,
                )
                .await
            };
            match enqueued {
                Ok(_) => chats_enqueued += 1,
                Err(_) => { /* v4 warns per chat and continues */ }
            }
        }

        users_processed += 1;
    }

    Ok((users_processed, chats_enqueued))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashSet;

    #[test]
    fn gate_exempt_and_off_duty_and_sticky() {
        // Help / Brahma chats are never enqueued.
        assert!(!chat_needs_classification(&json!({ "chatType": "help" })));
        assert!(!chat_needs_classification(&json!({ "chatType": "brahma" })));
        // Operator waved the Concierge off.
        assert!(!chat_needs_classification(
            &json!({ "chatType": "salon", "conciergeOverride": "OFF" })
        ));
        // Sticky dangerous — never re-checked, even grown.
        assert!(!chat_needs_classification(&json!({
            "chatType": "salon", "isDangerousChat": true,
            "dangerClassifiedAtMessageCount": 1, "messageCount": 100
        })));
    }

    #[test]
    fn gate_never_classified_and_grown_safe() {
        // Absent AND explicit-null both read as never-classified.
        assert!(chat_needs_classification(&json!({ "chatType": "salon" })));
        assert!(chat_needs_classification(
            &json!({ "chatType": "salon", "isDangerousChat": null })
        ));
        // Safe + grown → re-check.
        assert!(chat_needs_classification(&json!({
            "chatType": "salon", "isDangerousChat": false,
            "dangerClassifiedAtMessageCount": 5, "messageCount": 10
        })));
        // Safe, not grown (equal count) → skip.
        assert!(!chat_needs_classification(&json!({
            "chatType": "salon", "isDangerousChat": false,
            "dangerClassifiedAtMessageCount": 10, "messageCount": 10
        })));
        // Safe but no recorded classification count → skip (v4 requires it).
        assert!(!chat_needs_classification(&json!({
            "chatType": "salon", "isDangerousChat": false, "messageCount": 10
        })));
    }

    #[test]
    fn profile_resolution_participant_first_then_fallback() {
        let valid: HashSet<String> = ["p1".to_string(), "p2".to_string()].into_iter().collect();
        let available = vec!["p1".to_string(), "p2".to_string()];

        // A user-controlled participant's profile is skipped; the LLM one wins.
        let chat = json!({ "participants": [
            { "controlledBy": "user", "connectionProfileId": "p1" },
            { "controlledBy": "llm", "connectionProfileId": "p2" },
        ]});
        assert_eq!(
            resolve_scan_connection_profile_id(&chat, &valid, &available),
            Some("p2".to_string())
        );

        // A stale participant profile falls back to the first available.
        let chat = json!({ "participants": [
            { "controlledBy": "llm", "connectionProfileId": "gone" },
        ]});
        assert_eq!(
            resolve_scan_connection_profile_id(&chat, &valid, &available),
            Some("p1".to_string())
        );

        // No participants + no profiles → skip.
        let chat = json!({ "participants": [] });
        assert_eq!(resolve_scan_connection_profile_id(&chat, &valid, &[]), None);
    }
}
