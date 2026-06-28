//! Tier-1 differential test #13 (Wave 2 / B5): the turn-state machine.
//!
//! Covers the queue ops (add/remove/pop/nudge/resetSkip), getQueuePosition,
//! calculateTurnStateFromHistory, updateTurnStateAfterMessage, and the
//! computeSpokenThisCycle{AfterMessage,AfterSkip} wrap. States compared
//! field-for-field; the JSON-string cycle outputs compared exactly.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/turn-state.ts \
//!     > /tmp/oracle-turn-state.ndjson
//! Run:
//!   QT_ORACLE_TURN_STATE=/tmp/oracle-turn-state.ndjson cargo test -p quilltap-harness

use quilltap_core::chat_predicates::ParticipantStatus;
use quilltap_core::turn_state::{
    add_to_queue, calculate_turn_state_from_history, compute_spoken_this_cycle_after_message,
    compute_spoken_this_cycle_after_skip, get_queue_position, nudge_participant, pop_from_queue,
    remove_from_queue, reset_cycle_for_user_skip, update_turn_state_after_message, MessageView,
    ParticipantView, TurnState,
};
use serde::Deserialize;

#[derive(Deserialize, Clone)]
struct WireState {
    #[serde(rename = "spokenSinceUserTurn")]
    spoken_since_user_turn: Vec<String>,
    #[serde(rename = "currentTurnParticipantId")]
    current_turn_participant_id: Option<String>,
    queue: Vec<String>,
    #[serde(rename = "lastSpeakerId")]
    last_speaker_id: Option<String>,
}

impl WireState {
    fn to_core(&self) -> TurnState {
        TurnState {
            spoken_since_user_turn: self.spoken_since_user_turn.clone(),
            current_turn_participant_id: self.current_turn_participant_id.clone(),
            queue: self.queue.clone(),
            last_speaker_id: self.last_speaker_id.clone(),
        }
    }
}

fn assert_state(got: &TurnState, want: &WireState, ctx: &str) {
    assert_eq!(
        got.spoken_since_user_turn, want.spoken_since_user_turn,
        "{ctx} spoken"
    );
    assert_eq!(
        got.current_turn_participant_id, want.current_turn_participant_id,
        "{ctx} current"
    );
    assert_eq!(got.queue, want.queue, "{ctx} queue");
    assert_eq!(got.last_speaker_id, want.last_speaker_id, "{ctx} last");
}

#[derive(Deserialize, Clone)]
struct WireMsg {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    role: String,
    #[serde(rename = "participantId")]
    participant_id: Option<String>,
    #[serde(rename = "targetParticipantIds")]
    target_participant_ids: Option<Vec<String>>,
}

impl WireMsg {
    fn to_core(&self) -> MessageView {
        MessageView {
            msg_type: self.msg_type.clone(),
            role: self.role.clone(),
            participant_id: self.participant_id.clone(),
            target_participant_ids: self.target_participant_ids.clone(),
        }
    }
}

#[derive(Deserialize, Clone)]
struct WirePart {
    id: String,
    #[serde(rename = "type")]
    participant_type: String,
    status: String,
    #[serde(rename = "characterId")]
    character_id: Option<String>,
}

