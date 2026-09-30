//! "Try uncensored" — the operator's per-request escape hatch (port of v4
//! `lib/services/dangerous-content/retry-uncensored.ts`, NEW at `ce2f1dabf`,
//! #77).
//!
//! A refusal the Concierge could not get past, or a *soft* refusal no detector
//! can see (a polite paragraph, a sanitized picture), is answered by one click
//! that sends the same request straight to the uncensored desk. This module
//! decides whether that may happen and who takes it; the callers (the
//! `messageRetryUncensored` verb, the `chatRetryImageUncensored` verb and the
//! story-background job) do the generating.
//!
//! Rules of record (v4's, verbatim in substance):
//!
//! - **The retry never changes the chat's state.** Switching the chat is the
//!   sidebar's job, or the Concierge's after N refusals.
//! - **A Locked chat is never retried uncensored** (`'locked'`).
//! - **Off duty does not bar it.** "Off duty" stops the Concierge acting on his
//!   own; this is the operator acting, explicitly, on one request. Nor does an
//!   exempt chat type (help, Brahma): [`may_retry_uncensored`] asks the chat's
//!   STATE alone, and [`retry_policy`] puts the configured desk back even where
//!   the policy emptied it. v4's; ported as-is and recorded (E.7).
//! - The understudy comes from the one resolver ([`super::understudy`]),
//!   excluding the profile that answered (or refused) the original — by id
//!   where the trail or the chat's configuration names it, and by provider +
//!   model, which is what the original message records, so a profile
//!   reassigned since cannot hand the retry back to the model that already
//!   answered. (v4's `docs/developer/API.md` at `ce2f1dabf` omits that last
//!   exclusion — it arrived in the PR's review commit; the hunk is the spec.)
//! - The desk is the one *configured* ([`resolve_configured_concierge_desk`]),
//!   not the policy's, which is empty off duty.
//!
//! Reads only. The service's lines carry v4's `ConciergeRetryUncensored`
//! service logger and NO `[DangerousContent]` prefix; the route lines that
//! call it carry the prefix (`api/chat_media.rs`, `api/salon.rs`).

use serde_json::{json, Map, Value};

use crate::db::runtime::Db;
use crate::db::{characters_read, DbError};
use crate::services::participant_resolver::connection::resolve_connection_profile;

use super::chat_override::{concierge_state_may_fail_over, get_concierge_state};
use super::provider_routing::ApiKeyResolver;
use super::resolver::{
    resolve_concierge_settings, resolve_configured_concierge_desk, ResolvedConciergePolicy,
};
use super::understudy::{
    connection_profiles_find_all_or_empty, image_profiles_find_all_or_empty,
    resolve_image_understudy_on, resolve_text_understudy_on, ImageUnderstudyLookup,
    TextUnderstudyLookup, Understudy,
};

const TARGET: &str = "quilltap::concierge_retry_uncensored";

/// v4 `RetryUncensoredRefusal` — why a retry was refused: the chat is Locked,
/// or there is nobody to send it to. The wire token is the 409's `error`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryUncensoredRefusal {
    Locked,
    NoUnderstudy,
}

impl RetryUncensoredRefusal {
    /// The v4 wire token (`conflict(reason)` → `{"error":"locked"}`).
    pub fn as_str(self) -> &'static str {
        match self {
            RetryUncensoredRefusal::Locked => "locked",
            RetryUncensoredRefusal::NoUnderstudy => "no-understudy",
        }
    }
}

/// v4 `RetryUnderstudyResult<U>` — `{ ok: true, understudy }` or
/// `{ ok: false, reason }`.
pub type RetryUnderstudyResult = Result<Understudy, RetryUncensoredRefusal>;

/// v4 `mayRetryUncensored(chat)` — whether the chat's state permits an
/// uncensored retry at all (`conciergeStateMayFailOver`, i.e. not Locked).
pub fn may_retry_uncensored(chat: Option<&Value>) -> bool {
    concierge_state_may_fail_over(get_concierge_state(chat))
}

/// v4 `AnsweredBy` — who answered the original, as the message records it.
#[derive(Clone, Copy, Debug, Default)]
pub struct AnsweredBy<'a> {
    pub provider: Option<&'a str>,
    pub model_name: Option<&'a str>,
}

