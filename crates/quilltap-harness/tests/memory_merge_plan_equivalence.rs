//! Tier-1 differential test (P4.D266 U1, v4 `3f7320138` F3 / F9 at the pin
//! `01a83539d`): the pure memory-merge plan + the reinforcement footnote cap —
//! `quilltap_core::memory_merge::plan_memory_merge` and
//! `quilltap_core::memory_gate::{append_capped_footnotes,
//! count_reinforcement_footnotes, MAX_REINFORCEMENT_FOOTNOTES}` against v4's
//! REAL `planMemoryMerge` / `appendCappedFootnotes` /
//! `countReinforcementFootnotes`.
//!
//! The plan's `patch` is compared BYTE-WISE through `serde_json::to_string`
//! (a `preserve_order` map's `==` is order-blind — v4's key order is part of
//! the comparand). Rows: 0 / 7 / 8 / 9 existing footnotes × 0 / 1 / 3 details,
//! `\r\n` / no-space / indented footnote lines, two losers sharing a detail,
//! counts absent / null / 0 / 3, link unions with duplicates / an excluded id
//! / a loser's link to the survivor / an order-only difference, `occurredAt`
//! unparseable / empty / date-only / offset / ties, and `lastReinforcedAt`
//! where the UTF-16 string max and the date max disagree (incl. an astral vs
//! U+E000–U+FFFF pair).
//!
//! Generate the oracle output:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   PATH=$N:$PATH npx tsx $V5W/harness/oracle/cases/memory-merge-plan.ts \
//!     > /tmp/oracle-memory-merge-plan.ndjson
//! Run:
//!   QT_ORACLE_MEMORY_MERGE_PLAN=/tmp/oracle-memory-merge-plan.ndjson \
//!     cargo test -p quilltap-harness --test memory_merge_plan_equivalence -- --nocapture

use quilltap_core::memory_gate::{
    append_capped_footnotes, count_reinforcement_footnotes, MAX_REINFORCEMENT_FOOTNOTES,
};
use quilltap_core::memory_merge::plan_memory_merge;
use serde_json::Value;

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn memory_merge_plan_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_MEMORY_MERGE_PLAN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_MEMORY_MERGE_PLAN to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let (mut constants, mut appends, mut plans) = (0usize, 0usize, 0usize);
    let mut failures: Vec<String> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let id = row["id"].as_str().unwrap().to_string();
        match row["kind"].as_str().unwrap() {
            "constant" => {
                assert_eq!(
                    row["out"].as_u64().unwrap() as usize,
                    MAX_REINFORCEMENT_FOOTNOTES,
                    "{id}"
                );
                constants += 1;
            }
            "append" => {
                let content = row["content"].as_str().unwrap();
                let details = strings(&row["details"]);
                let count = count_reinforcement_footnotes(content);
                if count as u64 != row["count"].as_u64().unwrap() {
                    failures.push(format!("{id}: count {count} vs {}", row["count"]));
                }
                let (got_content, got_appended) = append_capped_footnotes(content, &details);
                let out = &row["out"];
                if got_content != out["content"].as_str().unwrap()
                    || got_appended != strings(&out["appended"])
                {
                    failures.push(format!(
                        "{id}: rust ({got_content:?}, {got_appended:?}) vs oracle {out}"
                    ));
                }
                appends += 1;
            }
            "plan" => {
                let survivor = &row["survivor"];
                let losers: Vec<Value> = row["losers"].as_array().unwrap().clone();
                let exclude = strings(&row["exclude"]);
                let plan = plan_memory_merge(survivor, &losers, &exclude);
                let out = &row["out"];
                let got_patch = serde_json::to_string(&plan.patch).unwrap();
                let want_patch = serde_json::to_string(&out["patch"]).unwrap();
                if plan.survivor_id != out["survivorId"].as_str().unwrap()
                    || got_patch != want_patch
                    || plan.merged_details != strings(&out["mergedDetails"])
                    || plan.content_changed != out["contentChanged"].as_bool().unwrap()
                {
                    failures.push(format!(
                        "{id}:\n  rust:   patch={got_patch} merged={:?} changed={}\n  oracle: patch={want_patch} merged={} changed={}",
                        plan.merged_details,
                        plan.content_changed,
                        out["mergedDetails"],
                        out["contentChanged"]
                    ));
                }
                plans += 1;
            }
            other => panic!("unknown row kind {other}"),
        }
    }

    assert!(
        constants == 1 && appends > 0 && plans > 0,
        "oracle file looks incomplete: {constants}/{appends}/{plans}"
    );
    assert!(failures.is_empty(), "diverged:\n{}", failures.join("\n"));
    eprintln!(
        "OK: memory-merge-plan matched oracle ({constants} constant, {appends} append, {plans} plan rows)."
    );
}
