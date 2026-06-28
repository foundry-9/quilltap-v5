//! Tier-1 differential test #15 (Wave 2 / B6b): predicted turn order (display).
//!
//! Covers computePredictedTurnOrder — the full ordering pipeline (generating,
//! next, queue, eligible-by-talkativeness, user, spoken, inactive tail).
//! Entry lists compared in order, field-for-field.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/turn-order.ts \
//!     > /tmp/oracle-turn-order.ndjson
//! Run:
//!   QT_ORACLE_TURN_ORDER=/tmp/oracle-turn-order.ndjson cargo test -p quilltap-harness

use quilltap_core::turn_order::{compute_predicted_turn_order, TurnOrderParticipant};
use serde::Deserialize;

#[derive(Deserialize)]
struct WirePart {
    id: String,
    status: String,
    #[serde(rename = "controlledBy")]
    controlled_by: String,
    talkativeness: Option<f64>,
}

#[derive(Deserialize)]
struct WireEntry {
    #[serde(rename = "participantId")]
    participant_id: String,
    position: Option<i64>,
    status: String,
}

#[derive(Deserialize)]
struct Scenario {
    participants: Vec<WirePart>,
    queue: Vec<String>,
    spoken: Vec<String>,
    #[serde(rename = "lastSpeakerId")]
    last_speaker_id: Option<String>,
    #[serde(rename = "nextSpeakerId")]
    next_speaker_id: Option<String>,
    #[serde(rename = "isGenerating")]
    is_generating: bool,
    #[serde(rename = "respondingParticipantId")]
    responding_participant_id: Option<String>,
    #[serde(rename = "userParticipantId")]
    user_participant_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum OracleRow {
    #[serde(rename = "order")]
    Order {
        id: String,
        scenario: Scenario,
        out: Vec<WireEntry>,
    },
}

#[test]
fn turn_order_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_TURN_ORDER") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_TURN_ORDER to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let OracleRow::Order { id, scenario, out } =
            serde_json::from_str::<OracleRow>(line).unwrap();

        let participants: Vec<TurnOrderParticipant> = scenario
            .participants
            .iter()
            .map(|p| TurnOrderParticipant {
                id: p.id.clone(),
                status: Some(p.status.clone()),
                controlled_by: Some(p.controlled_by.clone()),
                talkativeness: p.talkativeness,
            })
            .collect();

        let got = compute_predicted_turn_order(
            &participants,
            &scenario.queue,
            &scenario.spoken,
            scenario.last_speaker_id.as_deref(),
            scenario.next_speaker_id.as_deref(),
            scenario.is_generating,
            scenario.responding_participant_id.as_deref(),
            scenario.user_participant_id.as_deref(),
        );

        assert_eq!(got.len(), out.len(), "order '{id}' length");
        for (i, (g, o)) in got.iter().zip(out.iter()).enumerate() {
            assert_eq!(
                g.participant_id, o.participant_id,
                "order '{id}' entry {i} id"
            );
            assert_eq!(g.position, o.position, "order '{id}' entry {i} position");
            assert_eq!(g.status, o.status, "order '{id}' entry {i} status");
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: turn-order matched oracle ({count} scenarios).");
}
