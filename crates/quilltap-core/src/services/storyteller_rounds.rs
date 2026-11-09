//! Storyteller rounds — the lifecycle, engine half (v4
//! `lib/storyteller/round.service.ts`, P4.D343).
//!
//! 1. **Open** ([`open_storyteller_round`]): an operator post in a storyteller
//!    chat opens a round. Every invited seat is then asked for a bid in
//!    parallel; each card lands through [`land_bid`] and streams to the send as
//!    it lands. The parallel asks and the 45 s board race are the HOST's
//!    (`quilltap-host::storyteller_spine`, ruling R-3 — the engine holds no
//!    clock); the engine exposes the two constants.
//! 2. **Choose**: the operator runs a selection ([`plan_storyteller_run`]),
//!    lets the round lie ([`let_storyteller_round_lie`]), or asks a kept-quiet
//!    seat anyway ([`prepare_ask_anyway`] then [`land_bid`]).
//! 3. **Speak** ([`run_storyteller_round`]): the chosen seats take real turns,
//!    each limited to its bid's tier, then the floor returns to the operator.
//!    Storyteller mode never chains past the round.
//!
//! Bids are not part of the record: the round row holds them only while the
//! round is open (the LLM log keeps every call), and closing clears them.
//!
//! v4 serialises each round's row writes with a per-round promise chain
//! (`withRoundLock`, guarding `recordBid`'s read-modify-write). Here every
//! row write is ONE `Db::write` closure that re-reads the row inside the
//! writer — the single writer task is the lock.

use std::collections::HashMap;

use serde_json::Value;

use super::participant_resolver::to_speaker_participant;
use crate::db::runtime::Db;
use crate::db::storyteller_rounds::StorytellerRoundsRepository;
use crate::db::DbError;
use crate::realtime::bus::publish_realtime;
use crate::realtime::types::RealtimeTopic;
use crate::storyteller::cards::card_from_outcome;
use crate::storyteller::invite::{biddable_seats, choose_invitees, SeatCharacter, StorytellerSeat};
use crate::storyteller::types::{
    StorytellerBid, StorytellerBidCallOutcome, StorytellerBidRecord, StorytellerBidStatus,
    StorytellerClosedReason, StorytellerOrder, StorytellerRound, StorytellerSelection,
    StorytellerTier, StorytellerTurnContext,
};
use crate::weighted_random::DrawSource;

const CONTEXT: &str = "storyteller.round";

/// How long the send's stream waits for bids before handing the board over
/// (v4 `STORYTELLER_BOARD_TIMEOUT_MS`, D-open 1). The HOST owns the clock.
pub const STORYTELLER_BOARD_TIMEOUT_MS: u64 = 45_000;

/// At most this many concurrent speaking turns per connection profile (v4
/// `STORYTELLER_CONCURRENCY_PER_PROFILE`, D-open 2). The cap is applied by
/// [`run_storyteller_round`] (v4 `runWithCap`); the host supplies truly
/// concurrent turns.
pub const STORYTELLER_CONCURRENCY_PER_PROFILE: usize = 4;

/// v4 `StorytellerRoundError(message, status)` — a refusal the API answers as
/// an ordinary 4xx.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorytellerRoundError {
    pub message: String,
    pub status: u16,
}

impl StorytellerRoundError {
    fn new(message: &str, status: u16) -> Self {
        Self {
            message: message.to_string(),
            status,
        }
    }
}

/// A refusal, or a database failure underneath (v4's rethrow → the 500).
#[derive(Debug)]
pub enum RoundServiceError {
    Refused(StorytellerRoundError),
    Db(DbError),
}

impl From<DbError> for RoundServiceError {
    fn from(e: DbError) -> Self {
        RoundServiceError::Db(e)
    }
}

impl From<StorytellerRoundError> for RoundServiceError {
    fn from(e: StorytellerRoundError) -> Self {
        RoundServiceError::Refused(e)
    }
}

