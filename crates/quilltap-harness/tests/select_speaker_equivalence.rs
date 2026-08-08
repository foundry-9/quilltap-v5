//! Tier-1 differential test #16 (Wave 2 / B7): weighted next-speaker selection.
//!
//! Covers selectNextSpeaker AND selectNextSpeakerAfterUserMessage (Bug 50 fair
//! rotation, v4 f6eac168) with Math.random injected (the oracle pins it per case
//! and emits the draw). Compares nextSpeakerId / reason / cycleComplete and the
//! debug block (eligible list, weights within 1e-12, randomValue within 1e-12,
//! allLLMNewCycle).
//!
//! Generate the oracle output (tsx imports the WORKTREE case file — point it at
//! this lane's copy, not main):
//!   cd ~/source/quilltap-server
//!   npx tsx <worktree>/harness/oracle/cases/select-speaker.ts \
//!     > /tmp/oracle-select-speaker.ndjson
//! Run:
//!   QT_ORACLE_SELECT_SPEAKER=/tmp/oracle-select-speaker.ndjson cargo test -p quilltap-harness

use std::collections::HashMap;

use quilltap_core::chat_predicates::ParticipantStatus;
use quilltap_core::select_speaker::{
    select_next_speaker, select_next_speaker_after_user_message, SpeakerParticipant,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct WirePart {
    id: String,
    #[serde(rename = "type")]
    participant_type: String,
    status: String,
    #[serde(rename = "characterId")]
    character_id: Option<String>,
    #[serde(rename = "controlledBy")]
    controlled_by: String,
    talkativeness: Option<f64>,
}

#[derive(Deserialize)]
struct Scenario {
    participants: Vec<WirePart>,
    characters: HashMap<String, Option<f64>>,
    queue: Vec<String>,
    spoken: Vec<String>,
    #[serde(rename = "lastSpeakerId")]
    last_speaker_id: Option<String>,
    random01: f64,
    /// v4 Bug 44 overlay: the chat's `impersonatingParticipantIds` (absent on
    /// pre-Bug-44 scenarios, which pass no overlay).
    #[serde(default)]
    impersonating: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct WireDebug {
    #[serde(rename = "eligibleSpeakers")]
    eligible_speakers: Vec<String>,
    weights: HashMap<String, f64>,
    #[serde(rename = "randomValue")]
    random_value: f64,
    #[serde(rename = "allLLMNewCycle", default)]
    all_llm_new_cycle: bool,
}

#[derive(Deserialize)]
struct WireResult {
    #[serde(rename = "nextSpeakerId")]
    next_speaker_id: Option<String>,
    reason: String,
    #[serde(rename = "cycleComplete")]
    cycle_complete: bool,
    #[serde(default)]
    debug: Option<WireDebug>,
}

/// A `select-after` scenario driving `selectNextSpeakerAfterUserMessage` (Bug 50
/// fair rotation): the persisted `spokenThisCycle` / `turnQueue` arrive as JSON
/// strings (fail-soft parsed by the helper), the poster becomes `lastSpeakerId`.
#[derive(Deserialize)]
struct AfterScenario {
    participants: Vec<WirePart>,
    characters: HashMap<String, Option<f64>>,
    poster: String,
    #[serde(rename = "persistedSpokenJson")]
    persisted_spoken_json: Option<String>,
    #[serde(rename = "turnQueueJson")]
    turn_queue_json: Option<String>,
    #[serde(rename = "userParticipantId")]
    user_participant_id: Option<String>,
    random01: f64,
    #[serde(default)]
    impersonating: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum OracleRow {
    #[serde(rename = "select")]
    Select {
        id: String,
        scenario: Scenario,
        out: WireResult,
    },
    #[serde(rename = "select-after")]
    SelectAfter {
        id: String,
        scenario: AfterScenario,
        out: WireResult,
    },
}

/// Map the wire participants to [`SpeakerParticipant`] (shared by both scenario
/// kinds).
fn to_speakers(parts: &[WirePart]) -> Vec<SpeakerParticipant> {
    parts
        .iter()
        .map(|p| SpeakerParticipant {
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
            controlled_by: p.controlled_by.clone(),
            talkativeness: p.talkativeness,
        })
        .collect()
}

/// Only characters with a talkativeness value enter the lookup map (a null value
/// behaves like "no character value" → 0.5 fallback).
fn to_characters(chars: &HashMap<String, Option<f64>>) -> HashMap<String, f64> {
    chars
        .iter()
        .filter_map(|(k, v)| v.map(|t| (k.clone(), t)))
        .collect()
}

/// Compare a Rust [`SelectionResult`]-shaped tuple against the oracle wire result.
fn assert_result(id: &str, got: &quilltap_core::select_speaker::SelectionResult, out: &WireResult) {
    assert_eq!(got.next_speaker_id, out.next_speaker_id, "{id} nextSpeaker");
    assert_eq!(got.reason, out.reason, "{id} reason");
    assert_eq!(got.cycle_complete, out.cycle_complete, "{id} cycleComplete");

    match (&got.debug, &out.debug) {
        (None, None) => {}
        (Some(g), Some(o)) => {
            assert_eq!(g.eligible_speakers, o.eligible_speakers, "{id} eligible");
            assert_eq!(
                g.all_llm_new_cycle, o.all_llm_new_cycle,
                "{id} allLLMNewCycle"
            );
            assert!(
                (g.random_value - o.random_value).abs() < 1e-12,
                "{id} randomValue: rust={} oracle={}",
                g.random_value,
                o.random_value
            );
            assert_eq!(g.weights.len(), o.weights.len(), "{id} weights size");
            for (k, gv) in &g.weights {
                let ov = o
                    .weights
                    .get(k)
                    .unwrap_or_else(|| panic!("{id} weights missing key {k}"));
                assert!(
                    (gv - ov).abs() < 1e-12,
                    "{id} weight[{k}]: rust={gv} oracle={ov}"
                );
            }
        }
        (g, o) => panic!(
            "{id}: debug presence mismatch rust={} oracle={}",
            g.is_some(),
            o.is_some()
        ),
    }
}

#[test]
fn select_speaker_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_SELECT_SPEAKER") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_SELECT_SPEAKER to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    let mut after_count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<OracleRow>(line).unwrap() {
            OracleRow::Select { id, scenario, out } => {
                let participants = to_speakers(&scenario.participants);
                let characters = to_characters(&scenario.characters);
                let got = select_next_speaker(
                    &participants,
                    &characters,
                    &scenario.queue,
                    &scenario.spoken,
                    scenario.last_speaker_id.as_deref(),
                    scenario.random01,
                    scenario.impersonating.as_deref(),
                );
                assert_result(&format!("select '{id}'"), &got, &out);
                count += 1;
            }
            OracleRow::SelectAfter { id, scenario, out } => {
                let participants = to_speakers(&scenario.participants);
                let characters = to_characters(&scenario.characters);
                let got = select_next_speaker_after_user_message(
                    &participants,
                    &characters,
                    &scenario.poster,
                    scenario.persisted_spoken_json.as_deref(),
                    scenario.turn_queue_json.as_deref(),
                    scenario.user_participant_id.as_deref(),
                    scenario.random01,
                    scenario.impersonating.as_deref(),
                );
                assert_result(&format!("select-after '{id}'"), &got, &out);
                after_count += 1;
            }
        }
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    assert!(
        after_count > 0,
        "oracle has no select-after rows (Bug 50 helper): regenerate at f6eac168"
    );
    eprintln!("OK: select-speaker matched oracle ({count} select, {after_count} select-after).");
}
