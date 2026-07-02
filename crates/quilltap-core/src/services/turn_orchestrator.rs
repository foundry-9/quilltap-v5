//! The **turn-orchestration decision core** — v4
//! `lib/services/chat-message/turn-orchestrator.service.ts` (`shouldChainNext`,
//! `persistTurnParticipantId`, lines ~52–248) and the turn-action mutation core
//! of `app/api/v1/chats/[id]/actions/turn.ts` (`handleTurnAction`, lines
//! ~31–184).
//!
//! This is the first *stateful* chat-orchestration service: it re-reads a chat +
//! its messages, applies the pause/depth/time guards and the all-LLM auto-pause,
//! pops the persisted turn queue (skipping the immediate last speaker), builds the
//! per-character talkativeness map, and delegates to the already-ported pure turn
//! manager ([`crate::select_speaker::select_next_speaker`] +
//! [`crate::turn_state::calculate_turn_state_from_history`]) to pick the next
//! speaker. The turn-action ops (`nudge` / `queue` / `dequeue` / `skipUserTurn` /
//! `query`) apply the pure queue mutators then persist `turnQueue` +
//! `lastTurnParticipantId` (+ `spokenThisCycleParticipantIds` for a skip).
//!
//! ## What is NOT here
//!
//! * `executeTurnChain` — the model-calling chain *driver* (a later wave-3 unit).
//!   The guards it feeds into `shouldChainNext` (`maxChainDepth`,
//!   `maxChainTimeMs`, `neverPauseForUser`, `singleTurn`) DO live here.
//! * The HTTP shell of `handleTurnAction` (auth, request parsing, the
//!   participant-active validation, the response body assembly): transport-side.
//!   Only the DECISION/MUTATION logic is ported.
//!
//! ## Injected impurities
//!
//! v4 reads two ambient values inside these functions; both are injected here so
//! the core is a pure function of its arguments:
//!   * **Wall clock** — v4's `Date.now() - chainStartTime` guard. The caller
//!     passes `now_ms` and `chain_start_time_ms`; the core never reads a clock.
//!   * **`Math.random()`** — the sole impurity inside `selectNextSpeaker`. The
//!     already-ported [`select_next_speaker`] takes `random01` (the value
//!     `Math.random()` would return, in `[0, 1)`); the caller threads it through.
//!
//! ## Faithful v4 shapes (the traps)
//!
//! * **Guard order** (v4 `shouldChainNext`): chat-not-found → `isPaused` →
//!   `maxChainDepth` → `maxChainTimeMs` → compute turn state → all-LLM pause →
//!   queue pop → weighted selection → next-speaker resolution.
//! * **The all-LLM pause WRITE happens even when the decision is "don't chain"**:
//!   crossing the threshold sets `isPaused: true` and returns `paused`.
//! * **The queue pop skips ONLY the immediate last speaker** (`turnState
//!   .lastSpeakerId`), then persists the popped queue back — even if it skipped
//!   entries.
//! * **`chats.update` never mints `updatedAt`** here (no `updated_at` in the
//!   patch), so every write preserves the row's timestamp.
//! * The talkativeness map is keyed by `characterId`; a per-participant override
//!   wins over the character value inside [`select_next_speaker`].

use std::collections::HashMap;

use serde_json::Value;

use crate::all_llm_pause::should_pause_for_all_llm;
use crate::chat_predicates::ParticipantStatus;
use crate::db::runtime::Db;
use crate::db::{chats_messages_read, chats_read, DbError};
use crate::participant_filters::{
    find_user_participant, get_active_character_participants, is_all_llm_chat,
    ParticipantView as FilterParticipant,
};
use crate::select_speaker::{select_next_speaker, SelectionResult, SpeakerParticipant};
use crate::turn_state::{
    add_to_queue, calculate_turn_state_from_history, compute_spoken_this_cycle_after_skip,
    get_queue_position, nudge_participant, remove_from_queue, MessageView, ParticipantView,
    TurnState,
};

// ---------------------------------------------------------------------------
// Config + result types (v4 `ChainConfig` / `ChainDecision`).
// ---------------------------------------------------------------------------

/// Chain-driver limits (v4 `ChainConfig`). Only `max_chain_depth` /
/// `max_chain_time_ms` are read by [`should_chain_next`]; the retry fields live
/// with the (not-yet-ported) chain driver but are carried for shape fidelity.
#[derive(Clone, Debug, PartialEq)]
pub struct ChainConfig {
    pub max_chain_depth: i64,
    pub max_chain_time_ms: i64,
}