fn no_longer_open() -> StorytellerRoundError {
    StorytellerRoundError::new("That round is no longer open", 409)
}

/// v4 `pendingCard(seat, status)`.
pub fn pending_card(seat: &StorytellerSeat, kept_quiet: bool, now: &str) -> StorytellerBid {
    StorytellerBid {
        participant_id: seat.participant.id.clone(),
        character_id: seat.participant.character_id.clone().unwrap_or_default(),
        character_name: seat
            .character_name
            .clone()
            .unwrap_or_else(|| "Unknown".to_string()),
        status: if kept_quiet {
            StorytellerBidStatus::KeptQuiet
        } else {
            StorytellerBidStatus::Pending
        },
        stated_tier: Some(None),
        tier: Some(None),
        tier_raised: false,
        tools: Vec::new(),
        plan: Some(None),
        reason: Some(kept_quiet.then(|| "Kept quiet this round (talkativeness)".to_string())),
        talkativeness: seat.talkativeness,
        silent: seat.silent,
        asked_at: Some((!kept_quiet).then(|| now.to_string())),
        answered_at: Some(None),
    }
}

/// v4 `recordCard`: `recordBid` under the round's lock; publishes
/// `storytellerRounds` when a row came back.
pub async fn record_card(
    db: &Db,
    round_id: &str,
    chat_id: &str,
    card: StorytellerBid,
) -> Result<Option<StorytellerRound>, DbError> {
    let round_id_owned = round_id.to_string();
    let updated = db
        .write(move |w| {
            let now = crate::clock::now_iso();
            StorytellerRoundsRepository::new(w.main().connection()).record_bid(
                &round_id_owned,
                card,
                &now,
            )
        })
        .await?;
    if updated.is_some() {
        publish_realtime(RealtimeTopic::StorytellerRounds, Some(chat_id));
    }
    Ok(updated)
}

/// v4 `bidForSeat`'s tail: turn the bid call's outcome into the card, log
/// `Storyteller bid landed`, record it. The call itself is the host's
/// (`run_storyteller_bid`). Never fails: a failed record is logged and the
/// answered card still returned (v4 awaits `recordCard` inside the same
/// never-throwing chain only for the realtime side effect).
pub async fn land_bid(
    db: &Db,
    round_id: &str,
    chat_id: &str,
    card: &StorytellerBid,
    outcome: &StorytellerBidCallOutcome,
) -> Result<StorytellerBid, DbError> {
    let answered = card_from_outcome(card, outcome, &crate::clock::now_iso());
    tracing::info!(
        target: "quilltap::storyteller",
        context = CONTEXT,
        chatId = %chat_id,
        roundId = %round_id,
        participantId = %card.participant_id,
        status = answered.status.as_str(),
        tier = answered.tier_value().map(|t| t.as_str()),
        "Storyteller bid landed"
    );
    record_card(db, round_id, chat_id, answered.clone()).await?;
    Ok(answered)
}

/// v4 `abandonStorytellerRound(chatId, reason)`: close the chat's open or
/// running round, if it has one.
pub async fn abandon_storyteller_round(
    db: &Db,
    chat_id: &str,
    reason: StorytellerClosedReason,
) -> Result<(), DbError> {
    let chat_id_owned = chat_id.to_string();
    let closed: Option<String> = db
        .write(move |w| {
            let repo = StorytellerRoundsRepository::new(w.main().connection());
            let Some(active) = repo.find_active_for_chat(&chat_id_owned) else {
                return Ok(None);
            };
            let now = crate::clock::now_iso();
            repo.mark_closed(&active.id, reason, None, &now)?;
            Ok(Some(active.id))
        })
        .await?;
    if let Some(round_id) = closed {
        publish_realtime(RealtimeTopic::StorytellerRounds, Some(chat_id));
        tracing::info!(
            target: "quilltap::storyteller",
            context = CONTEXT,
            chatId = %chat_id,
            roundId = %round_id,
            reason = reason.as_str(),
            "Storyteller round closed unrun"
        );
    }
    Ok(())
}

