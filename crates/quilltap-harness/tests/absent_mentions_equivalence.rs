//! Tier-1 differential (P4.D347, v4 `9a88f862f`): the absent-character name
//! chips — `presenceOf` / `spokenText` / `findAbsentMentions`
//! (`lib/chat/absent-mentions.ts`) over v4's REAL `offSceneCardFields` and
//! `stripToolArtifacts`, against `quilltap_core::services::absent_mentions`.
//!
//! A `find` row's mentions are compared as `serde_json::to_string` BYTES — the
//! key order (`messageId, characterId, text, card{name, aliases, pronouns,
//! body}`) is the wire contract the Salon chips read — and so is the ORDER of
//! the mentions (v4 walks the pending list from its end, so two absent people
//! first named in one message come out reversed).
//!
//! Generate the oracle output (Node 24, from the pinned v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/absent-mentions.ts \
//!     > /tmp/oracle-absent-mentions.ndjson
//! Run:
//!   QT_ORACLE_ABSENT_MENTIONS=/tmp/oracle-absent-mentions.ndjson \
//!     cargo test -p quilltap-harness --test absent_mentions_equivalence -- --nocapture

use quilltap_core::services::absent_mentions::{find_absent_mentions, presence_of, spoken_text};
use quilltap_core::services::off_scene::OffSceneCharacterCard;
use serde_json::Value;

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

fn participants(v: &Value) -> Vec<Value> {
    v.get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

#[test]
fn absent_mentions_match_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_ABSENT_MENTIONS") else {
        eprintln!("SKIP: set QT_ORACLE_ABSENT_MENTIONS to the oracle NDJSON (see test header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut failures = Vec::new();
    let mut counts = [0usize; 3];
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let id = s(&row, "id").unwrap();
        let chat_type = s(&row, "chatType");
        let persona = s(&row, "personaName");
        let (got, want) = match row["kind"].as_str().unwrap() {
            "presence" => {
                counts[0] += 1;
                let p = presence_of(
                    &participants(&row),
                    chat_type.as_deref(),
                    persona.as_deref(),
                );
                let got: Vec<bool> = row["probes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| p.is_in_room(c["id"].as_str().unwrap(), c["name"].as_str().unwrap()))
                    .collect();
                (
                    serde_json::to_string(&got).unwrap(),
                    serde_json::to_string(&row["out"]).unwrap(),
                )
            }
            "spoken" => {
                counts[1] += 1;
                let i = &row["input"];
                let got = spoken_text(
                    i.get("type").and_then(Value::as_str),
                    i.get("role").and_then(Value::as_str),
                    i.get("content").and_then(Value::as_str),
                    i.get("systemSender").and_then(Value::as_str),
                );
                (
                    serde_json::to_string(&got).unwrap(),
                    serde_json::to_string(&row["out"]).unwrap(),
                )
            }
            "find" => {
                counts[2] += 1;
                let candidates: Vec<OffSceneCharacterCard> = row["candidates"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| OffSceneCharacterCard::from_value(c).expect("candidate"))
                    .collect();
                let p = presence_of(
                    &participants(&row),
                    chat_type.as_deref(),
                    persona.as_deref(),
                );
                let got = find_absent_mentions(
                    row["messages"].as_array().unwrap(),
                    &candidates,
                    &p,
                    row.get("userCharacterName").and_then(Value::as_str),
                );
                (
                    serde_json::to_string(&got).unwrap(),
                    serde_json::to_string(&row["out"]).unwrap(),
                )
            }
            other => panic!("unknown kind {other}"),
        };
        if got != want {
            failures.push(format!("{id}:\n   v4 {want}\n   v5 {got}"));
        }
    }
    assert!(
        counts[0] >= 8 && counts[1] >= 10 && counts[2] >= 12,
        "oracle looks partial: {counts:?}"
    );
    assert!(
        failures.is_empty(),
        "{} rows differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!(
        "OK: absent-mentions matched oracle ({} presence, {} spoken, {} find rows).",
        counts[0], counts[1], counts[2]
    );
}
