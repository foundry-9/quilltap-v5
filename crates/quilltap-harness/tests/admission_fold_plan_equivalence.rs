//! Tier-1 differential: v4's ONE COMBINE at the pin (`83334b9a5`) —
//! `crate::admission::{fold_into, FoldSource::from_value, FOLD_CAPS}` (P4.D289
//! unit 7, C1-A item 6) against v4's REAL `foldInto` / `FOLD_CAPS`
//! (`lib/memory/admission/fold.ts`).
//!
//! Rows: the nine `foldInto` rows of `admission-fold.test.ts`, the
//! growing-content judge, the footnote cap with and without an anchor change
//! (bug 204 — an anchor change re-embeds AT the cap), entities
//! case-insensitive survivor-first ≤ the cap and grown past the anchor line's
//! four (O13 — `reembed` fires; faithful, do not "fix"), blank / non-string
//! entities, `narrativeTime` fill, `latest` (UTF-16 string max; `??` keeps an
//! empty `lastReinforcedAt` from falling back to `createdAt`), the delta over
//! fractional / zero / negative / NaN / infinite / null / string counts (bug
//! 226 — a DELTA, never an absolute), links (never removed), `occurredAt`
//! edges, custom caps. Every plan field compares; the `patch` compares as
//! JSON BYTES (`serde_json::to_string` vs v4's `JSON.stringify` — key order is
//! part of the comparand) and as `Object.keys` order. Floats at 1e-12 (a NaN
//! delta only matches a NaN).
//!
//! Non-finite numbers ride as strings (`"NaN"`, `"Infinity"`).
//!
//! Generate the oracle output, then run:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/admission-fold-plan.ts" > /tmp/oracle-admission-fold-plan.ndjson
//!   cd "$V5W"
//!   QT_ORACLE_ADMISSION_FOLD_PLAN=/tmp/oracle-admission-fold-plan.ndjson \
//!     cargo test -p quilltap-harness --test admission_fold_plan_equivalence -- --nocapture

use quilltap_core::admission::{fold_into, FoldCaps, FoldPlan, FoldSource, FOLD_CAPS};
use serde_json::Value;

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

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

fn same_num(got: f64, want: f64) -> bool {
    if want.is_nan() {
        got.is_nan()
    } else if want.is_infinite() {
        got == want
    } else {
        (got - want).abs() <= 1e-12
    }
}

/// Every plan field against the oracle's `out`; `None` when they agree.
fn compare(plan: &FoldPlan, out: &Value) -> Option<String> {
    let got_patch = serde_json::to_string(&plan.patch).unwrap();
    let ok = plan.survivor_id == out["survivorId"].as_str().unwrap()
        && plan.footnotes == strings(&out["footnotes"])
        && plan.content == out["content"].as_str().unwrap()
        && plan.add_links == strings(&out["addLinks"])
        && got_patch == out["patchJson"].as_str().unwrap()
        && plan.patch.keys() == strings(&out["patchKeys"])
        && same_num(plan.reinforcement_delta, num(&out["reinforcementDelta"]))
        && plan.reinforced_at.as_deref() == out["reinforcedAt"].as_str()
        && plan.at_cap == out["atCap"].as_bool().unwrap()
        && plan.reembed == out["reembed"].as_bool().unwrap();
    if ok {
        None
    } else {
        Some(format!(
            "rust: survivor={} footnotes={:?} content={:?} addLinks={:?} patch={got_patch} keys={:?} delta={} reinforcedAt={:?} atCap={} reembed={}\n  oracle: {out}",
            plan.survivor_id,
            plan.footnotes,
            plan.content,
            plan.add_links,
            plan.patch.keys(),
            plan.reinforcement_delta,
            plan.reinforced_at,
            plan.at_cap,
            plan.reembed
        ))
    }
}

#[test]
fn admission_fold_plan_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_ADMISSION_FOLD_PLAN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_ADMISSION_FOLD_PLAN to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let (mut constants, mut folds) = (0usize, 0usize);
    let mut failures: Vec<String> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let id = row["id"].as_str().unwrap().to_string();
        match row["kind"].as_str().unwrap() {
            "constant" => {
                let out = &row["out"];
                if FOLD_CAPS.footnote_cap as u64 != out["footnoteCap"].as_u64().unwrap()
                    || FOLD_CAPS.entity_cap as u64 != out["entityCap"].as_u64().unwrap()
                {
                    failures.push(format!("{id}: rust {FOLD_CAPS:?} vs oracle {out}"));
                }
                constants += 1;
            }
            "fold" => {
                let caps = FoldCaps {
                    footnote_cap: row["caps"]["footnoteCap"].as_u64().unwrap() as usize,
                    entity_cap: row["caps"]["entityCap"].as_u64().unwrap() as usize,
                };
                let sources: Vec<FoldSource> = row["sources"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(FoldSource::from_value)
                    .collect();
                let plan = fold_into(&row["survivor"], &sources, &caps);
                if let Some(diff) = compare(&plan, &row["out"]) {
                    failures.push(format!("{id}:\n  {diff}"));
                }
                folds += 1;
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
    assert_eq!((constants, folds), (1, 50), "row counts (constant, fold)");
    eprintln!("OK: admission-fold-plan matched oracle ({constants} constant, {folds} fold rows).");
}
