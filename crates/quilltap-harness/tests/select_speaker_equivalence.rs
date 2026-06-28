//! Tier-1 differential test #16 (Wave 2 / B7): weighted next-speaker selection.
//!
//! Covers selectNextSpeaker with Math.random injected (the oracle pins it per
//! case and emits the draw). Compares nextSpeakerId / reason / cycleComplete and
//! the debug block (eligible list, weights within 1e-12, randomValue within
//! 1e-12, allLLMNewCycle).
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/select-speaker.ts \
//!     > /tmp/oracle-select-speaker.ndjson
//! Run:
//!   QT_ORACLE_SELECT_SPEAKER=/tmp/oracle-select-speaker.ndjson cargo test -p quilltap-harness

use std::collections::HashMap;

use quilltap_core::chat_predicates::ParticipantStatus;
use quilltap_core::select_speaker::{select_next_speaker, SpeakerParticipant};
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

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum OracleRow {
    #[serde(rename = "select")]
    Select {
        id: String,
        scenario: Scenario,
        out: WireResult,
    },
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
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let OracleRow::Select { id, scenario, out } =
            serde_json::from_str::<OracleRow>(line).unwrap();

        let participants: Vec<SpeakerParticipant> = scenario
            .participants
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
            .collect();
        // Only characters with a talkativeness value enter the lookup map (a
        // null value behaves like "no character value" → 0.5 fallback).
        let characters: HashMap<String, f64> = scenario
            .characters
            .iter()
            .filter_map(|(k, v)| v.map(|t| (k.clone(), t)))
            .collect();

        let got = select_next_speaker(
            &participants,
            &characters,
            &scenario.queue,
            &scenario.spoken,
            scenario.last_speaker_id.as_deref(),
            scenario.random01,
        );

        assert_eq!(
            got.next_speaker_id, out.next_speaker_id,
            "select '{id}' nextSpeaker"
        );
        assert_eq!(got.reason, out.reason, "select '{id}' reason");
        assert_eq!(
            got.cycle_complete, out.cycle_complete,
            "select '{id}' cycleComplete"
        );

        match (&got.debug, &out.debug) {
            (None, None) => {}
            (Some(g), Some(o)) => {
                assert_eq!(
                    g.eligible_speakers, o.eligible_speakers,
                    "select '{id}' eligible"
                );
                assert_eq!(
                    g.all_llm_new_cycle, o.all_llm_new_cycle,
                    "select '{id}' allLLMNewCycle"
                );
                assert!(
                    (g.random_value - o.random_value).abs() < 1e-12,
                    "select '{id}' randomValue: rust={} oracle={}",
                    g.random_value,
                    o.random_value
                );
                assert_eq!(
                    g.weights.len(),
                    o.weights.len(),
                    "select '{id}' weights size"
                );
                for (k, gv) in &g.weights {
                    let ov = o
                        .weights
                        .get(k)
                        .unwrap_or_else(|| panic!("select '{id}' weights missing key {k}"));
                    assert!(
                        (gv - ov).abs() < 1e-12,
                        "select '{id}' weight[{k}]: rust={gv} oracle={ov}"
                    );
                }
            }
            (g, o) => panic!(
                "select '{id}': debug presence mismatch rust={} oracle={}",
                g.is_some(),
                o.is_some()
            ),
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: select-speaker matched oracle ({count} scenarios).");
}
