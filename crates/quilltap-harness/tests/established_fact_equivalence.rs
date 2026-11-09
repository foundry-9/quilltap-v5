//! Tier-1 differential: `lib/chat/established-fact.ts` (P4.D372, v4
//! `db6190b44`, pin `624d0d25c`) against `quilltap_core::established_fact`.
//!
//! The oracle drives v4's REAL pure module over a fixed corpus (see the case
//! header): the exported constants, the three predicates + `establishedFactKindOf`
//! over the full field grid, `frameEstablishedFact` (both kinds × both voices,
//! and the re-frame), `stripEstablishedFactFrame`, `framedEstablishedFactBody`,
//! `frameEstablishedFacts` (with v4's same-array identity as `same`) and
//! `countEstablishedFacts`. Every comparand is exact; arrays are compared as
//! serialized BYTES so a key-order difference from v4's spread is a red
//! (`serde_json::Map` equality ignores order).
//!
//! Red-first is a MUTATION run (the module is new; P4.D372 item 1): each mutant
//! below was applied to `established_fact.rs`, this family run, and the
//! flipped row recorded in the lane record.
//!
//! Generate the oracle (Node 24, from the pinned v4 worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   $N/npx tsx $V5W/harness/oracle/cases/established-fact.ts \
//!     > /tmp/oracle-established-fact.ndjson
//! Run:
//!   QT_ORACLE_ESTABLISHED_FACT=/tmp/oracle-established-fact.ndjson \
//!     cargo test -p quilltap-harness --test established_fact_equivalence -- --nocapture

use quilltap_core::established_fact::{
    count_established_facts, established_fact_kind_of, frame_established_fact,
    frame_established_facts, framed_established_fact_body_of_value, is_established_fact,
    is_established_fact_announcement, is_established_fact_seat_line, strip_established_fact_frame,
    EstablishedFactFields, EstablishedFactKind, EstablishedFactVoice, ESTABLISHED_FACT_HOST_FRAME,
    ESTABLISHED_FACT_KIND, ESTABLISHED_FACT_PLAIN_FRAME, ESTABLISHED_FACT_REMINDER_HEADER,
    ESTABLISHED_FACT_SEAT_FRAME, ESTABLISHED_FACT_SPEAKER_LABEL, ESTABLISHED_FACT_TRANSCRIPT_RULE,
};
use serde_json::Value;

fn voice(v: &Value) -> EstablishedFactVoice {
    match v.as_str() {
        Some("host") => EstablishedFactVoice::Host,
        Some("plain") => EstablishedFactVoice::Plain,
        other => panic!("bad voice {other:?}"),
    }
}

fn kind(v: &Value) -> EstablishedFactKind {
    match v.as_str() {
        Some("seat") => EstablishedFactKind::Seat,
        Some("announcement") => EstablishedFactKind::Announcement,
        other => panic!("bad kind {other:?}"),
    }
}

#[test]
fn established_fact_matches_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_ESTABLISHED_FACT") else {
        eprintln!("SKIP: QT_ORACLE_ESTABLISHED_FACT unset");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("ndjson row"))
        .collect();
    assert!(rows.len() > 200, "oracle too short: {} rows", rows.len());

    let mut failures = Vec::new();
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for (i, r) in rows.iter().enumerate() {
        let op = r["op"].as_str().expect("op");
        *counts.entry(op.to_string()).or_default() += 1;
        let mut check = |what: &str, got: Value, want: &Value| {
            let (g, w) = (got.to_string(), want.to_string());
            if g != w {
                failures.push(format!("row {i} {op} {what}: v5 {g} vs v4 {w}\n  row: {r}"));
            }
        };
        match op {
            "constants" => {
                check("kind", ESTABLISHED_FACT_KIND.into(), &r["kind"]);
                check(
                    "speakerLabel",
                    ESTABLISHED_FACT_SPEAKER_LABEL.into(),
                    &r["speakerLabel"],
                );
                check(
                    "seatFrame",
                    ESTABLISHED_FACT_SEAT_FRAME.into(),
                    &r["seatFrame"],
                );
                check(
                    "hostFrame",
                    ESTABLISHED_FACT_HOST_FRAME.into(),
                    &r["hostFrame"],
                );
                check(
                    "plainFrame",
                    ESTABLISHED_FACT_PLAIN_FRAME.into(),
                    &r["plainFrame"],
                );
                check(
                    "reminderHeader",
                    ESTABLISHED_FACT_REMINDER_HEADER.into(),
                    &r["reminderHeader"],
                );
                check(
                    "transcriptRule",
                    ESTABLISHED_FACT_TRANSCRIPT_RULE.into(),
                    &r["transcriptRule"],
                );
            }
            "predicates" => {
                let f = EstablishedFactFields::from_value(&r["input"]);
                check(
                    "announcement",
                    is_established_fact_announcement(&f).into(),
                    &r["announcement"],
                );
                check(
                    "seatLine",
                    is_established_fact_seat_line(&f).into(),
                    &r["seatLine"],
                );
                check("isFact", is_established_fact(&f).into(), &r["isFact"]);
                let k = established_fact_kind_of(&f).map_or(Value::Null, |k| k.as_str().into());
                check("kind", k, &r["kind"]);
            }
            "frame" => {
                let out = frame_established_fact(
                    r["body"].as_str().unwrap(),
                    kind(&r["kind"]),
                    voice(&r["voice"]),
                );
                check("out", out.into(), &r["out"]);
            }
            "strip" => {
                check(
                    "out",
                    strip_established_fact_frame(r["body"].as_str().unwrap()).into(),
                    &r["out"],
                );
            }
            "framedBody" => {
                let out = framed_established_fact_body_of_value(&r["input"], voice(&r["voice"]));
                check("out", out.map_or(Value::Null, Value::String), &r["out"]);
            }
            "frameAll" => {
                let input = r["input"].as_array().unwrap().clone();
                let got = frame_established_facts(&input, voice(&r["voice"]));
                check("same", got.is_none().into(), &r["same"]);
                let out = got.unwrap_or(input);
                check("out", Value::Array(out), &r["out"]);
            }
            "count" => {
                let input = r["input"].as_array().unwrap();
                let (seat, ann) =
                    count_established_facts(input.iter().map(EstablishedFactFields::from_value));
                check("seat", seat.into(), &r["seat"]);
                check("announcement", ann.into(), &r["announcement"]);
            }
            other => panic!("unknown op {other}"),
        }
    }
    eprintln!(
        "established_fact_equivalence: {} rows {counts:?}",
        rows.len()
    );
    assert!(
        counts.get("predicates") == Some(&90),
        "the field grid is 90 rows: {counts:?}"
    );
    assert!(
        failures.is_empty(),
        "{} divergences:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
