//! Storyteller mode — the four chat actions (v4
//! `app/api/v1/chats/[id]/actions/storyteller.ts`, P4.D343; contract W1).
//!
//! - `GET ?action=storyteller-round` → `{ round }`: the chat's active round,
//!   else its latest, else `null` (a closed round's `bids` is `[]`).
//! - `POST ?action=storyteller-run` → the selection runs as a stream; the
//!   dispatch reply is EMPTY (`Value::Null`, ruling R-8) and every frame rides
//!   the chat-scoped `Event` channel exactly like `ChatSend`'s. The plan's
//!   refusals answer BEFORE any frame.
//! - `POST ?action=storyteller-ask` → `{ bid }`: the ANSWERED card (the
//!   dispatch awaits the bid call).
//! - `POST ?action=storyteller-lie` → `{ closed: true }`.
//!
//! The three POSTs run after the chat 404 (`Chat not found`); the GET has
//! none (v4 dispatches GET actions before any chat lookup). A
//! [`StorytellerRoundError`] logs INFO `[Storyteller] Request refused` and
//! answers its status; a malformed body answers v4's ZodError arm, a plain 400
//! `Invalid request`.

use std::future::Future;
use std::pin::Pin;

use serde_json::{json, Value};

use super::types::{CoreError, ErrorKind, Response};
use crate::db::runtime::Db;
use crate::services::storyteller_rounds::{
    active_or_latest_round, let_storyteller_round_lie, RoundServiceError, StorytellerRoundError,
};
use crate::storyteller::types::{StorytellerBid, StorytellerOrder};

/// A planned selection the host runs (v4 `handleStorytellerRun`'s request,
/// validated).
#[derive(Debug, Clone)]
pub struct StorytellerRunRequest {
    pub chat_id: String,
    pub round_id: String,
    pub participant_ids: Vec<String>,
    pub order: StorytellerOrder,
    pub keep_order: bool,
}

/// The host's half of the two model-calling verbs: the run (plan, then the
/// turns on the chat's stream) and the ask (one bid call). A refusal comes back
/// as the typed [`RoundServiceError::Refused`].
pub type StorytellerFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, StorytellerDriverError>> + Send + 'a>>;

/// What a storyteller driver answers on failure.
#[derive(Debug)]
pub enum StorytellerDriverError {
    Refused(StorytellerRoundError),
    Core(CoreError),
}

impl From<RoundServiceError> for StorytellerDriverError {
    fn from(e: RoundServiceError) -> Self {
        match e {
            RoundServiceError::Refused(r) => StorytellerDriverError::Refused(r),
            RoundServiceError::Db(d) => match Response::error(ErrorKind::Internal, d.to_string()) {
                Response::Error(e) => StorytellerDriverError::Core(e),
                _ => unreachable!("Response::error builds an Error"),
            },
        }
    }
}

/// The host-side driver (implemented by `quilltap-host`'s storyteller spine).
pub trait StorytellerDriver: Send + Sync {
    /// v4 `handleStorytellerRun`: plan (refusals before any frame), then run
    /// the selection on the chat's stream.
    fn run(&self, req: StorytellerRunRequest) -> StorytellerFuture<'_, ()>;
    /// v4 `askStorytellerAnyway`: the gates, the card re-set, the bid call;
    /// answers the ANSWERED card.
    fn ask(
        &self,
        chat_id: String,
        round_id: String,
        participant_id: String,
    ) -> StorytellerFuture<'_, StorytellerBid>;
}

fn chat_exists(db: &Db, chat_id: &str) -> bool {
    crate::db::fallback::find_by_id_or_none("chats", chat_id, || {
        db.read_main(|c| crate::db::chats_read::find_by_id(c, chat_id))
    })
    .is_some()
}

fn invalid_request() -> Response {
    Response::error(ErrorKind::BadRequest, "Invalid request")
}

/// v4 `roundError`: INFO `[Storyteller] Request refused {chatId, action,
/// reason}` and the error's status.
pub fn refused(chat_id: &str, action: &str, err: &StorytellerRoundError) -> Response {
    tracing::info!(
        target: "quilltap::storyteller",
        chatId = %chat_id,
        action = %action,
        reason = %err.message,
        "[Storyteller] Request refused"
    );
    let kind = match err.status {
        409 => ErrorKind::Conflict,
        404 => ErrorKind::NotFound,
        _ => ErrorKind::BadRequest,
    };
    Response::error(kind, err.message.clone())
}

fn driver_error(chat_id: &str, action: &str, e: StorytellerDriverError) -> Response {
    match e {
        StorytellerDriverError::Refused(r) => refused(chat_id, action, &r),
        StorytellerDriverError::Core(c) => Response::Error(c),
    }
}