/// The chat row's seats and characters, as `biddableSeats` / the planner read
/// them.
fn room_of(
    db: &Db,
    chat: &Value,
) -> Result<
    (
        Vec<crate::select_speaker::SpeakerParticipant>,
        HashMap<String, Value>,
    ),
    DbError,
> {
    let participants: Vec<crate::select_speaker::SpeakerParticipant> = chat
        .get("participants")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(to_speaker_participant).collect())
        .unwrap_or_default();
    let room = crate::room_characters::load_room_characters_from_db(db, &participants, &[])?;
    Ok((participants, room))
}

fn seat_characters(room: &HashMap<String, Value>) -> HashMap<String, SeatCharacter> {
    room.iter()
        .map(|(id, c)| {
            (
                id.clone(),
                SeatCharacter {
                    name: c
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    talkativeness: c.get("talkativeness").and_then(Value::as_f64),
                    archived: c
                        .get("archivedAt")
                        .is_some_and(|v| !v.is_null() && v.as_str() != Some("")),
                },
            )
        })
        .collect()
}

/// v4 `openStorytellerRound`'s engine half: abandon an open round
/// (`superseded`) FIRST, read the chat (missing → `Chat not found`), pick the
/// biddable seats and the invitees (ONE `random` draw per seat when the chat's
/// filter is on), create the row `open` with the pending cards then the
/// kept-quiet ones, publish, log `Storyteller round opened`. Returns the row;
/// its `pending` cards are the ones the host asks.
pub async fn open_storyteller_round(
    db: &Db,
    chat_id: &str,
    trigger_message_id: Option<&str>,
    random: &mut (dyn FnMut() -> f64 + Send),
) -> Result<StorytellerRound, RoundServiceError> {
    abandon_storyteller_round(db, chat_id, StorytellerClosedReason::Superseded).await?;

    let chat_id_owned = chat_id.to_string();
    let chat = db
        .read_main(move |c| crate::db::chats_read::find_by_id(c, &chat_id_owned))?
        .ok_or_else(|| RoundServiceError::Db(DbError::Internal("Chat not found".to_string())))?;
    let (participants, room) = room_of(db, &chat)?;
    let impersonating = crate::db::chats_impersonation::read_impersonating(&chat);
    let seats = biddable_seats(&participants, &seat_characters(&room), Some(&impersonating));
    let filter = chat.get("storytellerInviteFilter").and_then(Value::as_bool) == Some(true);
    let (invited, kept_quiet) = choose_invitees(seats, filter, random);

    let now = crate::clock::now_iso();
    let mut bids: Vec<StorytellerBid> = invited
        .iter()
        .map(|s| pending_card(s, false, &now))
        .collect();
    bids.extend(kept_quiet.iter().map(|s| pending_card(s, true, &now)));
    let id = uuid::Uuid::new_v4().to_string();
    let chat_id_owned = chat_id.to_string();
    let trigger = trigger_message_id.map(str::to_string);
    let round = db
        .write(move |w| {
            let now = crate::clock::now_iso();
            StorytellerRoundsRepository::new(w.main().connection()).create(
                &chat_id_owned,
                trigger.as_deref(),
                crate::storyteller::types::StorytellerRoundStatus::Open,
                bids,
                &id,
                &now,
            )
        })
        .await?;
    publish_realtime(RealtimeTopic::StorytellerRounds, Some(chat_id));
    tracing::info!(
        target: "quilltap::storyteller",
        context = CONTEXT,
        chatId = %chat_id,
        roundId = %round.id,
        invited = invited.len(),
        keptQuiet = kept_quiet.len(),
        "Storyteller round opened"
    );
    Ok(round)
}