impl Default for ChainConfig {
    /// v4 `DEFAULT_CHAIN_CONFIG`: depth 20, 300 000 ms (5 min).
    fn default() -> Self {
        ChainConfig {
            max_chain_depth: 20,
            max_chain_time_ms: 300_000,
        }
    }
}

/// Autonomous-room guard (v4 `ChainGuards`). When `never_pause_for_user` is set,
/// the all-LLM pause threshold is bypassed (the autonomous-room runner enforces
/// its own budget caps; the in-chain pause would short-circuit it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChainGuards {
    pub never_pause_for_user: bool,
}

/// Why the chain (dis)continued (v4 `ChainDecision.reason`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainReason {
    UserTurn,
    Paused,
    MaxDepth,
    MaxTime,
    Error,
    NoNextSpeaker,
    CycleComplete,
    Continue,
}

impl ChainReason {
    /// The v4 string form (used in the differential to compare decisions).
    pub fn as_str(self) -> &'static str {
        match self {
            ChainReason::UserTurn => "user_turn",
            ChainReason::Paused => "paused",
            ChainReason::MaxDepth => "max_depth",
            ChainReason::MaxTime => "max_time",
            ChainReason::Error => "error",
            ChainReason::NoNextSpeaker => "no_next_speaker",
            ChainReason::CycleComplete => "cycle_complete",
            ChainReason::Continue => "continue",
        }
    }
}

/// The per-step chain decision (v4 `ChainDecision`). `participant_id` /
/// `character_name` are present only when a next speaker was resolved (chain or a
/// user-turn stop that surfaces which user character is up).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainDecision {
    pub chain: bool,
    pub participant_id: Option<String>,
    pub character_name: Option<String>,
    pub reason: ChainReason,
}

impl ChainDecision {
    fn stop(reason: ChainReason) -> Self {
        ChainDecision {
            chain: false,
            participant_id: None,
            character_name: None,
            reason,
        }
    }
}

// ---------------------------------------------------------------------------
// Shared marshaling: chat JSON → the pure turn-manager view structs.
// ---------------------------------------------------------------------------

/// Parse a status string (default `"active"`) into a [`ParticipantStatus`].
/// Unknown values map to `Absent` (not present), matching v4's Zod enum + the
/// present-set membership check.
fn parse_status(s: Option<&str>) -> ParticipantStatus {
    match s.unwrap_or("active") {
        "active" => ParticipantStatus::Active,
        "silent" => ParticipantStatus::Silent,
        "removed" => ParticipantStatus::Removed,
        _ => ParticipantStatus::Absent,
    }
}

fn str_field<'a>(p: &'a Value, key: &str) -> Option<&'a str> {
    p.get(key).and_then(Value::as_str)
}

/// A participant's non-empty character id (JS truthiness: empty string dropped).
fn nonempty_character_id(p: &Value) -> Option<String> {
    str_field(p, "characterId")
        .filter(|c| !c.is_empty())
        .map(String::from)
}

/// Build a [`turn_state::ParticipantView`] from a marshaled participant object.
fn to_turnstate_participant(p: &Value) -> ParticipantView {
    ParticipantView {
        id: str_field(p, "id").unwrap_or_default().to_string(),
        participant_type: str_field(p, "type").unwrap_or_default().to_string(),
        status: parse_status(str_field(p, "status")),
        character_id: nonempty_character_id(p),
    }
}

/// Build a [`participant_filters::ParticipantView`] from a marshaled participant.
fn to_filter_participant(p: &Value) -> FilterParticipant {
    FilterParticipant {
        id: str_field(p, "id").unwrap_or_default().to_string(),
        status: parse_status(str_field(p, "status")),
        controlled_by: str_field(p, "controlledBy").unwrap_or("llm").to_string(),
        character_id: nonempty_character_id(p),
    }
}

/// Build a [`select_speaker::SpeakerParticipant`] from a marshaled participant.
fn to_speaker_participant(p: &Value) -> SpeakerParticipant {
    SpeakerParticipant {
        id: str_field(p, "id").unwrap_or_default().to_string(),
        participant_type: str_field(p, "type").unwrap_or_default().to_string(),
        status: parse_status(str_field(p, "status")),
        character_id: nonempty_character_id(p),
        controlled_by: str_field(p, "controlledBy").unwrap_or("llm").to_string(),
        talkativeness: p.get("talkativeness").and_then(Value::as_f64),
    }
}

