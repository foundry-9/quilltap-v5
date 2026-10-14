//! Tier-1 differential: JS `Date.parse` — V8 13.6.233.17 inside Node 24.13.1,
//! the runtime v4 runs on — vs v5's whole-parser port
//! (`quilltap_core::v8_date_parse`, reached through
//! `memory_scoring::date_ms` → `episodic::js_date_parse_ms`). P4.D289, the
//! human's 2026-10-14 ruling on the `JS_DATE_PARSE_ISO_SUBSET` finding: port
//! V8's parser.
//!
//! The case emits ~150 hand-picked strings (ES5 ISO incl. the expanded year
//! and the 24:00 rule; every legacy shape; the units V8 treats specially) and
//! 20,000 SEEDED fuzz strings of token soup, each with `Date.parse`'s answer
//! (epoch ms or `"NaN"`). Every row must agree exactly — a NaN only with a
//! `None`. The case runs under `TZ=UTC`, where V8's local-time reading of an
//! offset-less string equals v5's UTC reading (`ZONELESS_PARSE_AS_UTC`).
//!
//! Generate the oracle output, then run:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/js-date-parse.ts" > /tmp/oracle-js-date-parse.ndjson
//!   cd "$V5W"
//!   QT_ORACLE_JS_DATE_PARSE=/tmp/oracle-js-date-parse.ndjson \
//!     cargo test -p quilltap-harness --test js_date_parse_equivalence -- --nocapture

use quilltap_core::memory_scoring::date_ms;
use serde_json::Value;

#[test]
fn date_parse_matches_v8() {
    let path = match std::env::var("QT_ORACLE_JS_DATE_PARSE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_JS_DATE_PARSE to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let (mut hand, mut fuzz, mut finite) = (0usize, 0usize, 0usize);
    let mut bad: Vec<String> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let s = row["s"].as_str().unwrap();
        match row["kind"].as_str().unwrap() {
            "hand" => hand += 1,
            "fuzz" => fuzz += 1,
            other => panic!("unknown row kind {other}"),
        }
        let want = match &row["out"] {
            Value::Number(n) => Some(n.as_f64().unwrap()),
            Value::String(t) if t == "NaN" => None,
            other => panic!("bad out {other}"),
        };
        let got = date_ms(s);
        let got = (!got.is_nan()).then_some(got);
        if want.is_some() {
            finite += 1;
        }
        if got != want {
            bad.push(format!("  {s:?}: v8={want:?} v5={got:?}"));
        }
    }
    assert!(
        bad.is_empty(),
        "{} of {} rows diverge from V8's Date.parse:\n{}",
        bad.len(),
        hand + fuzz,
        bad.iter().take(60).cloned().collect::<Vec<_>>().join("\n")
    );
    assert_eq!(fuzz, 20_000, "the fuzz corpus drifted");
    assert!(hand >= 150, "the hand corpus shrank: {hand}");
    assert!(
        finite > 3_000,
        "too few finite rows ({finite}) — the corpus lost its legacy reach"
    );
    eprintln!("OK: js-date-parse matched V8 on {hand} hand + {fuzz} fuzz rows ({finite} finite).");
}