/// The board timeout line (v4 `:197-203`) — logged by the host's opener when
/// its race times out (the clock is the host's).
pub fn log_board_timeout(chat_id: &str, round_id: &str, timeout_ms: u64) {
    tracing::info!(
        target: "quilltap::storyteller",
        context = CONTEXT,
        chatId = %chat_id,
        roundId = %round_id,
        timeoutMs = timeout_ms,
        "Storyteller board timeout — the board opens with bids still out"
    );
}

/// The round's current row (v4 `findById(round.id) ?? round`).
pub fn find_round(db: &Db, round_id: &str) -> Result<Option<StorytellerRound>, DbError> {
    let id = round_id.to_string();
    db.read_main(move |c| Ok(StorytellerRoundsRepository::new(c).find_by_id(&id)))
}

/// The GET's answer (v4 `findActiveForChat ?? findLatestForChat`).
pub fn active_or_latest_round(db: &Db, chat_id: &str) -> Result<Option<StorytellerRound>, DbError> {
    let id = chat_id.to_string();
    db.read_main(move |c| {
        let repo = StorytellerRoundsRepository::new(c);
        Ok(repo
            .find_active_for_chat(&id)
            .or_else(|| repo.find_latest_for_chat(&id)))
    })
}

fn open_round_of(
    db: &Db,
    chat_id: &str,
    round_id: &str,
) -> Result<StorytellerRound, RoundServiceError> {
    match find_round(db, round_id)? {
        Some(r)
            if r.chat_id == chat_id
                && r.status == crate::storyteller::types::StorytellerRoundStatus::Open =>
        {
            Ok(r)
        }
        _ => Err(no_longer_open().into()),
    }
}

/// v4 `askStorytellerAnyway`'s first half: the 409 / 404 gates, the card
/// re-set to `pending` and recorded. The host then asks for the bid and lands
/// it through [`land_bid`].
pub async fn prepare_ask_anyway(
    db: &Db,
    chat_id: &str,
    round_id: &str,
    participant_id: &str,
) -> Result<(StorytellerRound, StorytellerBid), RoundServiceError> {
    let round = open_round_of(db, chat_id, round_id)?;
    let Some(card) = round
        .bids
        .iter()
        .find(|b| b.participant_id == participant_id)
        .cloned()
    else {
        return Err(StorytellerRoundError::new("That seat is not on this round", 404).into());
    };
    let mut asking = card;
    asking.status = StorytellerBidStatus::Pending;
    asking.reason = Some(None);
    asking.asked_at = Some(Some(crate::clock::now_iso()));
    asking.answered_at = Some(None);
    record_card(db, &round.id, chat_id, asking.clone()).await?;
    Ok((round, asking))
}

/// v4 `letStorytellerRoundLie`: select nobody; the floor stays with the
/// operator.
pub async fn let_storyteller_round_lie(
    db: &Db,
    chat_id: &str,
    round_id: &str,
) -> Result<(), RoundServiceError> {
    let round = open_round_of(db, chat_id, round_id)?;
    let selection = StorytellerSelection {
        participant_ids: Vec::new(),
        order: None,
        chosen_at: crate::clock::now_iso(),
        names: Vec::new(),
    };
    let id = round.id.clone();
    db.write(move |w| {
        let now = crate::clock::now_iso();
        StorytellerRoundsRepository::new(w.main().connection()).mark_closed(
            &id,
            StorytellerClosedReason::LetLie,
            Some(Some(selection)),
            &now,
        )
    })
    .await?;
    publish_realtime(RealtimeTopic::StorytellerRounds, Some(chat_id));
    tracing::info!(
        target: "quilltap::storyteller",
        context = CONTEXT,
        chatId = %chat_id,
        roundId = %round.id,
        "Storyteller round let lie"
    );
    Ok(())
}

