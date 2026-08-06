//! P4.D37 (v4 `424a7381`) tier-1 EXACT differential: the pure ad-hoc-announcement
//! speaker-attribution leaves (`quilltap_core::announcement_attribution`) vs v4's
//! REAL `lib/chat/context/announcement-attribution.ts`.
//!
//! Three kinds of record, all driven off the SAME NDJSON so the inputs cannot
//! drift apart: `resolve` (the name resolution, incl. the deliberate "return null
//! rather than invent a name" arm), `collect` (the up-front id gather), and
//! `attribute` (the whole output message array — so "every other field survives"
//! is DIFFED, not asserted, and the idempotency arm re-runs the pass on its own
//! output exactly as the oracle does).
//!
//! Coverage is asserted by shape: every record in the NDJSON must be consumed and
//! every kind must be non-empty (`harness-corpus-shape-constants-rot`).
//!
//! Generate the oracle (Node 24, from the v4 checkout):
//!   cd ~/source/quilltap-server
//!   ~/.nvm/versions/node/v24.13.1/bin/npx tsx \
//!     <v5-worktree>/harness/oracle/cases/announcement-attribution.ts \
//!     > /tmp/oracle-announcement-attribution.ndjson
//! Run:
//!   QT_ORACLE_ANNOUNCEMENT_ATTRIBUTION=/tmp/oracle-announcement-attribution.ndjson \
//!     cargo test -p quilltap-harness --test announcement_attribution_equivalence -- --nocapture

use std::collections::HashMap;

use quilltap_core::announcement_attribution::{
    attribute_adhoc_announcements, collect_announcer_character_ids, resolve_announcer_name,
    AnnouncerAttributable, CustomAnnouncer,
};
use serde_json::{json, Value};

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

/// The oracle's `names` pairs → the caller's up-front lookup map.
fn name_map(v: &Value) -> HashMap<String, String> {
    v.as_array()
        .expect("names is an array of [id, name] pairs")
        .iter()
        .map(|pair| {
            (
                pair[0].as_str().unwrap().to_string(),
                pair[1].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn announcement_attribution_matches_oracle() {
    let Some(path) = env_or_skip("QT_ORACLE_ANNOUNCEMENT_ATTRIBUTION") else {
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap();
    let records: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(
        !records.is_empty(),
        "the oracle NDJSON is empty — regenerate it"
    );

    let mut failed: Vec<String> = Vec::new();
    let mut counts: HashMap<&str, usize> = HashMap::new();

    for rec in &records {
        let kind = rec["kind"].as_str().expect("kind");
        let name = rec["name"].as_str().expect("name");
        *counts.entry(kind).or_default() += 1;

        let (got, want): (Value, Value) = match kind {
            "resolve" => {
                let announcer = CustomAnnouncer::from_value(Some(&rec["announcer"]));
                // The staff fallback (bug 28): the record carries an optional
                // `systemSender` (null when absent).
                let system_sender = rec["systemSender"].as_str();
                let out =
                    resolve_announcer_name(announcer, &name_map(&rec["names"]), system_sender);
                (json!(out), rec["output"].clone())
            }
            "collect" => {
                let messages: Vec<Value> = rec["messages"].as_array().unwrap().clone();
                (
                    json!(collect_announcer_character_ids(&messages)),
                    rec["output"].clone(),
                )
            }
            "attribute" => {
                let messages: Vec<Value> = rec["messages"].as_array().unwrap().clone();
                let names = name_map(&rec["names"]);
                let once = attribute_adhoc_announcements(&messages, &names);
                // The idempotency arm re-runs the pass on its OWN output, exactly
                // as the oracle does — a retry or a regenerate must not stack tags.
                let out = if rec["twice"].as_bool().unwrap_or(false) {
                    attribute_adhoc_announcements(&once, &names)
                } else {
                    once
                };
                (json!(out), rec["output"].clone())
            }
            other => panic!("unknown oracle record kind {other}"),
        };

        if got != want {
            eprintln!("[{kind}/{name}] MISMATCH\n  GOT : {got}\n  WANT: {want}");
            failed.push(format!("{kind}/{name}"));
        } else {
            eprintln!("[{kind}/{name}] OK.");
        }
    }

    assert!(
        failed.is_empty(),
        "announcement-attribution FAILED: {failed:?}"
    );
    for kind in ["resolve", "collect", "attribute"] {
        assert!(
            counts.get(kind).copied().unwrap_or(0) > 0,
            "the corpus lost every `{kind}` record — a truncated fixture must not pass silently"
        );
    }
    eprintln!(
        "announcement-attribution: {} records driven.",
        records.len()
    );
}

/// The trait's production carrier is `WhisperMessage`; the differential drives the
/// JSON impl. Keep the two honest about the one field the pass rewrites.
#[test]
fn value_carrier_spread_replaces_only_content() {
    let m = json!({ "id": "m1", "role": "ASSISTANT", "content": "hi" });
    let out = m.with_content("[X] hi".to_string());
    assert_eq!(out["content"], json!("[X] hi"));
    assert_eq!(out["id"], json!("m1"));
    assert_eq!(out["role"], json!("ASSISTANT"));
}
