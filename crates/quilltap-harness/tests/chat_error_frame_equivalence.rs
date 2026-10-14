//! P4.D301 tier-1 differential: the chat stream's ERROR frame (v4
//! `lib/services/chat-message/streaming.service.ts` `encodeErrorEvent` +
//! `ErrorEventExtras`, v4 `93c5af6ad` — bug 241) vs the
//! `quilltap_core::api::ChatErrorPayload` the Salon spine
//! (`quilltap-host/src/spine.rs`, `run_send`'s two error shells) puts on the
//! Event channel.
//!
//! Both sides read the SAME corpus (`harness/oracle/fixtures/chat-error-frame
//! .json`). v4 emits the frame BODY (the bytes between `data: ` and `\n\n`);
//! this side builds the payload the spine would build for each row and compares
//! `serde_json::to_string` byte for byte — never `Value ==`, which is blind to
//! key order (memory note `serde-json-map-eq-ignores-order`), and key order is
//! the point: v4 spreads the extras AFTER `details`, and an absent or
//! `undefined` extra is NO key (the three-key frame every caller but the send
//! shell still sends). v5 has no `undefined`: the corpus's `"__UNDEFINED__"`
//! sentinel and a missing key are both `None`, and `skip_serializing_if` drops
//! the key — the `undefined-extras` / `empty-extras` / `*-explicit-undefined`
//! rows are what prove that is v4's answer too.
//!
//! Regenerate + run (a pure tsx oracle; the corpus is committed):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -f /tmp/oracle-chat-error-frame.ndjson
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/encode-error-event.ts \
//!     > /tmp/oracle-chat-error-frame.ndjson
//!   cd $V5W
//!   QT_ORACLE_CHAT_ERROR_FRAME=/tmp/oracle-chat-error-frame.ndjson \
//!     cargo test -p quilltap-harness --test chat_error_frame_equivalence -- --nocapture

use std::collections::HashMap;

use quilltap_core::api::ChatErrorPayload;
use serde_json::Value;

fn corpus_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-error-frame.json")
}

/// One flag as v5 can hold it: `true` / `false` → `Some`, the `undefined`
/// sentinel or a missing key → `None`.
fn flag(extras: &serde_json::Map<String, Value>, key: &str) -> Option<bool> {
    extras.get(key).and_then(Value::as_bool)
}

/// The payload the spine builds for one corpus row.
fn payload_for(case: &Value) -> ChatErrorPayload {
    let s = |k: &str| case[k].as_str().expect(k).to_string();
    let (user, tools) = match &case["extras"] {
        Value::Object(extras) => (
            flag(extras, "userMessagePersisted"),
            flag(extras, "pendingToolResultsPersisted"),
        ),
        Value::String(sentinel) => {
            assert!(
                sentinel == "__ABSENT__" || sentinel == "__UNDEFINED__",
                "unknown extras sentinel {sentinel}"
            );
            (None, None)
        }
        other => panic!("bad extras {other}"),
    };
    ChatErrorPayload {
        error: s("error"),
        error_type: s("errorType"),
        details: s("details"),
        user_message_persisted: user,
        pending_tool_results_persisted: tools,
    }
}

#[test]
fn chat_error_frames_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_CHAT_ERROR_FRAME") else {
        eprintln!("SKIP: set QT_ORACLE_CHAT_ERROR_FRAME to the oracle NDJSON (see header).");
        return;
    };
    let body = std::fs::read_to_string(&oracle_path).expect("read oracle");
    let corpus: Value =
        serde_json::from_str(&std::fs::read_to_string(corpus_path()).expect("read corpus"))
            .expect("parse corpus");
    let cases = corpus["cases"].as_array().expect("cases");

    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in body.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("oracle row");
        assert_eq!(row["prefix"], "data: ", "v4's frame prefix moved");
        assert_eq!(row["suffix"], "\n\n", "v4's frame suffix moved");
        oracle.insert(row["name"].as_str().unwrap().to_string(), row);
    }
    assert_eq!(
        oracle.len(),
        cases.len(),
        "oracle row count != corpus case count — an empty or stale NDJSON?"
    );

    let mut failures = Vec::new();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let want = oracle[name]["body"].as_str().expect("body").to_string();
        let got = serde_json::to_string(&payload_for(case)).expect("serialize");
        if got != want {
            failures.push(format!("{name}:\n  v4 {want}\n  v5 {got}"));
        }
    }
    eprintln!(
        "chat_error_frame: {} of {} rows equal",
        cases.len() - failures.len(),
        cases.len()
    );
    assert!(
        failures.is_empty(),
        "frame bytes differ:\n{}",
        failures.join("\n")
    );
}

/// The identities the corpus exists to pin, asserted on v4's own bytes as well
/// as diffed — "both sides agree" and "both sides agree AND the rule holds" are
/// different claims.
#[test]
fn chat_error_frame_identities_hold_in_the_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_CHAT_ERROR_FRAME") else {
        eprintln!("SKIP: set QT_ORACLE_CHAT_ERROR_FRAME to the oracle NDJSON (see header).");
        return;
    };
    let body = std::fs::read_to_string(&oracle_path).expect("read oracle");
    let by: HashMap<String, String> = body
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let row: Value = serde_json::from_str(l).unwrap();
            (
                row["name"].as_str().unwrap().to_string(),
                row["body"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    // An omitted, an `undefined` and an empty extras argument are ONE frame.
    assert_eq!(by["no-extras"], by["undefined-extras"]);
    assert!(!by["empty-extras"].contains("Persisted"));
    // An explicit-undefined flag is no key.
    assert!(!by["user-explicit-undefined"].contains("userMessagePersisted"));
    assert!(!by["both-explicit-undefined"].contains("Persisted"));
    // The extras follow `details`, user before tools.
    let both = &by["both-false"];
    let d = both.find("\"details\"").unwrap();
    let u = both.find("\"userMessagePersisted\"").unwrap();
    let t = both.find("\"pendingToolResultsPersisted\"").unwrap();
    assert!(d < u && u < t, "v4 key order: {both}");
}