/// One planned speaking turn (v4 `StorytellerPlannedTurn`).
#[derive(Debug, Clone, PartialEq)]
pub struct StorytellerPlannedTurn {
    pub bid: StorytellerBid,
    pub participant_id: String,
    pub character_name: String,
    /// Speaks in the concurrent group (all-at-once Simple / Lookup).
    pub concurrent: bool,
    /// The seat's connection profile — the concurrency cap is per profile.
    pub profile_key: String,
}

/// v4 `planStorytellerRun`: validate a selection against a round and put it
/// in speaking order — the turn manager's rotation (drawn now, and PERSISTED,
/// if no cycle is under way) unless the operator arranged the board
/// (`keep_order`), with all-at-once Simple / Lookup seats first as one
/// concurrent group. Refusals in v4's order: 409 → empty → not a bid →
/// destroy-alone → chat missing.
pub async fn plan_storyteller_run(
    db: &Db,
    chat_id: &str,
    round_id: &str,
    participant_ids: &[String],
    order: StorytellerOrder,
    keep_order: bool,
    draws: &DrawSource,
) -> Result<(StorytellerRound, Vec<StorytellerPlannedTurn>), RoundServiceError> {
    let round = open_round_of(db, chat_id, round_id)?;
    // `[...new Set(participantIds)]` — first-seen order.
    let mut chosen: Vec<String> = Vec::new();
    for id in participant_ids {
        if !chosen.contains(id) {
            chosen.push(id.clone());
        }
    }
    if chosen.is_empty() {
        return Err(StorytellerRoundError::new(
            "Choose at least one bid, or let the round lie",
            400,
        )
        .into());
    }
    let mut bids: Vec<StorytellerBid> = Vec::with_capacity(chosen.len());
    for id in &chosen {
        match round.bids.iter().find(|b| &b.participant_id == id) {
            Some(b) if b.status == StorytellerBidStatus::Bid && b.tier_value().is_some() => {
                bids.push(b.clone())
            }
            _ => {
                return Err(StorytellerRoundError::new(
                    "Only a seat that bid to speak can be chosen",
                    400,
                )
                .into())
            }
        }
    }
    if bids
        .iter()
        .any(|b| b.tier_value() == Some(StorytellerTier::Destroy))
        && bids.len() > 1
    {
        return Err(
            StorytellerRoundError::new("A Destroy bid must be chosen on its own", 400).into(),
        );
    }
    let chat_id_owned = chat_id.to_string();
    let Some(chat) = db.read_main(move |c| crate::db::chats_read::find_by_id(c, &chat_id_owned))?
    else {
        return Err(StorytellerRoundError::new("Chat not found", 404).into());
    };

    let ordered: Vec<StorytellerBid> = if keep_order {
        // The operator dragged the cards into the order they want; that order stands.
        tracing::debug!(
            target: "quilltap::storyteller",
            chatId = %chat_id,
            roundId = %round_id,
            participantIds = ?chosen,
            "Storyteller run keeps the operator's order"
        );
        bids
    } else {
        let cycle = rotation_order(db, chat_id, &chat, draws).await?;
        let rank = |id: &str| -> usize {
            match cycle.iter().position(|c| c == id) {
                Some(i) => i,
                None => cycle.len() + chosen.iter().position(|c| c == id).unwrap_or(0),
            }
        };
        let mut sorted = bids;
        sorted.sort_by_key(|b| rank(&b.participant_id));
        sorted
    };

    let participants = chat.get("participants").and_then(Value::as_array);
    let mut turns: Vec<StorytellerPlannedTurn> = ordered
        .into_iter()
        .map(|bid| {
            let profile_key = participants
                .and_then(|ps| {
                    ps.iter().find(|p| {
                        p.get("id").and_then(Value::as_str) == Some(bid.participant_id.as_str())
                    })
                })
                .and_then(|p| p.get("connectionProfileId"))
                .and_then(Value::as_str)
                .unwrap_or("default")
                .to_string();
            let concurrent = order == StorytellerOrder::Simultaneous
                && matches!(
                    bid.tier_value(),
                    Some(StorytellerTier::Simple | StorytellerTier::Lookup)
                );
            StorytellerPlannedTurn {
                participant_id: bid.participant_id.clone(),
                character_name: bid.character_name.clone(),
                concurrent,
                profile_key,
                bid,
            }
        })
        .collect();
    // All at once: the concurrent group first, then the Act / Destroy tail in
    // the order above (a stable sort).
    turns.sort_by_key(|t| !t.concurrent);
    Ok((round, turns))
}

