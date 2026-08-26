//! P4.D123 tier-1 static-table differential: the two activity tables v4
//! `664cfca84` introduced, diffed against v4's REAL exports.
//!
//! * `activity_kinds` — v4's `ACTIVITY_KINDS`, in order (the wire key order).
//! * `job_type_activity` — v4's `JOB_TYPE_ACTIVITY`, entry order included.
//! * `background_job_type_enum` — v4's `BackgroundJobTypeEnum.options`. v4 gets
//!   `JOB_TYPE_ACTIVITY`'s TOTALITY from the type system
//!   (`Record<BackgroundJobType, …>`); v5's job types are strings, so this arm
//!   re-derives the same property against v4's actual enum — both directions —
//!   and against v5's own enqueue gate.
//! * `task_type_activity` — v4's cheap-LLM `TASK_TYPE_ACTIVITY` (module-private
//!   in v4; the oracle reads it strictly out of the source, see that header).
//!
//! `ACTIVITY_CHIPS` is deliberately absent: client-only display metadata, which
//! the Angular half (P4.D125) transcribes.
//!
//! Regenerate + run (self-contained; a pure tsx oracle, no fixture):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -f /tmp/oracle-activity-tables.ndjson
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/activity-tables.ts \
//!     > /tmp/oracle-activity-tables.ndjson
//!   cd $V5W
//!   QT_ORACLE_ACTIVITY_TABLES=/tmp/oracle-activity-tables.ndjson \
//!     cargo test -p quilltap-harness --test activity_tables_equivalence -- --nocapture

use std::collections::{BTreeSet, HashMap};

use quilltap_core::api::system_data::JOB_TYPES;
use quilltap_core::services::activity_kinds::{
    activity_kind_for_job_type, ACTIVITY_KINDS, JOB_TYPE_ACTIVITY,
};
use quilltap_core::services::cheap_llm_exec::TASK_TYPE_ACTIVITY;
use serde_json::{json, Value};

fn load(path: &str) -> HashMap<String, Value> {
    let mut out = HashMap::new();
    for line in std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read oracle {path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        out.insert(v["name"].as_str().unwrap().to_string(), v["value"].clone());
    }
    out
}

#[test]
fn activity_tables_match_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_ACTIVITY_TABLES") else {
        eprintln!("SKIP: set QT_ORACLE_ACTIVITY_TABLES to the oracle NDJSON (see header).");
        return;
    };
    let oracle = load(&path);
    let mut failed: Vec<String> = Vec::new();
    let mut ran = 0usize;

    // --- ACTIVITY_KINDS, in order ------------------------------------------
    {
        let expected = oracle
            .get("activity_kinds")
            .unwrap_or_else(|| panic!("oracle missing activity_kinds"));
        let got: Value = ACTIVITY_KINDS.iter().map(|k| k.as_str()).collect();
        if &got != expected {
            failed.push(format!("activity_kinds: {got} != {expected}"));
        }
        ran += 1;
    }

    // --- JOB_TYPE_ACTIVITY, entry order included ---------------------------
    {
        let expected = oracle
            .get("job_type_activity")
            .unwrap_or_else(|| panic!("oracle missing job_type_activity"));
        let got: Value = JOB_TYPE_ACTIVITY
            .iter()
            .map(|(t, kind)| {
                json!({
                    "type": t,
                    "kind": kind.map(|k| k.as_str()),
                })
            })
            .collect();
        if &got != expected {
            failed.push(format!(
                "job_type_activity:\n  rust:   {got}\n  oracle: {expected}"
            ));
        }
        ran += 1;
    }

    // --- totality against v4's REAL enum, both directions -------------------
    {
        let enum_types: BTreeSet<String> = oracle
            .get("background_job_type_enum")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("oracle missing background_job_type_enum"))
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        let table: BTreeSet<String> = JOB_TYPE_ACTIVITY
            .iter()
            .map(|(t, _)| (*t).to_string())
            .collect();
        if table != enum_types {
            failed.push(format!(
                "job_type_activity totality: table {table:?} != v4 enum {enum_types:?}"
            ));
        }
        // …and v5's own enqueue gate is the same set (a drift in either reds).
        let gate: BTreeSet<String> = JOB_TYPES.iter().map(|t| (*t).to_string()).collect();
        if gate != enum_types {
            failed.push(format!(
                "v5 enqueue gate JOB_TYPES {gate:?} != v4 enum {enum_types:?}"
            ));
        }
        // The tolerant lookup answers for every enum member and refuses beyond.
        for t in &enum_types {
            let mapped = JOB_TYPE_ACTIVITY.iter().find(|(k, _)| k == t);
            if mapped.is_none() {
                failed.push(format!("job type {t} unmapped"));
            }
        }
        if activity_kind_for_job_type("SOME_FUTURE_JOB").is_some() {
            failed.push("unknown job type must map to no kind".to_string());
        }
        ran += 1;
    }

    // --- TASK_TYPE_ACTIVITY, entry order included --------------------------
    {
        let expected = oracle
            .get("task_type_activity")
            .unwrap_or_else(|| panic!("oracle missing task_type_activity"));
        let got: Value = TASK_TYPE_ACTIVITY
            .iter()
            .map(|(t, kind)| json!({ "taskType": t, "kind": kind.as_str() }))
            .collect();
        if &got != expected {
            failed.push(format!(
                "task_type_activity:\n  rust:   {got}\n  oracle: {expected}"
            ));
        }
        ran += 1;
    }

    assert!(
        failed.is_empty(),
        "{} arm(s) failed:\n{}",
        failed.len(),
        failed.join("\n")
    );
    assert_eq!(ran, 4, "expected 4 arms to run, ran {ran}");
}