/// `GET ?action=storyteller-round`.
///
/// NO chat 404 here: v4's `handleGet` dispatches its actions BEFORE (and
/// without) any chat lookup, and `handleGetStorytellerRound` reads the rounds
/// by `chatId` alone — so a missing chat answers 200 `{round: null}`
/// (measured: `chat_turn_mode_routes_equivalence`'s `round_missing_chat`). The
/// three POSTs keep the 404: v4's `handlePost` looks the chat up first.
pub fn chat_storyteller_round(db: &Db, chat_id: &str) -> Response {
    match active_or_latest_round(db, chat_id) {
        Ok(round) => Response::StorytellerRound(json!({ "round": round })),
        Err(e) => Response::error(ErrorKind::Internal, e.to_string()),
    }
}

/// v4 `StorytellerRunRequestSchema.parse(body)`: `roundId` a string,
/// `participantIds` a string array, `order` one of the two, `keepOrder` a
/// boolean. Any miss is v4's ZodError arm.
fn parse_run(
    round_id: &Value,
    participant_ids: &Value,
    order: &Option<Value>,
    keep_order: &Option<Value>,
) -> Option<(String, Vec<String>, StorytellerOrder, bool)> {
    let round_id = round_id.as_str()?.to_string();
    let ids = participant_ids
        .as_array()?
        .iter()
        .map(|v| v.as_str().map(str::to_string))
        .collect::<Option<Vec<_>>>()?;
    let order = match order {
        None => StorytellerOrder::Sequential,
        Some(v) => StorytellerOrder::from_str(v.as_str()?)?,
    };
    let keep_order = match keep_order {
        None => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return None,
    };
    Some((round_id, ids, order, keep_order))
}

/// `POST ?action=storyteller-run`. INFO `[Storyteller] Running a selection`
/// once the body parses, then the driver (whose plan answers the refusals
/// before any frame).
pub async fn chat_storyteller_run(
    db: &Db,
    driver: Option<&dyn StorytellerDriver>,
    chat_id: &str,
    round_id: &Value,
    participant_ids: &Value,
    order: &Option<Value>,
    keep_order: &Option<Value>,
) -> Response {
    if !chat_exists(db, chat_id) {
        return Response::error(ErrorKind::NotFound, "Chat not found");
    }
    let Some((round_id, participant_ids, order, keep_order)) =
        parse_run(round_id, participant_ids, order, keep_order)
    else {
        return invalid_request();
    };
    tracing::info!(
        target: "quilltap::storyteller",
        chatId = %chat_id,
        roundId = %round_id,
        count = participant_ids.len(),
        order = order.as_str(),
        keepOrder = keep_order,
        "[Storyteller] Running a selection"
    );
    let Some(driver) = driver else {
        return Response::error(ErrorKind::Internal, "storyteller dispatch not assembled");
    };
    match driver
        .run(StorytellerRunRequest {
            chat_id: chat_id.to_string(),
            round_id,
            participant_ids,
            order,
            keep_order,
        })
        .await
    {
        Ok(()) => Response::StorytellerRun(Value::Null),
        Err(e) => driver_error(chat_id, "storyteller-run", e),
    }
}

/// `POST ?action=storyteller-ask`.
pub async fn chat_storyteller_ask(
    db: &Db,
    driver: Option<&dyn StorytellerDriver>,
    chat_id: &str,
    round_id: &Value,
    participant_id: &Value,
) -> Response {
    if !chat_exists(db, chat_id) {
        return Response::error(ErrorKind::NotFound, "Chat not found");
    }
    let (Some(round_id), Some(participant_id)) = (round_id.as_str(), participant_id.as_str())
    else {
        return invalid_request();
    };
    let Some(driver) = driver else {
        return Response::error(ErrorKind::Internal, "storyteller dispatch not assembled");
    };
    match driver
        .ask(
            chat_id.to_string(),
            round_id.to_string(),
            participant_id.to_string(),
        )
        .await
    {
        Ok(bid) => Response::StorytellerAsk(json!({ "bid": bid })),
        Err(e) => driver_error(chat_id, "storyteller-ask", e),
    }
}

/// `POST ?action=storyteller-lie`.
pub async fn chat_storyteller_lie(db: &Db, chat_id: &str, round_id: &Value) -> Response {
    if !chat_exists(db, chat_id) {
        return Response::error(ErrorKind::NotFound, "Chat not found");
    }
    let Some(round_id) = round_id.as_str() else {
        return invalid_request();
    };
    match let_storyteller_round_lie(db, chat_id, round_id).await {
        Ok(()) => Response::StorytellerLie(json!({ "closed": true })),
        Err(RoundServiceError::Refused(r)) => refused(chat_id, "storyteller-lie", &r),
        Err(RoundServiceError::Db(d)) => Response::error(ErrorKind::Internal, d.to_string()),
    }
}