/// The turn manager's rotation for this chat (v4 `calculateTurnStateFromHistory`
/// over the chat's `type === 'message'` events, then `resolveCycleOrder`, which
/// may DRAW and PERSIST a fresh cycle — a real write).
async fn rotation_order(
    db: &Db,
    chat_id: &str,
    chat: &Value,
    draws: &DrawSource,
) -> Result<Vec<String>, DbError> {
    let (participants, room) = room_of(db, chat)?;
    let chat_id_owned = chat_id.to_string();
    let messages =
        db.read_main(move |c| crate::db::chats_messages_read::get_messages(c, &chat_id_owned))?;
    let views = super::turn_orchestrator::to_message_views(&messages);
    let turn_state = crate::turn_state::calculate_turn_state_from_history_with_cycle(
        &views,
        chat.get("spokenThisCycleParticipantIds")
            .and_then(Value::as_str),
        chat.get("cycleOrderParticipantIds").and_then(Value::as_str),
    );
    let characters = crate::room_characters::to_speaker_characters(&room);
    Ok(crate::cycle_order::resolve_cycle_order(
        db,
        chat_id,
        &participants,
        &characters,
        &turn_state,
        draws,
    )
    .await)
}

/// The result of one speaking turn, as the round runner needs it (v4
/// `StorytellerTurnOutcome`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StorytellerTurnOutcome {
    pub spoke: bool,
    pub yielded: bool,
    pub message_id: Option<String>,
    /// Whether the turn carried a scene-tracking context (the host keeps the
    /// context itself; v4's round housekeeping runs off the LAST outcome's).
    pub has_scene_tracking: bool,
}

/// How a round ended (v4 `'ran' | 'stopped' | 'yielded'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundEnding {
    Ran,
    Stopped,
    Yielded,
}

impl RoundEnding {
    pub fn as_str(self) -> &'static str {
        match self {
            RoundEnding::Ran => "ran",
            RoundEnding::Stopped => "stopped",
            RoundEnding::Yielded => "yielded",
        }
    }
}

/// v4 `contextFor(turn, spokenSince)`.
pub fn turn_context_for(
    turns: &[StorytellerPlannedTurn],
    index: usize,
    round_id: &str,
    spoken_since: Vec<String>,
) -> StorytellerTurnContext {
    let turn = &turns[index];
    let tier = turn
        .bid
        .tier_value()
        .expect("a planned turn carries a tier");
    let first_concurrent = turns.iter().position(|t| t.concurrent);
    StorytellerTurnContext {
        concurrent_follower: turn.concurrent && first_concurrent != Some(index),
        bid_record: StorytellerBidRecord {
            round_id: round_id.to_string(),
            tier,
            plan: turn.bid.plan_value().unwrap_or("").to_string(),
            tools_named: turn.bid.tools.clone(),
        },
        effect_ceiling: tier.effect_ceiling(),
        spoken_since,
    }
}

