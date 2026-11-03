//! Tier-1 differential (P4.D348): the turn holds —
//! `quilltap_core::services::turn_holds::{resolve_turn_hold,
//! qualifies_for_fair_rotation_check}` vs v4's REAL `resolveTurnHold` /
//! `qualifiesForFairRotationCheck` (`lib/services/chat-message/
//! turn-holds.service.ts`, `a8214b253` bug 262, at `236c04e35`).
//!
//! Both grids are computed on both sides (never transcribed): `resolve` over
//! `isPaused` × `turnMode` × `continueMode` × whisper targets ×
//! `neverPauseForUser`; `qualify` over `continueMode` ×
//! `respondingParticipantId` × targets × `nudge` × `content` × `hold` × five
//! participant rosters. An absent key and the JSON value map onto v5's
//! `SendMessageOptions` exactly as the spine's request decode does.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   npx tsx $V5W/harness/oracle/cases/turn-holds.ts \
//!     > /tmp/oracle-turn-holds.ndjson
//! Run:
//!   QT_ORACLE_TURN_HOLDS=/tmp/oracle-turn-holds.ndjson \
//!     cargo test -p quilltap-harness --test turn_holds_equivalence

use quilltap_core::services::orchestrator::SendMessageOptions;
use quilltap_core::services::turn_holds::{
    qualifies_for_fair_rotation_check, resolve_turn_hold, TurnHold,
};
use serde_json::Value;

fn options_of(o: &Value) -> SendMessageOptions {
    SendMessageOptions {
        continue_mode: o.get("continueMode").and_then(Value::as_bool) == Some(true),
        content: o
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        responding_participant_id: o
            .get("respondingParticipantId")
            .and_then(Value::as_str)
            .map(str::to_string),
        target_participant_ids: o
            .get("targetParticipantIds")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            }),
        never_pause_for_user: o.get("neverPauseForUser").and_then(Value::as_bool) == Some(true),
        nudge: o.get("nudge").and_then(Value::as_bool),
        ..Default::default()
    }
}

fn hold_str(h: Option<TurnHold>) -> Value {
    match h {
        Some(h) => Value::String(h.as_str().to_string()),
        None => Value::Null,
    }
}

fn hold_of(v: &Value) -> Option<TurnHold> {
    match v.as_str() {
        Some("paused") => Some(TurnHold::Paused),
        Some("storyteller") => Some(TurnHold::Storyteller),
        None => None,
        Some(other) => panic!("unknown hold {other}"),
    }
}

#[test]
fn turn_holds_equivalence() {
    let Ok(path) = std::env::var("QT_ORACLE_TURN_HOLDS") else {
        eprintln!("QT_ORACLE_TURN_HOLDS not set; skipping");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read the oracle NDJSON");
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    let mut held = std::collections::BTreeMap::<String, usize>::new();
    let mut failures = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("parse an oracle row");
        let kind = row["kind"].as_str().unwrap().to_string();
        let id = row["id"].as_str().unwrap().to_string();
        *counts.entry(kind.clone()).or_default() += 1;
        let options = options_of(&row["options"]);
        let got = match kind.as_str() {
            "resolve" => hold_str(resolve_turn_hold(&row["chat"], &options)),
            "qualify" => {
                let participants = row["participants"].as_array().cloned().unwrap();
                Value::Bool(qualifies_for_fair_rotation_check(
                    &participants,
                    &options,
                    hold_of(&row["hold"]),
                ))
            }
            other => panic!("unknown kind {other}"),
        };
        *held.entry(format!("{kind}={got}")).or_default() += 1;
        if got != row["out"] {
            failures.push(format!("{id}: v5 {got} != v4 {}", row["out"]));
        }
    }
    eprintln!("turn_holds_equivalence counts: {counts:?} outcomes: {held:?}");
    // Vacuity guards: both grids arrived whole, and every outcome is reached.
    assert_eq!(counts.get("resolve"), Some(&432), "{counts:?}");
    assert_eq!(counts.get("qualify"), Some(&2430), "{counts:?}");
    for outcome in [
        "resolve=\"paused\"",
        "resolve=\"storyteller\"",
        "resolve=null",
        "qualify=true",
        "qualify=false",
    ] {
        assert!(held.contains_key(outcome), "no {outcome} row: {held:?}");
    }
    assert!(
        failures.is_empty(),
        "{} divergence(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
