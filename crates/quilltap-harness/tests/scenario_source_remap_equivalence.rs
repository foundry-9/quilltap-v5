//! Tier-1 differential test — P4.D288 (C1-11):
//! `quilltap_core::services::carriers::scenario_source_remap` against v4's
//! REAL `lib/scenarios/source-remap.ts` (`545e299bb`) at the `83334b9a5` pin —
//! `read_scenario_source` over an object, a JSON string, nothing (`null`, an
//! absent key, `''`), malformed text, non-objects, and `ScenarioSourceSchema`'s
//! edges (a 65- / 63-char hash, an empty / 500 / 501-unit path, an astral path
//! either side of the 500 boundary — Zod ≥ 4.5 counts code points past the
//! unit budget — a bad uuid, an extra key stripped); then
//! `remap_scenario_source` under a mount map (mapped, unmapped → `None`, mapped
//! to `''` → `None`). Answers compared byte-for-byte (key order included).
//!
//! Regenerate (Node 24, from the round's pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/scenario-source-remap.ts > /tmp/oracle-scenario-source-remap.ndjson
//! Run:
//!   QT_ORACLE_SCENARIO_SOURCE_REMAP=/tmp/oracle-scenario-source-remap.ndjson \
//!     cargo test -p quilltap-harness --test scenario_source_remap_equivalence

use quilltap_core::services::carriers::scenario_source_remap::{
    read_scenario_source, remap_scenario_source,
};
use serde_json::Value;

fn answer(s: Option<quilltap_core::db::chats::ScenarioSource>) -> String {
    s.map(|s| s.to_json_text())
        .unwrap_or_else(|| "null".to_string())
}

#[test]
fn scenario_source_remap_matches_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_SCENARIO_SOURCE_REMAP") else {
        eprintln!("SKIP: set QT_ORACLE_SCENARIO_SOURCE_REMAP to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let (mut reads, mut remaps) = (0usize, 0usize);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let case: Value = serde_json::from_str(line).expect("oracle line");
        let label = case["label"].as_str().unwrap();
        let value = case.get("value").cloned().unwrap_or(Value::Null);
        let got = match case["case"].as_str().unwrap() {
            "read" => {
                reads += 1;
                answer(read_scenario_source(&value))
            }
            "remap" => {
                remaps += 1;
                let map = case["map"].as_object().unwrap().clone();
                answer(remap_scenario_source(&value, |id| {
                    map.get(id).and_then(Value::as_str).map(str::to_string)
                }))
            }
            other => panic!("unknown case {other}"),
        };
        assert_eq!(got, case["result"].to_string(), "{label}");
    }
    assert_eq!((reads, remaps), (20, 15), "the oracle's case counts");
    eprintln!("OK: scenario source remap matched v4 ({reads} reads, {remaps} remaps).");
}
