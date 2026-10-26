//! The interrupt actions (P4.D335 — v4 `app/api/v1/chats/[id]/actions/
//! interrupt.ts`, `2b5e8ee24`, at `322a5488e`).
//!
//! `?action=stop-turn` (hard stop) and `?action=raise-hand` (soft interrupt,
//! `{ raised }`): the operator's two ways of cutting in on a running Salon
//! stream. Both act on the in-process registry in
//! [`crate::services::turn_control`]; the stream itself does the saving and
//! closing, so these handlers only flip state and (for a stop) wait for the
//! stream to finish putting its partial reply away.
//!
//! The chat is loaded FIRST for both (v4's `handlePost` reads it and answers
//! `notFound('Chat')` before dispatching any action), through v4's FALLBACK
//! `findById` (a failed read is `null`, so the 404 answers it).

use serde_json::Value;

use crate::db::runtime::Db;
use crate::services::turn_control;

use super::types::{ErrorKind, RaiseHandResultDto, Response, StopTurnResultDto};
use super::zod_issues::{key, ZodIssue};

fn load_chat(db: &Db, chat_id: &str) -> Option<Value> {
    crate::db::fallback::find_by_id_or_none("chats", chat_id, || {
        db.read_main(|c| crate::db::chats_read::find_by_id(c, chat_id))
    })
}

/// Hard stop (v4 `handleStopTurn`). A room with a rotation is paused first — so
/// the chain's next decision, which re-reads the flag, already sees it — and
/// then every open stream for the chat is aborted. Resolves once they have
/// saved what had streamed (bounded), so the client's refetch finds the kept
/// reply.
pub async fn chat_stop_turn(db: &Db, chat_id: &str) -> Response {
    let Some(chat) = load_chat(db, chat_id) else {
        return Response::error(ErrorKind::NotFound, "Chat not found");
    };

    let participants: Vec<crate::participant_filters::ParticipantView> =
        crate::services::participant_resolver::participants_of(&chat)
            .iter()
            .map(crate::services::participant_resolver::to_filter_participant)
            .collect();
    let pause = chat.get("chatType").and_then(Value::as_str) != Some("autonomous")
        && crate::participant_filters::is_multi_character_chat(&participants);
    // An already-paused room is NOT re-written (no `updatedAt` churn).
    let is_paused = chat.get("isPaused").and_then(Value::as_bool) == Some(true);
    if pause && !is_paused {
        let cid = chat_id.to_string();
        if let Err(e) = db
            .write(move |w| {
                w.main().chats().update(
                    &cid,
                    &crate::db::chats::ChatUpdate {
                        is_paused: Some(true),
                        ..Default::default()
                    },
                )
            })
            .await
        {
            return Response::error(ErrorKind::Internal, e.to_string());
        }
    }

    let result = turn_control::stop_active_turns(chat_id, None).await;
    tracing::info!(
        target: "quilltap::interrupt",
        chat_id = %chat_id,
        // The PREDICATE, not whether a write happened (v4 `paused: pause`).
        paused = pause,
        stopped = result.stopped,
        settled = result.settled,
        "[Interrupt] Stop handled"
    );
    Response::StopTurn(StopTurnResultDto {
        stopped: result.stopped,
        settled: result.settled,
        paused: pause,
    })
}

/// v4 `raiseHandSchema = z.object({ raised: z.boolean().prefault(true) })` —
/// absent → `true`; a boolean is honoured; an explicit `null` or any
/// non-boolean is a Zod `invalid_type` issue (v4's `validationError` 400 via
/// the route's ZodError arm). Extra keys are stripped (no `.strict()`).
fn parse_raised(raised: &Option<Option<Value>>) -> Result<bool, Value> {
    match raised {
        None => Ok(true),
        Some(Some(Value::Bool(b))) => Ok(*b),
        Some(inner) => {
            let explicit_null = Value::Null;
            Err(Value::Array(vec![ZodIssue::invalid_type(
                "boolean",
                vec![key("raised")],
                Some(inner.as_ref().unwrap_or(&explicit_null)),
            )
            .to_value()]))
        }
    }
}

/// Soft interrupt (v4 `handleRaiseHand`). Raises (or lowers) the operator's
/// hand on the open stream; the next character built to speak is asked whether
/// to yield. `active: false` means nothing was running, so the floor is already
/// the operator's.
pub async fn chat_raise_hand(db: &Db, chat_id: &str, raised: &Option<Option<Value>>) -> Response {
    if load_chat(db, chat_id).is_none() {
        return Response::error(ErrorKind::NotFound, "Chat not found");
    }
    let raised = match parse_raised(raised) {
        Ok(r) => r,
        Err(issues) => return Response::validation_error(issues),
    };
    let streams = turn_control::set_hand_raised(chat_id, raised);
    tracing::info!(
        target: "quilltap::interrupt",
        chat_id = %chat_id,
        raised = raised,
        streams = streams,
        "[Interrupt] Hand state changed"
    );
    Response::RaiseHand(RaiseHandResultDto {
        raised: raised && streams > 0,
        active: streams > 0,
    })
}