/// v4 `retryPolicy(chatSettings, chat)` — the chat's own policy, with the desk
/// as configured. Off duty the policy's desk is empty, and an operator who
/// named an uncensored profile must not be told there is none.
pub fn retry_policy(chat_settings: Option<&Value>, chat: &Value) -> ResolvedConciergePolicy {
    ResolvedConciergePolicy {
        desk: resolve_configured_concierge_desk(chat_settings),
        ..resolve_concierge_settings(chat_settings, Some(chat))
    }
}

fn str_of<'v>(v: &'v Value, key: &str) -> Option<&'v str> {
    v.get(key).and_then(Value::as_str)
}

/// v4 `sameModelIds(profiles, answeredBy)` — ids of the profiles that share the
/// original's provider AND model; `[]` unless BOTH are truthy.
fn same_model_ids(profiles: &[Value], answered_by: AnsweredBy<'_>) -> Vec<String> {
    let (Some(provider), Some(model_name)) = (
        answered_by.provider.filter(|s| !s.is_empty()),
        answered_by.model_name.filter(|s| !s.is_empty()),
    ) else {
        return Vec::new();
    };
    profiles
        .iter()
        .filter(|p| {
            str_of(p, "provider") == Some(provider) && str_of(p, "modelName") == Some(model_name)
        })
        .filter_map(|p| str_of(p, "id").map(str::to_string))
        .collect()
}

/// The kinds a trail row names (`profileKind`, absent = connection).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryProfileKind {
    Connection,
    Image,
}

impl RetryProfileKind {
    fn as_str(self) -> &'static str {
        match self {
            RetryProfileKind::Connection => "connection",
            RetryProfileKind::Image => "image",
        }
    }
}

/// v4 `trailProfileIds(trail, kind)` — the profile ids a stored trail names,
/// filtered by kind (`(a.profileKind ?? 'connection') === kind`).
fn trail_profile_ids(trail: Option<&Value>, kind: RetryProfileKind) -> Vec<String> {
    trail
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter(|a| str_of(a, "profileKind").unwrap_or("connection") == kind.as_str())
        .filter_map(|a| str_of(a, "profileId").map(str::to_string))
        .collect()
}

/// The error message v4 logs for a caught throw (`error instanceof Error ?
/// error.message : String(error)`).
fn db_error_message(e: &DbError) -> String {
    format!("{e:?}")
}