/// Build the [`MessageView`] list the turn machine reads from `getMessages`.
/// Every returned row is a `type: 'message'` event (v4 filters to those before
/// calling the turn manager).
fn to_message_views(messages: &[Value]) -> Vec<MessageView> {
    messages
        .iter()
        .filter(|m| str_field(m, "type") == Some("message"))
        .map(|m| MessageView {
            msg_type: Some("message".to_string()),
            role: str_field(m, "role").unwrap_or_default().to_string(),
            participant_id: str_field(m, "participantId").map(String::from),
            target_participant_ids: m.get("targetParticipantIds").and_then(|t| {
                t.as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
            }),
        })
        .collect()
}

/// The message events that carry a `role` (for the all-LLM turn count + presence
/// check). v4 reads `role` off the same filtered `MessageEvent[]`.
fn message_roles(messages: &[Value]) -> Vec<String> {
    messages
        .iter()
        .filter(|m| str_field(m, "type") == Some("message"))
        .map(|m| str_field(m, "role").unwrap_or_default().to_string())
        .collect()
}

/// Parse the persisted `turnQueue` JSON string (v4 `JSON.parse(freshChat.turnQueue
/// || '[]')`, falling back to `[]` on any parse failure). Only string elements are
/// kept.
fn parse_turn_queue(chat: &Value) -> Vec<String> {
    let s = chat
        .get("turnQueue")
        .and_then(Value::as_str)
        .unwrap_or("[]");
    match serde_json::from_str::<Value>(s) {
        Ok(Value::Array(arr)) => arr
            .into_iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => Vec::new(),
    }
}