/// The hooks the round runner calls (v4 `runTurn` / `onTurnStart` /
/// `onTurnEnd` / `isCurrentTurnStopped`). The host implements them over the
/// orchestrator; the differential cans them. Every method takes `&self`: the
/// all-at-once group runs several turns at once over ONE runner (an
/// implementer keeps its mutable state behind a `Cell` / `RefCell`). The
/// futures need not be `Send` — the round runs on one current-thread task,
/// v4's own event-loop model.
#[allow(async_fn_in_trait)]
pub trait RoundTurnRunner {
    async fn run_turn(
        &self,
        turn: &StorytellerPlannedTurn,
        context: StorytellerTurnContext,
    ) -> StorytellerTurnOutcome;
    fn on_turn_start(&self, turn: &StorytellerPlannedTurn, depth: usize);
    fn on_turn_end(
        &self,
        turn: &StorytellerPlannedTurn,
        depth: usize,
        outcome: &StorytellerTurnOutcome,
    );
    fn is_stopped(&self) -> bool;
}

/// One cooperative yield: `Pending` once (waking itself), then `Ready`. It
/// stands where v4 `await`s an already-settled promise — one microtask tick —
/// so the group's hooks fire in v4's order (`plan_storyteller_run_equivalence`
/// row `simultaneous-cap-five-on-one-profile-plus-one`: every worker's first
/// turn starts, then every first turn ends, then the second wave starts).
/// `tokio::task::yield_now` needs the `rt` feature core does not enable.
pub(crate) struct YieldOnce(bool);

