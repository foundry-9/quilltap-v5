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
//! - [`any_user_wants_summary_classification`] — the "skip starting the
//!   scheduler entirely" pre-check (v4 `scheduleDangerScan`'s gate — since
//!   `3b463d6b1` (#76): does ANY user have the Concierge on duty with the
//!   summary classifier opted in? A CHECK FAILURE also skips — v4 warns and
//!   returns without starting).
//! - [`run_scheduled_danger_scan`] — one sweep pass (v4
//!   `runScheduledDangerScan`).
//!
//! ## The settings read
//!
//! v4 iterates `repos.chatSettings.findAll()` and asks each row
//! `wantsSummaryClassification` — `readConciergeSettings(settings).enabled &&
//! …preScreen.summaryClassification` (v4 `3b463d6b1`, #76; it used to ask the
//! retired mode). The port reads a scoped two-column SELECT (`userId`,
//! `conciergeSettings`) in the backend's default rowid order (matching v4
//! `findAll`'s insertion order — the order determines the enqueue order and so
//! the minted job rows' natural order), each cell read through the schema twin
//! exactly as the hydrated read does (a NULL / absent cell is the `.default()`
//! literal — summary classification off). A table without the column (an
//! instance from before `add-concierge-settings-v1`) reads every cell as NULL.

use serde_json::Value;

use crate::chat_predicates::is_moderation_exempt_chat_type;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::services::dangerous_content::chat_override::is_classifier_on_duty;
use crate::services::dangerous_content::resolver::read_stored_concierge_settings;
use crate::services::queue_service::{
    enqueue_chat_danger_classification_with_priority, enqueue_context_summary,
};

/// One scoped `chat_settings` row: the user id + its `conciergeSettings`, read
/// through the schema twin.
pub struct DangerScanUserSettings {
    pub user_id: String,
    pub concierge_settings: Value,
}

/// The scoped read behind both the pre-check and the sweep: `userId` +
/// `conciergeSettings` for every `chat_settings` row, in rowid (insertion)
/// order — v4 `findAll`'s order.
pub fn find_all_danger_scan_settings(
    conn: &rusqlite::Connection,
) -> Result<Vec<DangerScanUserSettings>, DbError> {
    let cols =
        crate::db::tolerant_select_list(conn, "chat_settings", &["userId", "conciergeSettings"])?;
    let mut stmt = conn.prepare(&format!("SELECT {cols} FROM chat_settings"))?;
    let rows = stmt
        .query_map([], |row| {
            let user_id: String = row.get(0)?;
            let cs: Option<String> = row.get(1)?;
            Ok((user_id, cs))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .map(|(user_id, cs)| DangerScanUserSettings {
            user_id,
            concierge_settings: crate::db::chat_settings::read_concierge_settings_cell(cs),
        })
        .collect())
}

/// v4 `wantsSummaryClassification(settings)` — whether a user has asked for
/// the summary classifier and its sweep: the Concierge on duty, and the
/// summary classifier opted in. Per-chat state (Moderated only) is checked
/// chat by chat.
fn wants_summary_classification(settings: &DangerScanUserSettings) -> bool {
    let concierge = read_stored_concierge_settings(Some(&settings.concierge_settings));
    concierge.enabled && concierge.pre_screen.summary_classification
}

/// v4 `scheduleDangerScan`'s pre-check: does ANY user have the summary
/// classifier on? When `false`, the host does not start the scan loop at all.
/// A read failure is v4's catch: warn and do not start.
pub async fn any_user_wants_summary_classification(db: &Db) -> bool {
    let settings = match db.read_main(find_all_danger_scan_settings) {
        Ok(settings) => settings,
        Err(error) => {
            tracing::warn!(
                target: "quilltap::scheduled_danger_scan",
                error = %error,
                "Could not check Concierge settings, skipping danger scan scheduler"
            );
            return false;
        }
    };
    let opted_in = settings
        .iter()
        .filter(|s| wants_summary_classification(s))
        .count();
    tracing::debug!(
        target: "quilltap::scheduled_danger_scan",
        users = settings.len(),
        summary_classification_users = opted_in,
        "Danger scan scheduler pre-check"
    );
    if opted_in == 0 {
        tracing::info!(
            target: "quilltap::scheduled_danger_scan",
            "Danger scan scheduler not started — no user has the Concierge's summary classification on"
        );
        return false;
    }
    true
}

/// The per-chat classification gate (v4's `unclassified` filter, lifted out as
/// a pure function over the hydrated chat `Value`):
///
/// 1. Moderation-exempt chat types (Help Chat, Brahma Console) → never.
/// 2. A chat that is not Moderated (Locked, or Unmoderated — v4 `4d370a90f`'s
///    `isClassifierOnDuty`) → never; the Concierge only moves a Moderated chat.
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
    // Only a Moderated chat is the Concierge's to move (Locked is the
    // operator's; Unmoderated has nowhere further to go).
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
/// Per user who wants the summary classifier (v4 `3b463d6b1`): read the user's chats, filter through
/// [`chat_needs_classification`], resolve each chat's profile, and enqueue per
/// the decision tree at priority `-2`. A per-chat enqueue failure is swallowed
/// (v4 warns and continues); a user with zero unclassified chats still counts
/// as processed. Top-level read failures propagate (v4 rethrows).
pub async fn run_scheduled_danger_scan(db: &Db) -> Result<(usize, usize), DbError> {
    let all_settings = db.read_main(find_all_danger_scan_settings)?;

    let mut users_processed = 0usize;
    let mut chats_enqueued = 0usize;

    for settings in &all_settings {
        // Only users who opted in to the summary classifier are swept.
        if !wants_summary_classification(settings) {
            tracing::debug!(
                target: "quilltap::scheduled_danger_scan",
                user_id = %settings.user_id,
                "Skipping user without summary classification"
            );
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
            // string are all falsy. Since v4's bug 158 (`da9c4f34f`) the same
            // test runs on `chat.scenarioText` beside it — a scenario stands in
            // for a summary until the first fold. Before bug 158 the scenario
            // arrived here disguised as a summary, so this branch already took
            // every scenario-bearing chat; naming it changes nothing for a SHORT
            // chat, which the `else` reaches by a different route to the same
            // job. What moves is the LONG one: over 50 messages the old tree
            // summarized first and this one classifies directly.
            let has_summary = chat
                .get("contextSummary")
                .and_then(Value::as_str)
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            let has_scenario = chat
                .get("scenarioText")
                .and_then(Value::as_str)
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            let message_count = chat
                .get("messageCount")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);

            let enqueued = if has_summary || has_scenario {
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
        // Not Moderated (v4 `4d370a90f`): Locked and Unmoderated are skipped …
        assert!(!chat_needs_classification(
            &json!({ "chatType": "salon", "conciergeMode": "locked" })
        ));
        assert!(!chat_needs_classification(
            &json!({ "chatType": "salon", "conciergeMode": "unmoderated" })
        ));
        // … and the legacy pair no longer takes the Concierge off the case.
        assert!(chat_needs_classification(
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