fn participants_array(chat: &Value) -> Vec<Value> {
    chat.get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// JSON-encode a list of ids exactly as JS `JSON.stringify` does (compact).
fn json_ids(ids: &[String]) -> String {
    serde_json::to_string(ids).expect("string array always serializes")
}

/// Load the talkativeness map keyed by characterId (v4 builds a `Map<string,
/// Character>` and `selectNextSpeaker` reads `.talkativeness`; here we read the
/// scalar directly). Only the given active-character participants' characters are
/// fetched, one `findById` each (v4's per-participant read), dropping any that
/// don't resolve.
fn load_talkativeness_map(
    db: &Db,
    participants: &[&FilterParticipant],
) -> Result<HashMap<String, f64>, DbError> {
    let mut map = HashMap::new();
    for p in participants {
        if let Some(cid) = &p.character_id {
            if let Some(ch) = read_character(db, cid)? {
                if let Some(t) = ch.get("talkativeness").and_then(Value::as_f64) {
                    map.insert(cid.clone(), t);
                }
            }
        }
    }
    Ok(map)
}

/// Read a character (vault-overlaid, v4 `repos.characters.findById`) — the nested
/// main+mount read the vault overlay needs.
fn read_character(db: &Db, id: &str) -> Result<Option<Value>, DbError> {
    let id = id.to_string();
    db.read_main(|main| {
        db.read_mount_index(|mount| crate::db::characters_read::find_by_id(main, mount, &id))
    })
}

/// The character name for a participant (v4 reads `char.name`), or `None` when the
/// participant has no character id or the character doesn't resolve.
fn character_name_for(db: &Db, participant: &Value) -> Result<Option<String>, DbError> {
    let Some(cid) = nonempty_character_id(participant) else {
        return Ok(None);
    };
    let ch = read_character(db, &cid)?;
    Ok(ch
        .as_ref()
        .and_then(|c| c.get("name"))
        .and_then(Value::as_str)
        .map(String::from))
}

// ---------------------------------------------------------------------------
// shouldChainNext (turn-orchestrator.service.ts:52–229)
// ---------------------------------------------------------------------------

/// The per-step chain decision (v4 `shouldChainNext`). Re-reads the chat, applies
/// the guards + all-LLM auto-pause (which may write `isPaused: true`), pops the
/// turn queue (writing it back), selects the next speaker, and returns the
/// decision. `now_ms` / `chain_start_time_ms` inject v4's wall clock; `random01`
/// injects `Math.random()`.
// The wide signature mirrors v4's `shouldChainNext(repos, chatId,
// userParticipantId, chainDepth, chainStartTime, config, guards)` plus the two
// injected impurities (`now_ms`, `random01`); grouping them into a struct would
// obscure the 1:1 correspondence with the oracle.
#[allow(clippy::too_many_arguments)]
pub async fn should_chain_next(
    db: &Db,
    chat_id: &str,
    user_participant_id: Option<&str>,
    chain_depth: i64,
    now_ms: i64,
    chain_start_time_ms: i64,
    config: &ChainConfig,
    guards: ChainGuards,
    random01: f64,
) -> Result<ChainDecision, DbError> {
    // Re-read chat for fresh state (isPaused may have been set by a stop button).
    let chat_id_owned = chat_id.to_string();
    let fresh_chat = db.read_main(move |conn| chats_read::find_by_id(conn, &chat_id_owned))?;
    let Some(fresh_chat) = fresh_chat else {
        // v4 warns + returns { chain: false, reason: 'error' }.
        return Ok(ChainDecision::stop(ChainReason::Error));
    };

    // Check pause.
    if fresh_chat
        .get("isPaused")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(ChainDecision::stop(ChainReason::Paused));
    }

    // Check max depth.
    if chain_depth >= config.max_chain_depth {
        return Ok(ChainDecision::stop(ChainReason::MaxDepth));
    }

    // Check max time (injected wall clock).
    let elapsed = now_ms - chain_start_time_ms;
    if elapsed >= config.max_chain_time_ms {
        return Ok(ChainDecision::stop(ChainReason::MaxTime));
    }

    // Get messages and compute turn state.
    let chat_id_owned = chat_id.to_string();
    let messages =
        db.read_main(move |conn| chats_messages_read::get_messages(conn, &chat_id_owned))?;
    let message_views = to_message_views(&messages);
    let roles = message_roles(&messages);

    let participants = participants_array(&fresh_chat);
    let filter_participants: Vec<FilterParticipant> =
        participants.iter().map(to_filter_participant).collect();

    let spoken_json = fresh_chat
        .get("spokenThisCycleParticipantIds")
        .and_then(Value::as_str);
    let turn_state = calculate_turn_state_from_history(&message_views, spoken_json);

    // All-LLM pause check. A chat is only truly all-LLM if there is no
    // user-controlled participant AND no USER messages in history (a user typing
    // means a human is present).
    let is_all_llm = is_all_llm_chat(&filter_participants);
    let has_user_presence = user_participant_id.is_some() || roles.iter().any(|r| r == "USER");
    let effective_all_llm = is_all_llm && !has_user_presence;
    if effective_all_llm {
        // Count assistant messages since the last user message.
        let mut turn_count: i64 = 0;
        for role in roles.iter().rev() {
            if role == "USER" {
                break;
            }
            if role == "ASSISTANT" {
                turn_count += 1;
            }
        }

        if should_pause_for_all_llm(turn_count) && turn_count > 0 && !guards.never_pause_for_user {
            // Pause the chat (write happens even though we return "don't chain").
            let chat_id_owned = chat_id.to_string();
            db.write(move |writers| {
                writers.main().chats().update(
                    &chat_id_owned,
                    &crate::db::chats::ChatUpdate {
                        is_paused: Some(true),
                        ..Default::default()
                    },
                )
            })
            .await?;
            return Ok(ChainDecision::stop(ChainReason::Paused));
        }
    }

    // Check the turn queue (persisted in DB).
    let mut turn_queue = parse_turn_queue(&fresh_chat);
    let mut next_participant_id: Option<String> = None;
    let mut selection_reason = String::new();

    if !turn_queue.is_empty() {
        // Pop from the front of the queue, skipping any entry matching the
        // participant who just spoke — otherwise a nudge (which both queues the
        // participant AND triggers an immediate response) would double-respond.
        while !turn_queue.is_empty() {
            let candidate = turn_queue.remove(0);
            if Some(candidate.as_str()) != turn_state.last_speaker_id.as_deref() {
                next_participant_id = Some(candidate);
                selection_reason = "queue".to_string();
                break;
            }
        }
        // Persist the updated queue (even if we skipped entries).
        let chat_id_owned = chat_id.to_string();
        let queue_json = json_ids(&turn_queue);
        db.write(move |writers| {
            writers.main().chats().update(
                &chat_id_owned,
                &crate::db::chats::ChatUpdate {
                    turn_queue: Some(queue_json),
                    ..Default::default()
                },
            )
        })
        .await?;
    }

    if next_participant_id.is_none() && selection_reason != "queue" {
        // Weighted turn-selection algorithm.
        let active_character_participants = get_active_character_participants(&filter_participants);
        let talkativeness = load_talkativeness_map(db, &active_character_participants)?;

        let speaker_participants: Vec<SpeakerParticipant> =
            participants.iter().map(to_speaker_participant).collect();

        let result: SelectionResult = select_next_speaker(
            &speaker_participants,
            &talkativeness,
            &[], // queue already consulted above; v4 passes turnState.queue which is empty here
            &turn_state.spoken_since_user_turn,
            turn_state.last_speaker_id.as_deref(),
            random01,
        );

        next_participant_id = result.next_speaker_id.clone();
        selection_reason = result.reason.to_string();
    }

    // No next speaker.
    let Some(next_participant_id) = next_participant_id else {
        let mapped = match selection_reason.as_str() {
            "cycle_complete" => ChainReason::CycleComplete,
            "user_turn" => ChainReason::UserTurn,
            _ => ChainReason::NoNextSpeaker,
        };
        return Ok(ChainDecision::stop(mapped));
    };

    // Resolve the next-speaker participant.
    let next_participant = participants
        .iter()
        .find(|p| str_field(p, "id") == Some(next_participant_id.as_str()))
        .cloned();
    let Some(next_participant) = next_participant else {
        return Ok(ChainDecision::stop(ChainReason::Error));
    };

    if str_field(&next_participant, "controlledBy") == Some("user") {
        // Surface which user character is "up" so the UI can label the pause.
        let character_name = character_name_for(db, &next_participant)?;
        return Ok(ChainDecision {
            chain: false,
            participant_id: Some(next_participant_id),
            character_name,
            reason: ChainReason::UserTurn,
        });
    }

    // Find the character name (v4 defaults to 'Unknown').
    let character_name =
        character_name_for(db, &next_participant)?.unwrap_or_else(|| "Unknown".to_string());

    Ok(ChainDecision {
        chain: true,
        participant_id: Some(next_participant_id),
        character_name: Some(character_name),
        reason: ChainReason::Continue,
    })
}

