//! The `chatRecallReplay` dispatch verb (P4.d13 — episodic recall §3): v4
//! `app/api/v1/chats/[id]/actions/recall-replay.ts` (`handleRecallReplay`).
//!
//! Body coercion, the chat-settings / connection-profile anchor resolution, and
//! the cheap-LLM ladder run here; the replay itself
//! ([`crate::services::recall_replay::run_recall_replay`]) executes on the host
//! driver seam — like the courier resolve, only the composing host holds the
//! completion + embedding providers the distill/search ride.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;

use crate::cheap_llm::{get_cheap_llm_provider, CheapLlmProfile, CheapLlmSelection};
use crate::db::runtime::Db;
use crate::db::{chat_settings, chats_read, connection_profiles};
use crate::services::image_job_common::{
    cheap_llm_config_from_settings, cheap_llm_profile_from_value,
};
use crate::services::recall_replay::RunRecallReplayInput;

use super::types::{ErrorKind, Response};

/// The host seam: runs [`crate::services::recall_replay::run_recall_replay`]
/// with the host's completion provider + cheap executor + embedding provider.
/// `Err(message)` maps to v4's catch → 400.
pub trait RecallReplayDriver: Send + Sync {
    fn run(
        &self,
        input: RunRecallReplayInput,
    ) -> Pin<Box<dyn Future<Output = Result<Value, String>> + Send + '_>>;
}

fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}

/// v4 `handleRecallReplay`. `user_id` is the single user (the route's
/// authenticated user).
#[allow(clippy::too_many_arguments)] // mirrors the route's parameter surface
pub async fn recall_replay(
    db: &Db,
    driver: Option<&Arc<dyn RecallReplayDriver>>,
    user_id: &str,
    chat_id: &str,
    turn_index: Option<&Value>,
    character_id: Option<&Value>,
    limit: Option<&Value>,
    now_ms: f64,
) -> Response {
    // Body coercion — v4's silent-undefined semantics: a wrong-typed value
    // falls back to the default, never errors. `Number.isInteger` ⇔ a JSON
    // number with zero fract.
    let turn_index = turn_index
        .and_then(Value::as_f64)
        .filter(|n| n.fract() == 0.0 && *n >= 1.0);
    // v4 `typeof body.characterId === 'string'` — any string, even empty (an
    // empty id then fails participant resolution inside the run, as in v4).
    let character_id = character_id.and_then(Value::as_str).map(str::to_string);
    let limit = limit
        .and_then(Value::as_f64)
        .filter(|n| n.fract() == 0.0 && *n >= 1.0)
        .map(|n| n.min(100.0));

    // The chat (the v4 route wrapper resolves it before the action handler; a
    // missing chat 404s there).
    let chat = {
        let cid = chat_id.to_string();
        match db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
            Ok(Some(c)) => c,
            Ok(None) => return Response::error(ErrorKind::NotFound, "Chat not found"),
            Err(e) => return Response::error(ErrorKind::Internal, e.to_string()),
        }
    };

    // v4: `repos.chatSettings.findByUserId(user.id)` → 400 when absent.
    let settings = {
        let uid = user_id.to_string();
        match db.read_main(move |c| chat_settings::find_by_user_id(c, &uid)) {
            Ok(s) => s,
            Err(e) => return Response::error(ErrorKind::Internal, e.to_string()),
        }
    };
    let Some(settings) = settings else {
        return bad_request("Chat settings not found.");
    };

    // Anchor on the first participant's profile when set, else any profile —
    // the cheap-LLM resolver only needs a fallback anchor.
    let available: Vec<Value> = {
        let uid = user_id.to_string();
        match db.read_main(move |c| connection_profiles::find_by_user_id(c, &uid)) {
            Ok(p) => p,
            Err(e) => return Response::error(ErrorKind::Internal, e.to_string()),
        }
    };
    let participant_profile_id =
        chat.get("participants")
            .and_then(Value::as_array)
            .and_then(|ps| {
                ps.iter().find_map(|p| {
                    p.get("connectionProfileId")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                })
            });
    let anchor_value = participant_profile_id
        .as_deref()
        .and_then(|pid| {
            available
                .iter()
                .find(|p| p.get("id").and_then(Value::as_str) == Some(pid))
        })
        .or_else(|| available.first());
    let Some(anchor_value) = anchor_value else {
        return bad_request("No connection profiles configured.");
    };

    let profiles: Vec<CheapLlmProfile> =
        available.iter().map(cheap_llm_profile_from_value).collect();
    let anchor: CheapLlmProfile = cheap_llm_profile_from_value(anchor_value);
    let cheap_llm: CheapLlmSelection = get_cheap_llm_provider(
        &anchor,
        &cheap_llm_config_from_settings(settings.get("cheapLLMSettings")),
        &profiles,
        false,
        None,
    );
    // v4 guards `if (!cheapLLM)` with "No cheap LLM provider available." — dead
    // code there (getCheapLLMProvider's ladder always falls back to the anchor
    // profile and never returns null), and structurally unrepresentable here.

    let Some(driver) = driver else {
        return Response::error(
            ErrorKind::Internal,
            "The recall-replay runner is not assembled on this host.",
        );
    };

    let input = RunRecallReplayInput {
        chat_id: chat_id.to_string(),
        user_id: user_id.to_string(),
        cheap_llm,
        turn_index,
        character_id,
        limit,
        now_ms,
    };
    match driver.run(input).await {
        Ok(result) => Response::RecallReplay(result),
        // v4: catch → errorResponse(message, 400).
        Err(message) => bad_request(message),
    }
}
