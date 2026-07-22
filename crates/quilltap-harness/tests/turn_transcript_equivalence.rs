//! Tier-1 differential test: the **per-turn transcript builder**
//! (`quilltap_core::services::turn_transcript` — v4
//! `lib/services/chat-message/turn-transcript.ts`, P4.6bj unit 1).
//!
//! Both sides read the SAME committed corpus
//! (`harness/oracle/fixtures/turn-transcript.json`); the oracle drives v4's
//! REAL `buildTurnTranscript` + `findTurnOpenerMessageId` and records the raw
//! `JSON.stringify` of each result (undefined keys dropped); this test drives
//! the Rust port and rebuilds the same presence shape (the three
//! `userCharacter*` passthrough keys mirror the fixture options' key
//! presence; `isUserControlled` appears only when true — v4 never sets it on
//! AI slices), then compares as JSON values, exact.
//!
//! Generate the oracle (from the v4 checkout):
//!   cd ~/source/quilltap-server
//!   npx tsx <v5>/harness/oracle/cases/turn-transcript.ts > /tmp/oracle-turn-transcript.ndjson
//! Run:
//!   QT_ORACLE_TURN_TRANSCRIPT=/tmp/oracle-turn-transcript.ndjson \
//!     cargo test -p quilltap-harness --test turn_transcript_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::memory_format::Pronouns;
use quilltap_core::memory_tasks::{TurnCharacterSlice, TurnTranscript};
use quilltap_core::services::turn_transcript::{
    build_turn_transcript, find_turn_opener_message_id, BuildTurnTranscriptOptions,
};
use serde_json::{json, Map, Value};

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/turn-transcript.json")
}

fn pronouns_from_option(v: Option<&Value>) -> Option<Pronouns> {
    let p = v?;
    Some(Pronouns {
        subject: p.get("subject")?.as_str()?.to_string(),
        object: p.get("object")?.as_str()?.to_string(),
        possessive: p.get("possessive")?.as_str()?.to_string(),
    })
}

fn pronouns_to_value(p: Option<&Pronouns>) -> Value {
    match p {
        None => Value::Null,
        Some(p) => json!({
            "subject": p.subject,
            "object": p.object,
            "possessive": p.possessive,
        }),
    }
}

fn opt_str(v: Option<&String>) -> Value {
    v.map(|s| Value::from(s.clone())).unwrap_or(Value::Null)
}

fn slice_to_value(s: &TurnCharacterSlice) -> Value {
    let mut obj = Map::new();
    obj.insert("characterId".into(), Value::from(s.character_id.clone()));
    obj.insert(
        "characterName".into(),
        Value::from(s.character_name.clone()),
    );
    obj.insert(
        "characterPronouns".into(),
        pronouns_to_value(s.character_pronouns.as_ref()),
    );
    obj.insert("text".into(), Value::from(s.text.clone()));
    obj.insert(
        "contributingMessageIds".into(),
        Value::from(s.contributing_message_ids.clone()),
    );
    obj.insert(
        "lastMessageCreatedAt".into(),
        opt_str(s.last_message_created_at.as_ref()),
    );
    // v4 sets `isUserControlled` only on the promoted opener slice; the AI
    // slices leave it undefined (dropped by JSON.stringify).
    if s.is_user_controlled {
        obj.insert("isUserControlled".into(), Value::from(true));
    }
    Value::Object(obj)
}

/// Rebuild v4's `JSON.stringify(transcript)` presence shape. The three
/// `userCharacter*` keys are verbatim option passthroughs in v4, so their
/// presence mirrors the fixture options' key presence (`undefined` dropped,
/// explicit `null` kept).
fn transcript_to_value(t: &TurnTranscript, options: &Value) -> Value {
    let mut obj = Map::new();
    obj.insert(
        "turnOpenerMessageId".into(),
        opt_str(t.turn_opener_message_id.as_ref()),
    );
    obj.insert("userMessage".into(), opt_str(t.user_message.as_ref()));
    if options.get("userCharacterId").is_some() {
        obj.insert(
            "userCharacterId".into(),
            opt_str(t.user_character_id.as_ref()),
        );
    }
    if options.get("userCharacterName").is_some() {
        obj.insert(
            "userCharacterName".into(),
            opt_str(t.user_character_name.as_ref()),
        );
    }
    if options.get("userCharacterPronouns").is_some() {
        obj.insert(
            "userCharacterPronouns".into(),
            pronouns_to_value(t.user_character_pronouns.as_ref()),
        );
    }
    obj.insert(
        "characterSlices".into(),
        Value::from(
            t.character_slices
                .iter()
                .map(slice_to_value)
                .collect::<Vec<_>>(),
        ),
    );
    obj.insert(
        "latestAssistantMessageId".into(),
        opt_str(t.latest_assistant_message_id.as_ref()),
    );
    obj.insert("turnTimestamp".into(), opt_str(t.turn_timestamp.as_ref()));
    Value::Object(obj)
}

#[test]
fn turn_transcript_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_TURN_TRANSCRIPT") else {
        eprintln!("QT_ORACLE_TURN_TRANSCRIPT not set — SKIP");
        return;
    };
    let oracle_raw = std::fs::read_to_string(&oracle_path).expect("read oracle NDJSON");
    let mut oracle_rows: HashMap<String, Value> = HashMap::new();
    for line in oracle_raw.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("oracle line JSON");
        let id = row["id"].as_str().expect("oracle row id").to_string();
        oracle_rows.insert(id, row);
    }

    let corpus: Value =
        serde_json::from_str(&std::fs::read_to_string(corpus_path()).expect("read corpus"))
            .expect("corpus JSON");
    let characters: HashMap<String, Value> = corpus["characters"]
        .as_object()
        .expect("characters map")
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let participants: Vec<Value> = corpus["participants"]
        .as_array()
        .expect("participants")
        .clone();
    let cases = corpus["cases"].as_array().expect("cases");
    assert_eq!(
        cases.len(),
        oracle_rows.len(),
        "corpus/oracle case-count mismatch — regenerate the oracle"
    );

    for case in cases {
        let id = case["id"].as_str().expect("case id");
        let messages: Vec<Value> = case["messages"].as_array().expect("messages").clone();
        let options_json = &case["options"];
        let options = BuildTurnTranscriptOptions {
            turn_opener_message_id: options_json
                .get("turnOpenerMessageId")
                .and_then(Value::as_str)
                .map(String::from),
            extraction_anchor_message_id: options_json
                .get("extractionAnchorMessageId")
                .and_then(Value::as_str)
                .map(String::from),
            user_character_id: options_json
                .get("userCharacterId")
                .and_then(Value::as_str)
                .map(String::from),
            user_character_name: options_json
                .get("userCharacterName")
                .and_then(Value::as_str)
                .map(String::from),
            user_character_pronouns: pronouns_from_option(
                options_json
                    .get("userCharacterPronouns")
                    .filter(|v| !v.is_null()),
            ),
        };

        let want = oracle_rows
            .get(id)
            .unwrap_or_else(|| panic!("{id}: missing from oracle — regenerate"));

        let got_opener = find_turn_opener_message_id(&messages);
        assert_eq!(
            opt_str(got_opener.as_ref()),
            want["opener"],
            "{id}: findTurnOpenerMessageId diverges"
        );

        let transcript = build_turn_transcript(&messages, &participants, &characters, &options);
        let got = transcript_to_value(&transcript, options_json);
        assert_eq!(
            got, want["transcript"],
            "{id}: transcript diverges\n got: {got:#}\nwant: {:#}",
            want["transcript"]
        );
    }
    println!("turn_transcript: {} cases OK", cases.len());
}