/// Persist the last turn participant id for turn-state restoration on reload (v4
/// `persistTurnParticipantId`). v4 swallows write errors (best-effort); here the
/// error surfaces to the caller, which may choose to ignore it (the chain driver
/// logs + continues).
pub async fn persist_turn_participant_id(
    db: &Db,
    chat_id: &str,
    participant_id: Option<&str>,
) -> Result<(), DbError> {
    let chat_id_owned = chat_id.to_string();
    let pid = participant_id.map(String::from);
    db.write(move |writers| {
        writers.main().chats().update(
            &chat_id_owned,
            &crate::db::chats::ChatUpdate {
                last_turn_participant_id: Some(pid),
                ..Default::default()
            },
        )
    })
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// The turn-action mutation core (actions/turn.ts:31–184).
// ---------------------------------------------------------------------------

/// A turn action (v4 `turnActionSchema.action`). The HTTP validation (participant
/// active, skip-user only on user-controlled) stays transport-side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnAction {
    Nudge,
    Queue,
    Dequeue,
    SkipUserTurn,
    Query,
}

/// The turn-action decision result (the subset of v4's response body that the
/// core computes — the DECISION, not the HTTP envelope). `next_speaker_*` come
/// from the selection; `queue` is the post-mutation queue; `queue_position` is
/// the affected participant's 1-indexed slot (0 = not queued).
#[derive(Clone, Debug, PartialEq)]
pub struct TurnActionResult {
    pub action: TurnAction,
    pub next_speaker_id: Option<String>,
    pub next_speaker_name: Option<String>,
    pub next_speaker_controlled_by: Option<String>,
    pub reason: String,
    pub explanation: &'static str,
    pub cycle_complete: bool,
    pub is_users_turn: bool,
    pub queue: Vec<String>,
    pub queue_position: Option<i64>,
}

