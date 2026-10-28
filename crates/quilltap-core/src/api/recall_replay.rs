//! The `chatRecallReplay` dispatch verb (P4.d13 — episodic recall §3): v4
//! `app/api/v1/chats/[id]/actions/recall-replay.ts` (`handleRecallReplay`).
//!
//! Body coercion, the R7 harness fields' validation (`memoryBudget` coerced;
//! `tuning` / `signals` through v4's Zod schemas, refused with v4's
//! `validationError` body; `asOf` a strict boolean), the chat-settings /
//! connection-profile anchor resolution, and the cheap-LLM ladder run here —
//! the ladder runs even when `signals` are pinned, exactly as v4's does; the replay itself
//! ([`crate::services::recall_replay::run_recall_replay`]) executes on the host
//! driver seam — like the courier resolve, only the composing host holds the
//! completion + embedding providers the distill/search ride.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;

use crate::cheap_llm::{cheap_llm_config_from_settings, cheap_llm_profile_from_value};
use crate::cheap_llm::{get_cheap_llm_provider, CheapLlmProfile, CheapLlmSelection};
use crate::db::runtime::Db;
use crate::db::{chat_settings, chats_read, connection_profiles};
use crate::recall_tuning::{validate_recall_replay_signals, validate_recall_tuning_input};
use crate::services::recall_replay::RunRecallReplayInput;

/// The route's `[Chats v1]` lines' target.
const LOG_TARGET: &str = "quilltap::chats";

/// The body fields of v4's `POST /api/v1/chats/[id]?action=recall-replay`, RAW
/// (the dispatch variant's carriers). `tuning` / `signals` / `as_of` are
/// tri-state — `Some(None)` is a present JSON `null`, which v4 refuses.
#[derive(Clone, Copy, Debug, Default)]
pub struct RecallReplayArgs<'a> {
    pub turn_index: Option<&'a Value>,
    pub character_id: Option<&'a Value>,
    pub limit: Option<&'a Value>,
    pub memory_budget: Option<&'a Value>,
    pub tuning: Option<Option<&'a Value>>,
    pub signals: Option<Option<&'a Value>>,
    pub as_of: Option<Option<&'a Value>>,
}

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
pub async fn recall_replay(
    db: &Db,
    driver: Option<&Arc<dyn RecallReplayDriver>>,
    user_id: &str,
    chat_id: &str,
    args: RecallReplayArgs<'_>,
    now_ms: f64,
) -> Response {
    let RecallReplayArgs {
        turn_index,
        character_id,
        limit,
        ..
    } = args;
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

    // v4 `typeof memoryBudget === 'number' && Number.isFinite && > 0 →
    // Math.floor`, else absent (a JSON number is always finite). `0.5` floors to
    // `0`, which the run then reads as "no budget".
    let memory_budget = args
        .memory_budget
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite() && *n > 0.0)
        .map(f64::floor);
    // `RecallTuningInputSchema.optional().safeParse(body.tuning)` — a present
    // `null` is validated (and refused) like any other value.
    let tuning = match args.tuning {
        None => None,
        Some(raw) => match validate_recall_tuning_input(raw.unwrap_or(&Value::Null)) {
            Ok(parsed) => Some(parsed),
            Err(issues) => {
                return Response::validation_error(crate::api::zod_issues::zod_issue_details(
                    &issues,
                ))
            }
        },
    };
    let signals = match args.signals {
        None => None,
        Some(raw) => match validate_recall_replay_signals(raw.unwrap_or(&Value::Null)) {
            Ok(parsed) => Some(parsed),
            Err(issues) => {
                return Response::validation_error(crate::api::zod_issues::zod_issue_details(
                    &issues,
                ))
            }
        },
    };
    // v4 `body.asOf !== undefined && typeof body.asOf !== 'boolean'` — `null`
    // included.
    let as_of = match args.as_of {
        None => false,
        Some(Some(Value::Bool(b))) => *b,
        Some(_) => return bad_request("asOf must be a boolean"),
    };
    // v4's `tuning: tuningParse.data ?? null` — the parsed object as `…Json`,
    // or a present `null`; an undefined `turnIndex` is omitted.
    let tuning_json = tuning
        .as_ref()
        .map(|(_, parsed)| serde_json::to_string(parsed).unwrap_or_default());
    tracing::debug!(
        target: LOG_TARGET,
        chatId = %chat_id,
        turnIndex = turn_index.map(|n| n as u64),
        tuningJson = tuning_json.as_deref(),
        tuning = tuning_json.is_none().then_some("null"),
        signalsPinned = signals.is_some(),
        asOf = as_of,
        "[Chats v1] Recall replay requested"
    );

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
        memory_budget,
        tuning: tuning.map(|(input, _)| input),
        signals,
        as_of,
    };
    match driver.run(input).await {
        Ok(result) => Response::RecallReplay(result),
        // v4: catch → WARN + errorResponse(message, 400).
        Err(message) => {
            tracing::warn!(
                target: LOG_TARGET,
                chatId = %chat_id,
                error = %message,
                "[Chats v1] Recall replay failed"
            );
            bad_request(message)
        }
    }
}
