//! Tier-1 differential: `buildEstablishedFactReminder` (P4.D372, v4
//! `db6190b44`, pin `624d0d25c`) against
//! `quilltap_core::services::established_fact_reminder`.
//!
//! Each oracle row is one event list through v4's REAL function. Its seams
//! are parameters — `loadSpeakerNames` answers a fixed map and is COUNTED —
//! and v4's DEBUG fields are captured off the logger singleton. Four
//! comparands per row: the section text (exact), the loader-call count, and
//! the debug line's presence and fields (`chatId` absent when v4's input had
//! none). Events are read through `WhisperEvent::from_value`, the production
//! row read.
//!
//! Red-first is a MUTATION run (the module is new; P4.D372 item 1): cap in
//! bytes, cap dropping the newest, `>=` for `> floor`, a NaN parse kept under
//! a floor, the loader called eagerly — each flips a named row (lane record).
//! The corpus's 200-fact row is Tier 2 item 23 (v4 drops 96).
//!
//! Generate the oracle (Node 24, from the pinned v4 worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   $N/npx tsx $V5W/harness/oracle/cases/established-fact-reminder.ts \
//!     > /tmp/oracle-ef-reminder.ndjson
//! Run:
//!   QT_ORACLE_EF_REMINDER=/tmp/oracle-ef-reminder.ndjson \
//!     cargo test -p quilltap-harness --test established_fact_reminder_equivalence -- --nocapture

use quilltap_core::core_whisper::WhisperEvent;
use quilltap_core::services::established_fact_reminder::{
    build_established_fact_reminder, ReminderInput,
};
use quilltap_core::services::speaker_names::SpeakerNames;
use serde_json::Value;

/// `key=value` pairs of a captured line, after the message.
fn fields_of(line: &str) -> Vec<(String, String)> {
    let tail = line
        .split_once("[EstablishedFact] Built the since-you-last-spoke reminder")
        .map(|(_, t)| t)
        .unwrap_or("");
    tail.split_whitespace()
        .filter_map(|kv| {
            kv.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
        })
        .collect()
}

fn v4_fields(meta: &Value) -> Vec<(String, String)> {
    meta.as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| {
            let s = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            (k.clone(), s)
        })
        .collect()
}

#[test]
fn established_fact_reminder_matches_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_EF_REMINDER") else {
        eprintln!("SKIP: QT_ORACLE_EF_REMINDER unset");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(rows.len() >= 28, "oracle too short: {}", rows.len());
    let mut failures = Vec::new();
    for r in &rows {
        let label = r["label"].as_str().unwrap();
        let input = &r["input"];
        let events: Vec<WhisperEvent> = input["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(WhisperEvent::from_value)
            .collect();
        let regen: Option<Vec<String>> = input.get("regenerationOfMessageIds").map(|v| {
            v.as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap().to_string())
                .collect()
        });
        let ri = ReminderInput {
            events: &events,
            responding_participant_id: input["respondingParticipantId"].as_str().unwrap(),
            joined_at: input.get("joinedAt").and_then(Value::as_str),
            has_history_access: input
                .get("hasHistoryAccess")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            regeneration_of_message_ids: regen.as_deref(),
            chat_id: input.get("chatId").and_then(Value::as_str),
        };
        let mut calls = 0usize;
        let (got, lines) = quilltap_core::test_support::captured_with(|| {
            build_established_fact_reminder(&ri, || {
                calls += 1;
                Ok(SpeakerNames::from_pairs([
                    ("p-user".to_string(), "Captain".to_string()),
                    ("p-a".to_string(), "Alice".to_string()),
                ]))
            })
            .expect("no loader error in the corpus")
        });
        if got != r["text"].as_str().unwrap() {
            failures.push(format!("{label}: text\n  v5 {got:?}\n  v4 {:?}", r["text"]));
        }
        if calls as u64 != r["loaderCalls"].as_u64().unwrap() {
            failures.push(format!(
                "{label}: loaderCalls v5 {calls} v4 {}",
                r["loaderCalls"]
            ));
        }
        let want_lines = r["lines"].as_array().unwrap();
        let got_lines: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("[EstablishedFact] Built the since-you-last-spoke reminder"))
            .collect();
        if got_lines.len() != want_lines.len() {
            failures.push(format!(
                "{label}: line count v5 {} v4 {}",
                got_lines.len(),
                want_lines.len()
            ));
        } else if let (Some(g), Some(w)) = (got_lines.first(), want_lines.first()) {
            if fields_of(g) != v4_fields(w) {
                failures.push(format!(
                    "{label}: line fields\n  v5 {:?}\n  v4 {:?}",
                    fields_of(g),
                    v4_fields(w)
                ));
            }
        }
    }
    eprintln!("established_fact_reminder_equivalence: {} rows", rows.len());
    assert!(
        failures.is_empty(),
        "{} divergences:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