/// v4 `resolveTextRetryUnderstudy` — who would take a text retry of
/// `target_message`, or why nobody will.
///
/// Excludes the responder's own profile, every connection profile already on
/// the message's trail, and every profile on the model that answered it, so
/// the retry never lands where the original did. No attachment MIME types and
/// no filter reach the resolver (v4 passes neither).
pub async fn resolve_text_retry_understudy<A: ApiKeyResolver>(
    db: &Db,
    api_keys: &A,
    user_id: &str,
    chat: &Value,
    chat_settings: Option<&Value>,
    target_message: &Value,
) -> RetryUnderstudyResult {
    let chat_id = str_of(chat, "id").unwrap_or("").to_string();
    let message_id = str_of(target_message, "id").unwrap_or("").to_string();

    if !may_retry_uncensored(Some(chat)) {
        tracing::info!(
            target: TARGET,
            chat_id = %chat_id,
            message_id = %message_id,
            "Uncensored text retry refused: the chat is Locked"
        );
        return Err(RetryUncensoredRefusal::Locked);
    }

    // `new Set(...)` — insertion-ordered, de-duplicated.
    let mut exclude: Vec<String> = Vec::new();
    let add = |exclude: &mut Vec<String>, id: String| {
        if !exclude.contains(&id) {
            exclude.push(id);
        }
    };
    for id in trail_profile_ids(
        target_message.get("routeTrail"),
        RetryProfileKind::Connection,
    ) {
        add(&mut exclude, id);
    }

    // The responder: `participant.characterId` → `characters.findById` →
    // `resolveConnectionProfile(participant, character)` (NO chat default —
    // v4 throws past step 3, and the throw is caught into the DEBUG line).
    let participant = str_of(target_message, "participantId").and_then(|pid| {
        chat.get("participants")
            .and_then(Value::as_array)
            .and_then(|ps| ps.iter().find(|p| str_of(p, "id") == Some(pid)))
            .cloned()
    });
    if let Some(participant) = participant.as_ref() {
        if let Some(character_id) = str_of(participant, "characterId").filter(|s| !s.is_empty()) {
            let cid = character_id.to_string();
            let found = db.read_main(|main| {
                db.read_mount_index(|mount| characters_read::find_by_id(main, mount, &cid))
            });
            let outcome: Result<Option<String>, String> = match found {
                Ok(Some(character)) => {
                    match resolve_connection_profile(participant, &character, None) {
                        Some(id) => Ok(Some(id)),
                        None => Err(format!(
                            "No connection profile found for participant {} (character: {})",
                            str_of(participant, "id").unwrap_or(""),
                            str_of(&character, "name").unwrap_or("")
                        )),
                    }
                }
                Ok(None) => Ok(None),
                Err(e) => Err(db_error_message(&e)),
            };
            match outcome {
                Ok(Some(id)) => add(&mut exclude, id),
                Ok(None) => {}
                Err(error) => tracing::debug!(
                    target: TARGET,
                    chat_id = %chat_id,
                    message_id = %message_id,
                    error = %error,
                    "Could not resolve the responder profile to exclude from the retry"
                ),
            }
        }
    }

    // v4's `repos.connections.findAll()` is a fallback read (a failed query
    // logs the repository ERROR and answers `[]`), so its `catch` — this
    // debug line — only sees what v5 alone can meet: the read pool (P4.124).
    match db.read_main(|c| Ok(connection_profiles_find_all_or_empty(c))) {
        Ok(connections) => {
            let answered_by = AnsweredBy {
                provider: str_of(target_message, "provider"),
                model_name: str_of(target_message, "modelName"),
            };
            for id in same_model_ids(&connections, answered_by) {
                add(&mut exclude, id);
            }
        }
        Err(e) => tracing::debug!(
            target: TARGET,
            chat_id = %chat_id,
            message_id = %message_id,
            error = %db_error_message(&e),
            "Could not list connection profiles to exclude the answering model from the retry"
        ),
    }

    let policy = retry_policy(chat_settings, chat);
    // A pool failure logs v4's catch line and answers "nobody" — v4's outcome
    // for EVERY database failure here (its reads are fallback reads), so the
    // route's 409 `no-understudy` is v4's answer too (P4.124 measured).
    let understudy = resolve_text_understudy_on(
        db,
        api_keys,
        TextUnderstudyLookup {
            user_id,
            uncensored_text_profile_id: policy.desk.text_profile_id.as_deref(),
            exclude: &exclude,
            turn_attachment_mime_types: &[],
            filter: None,
        },
    );

    tracing::debug!(
        target: TARGET,
        chat_id = %chat_id,
        message_id = %message_id,
        excludedJson = %serde_json::Value::from(exclude.clone()),
        understudy_profile_id = understudy.as_ref().map(|u| u.profile.id.as_str()),
        "Resolved the uncensored text retry understudy"
    );

    understudy.ok_or(RetryUncensoredRefusal::NoUnderstudy)
}