/// Process a turn action (v4 `handleTurnAction`, mutation core). Reads the chat +
/// messages, computes the turn state, applies the pure queue mutator for the
/// action, selects the next speaker (`random01` injects `Math.random`), and — for
/// state-modifying actions — persists `turnQueue` + `lastTurnParticipantId`
/// (+ `spokenThisCycleParticipantIds` for a skip). `query` writes nothing.
///
/// `participant_id` is required for every action except `query`.
pub async fn handle_turn_action(
    db: &Db,
    chat_id: &str,
    action: TurnAction,
    participant_id: Option<&str>,
    random01: f64,
) -> Result<TurnActionResult, DbError> {
    let chat_id_owned = chat_id.to_string();
    let chat = db.read_main(move |conn| chats_read::find_by_id(conn, &chat_id_owned))?;
    let chat = chat.ok_or_else(|| DbError::Key(format!("chat not found: {chat_id}")))?;

    let participants = participants_array(&chat);
    let filter_participants: Vec<FilterParticipant> =
        participants.iter().map(to_filter_participant).collect();

    // v4 resolves the user participant and threads it into
    // `calculateTurnStateFromHistory` / `selectNextSpeaker`; the ported pure
    // modules fold that resolution into their `controlledBy` handling, so the
    // value is not a separate argument here. Resolved for parity/documentation.
    let _user_participant_id = find_user_participant(&filter_participants).map(|p| p.id.clone());

    let chat_id_owned = chat_id.to_string();
    let messages =
        db.read_main(move |conn| chats_messages_read::get_messages(conn, &chat_id_owned))?;
    let message_views = to_message_views(&messages);

    let spoken_json = chat
        .get("spokenThisCycleParticipantIds")
        .and_then(Value::as_str);
    let mut turn_state = calculate_turn_state_from_history(&message_views, spoken_json);

    // Pre-computed cycle update for skipUserTurn (written below with the queue).
    let mut skip_cycle_update: Option<Option<String>> = None; // None = not-a-skip; Some(x) = the computed value (None = null)

    match action {
        TurnAction::Nudge => {
            let pid = participant_id.expect("nudge requires a participant id");
            turn_state = nudge_participant(&turn_state, pid);
        }
        TurnAction::Queue => {
            let pid = participant_id.expect("queue requires a participant id");
            turn_state = add_to_queue(&turn_state, pid);
        }
        TurnAction::Dequeue => {
            let pid = participant_id.expect("dequeue requires a participant id");
            turn_state = remove_from_queue(&turn_state, pid);
        }
        TurnAction::Query => {
            // Read-only: just compute next speaker from current state.
        }
        TurnAction::SkipUserTurn => {
            let pid = participant_id.expect("skipUserTurn requires a participant id");
            // Record the user-controlled participant as having "taken" their turn,
            // and treat them as the last speaker so the next pick excludes them.
            let ts_participants: Vec<ParticipantView> =
                participants.iter().map(to_turnstate_participant).collect();
            let computed = compute_spoken_this_cycle_after_skip(pid, &ts_participants, spoken_json);
            skip_cycle_update = Some(computed.clone());
            if let Some(update) = &computed {
                // Mirror v4: parse the JSON array into spokenSinceUserTurn.
                if let Ok(Value::Array(arr)) = serde_json::from_str::<Value>(update) {
                    let parsed: Vec<String> = arr
                        .into_iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect();
                    turn_state = TurnState {
                        spoken_since_user_turn: parsed,
                        ..turn_state
                    };
                } else if !turn_state.spoken_since_user_turn.iter().any(|id| id == pid) {
                    // Fallback manual append (cycle wrap won't be observed locally).
                    let mut s = turn_state.spoken_since_user_turn.clone();
                    s.push(pid.to_string());
                    turn_state = TurnState {
                        spoken_since_user_turn: s,
                        ..turn_state
                    };
                }
            }
            turn_state = TurnState {
                last_speaker_id: Some(pid.to_string()),
                ..turn_state
            };
        }
    }

    // Build the talkativeness map (v4 reads active-character participants).
    let active_character_participants = get_active_character_participants(&filter_participants);
    let talkativeness = load_talkativeness_map(db, &active_character_participants)?;
    let speaker_participants: Vec<SpeakerParticipant> =
        participants.iter().map(to_speaker_participant).collect();

    let next_speaker = select_next_speaker(
        &speaker_participants,
        &talkativeness,
        &turn_state.queue,
        &turn_state.spoken_since_user_turn,
        turn_state.last_speaker_id.as_deref(),
        random01,
    );

    // Persist turnQueue + lastTurnParticipantId for state-modifying actions.
    if action != TurnAction::Query {
        let chat_id_owned = chat_id.to_string();
        let queue_json = json_ids(&turn_state.queue);
        let next_id = next_speaker.next_speaker_id.clone();
        let spoken_write = match (action, &skip_cycle_update) {
            (TurnAction::SkipUserTurn, Some(Some(v))) => Some(v.clone()),
            _ => None,
        };
        db.write(move |writers| {
            writers.main().chats().update(
                &chat_id_owned,
                &crate::db::chats::ChatUpdate {
                    turn_queue: Some(queue_json),
                    last_turn_participant_id: Some(next_id),
                    spoken_this_cycle_participant_ids: spoken_write,
                    ..Default::default()
                },
            )
        })
        .await?;
    }

    // Determine the next speaker's character info.
    let next_speaker_participant = next_speaker.next_speaker_id.as_ref().and_then(|id| {
        participants
            .iter()
            .find(|p| str_field(p, "id") == Some(id.as_str()))
    });
    let next_speaker_name = match next_speaker_participant {
        Some(p) => character_name_for(db, p)?,
        None => None,
    };
    let next_speaker_controlled_by = next_speaker_participant
        .and_then(|p| str_field(p, "controlledBy"))
        .map(String::from);

    let queue_position = participant_id.map(|pid| get_queue_position(&turn_state, pid));

    Ok(TurnActionResult {
        action,
        next_speaker_id: next_speaker.next_speaker_id.clone(),
        next_speaker_name,
        next_speaker_controlled_by,
        reason: next_speaker.reason.to_string(),
        explanation: crate::select_speaker::get_selection_explanation(&next_speaker),
        cycle_complete: next_speaker.cycle_complete,
        is_users_turn: crate::select_speaker::is_users_turn(&next_speaker),
        queue: turn_state.queue.clone(),
        queue_position,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn chain_config_defaults_match_v4() {
        let c = ChainConfig::default();
        assert_eq!(c.max_chain_depth, 20);
        assert_eq!(c.max_chain_time_ms, 300_000);
    }

    #[test]
    fn chain_reason_strings_match_v4() {
        assert_eq!(ChainReason::UserTurn.as_str(), "user_turn");
        assert_eq!(ChainReason::Paused.as_str(), "paused");
        assert_eq!(ChainReason::MaxDepth.as_str(), "max_depth");
        assert_eq!(ChainReason::MaxTime.as_str(), "max_time");
        assert_eq!(ChainReason::Error.as_str(), "error");
        assert_eq!(ChainReason::NoNextSpeaker.as_str(), "no_next_speaker");
        assert_eq!(ChainReason::CycleComplete.as_str(), "cycle_complete");
        assert_eq!(ChainReason::Continue.as_str(), "continue");
    }

    #[test]
    fn parse_status_defaults_and_unknown() {
        assert_eq!(parse_status(None), ParticipantStatus::Active);
        assert_eq!(parse_status(Some("active")), ParticipantStatus::Active);
        assert_eq!(parse_status(Some("silent")), ParticipantStatus::Silent);
        assert_eq!(parse_status(Some("removed")), ParticipantStatus::Removed);
        // Unknown → Absent (not present).
        assert_eq!(parse_status(Some("bogus")), ParticipantStatus::Absent);
    }

    #[test]
    fn parse_turn_queue_falls_back_to_empty() {
        assert_eq!(parse_turn_queue(&json!({})), Vec::<String>::new());
        assert_eq!(
            parse_turn_queue(&json!({ "turnQueue": "[]" })),
            Vec::<String>::new()
        );
        assert_eq!(
            parse_turn_queue(&json!({ "turnQueue": "not json" })),
            Vec::<String>::new()
        );
        assert_eq!(
            parse_turn_queue(&json!({ "turnQueue": "[\"a\",\"b\"]" })),
            vec!["a".to_string(), "b".to_string()]
        );
        // Non-string elements are dropped (v4 keeps only strings).
        assert_eq!(
            parse_turn_queue(&json!({ "turnQueue": "[\"a\",1,null,\"b\"]" })),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn json_ids_is_compact() {
        assert_eq!(json_ids(&[]), "[]");
        assert_eq!(
            json_ids(&["a".to_string(), "b".to_string()]),
            "[\"a\",\"b\"]"
        );
    }

    #[test]
    fn nonempty_character_id_drops_empty() {
        assert_eq!(
            nonempty_character_id(&json!({ "characterId": "x" })),
            Some("x".to_string())
        );
        assert_eq!(nonempty_character_id(&json!({ "characterId": "" })), None);
        assert_eq!(nonempty_character_id(&json!({})), None);
    }
}
