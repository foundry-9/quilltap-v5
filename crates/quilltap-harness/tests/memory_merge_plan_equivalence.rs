//! Tier-1 differential (P4.D266 U1; RE-AIMED at P4.D289, pin `83334b9a5`):
//! the merge corpus through v4's ONE COMBINE + the reinforcement footnote cap
//! — `quilltap_core::admission::{fold_into, FoldSource::from_value,
//! FOLD_CAPS}` and `quilltap_core::reinforcement_footnotes::{
//! append_capped_footnotes, count_reinforcement_footnotes,
//! MAX_REINFORCEMENT_FOOTNOTES}` against v4's REAL `foldInto` /
//! `appendCappedFootnotes` / `countReinforcementFootnotes`.
//!
//! v4 `8af764514` deleted `planMemoryMerge` (`lib/memory/memory-merge.ts`) and
//! the footnote helpers' old home (`lib/memory/memory-gate.ts`); every merge
//! is now `foldInto` (the losers passed as whole `Memory` rows, the
//! maintenance fold's shape).
//!
//! Red-first record (the re-aimed oracle against the UNCHANGED test, which
//! read `planMemoryMerge`'s `patch` / `mergedDetails` / `contentChanged`): the
//! unchanged test PANICKED at the new `fold-caps` constant row; measured
//! per row through the unchanged `plan_memory_merge` /
//! 2-argument `append_capped_footnotes`, the 17 `append` rows and the
//! `max-footnotes` constant stayed green, and 32 of 34 `plan` rows were RED —
//! every one on the PATCH alone (footnotes agreed on every row): the deleted
//! merge's write semantics. Retired BY MEASUREMENT, v4's fold result now the
//! expectation: the absolute `reinforcementCount` + recomputed
//! `reinforcedImportance` in the patch (all 32 — bug 226: a fold carries a
//! count DELTA), `content` in the patch (4 — the fold returns `content` +
//! `footnotes` outside its anchor-only patch), the whole-array
//! `relatedMemoryIds` rewrite (`links-union-minus-group`,
//! `links-survivor-duplicates`, `all-fields` — the fold's `addLinks` never
//! removes), `lastReinforcedAt` in the patch (`lra-string-vs-date`,
//! `lra-bmp-vs-astral`, `lra-survivor-null`, `all-fields` — now
//! `reinforcedAt`). `no-losers` and `count-zero` were green on both. The
//! `links-excluded` row (the merge's `exclude` batch) was dropped: a fold has
//! no exclude set — scrubbing links to rows retired in the same pass is v4
//! maintenance's `retireMemories` (P4.D297's).
//!
//! Every plan field compares; the `patch` compares as JSON BYTES
//! (`serde_json::to_string` vs v4's `JSON.stringify` — key order is part of
//! the comparand) and as `Object.keys` order.
//!
//! Generate the oracle output, then run:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/memory-merge-plan.ts" > /tmp/oracle-memory-merge-plan.ndjson
//!   cd "$V5W"
//!   QT_ORACLE_MEMORY_MERGE_PLAN=/tmp/oracle-memory-merge-plan.ndjson \
//!     cargo test -p quilltap-harness --test memory_merge_plan_equivalence -- --nocapture

use quilltap_core::admission::{fold_into, FoldSource, FOLD_CAPS};
use quilltap_core::reinforcement_footnotes::{
    append_capped_footnotes, count_reinforcement_footnotes, MAX_REINFORCEMENT_FOOTNOTES,
};
use serde_json::Value;

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

fn num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap(),
        Value::String(s) => match s.as_str() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            other => panic!("bad number token {other:?}"),
        },
        other => panic!("not a number: {other}"),
    }
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
                let out = &row["out"];
                let ok = match id.as_str() {
                    "max-footnotes" => {
                        out.as_u64().unwrap() as usize == MAX_REINFORCEMENT_FOOTNOTES
                    }
                    "fold-caps" => {
                        out["footnoteCap"].as_u64().unwrap() as usize == FOLD_CAPS.footnote_cap
                            && out["entityCap"].as_u64().unwrap() as usize == FOLD_CAPS.entity_cap
                    }
                    other => panic!("unknown constant {other}"),
                };
                if !ok {
                    failures.push(format!("{id}: oracle {out}"));
                }
                constants += 1;
            }
            "append" => {
                let content = row["content"].as_str().unwrap();
                let details = strings(&row["details"]);
                let count = count_reinforcement_footnotes(content);
                if count as u64 != row["count"].as_u64().unwrap() {
                    failures.push(format!("{id}: count {count} vs {}", row["count"]));
                }
                // v4's omitted `cap` is its default, MAX_REINFORCEMENT_FOOTNOTES.
                let (got_content, got_appended) =
                    append_capped_footnotes(content, &details, MAX_REINFORCEMENT_FOOTNOTES);
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
                let losers: Vec<FoldSource> = row["losers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(FoldSource::from_value)
                    .collect();
                let plan = fold_into(&row["survivor"], &losers, &FOLD_CAPS);
                let out = &row["out"];
                let got_patch = serde_json::to_string(&plan.patch).unwrap();
                let want_delta = num(&out["reinforcementDelta"]);
                let delta_ok = if want_delta.is_nan() {
                    plan.reinforcement_delta.is_nan()
                } else {
                    (plan.reinforcement_delta - want_delta).abs() <= 1e-12
                };
                if plan.survivor_id != out["survivorId"].as_str().unwrap()
                    || plan.footnotes != strings(&out["footnotes"])
                    || plan.content != out["content"].as_str().unwrap()
                    || plan.add_links != strings(&out["addLinks"])
                    || got_patch != out["patchJson"].as_str().unwrap()
                    || plan.patch.keys() != strings(&out["patchKeys"])
                    || !delta_ok
                    || plan.reinforced_at.as_deref() != out["reinforcedAt"].as_str()
                    || plan.at_cap != out["atCap"].as_bool().unwrap()
                    || plan.reembed != out["reembed"].as_bool().unwrap()
                {
                    failures.push(format!(
                        "{id}:\n  rust:   footnotes={:?} content={:?} addLinks={:?} patch={got_patch} delta={} reinforcedAt={:?} atCap={} reembed={}\n  oracle: {out}",
                        plan.footnotes,
                        plan.content,
                        plan.add_links,
                        plan.reinforcement_delta,
                        plan.reinforced_at,
                        plan.at_cap,
                        plan.reembed
                    ));
                }
                plans += 1;
            }
            other => panic!("unknown row kind {other}"),
        }
    }

    assert!(
        failures.is_empty(),
        "diverged ({}):\n{}",
        failures.len(),
        failures.join("\n")
    );
    // Corpus drift guard: the case's exact row counts.
    assert_eq!(
        (constants, appends, plans),
        (2, 17, 34),
        "row counts (constant, append, plan)"
    );
    eprintln!(
        "OK: memory-merge-plan matched oracle ({constants} constant, {appends} append, {plans} plan rows)."
    );
}