/// v4 `resolveImageRetryUnderstudy` — who would take an image retry, or why
/// nobody will. `exclude_profile_ids` is the image profile that drew (or
/// refused) the original (the picture arm passes the chat's RAW
/// `imageProfileId`, which may be null; the background arm the RESOLVED one),
/// plus any image profile on `trail`; `answered_by` (the picture's recorded
/// provider and model) excludes every image profile on that model too.
#[allow(clippy::too_many_arguments)]
pub async fn resolve_image_retry_understudy<A: ApiKeyResolver>(
    db: &Db,
    api_keys: &A,
    user_id: &str,
    chat: &Value,
    chat_settings: Option<&Value>,
    exclude_profile_ids: &[Option<&str>],
    trail: Option<&Value>,
    answered_by: AnsweredBy<'_>,
) -> RetryUnderstudyResult {
    let chat_id = str_of(chat, "id").unwrap_or("").to_string();

    if !may_retry_uncensored(Some(chat)) {
        tracing::info!(
            target: TARGET,
            chat_id = %chat_id,
            "Uncensored image retry refused: the chat is Locked"
        );
        return Err(RetryUncensoredRefusal::Locked);
    }

    // `[...new Set([...excludeProfileIds.filter(non-empty string),
    //  ...trailProfileIds(trail, 'image')])]`.
    let mut exclude: Vec<String> = Vec::new();
    let candidates = exclude_profile_ids
        .iter()
        .filter_map(|id| id.filter(|s| !s.is_empty()).map(str::to_string))
        .chain(trail_profile_ids(trail, RetryProfileKind::Image));
    for id in candidates {
        if !exclude.contains(&id) {
            exclude.push(id);
        }
    }
    if answered_by.provider.is_some_and(|s| !s.is_empty())
        && answered_by.model_name.is_some_and(|s| !s.is_empty())
    {
        // The text arm's fallback-read rule (P4.124).
        match db.read_main(|c| Ok(image_profiles_find_all_or_empty(c))) {
            Ok(profiles) => {
                for id in same_model_ids(&profiles, answered_by) {
                    if !exclude.contains(&id) {
                        exclude.push(id);
                    }
                }
            }
            Err(e) => tracing::debug!(
                target: TARGET,
                chat_id = %chat_id,
                error = %db_error_message(&e),
                "Could not list image profiles to exclude the answering model from the retry"
            ),
        }
    }

    let policy = retry_policy(chat_settings, chat);
    // The text arm's rule (P4.124).
    let understudy = resolve_image_understudy_on(
        db,
        api_keys,
        ImageUnderstudyLookup {
            user_id,
            uncensored_image_profile_id: policy.desk.image_profile_id.as_deref(),
            exclude: &exclude,
        },
    );

    tracing::debug!(
        target: TARGET,
        chat_id = %chat_id,
        excludedJson = %serde_json::Value::from(exclude.clone()),
        understudy_profile_id = understudy.as_ref().map(|u| u.profile.id.as_str()),
        "Resolved the uncensored image retry understudy"
    );

    understudy.ok_or(RetryUncensoredRefusal::NoUnderstudy)
}

/// The answering profile's four fields (v4 `{ id, name, provider, modelName }`).
#[derive(Clone, Copy, Debug)]
pub struct AnsweringProfile<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub provider: &'a str,
    pub model_name: &'a str,
}

/// v4 `composeRetryRouteTrail(priorTrail, answering, profileKind)` — the call
/// sheet for a retry's result: whatever failed or refused on the original (its
/// `answered` rows DROPPED), then the understudy that answered, marked `via:
/// 'concierge'`. Never empty: the answering row's `via` is the record that the
/// operator sent it to the uncensored desk.
///
/// The appended row's key order is v4's THIRD one — `profileKind` spread LAST,
/// after `outcome` (the chokepoint's `row()` spreads it before `trigger`, the
/// schema declares it before `detail`). An answered row carries none of
/// `trigger`/`evidence`/`detail`, so all three orders coincide on this row —
/// pinned on bytes anyway. The prior rows pass through as the stored values
/// they are.
pub fn compose_retry_route_trail(
    prior_trail: Option<&Value>,
    answering: AnsweringProfile<'_>,
    profile_kind: RetryProfileKind,
) -> Vec<Value> {
    let mut out: Vec<Value> = prior_trail
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter(|a| str_of(a, "outcome") != Some("answered"))
        .cloned()
        .collect();
    let mut row = Map::new();
    row.insert("profileId".into(), json!(answering.id));
    row.insert("profileName".into(), json!(answering.name));
    row.insert("provider".into(), json!(answering.provider));
    row.insert("modelName".into(), json!(answering.model_name));
    row.insert("via".into(), json!("concierge"));
    row.insert("outcome".into(), json!("answered"));
    if profile_kind == RetryProfileKind::Image {
        row.insert("profileKind".into(), json!("image"));
    }
    out.push(Value::Object(row));
    out
}

