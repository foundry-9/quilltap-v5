//! P4.D124 tier-1 differential: the realtime topic computation, diffed against
//! v4's REAL `lib/realtime/job-topics.ts` (`f3892158d`).
//!
//! The corpus lives ONCE, in the oracle: each NDJSON row carries the case's
//! INPUT alongside v4's hints, and this test reads the input back out and runs
//! `topics_for_completed_job` / `topics_for_write_batch` on it. Nothing is
//! transcribed, so the two sides cannot drift
//! (`blinded-comparand-hides-the-new-arm.md`).
//!
//! ⚠ The work order expected the write-batch leg to need a PAIRED corpus,
//! because v5's buffered writes were assumed to be typed. **Refuted by
//! measurement:** `write_partition::ChildWritePayload` is v4's `{method, args}`
//! verbatim — the Phase-2 partition port kept that representation deliberately
//! (it is the correctness property, not a Node workaround) — so one corpus
//! drives both sides directly.
//!
//! Regenerate + run (self-contained; a pure tsx oracle, no fixture):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -f /tmp/oracle-realtime-topics.ndjson
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/realtime-topics.ts \
//!     > /tmp/oracle-realtime-topics.ndjson
//!   cd $V5W
//!   QT_ORACLE_REALTIME_TOPICS=/tmp/oracle-realtime-topics.ndjson \
//!     cargo test -p quilltap-harness --test realtime_topics_equivalence -- --nocapture

use quilltap_core::realtime::job_topics::{
    topics_for_completed_job, topics_for_write_batch, TopicHint,
};
use quilltap_core::write_partition::ChildWritePayload;
use serde_json::{json, Value};

/// v4's hints as JSON: `{topic}` or `{topic, id}` — an unreadable id is
/// `undefined`, which `JSON.stringify` DROPS, so the key is absent rather than
/// null.
fn hints_json(hints: &[TopicHint]) -> Value {
    Value::Array(
        hints
            .iter()
            .map(|h| match &h.id {
                Some(id) => json!({ "topic": h.topic.as_str(), "id": id }),
                None => json!({ "topic": h.topic.as_str() }),
            })
            .collect(),
    )
}

#[test]
fn realtime_topics_match_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_REALTIME_TOPICS") else {
        eprintln!("SKIP: set QT_ORACLE_REALTIME_TOPICS to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read oracle {path}: {e}"));

    let mut failed: Vec<String> = Vec::new();
    let mut completed = 0usize;
    let mut batch = 0usize;

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("parse oracle line");
        let name = row["name"].as_str().unwrap().to_string();
        let expected = &row["output"];
        let input = &row["input"];

        let got = match row["kind"].as_str().unwrap() {
            "completed" => {
                completed += 1;
                // An ABSENT `jobType` key is v4's `undefined` argument.
                let job_type = input.get("jobType").and_then(Value::as_str);
                let payload = input.get("payload");
                hints_json(&topics_for_completed_job(job_type, payload))
            }
            "batch" => {
                batch += 1;
                let writes: Vec<ChildWritePayload> = input["writes"]
                    .as_array()
                    .expect("writes array")
                    .iter()
                    .map(|w| ChildWritePayload {
                        method: w["method"].as_str().expect("method").to_string(),
                        // An absent `args` key is v4's `undefined` args.
                        args: w
                            .get("args")
                            .and_then(Value::as_array)
                            .cloned()
                            .unwrap_or_default(),
                    })
                    .collect();
                hints_json(&topics_for_write_batch(&writes))
            }
            other => panic!("unknown case kind {other}"),
        };

        if &got != expected {
            failed.push(format!(
                "{name}:\n  rust:   {got}\n  oracle: {expected}\n  input:  {input}"
            ));
        } else {
            eprintln!("OK {name}");
        }
    }

    assert!(
        failed.is_empty(),
        "{} case(s) failed:\n{}",
        failed.len(),
        failed.join("\n")
    );
    // Shape assertions, not hand counts (`harness-corpus-shape-constants-rot`):
    // every job type twice (full payload + no payload) plus the targeted arms,
    // and a batch corpus that actually covers the namespaces.
    assert!(
        completed >= 2 * 23,
        "the completed corpus must drive every job type twice; got {completed}"
    );
    assert!(batch >= 10, "the write-batch corpus is too thin: {batch}");
}
