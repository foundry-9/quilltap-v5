//! Tier-1 differential test #14 (Wave 2 / B6a): all-LLM pause thresholds +
//! participant-list filters.
//!
//! Covers all-llm-pause.ts (5 fns) and the participant helpers in utils.ts
//! (find/list/predicate). Integers/bools exact; id lists in order.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/turn-pause-filters.ts \
//!     > /tmp/oracle-turn-pause-filters.ndjson
//! Run:
//!   QT_ORACLE_TURN_PAUSE_FILTERS=/tmp/oracle-turn-pause-filters.ndjson cargo test -p quilltap-harness

use quilltap_core::all_llm_pause::{
    get_current_pause_threshold, get_next_pause_interval, get_next_pause_threshold,
    get_turns_until_next_pause, should_pause_for_all_llm,
};
use quilltap_core::chat_predicates::ParticipantStatus;
use quilltap_core::participant_filters::{
    find_active_user_participant, find_user_controlled_participants, find_user_participant,
    get_active_character_participants, get_active_llm_participants, is_all_llm_chat,
    is_multi_character_chat, ParticipantView,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, Clone)]
struct WirePart {
    id: String,
    status: String,
    #[serde(rename = "controlledBy")]
    controlled_by: String,
    #[serde(rename = "characterId")]
    character_id: Option<String>,
}

fn parts_to_core(ps: &[WirePart]) -> Vec<ParticipantView> {
    ps.iter()
        .map(|p| ParticipantView {
            id: p.id.clone(),
            status: match p.status.as_str() {
                "active" => ParticipantStatus::Active,
                "silent" => ParticipantStatus::Silent,
                "absent" => ParticipantStatus::Absent,
                "removed" => ParticipantStatus::Removed,
                other => panic!("unknown status {other}"),
            },
            controlled_by: p.controlled_by.clone(),
            character_id: p.character_id.clone(),
        })
        .collect()
}

fn ids(rs: &[&ParticipantView]) -> Vec<String> {
    rs.iter().map(|p| p.id.clone()).collect()
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum OracleRow {
    #[serde(rename = "pause")]
    Pause {
        id: String,
        #[serde(rename = "fn")]
        func: String,
        #[serde(rename = "turnCount")]
        turn_count: i64,
        out: Value,
    },
    #[serde(rename = "find")]
    Find {
        id: String,
        #[serde(rename = "fn")]
        func: String,
        participants: Vec<WirePart>,
        #[serde(rename = "activeId")]
        active_id: Option<String>,
        out: Option<String>,
    },
    #[serde(rename = "list")]
    List {
        id: String,
        #[serde(rename = "fn")]
        func: String,
        participants: Vec<WirePart>,
        out: Vec<String>,
    },
    #[serde(rename = "pred")]
    Pred {
        id: String,
        #[serde(rename = "fn")]
        func: String,
        participants: Vec<WirePart>,
        out: bool,
    },
}

#[test]
fn turn_pause_filters_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_TURN_PAUSE_FILTERS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_TURN_PAUSE_FILTERS to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut counts = [0usize; 4];
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<OracleRow>(line).unwrap() {
            OracleRow::Pause {
                id,
                func,
                turn_count,
                out,
            } => {
                match func.as_str() {
                    "nextInterval" => assert_eq!(
                        get_next_pause_interval(turn_count),
                        out.as_i64().unwrap(),
                        "pause '{id}'"
                    ),
                    "should" => assert_eq!(
                        should_pause_for_all_llm(turn_count),
                        out.as_bool().unwrap(),
                        "pause '{id}'"
                    ),
                    "current" => assert_eq!(
                        get_current_pause_threshold(turn_count),
                        out.as_i64().unwrap(),
                        "pause '{id}'"
                    ),
                    "next" => assert_eq!(
                        get_next_pause_threshold(turn_count),
                        out.as_i64().unwrap(),
                        "pause '{id}'"
                    ),
                    "until" => assert_eq!(
                        get_turns_until_next_pause(turn_count),
                        out.as_i64().unwrap(),
                        "pause '{id}'"
                    ),
                    other => panic!("unknown pause fn {other}"),
                }
                counts[0] += 1;
            }
            OracleRow::Find {
                id,
                func,
                participants,
                active_id,
                out,
            } => {
                let core = parts_to_core(&participants);
                let got = match func.as_str() {
                    "user" => find_user_participant(&core),
                    "active" => find_active_user_participant(&core, active_id.as_deref()),
                    other => panic!("unknown find fn {other}"),
                };
                assert_eq!(got.map(|p| p.id.clone()), out, "find '{id}'");
                counts[1] += 1;
            }
            OracleRow::List {
                id,
                func,
                participants,
                out,
            } => {
                let core = parts_to_core(&participants);
                let got = match func.as_str() {
                    "userControlled" => ids(&find_user_controlled_participants(&core)),
                    "activeLLM" => ids(&get_active_llm_participants(&core)),
                    "activeChar" => ids(&get_active_character_participants(&core)),
                    other => panic!("unknown list fn {other}"),
                };
                assert_eq!(got, out, "list '{id}'");
                counts[2] += 1;
            }
            OracleRow::Pred {
                id,
                func,
                participants,
                out,
            } => {
                let core = parts_to_core(&participants);
                let got = match func.as_str() {
                    "multi" => is_multi_character_chat(&core),
                    "allLLM" => is_all_llm_chat(&core),
                    other => panic!("unknown pred fn {other}"),
                };
                assert_eq!(got, out, "pred '{id}'");
                counts[3] += 1;
            }
        }
    }

    assert!(
        counts.iter().all(|&c| c > 0),
        "oracle file looks empty/partial: {counts:?}"
    );
    eprintln!("OK: turn-pause-filters matched oracle (counts {counts:?}).");
}