/// [`compose_retry_route_trail`]`(None, answering, kind)`'s one row as the
/// typed [`RouteAttempt`](crate::services::route_trail::RouteAttempt) — for a
/// writer that takes typed rows (the story job's Lantern bubble, v4
/// `composeRetryRouteTrail(null, activeImageProfile, 'image')`). Persisted
/// through the schema, its bytes equal the composed row's (an answered row
/// has no `trigger` / `evidence` / `detail`, so the key orders coincide).
pub fn retry_answer_attempt(
    answering: AnsweringProfile<'_>,
    profile_kind: RetryProfileKind,
) -> crate::services::route_trail::RouteAttempt {
    use crate::services::route_trail::{RouteAttemptOutcome, RouteAttemptVia, RouteProfileKind};
    crate::services::route_trail::RouteAttempt {
        profile_id: answering.id.to_string(),
        profile_name: answering.name.to_string(),
        provider: answering.provider.to_string(),
        model_name: answering.model_name.to_string(),
        via: RouteAttemptVia::Concierge,
        outcome: RouteAttemptOutcome::Answered,
        trigger: None,
        evidence: None,
        profile_kind: (profile_kind == RetryProfileKind::Image).then_some(RouteProfileKind::Image),
        detail: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answering() -> AnsweringProfile<'static> {
        AnsweringProfile {
            id: "desk",
            name: "Desk",
            provider: "OPENROUTER",
            model_name: "m",
        }
    }

    #[test]
    fn compose_keeps_failures_drops_answers_and_ends_on_the_understudy() {
        let prior = json!([
            {"profileId": "a", "outcome": "refused"},
            {"profileId": "b", "outcome": "answered"},
            {"profileId": "c", "outcome": "failed"}
        ]);
        let trail = compose_retry_route_trail(Some(&prior), answering(), RetryProfileKind::Image);
        let ids: Vec<&str> = trail
            .iter()
            .map(|a| a["profileId"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["a", "c", "desk"]);
        // Bytes, key order included: `profileKind` LAST (v4's third order).
        assert_eq!(
            serde_json::to_string(trail.last().unwrap()).unwrap(),
            r#"{"profileId":"desk","profileName":"Desk","provider":"OPENROUTER","modelName":"m","via":"concierge","outcome":"answered","profileKind":"image"}"#
        );
    }

    #[test]
    fn compose_on_no_trail_is_one_connection_row_without_a_kind() {
        let trail = compose_retry_route_trail(None, answering(), RetryProfileKind::Connection);
        assert_eq!(
            serde_json::to_string(&trail).unwrap(),
            r#"[{"profileId":"desk","profileName":"Desk","provider":"OPENROUTER","modelName":"m","via":"concierge","outcome":"answered"}]"#
        );
        // A JSON null trail reads as none, as v4's `?? []`.
        assert_eq!(
            compose_retry_route_trail(
                Some(&Value::Null),
                answering(),
                RetryProfileKind::Connection
            ),
            trail
        );
    }

    #[test]
    fn may_retry_refuses_only_a_locked_chat() {
        assert!(may_retry_uncensored(Some(
            &json!({"conciergeMode": "moderated"})
        )));
        assert!(may_retry_uncensored(Some(
            &json!({"conciergeMode": "unmoderated"})
        )));
        assert!(may_retry_uncensored(Some(&json!({}))));
        assert!(!may_retry_uncensored(Some(
            &json!({"conciergeMode": "locked"})
        )));
    }

    #[test]
    fn same_model_needs_both_halves() {
        let ps = vec![
            json!({"id": "x", "provider": "P", "modelName": "M"}),
            json!({"id": "y", "provider": "P", "modelName": "N"}),
            json!({"id": "z", "provider": "P", "modelName": "M"}),
        ];
        let full = AnsweredBy {
            provider: Some("P"),
            model_name: Some("M"),
        };
        assert_eq!(same_model_ids(&ps, full), ["x", "z"]);
        let half = AnsweredBy {
            provider: Some("P"),
            model_name: None,
        };
        assert!(same_model_ids(&ps, half).is_empty());
        let empty = AnsweredBy {
            provider: Some("P"),
            model_name: Some(""),
        };
        assert!(same_model_ids(&ps, empty).is_empty());
    }

    #[test]
    fn trail_ids_filter_by_kind_with_connection_as_the_default() {
        let trail = json!([
            {"profileId": "c1"},
            {"profileId": "i1", "profileKind": "image"},
            {"profileId": "c2", "profileKind": "connection"}
        ]);
        assert_eq!(
            trail_profile_ids(Some(&trail), RetryProfileKind::Connection),
            ["c1", "c2"]
        );
        assert_eq!(
            trail_profile_ids(Some(&trail), RetryProfileKind::Image),
            ["i1"]
        );
        assert!(trail_profile_ids(None, RetryProfileKind::Image).is_empty());
    }
}