impl std::future::Future for YieldOnce {
    type Output = ();
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<()> {
        if self.0 {
            std::task::Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    }
}

pub(crate) fn yield_once() -> YieldOnce {
    YieldOnce(false)
}

/// v4 `runStorytellerRound`: mark the round running (INFO `Storyteller round
/// running`); run the all-at-once group — the concurrent (Simple / Lookup)
/// seats grouped by connection profile in first-seen order, each group
/// through `run_with_cap(group, STORYTELLER_CONCURRENCY_PER_PROFILE)`, every
/// group at once, every concurrent turn seeing `spokenSince = []`, a
/// concurrent turn's yield ignored, the stop checked ONCE after the group;
/// then, only when the group was not stopped, the sequential part in order
/// (the stop checked before each turn, a yield ends the round) with the names
/// of everyone who has spoken since — in COMPLETION order. One shared depth
/// counter (`myDepth = ++depth`, assigned at START), one shared `spoken`
/// list, and the last outcome is the last turn to FINISH. Then close it `ran`
/// (INFO `Storyteller round closed`). Returns the ending and the last outcome.
pub async fn run_storyteller_round<R: RoundTurnRunner>(
    db: &Db,
    chat_id: &str,
    round: &StorytellerRound,
    turns: &[StorytellerPlannedTurn],
    order: StorytellerOrder,
    runner: &R,
) -> Result<(RoundEnding, Option<StorytellerTurnOutcome>), DbError> {
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;

    let selection = StorytellerSelection {
        participant_ids: turns.iter().map(|t| t.participant_id.clone()).collect(),
        order: Some(order),
        chosen_at: crate::clock::now_iso(),
        names: turns.iter().map(|t| t.character_name.clone()).collect(),
    };
    let id = round.id.clone();
    let sel = selection.clone();
    db.write(move |w| {
        let now = crate::clock::now_iso();
        StorytellerRoundsRepository::new(w.main().connection()).mark_running(&id, sel, &now)
    })
    .await?;
    publish_realtime(RealtimeTopic::StorytellerRounds, Some(chat_id));
    tracing::info!(
        target: "quilltap::storyteller",
        context = CONTEXT,
        chatId = %chat_id,
        roundId = %round.id,
        order = order.as_str(),
        speakers = ?selection.names,
        "Storyteller round running"
    );

    // v4's closure-captured `spoken` / `lastOutcome` / `depth`.
    let spoken: RefCell<Vec<String>> = RefCell::new(Vec::new());
    let last_outcome: RefCell<Option<StorytellerTurnOutcome>> = RefCell::new(None);
    let depth = Cell::new(0usize);
    let mut ended = RoundEnding::Ran;

    // v4 `runOne`: the depth and `onTurnStart` before the turn's first await.
    let run_one = |index: usize, spoken_since: Vec<String>| {
        let (spoken, last_outcome, depth) = (&spoken, &last_outcome, &depth);
        async move {
            let turn = &turns[index];
            let my_depth = depth.get() + 1;
            depth.set(my_depth);
            runner.on_turn_start(turn, my_depth);
            let outcome = runner
                .run_turn(
                    turn,
                    turn_context_for(turns, index, &round.id, spoken_since),
                )
                .await;
            // v4 `await`s `runTurn`'s promise: one tick before the end hook.
            yield_once().await;
            runner.on_turn_end(turn, my_depth, &outcome);
            if outcome.spoke {
                spoken.borrow_mut().push(turn.character_name.clone());
            }
            *last_outcome.borrow_mut() = Some(outcome.clone());
            outcome
        }
    };

    // The concurrent group (all at once): everyone sees the transcript as it
    // stood at the operator's post; no one in the group answers another.
    let concurrent: Vec<usize> = (0..turns.len()).filter(|&i| turns[i].concurrent).collect();
    if !concurrent.is_empty() {
        // `byProfile` — a Map, so FIRST-SEEN order.
        let mut by_profile: Vec<(&str, VecDeque<usize>)> = Vec::new();
        for &i in &concurrent {
            let key = turns[i].profile_key.as_str();
            match by_profile.iter_mut().find(|(k, _)| *k == key) {
                Some((_, q)) => q.push_back(i),
                None => by_profile.push((key, VecDeque::from([i]))),
            }
        }
        let queues: Vec<RefCell<VecDeque<usize>>> = by_profile
            .into_iter()
            .map(|(_, q)| RefCell::new(q))
            .collect();
        // v4 `runWithCap(group, cap, fn)`: `min(cap, n)` workers per group,
        // each shifting the group's queue until it is empty; every group's
        // workers at once.
        let mut workers = Vec::new();
        for queue in &queues {
            let n = queue
                .borrow()
                .len()
                .min(STORYTELLER_CONCURRENCY_PER_PROFILE);
            for _ in 0..n {
                let run_one = &run_one;
                workers.push(async move {
                    loop {
                        let next = queue.borrow_mut().pop_front();
                        let Some(index) = next else { break };
                        run_one(index, Vec::new()).await;
                        // The worker `await`s `fn(item)`: one more tick before
                        // it shifts the next seat.
                        yield_once().await;
                    }
                });
            }
        }
        futures_util::future::join_all(workers).await;
        if runner.is_stopped() {
            ended = RoundEnding::Stopped;
        }
    }

    // The sequential part (in order, or the all-at-once Act / Destroy tail).
    if ended == RoundEnding::Ran {
        for index in (0..turns.len()).filter(|&i| !turns[i].concurrent) {
            if runner.is_stopped() {
                ended = RoundEnding::Stopped;
                break;
            }
            let since = spoken.borrow().clone();
            let outcome = run_one(index, since).await;
            if outcome.yielded {
                ended = RoundEnding::Yielded;
                break;
            }
        }
    }
    let spoken = spoken.borrow().clone();
    let last_outcome = last_outcome.borrow().clone();

    let id = round.id.clone();
    let sel = selection.clone();
    db.write(move |w| {
        let now = crate::clock::now_iso();
        StorytellerRoundsRepository::new(w.main().connection()).mark_closed(
            &id,
            StorytellerClosedReason::Ran,
            Some(Some(sel)),
            &now,
        )
    })
    .await?;
    publish_realtime(RealtimeTopic::StorytellerRounds, Some(chat_id));
    tracing::info!(
        target: "quilltap::storyteller",
        context = CONTEXT,
        chatId = %chat_id,
        roundId = %round.id,
        ended = ended.as_str(),
        spoke = ?spoken,
        "Storyteller round closed"
    );
    Ok((ended, last_outcome))
}