fn parts_to_core(ps: &[WirePart]) -> Vec<ParticipantView> {
    ps.iter()
        .map(|p| ParticipantView {
            id: p.id.clone(),
            participant_type: p.participant_type.clone(),
            status: match p.status.as_str() {
                "active" => ParticipantStatus::Active,
                "silent" => ParticipantStatus::Silent,
                "absent" => ParticipantStatus::Absent,
                "removed" => ParticipantStatus::Removed,
                other => panic!("unknown status {other}"),
            },
            character_id: p.character_id.clone(),
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum OracleRow {
    #[serde(rename = "queueOp")]
    QueueOp {
        id: String,
        op: String,
        state: WireState,
        arg: Option<String>,
        out: WireState,
        #[serde(default)]
        popped: Option<String>,
    },
    #[serde(rename = "queuePos")]
    QueuePos {
        id: String,
        state: WireState,
        #[serde(rename = "participantId")]
        participant_id: String,
        out: i64,
    },
    #[serde(rename = "calc")]
    Calc {
        id: String,
        messages: Vec<WireMsg>,
        #[serde(rename = "spokenJson")]
        spoken_json: Option<String>,
        out: WireState,
    },
    #[serde(rename = "update")]
    Update {
        id: String,
        state: WireState,
        message: WireMsg,
        out: WireState,
    },
    #[serde(rename = "computeMsg")]
    ComputeMsg {
        id: String,
        message: WireMsg,
        participants: Vec<WirePart>,
        #[serde(rename = "currentJson")]
        current_json: Option<String>,
        out: Option<String>,
    },
    #[serde(rename = "computeSkip")]
    ComputeSkip {
        id: String,
        #[serde(rename = "skippedId")]
        skipped_id: String,
        participants: Vec<WirePart>,
        #[serde(rename = "currentJson")]
        current_json: Option<String>,
        out: Option<String>,
    },
}

#[test]
fn turn_state_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_TURN_STATE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_TURN_STATE to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut counts = [0usize; 6];
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<OracleRow>(line).unwrap() {
            OracleRow::QueueOp {
                id,
                op,
                state,
                arg,
                out,
                popped,
            } => {
                let s = state.to_core();
                match op.as_str() {
                    "add" => assert_state(&add_to_queue(&s, arg.as_deref().unwrap()), &out, &id),
                    "remove" => {
                        assert_state(&remove_from_queue(&s, arg.as_deref().unwrap()), &out, &id)
                    }
                    "nudge" => {
                        assert_state(&nudge_participant(&s, arg.as_deref().unwrap()), &out, &id)
                    }
                    "resetSkip" => assert_state(&reset_cycle_for_user_skip(&s), &out, &id),
                    "pop" => {
                        let (next, got_pop) = pop_from_queue(&s);
                        assert_state(&next, &out, &id);
                        assert_eq!(got_pop, popped, "queueOp '{id}' popped");
                    }
                    other => panic!("unknown op {other}"),
                }
                counts[0] += 1;
            }
            OracleRow::QueuePos {
                id,
                state,
                participant_id,
                out,
            } => {
                assert_eq!(
                    get_queue_position(&state.to_core(), &participant_id),
                    out,
                    "queuePos '{id}'"
                );
                counts[1] += 1;
            }
            OracleRow::Calc {
                id,
                messages,
                spoken_json,
                out,
            } => {
                let msgs: Vec<MessageView> = messages.iter().map(WireMsg::to_core).collect();
                let got = calculate_turn_state_from_history(&msgs, spoken_json.as_deref());
                assert_state(&got, &out, &id);
                counts[2] += 1;
            }
            OracleRow::Update {
                id,
                state,
                message,
                out,
            } => {
                let got = update_turn_state_after_message(&state.to_core(), &message.to_core());
                assert_state(&got, &out, &id);
                counts[3] += 1;
            }
            OracleRow::ComputeMsg {
                id,
                message,
                participants,
                current_json,
                out,
            } => {
                let got = compute_spoken_this_cycle_after_message(
                    &message.to_core(),
                    &parts_to_core(&participants),
                    current_json.as_deref(),
                );
                assert_eq!(got, out, "computeMsg '{id}'");
                counts[4] += 1;
            }
            OracleRow::ComputeSkip {
                id,
                skipped_id,
                participants,
                current_json,
                out,
            } => {
                let got = compute_spoken_this_cycle_after_skip(
                    &skipped_id,
                    &parts_to_core(&participants),
                    current_json.as_deref(),
                );
                assert_eq!(got, out, "computeSkip '{id}'");
                counts[5] += 1;
            }
        }
    }

    assert!(
        counts.iter().all(|&c| c > 0),
        "oracle file looks empty/partial: {counts:?}"
    );
    eprintln!("OK: turn-state matched oracle (counts {counts:?}).");
}
