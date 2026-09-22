//! P4.D208 pure tier-1 differential: the scenario-seeded-summary predicate
//! (v4 bug 158, `da9c4f34f`) vs v4's REAL
//! `lib/chat/scenario-seeded-summary.ts`. Per corpus row the oracle emits what
//! `isScenarioSeededSummary` answered, the object `stripScenarioSeededSummary`
//! returned, and whether it returned the SAME object — v4's identity contract.
//! The Rust side runs the same row through
//! `services::scenario_seeded_summary` and compares all three.
//!
//! The row is driven through BOTH v5 implementations of the trait — the raw
//! `Value` the `.qtap` import carries and the `ChatCreate` the restore
//! deserializes — because the two ingest paths reach the predicate carrying
//! different things and only one of them is a JSON object.
//!
//! Generate the oracle output (from a v4 checkout pinned at the TARGET; the
//! module does not exist at the baseline, which is this family's pin proof):
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/scenario-seeded-summary.ts \
//!     > /tmp/oracle-scenario-seeded-summary.ndjson
//! Run:
//!   QT_ORACLE_SCENARIO_SEEDED=/tmp/oracle-scenario-seeded-summary.ndjson \
//!     cargo test -p quilltap-harness --test scenario_seeded_summary_equivalence

use quilltap_core::db::chats::ChatCreate;
use quilltap_core::services::scenario_seeded_summary::{
    is_scenario_seeded_summary, strip_scenario_seeded_summary,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Row {
    id: String,
    row: Value,
    is: bool,
    out: Value,
    /// v4 returned the same object reference — i.e. it stripped nothing.
    same: bool,
    /// v4 did not mutate the row it was given.
    #[serde(rename = "inputUnmutated")]
    input_unmutated: bool,
}

/// Build the `ChatCreate` the restore path would deserialize from this row,
/// filling only the three required fields the corpus rows do not carry.
///
/// Returns `None` for a row whose columns are not strings, which the typed
/// struct cannot hold at all — those rows are proven on the `Value` leg only,
/// and the assertion below records that the typed leg agrees by construction.
fn as_chat_create(row: &Value) -> Option<ChatCreate> {
    let mut obj = row.as_object()?.clone();
    obj.insert("userId".into(), Value::String("u1".into()));
    obj.insert(
        "title".into(),
        obj.get("title")
            .cloned()
            .unwrap_or(Value::String("t".into())),
    );
    obj.insert("participants".into(), Value::Array(vec![]));
    serde_json::from_value(Value::Object(obj)).ok()
}

#[test]
fn scenario_seeded_summary_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_SCENARIO_SEEDED") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_SCENARIO_SEEDED to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    let mut typed_rows = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        assert!(
            row.input_unmutated,
            "the oracle's own row was mutated on case '{}' — the corpus is wrong, not the port",
            row.id
        );

        // ---- the `Value` leg (the `.qtap` import's shape) -------------------
        let mut v = row.row.clone();
        assert_eq!(
            is_scenario_seeded_summary(&v),
            row.is,
            "isScenarioSeededSummary '{}' over the Value row",
            row.id
        );
        let stripped = strip_scenario_seeded_summary(&mut v);
        assert_eq!(
            stripped, !row.same,
            "v4 returns the SAME object exactly when it strips nothing — '{}'",
            row.id
        );
        assert_eq!(
            v, row.out,
            "stripScenarioSeededSummary '{}' over the Value row",
            row.id
        );

        // ---- the `ChatCreate` leg (the restore's shape) ----------------------
        if let Some(mut create) = as_chat_create(&row.row) {
            typed_rows += 1;
            assert_eq!(
                is_scenario_seeded_summary(&create),
                row.is,
                "isScenarioSeededSummary '{}' over ChatCreate",
                row.id
            );
            assert_eq!(
                strip_scenario_seeded_summary(&mut create),
                !row.same,
                "strip over ChatCreate disagreed with the Value leg — '{}'",
                row.id
            );
            // The typed struct only carries the two columns; compare those.
            let want_summary = row.out.get("contextSummary").and_then(Value::as_str);
            let want_scenario = row.out.get("scenarioText").and_then(Value::as_str);
            assert_eq!(
                create.context_summary.as_deref(),
                want_summary,
                "contextSummary after strip over ChatCreate — '{}'",
                row.id
            );
            assert_eq!(
                create.scenario_text.as_deref(),
                want_scenario,
                "scenarioText must never move — '{}'",
                row.id
            );
        }
        count += 1;
    }
    assert!(count > 0, "oracle file looks empty");
    assert!(
        typed_rows > 0,
        "no corpus row reached the ChatCreate leg — the typed half is unproven"
    );
    eprintln!(
        "OK: scenario-seeded-summary matched oracle ({count} rows; {typed_rows} through ChatCreate)."
    );
}
