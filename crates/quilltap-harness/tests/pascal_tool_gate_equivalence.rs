//! Tier-1 differential (P4.D333, v4 `a53024d3c`): the availability GATE over
//! any-of clauses, driven DIRECTLY — `evaluate_tool_gate` and
//! `gate_names_progress` against v4's real `evaluateToolGate` /
//! `gateNamesProgress`.
//!
//! A verdict is compared as its serialized BYTES: `heldGroup` is present only
//! for an any-of clause (the group that OPENED an `availableWhen` or CLOSED a
//! `withheldWhen`), absent on an `availableWhen` miss and for every
//! bare-object clause, in the key order `available, withheldBy, heldGroup`.
//! The definition family's `gate` rows prove the same through the committed
//! corpus; this is the regen-cheap direct drive that stands alone, and the only
//! home of `gateNamesProgress` — the roster's "derive the progress sheet?"
//! test, which since `a53024d3c` counts a group's `progress` KEYS.
//!
//! Generate the oracle output (v4 @ 322a5488e, Node 24
//! `~/.nvm/versions/node/v24.13.1/bin`):
//!   cd ~/source/quilltap-server
//!   TZ=UTC npx tsx \
//!     <V5W>/harness/oracle/cases/pascal-tool-gate.ts \
//!     > /tmp/oracle-pascal-tool-gate.ndjson
//! Run:
//!   QT_ORACLE_PASCAL_TOOL_GATE=/tmp/oracle-pascal-tool-gate.ndjson \
//!     cargo test -p quilltap-harness --test pascal_tool_gate_equivalence

use quilltap_core::pascal::custom_tool_types::safe_parse;
use quilltap_core::pascal::tool_gate::{evaluate_tool_gate, gate_names_progress};
use serde_json::{Map, Value};

fn load(id: &str, input_json: &str) -> quilltap_core::pascal::custom_tool_types::QtapCustomTool {
    let raw: Value = serde_json::from_str(input_json).expect("oracle input is JSON");
    safe_parse(&raw).unwrap_or_else(|e| panic!("case '{id}': definition does not load: {e:?}"))
}

#[test]
fn pascal_tool_gate_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PASCAL_TOOL_GATE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PASCAL_TOOL_GATE to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let (mut gates, mut held_groups, mut names, mut names_false) = (0usize, 0usize, 0usize, 0usize);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let id = row["id"].as_str().unwrap();
        let tool = load(id, row["inputJson"].as_str().unwrap());
        match row["kind"].as_str().unwrap() {
            "gate" => {
                let metadata: Option<Map<String, Value>> = row["metadata"].as_object().cloned();
                let progress: Option<Map<String, Value>> =
                    row.get("progress").and_then(Value::as_object).cloned();
                let got = serde_json::to_string(&evaluate_tool_gate(
                    &tool,
                    metadata.as_ref(),
                    progress.as_ref(),
                    // P4.D329's 4th positional: this family's rows pose no bag.
                    None,
                ))
                .unwrap();
                let want = row["verdict"].as_str().unwrap();
                assert_eq!(
                    got, want,
                    "gate case '{id}': verdict differs (heldGroup's absence included)"
                );
                held_groups += usize::from(want.contains("\"heldGroup\""));
                gates += 1;
            }
            "namesProgress" => {
                let want = row["namesProgress"].as_bool().unwrap();
                assert_eq!(
                    gate_names_progress(tool.available_when.as_ref()),
                    want,
                    "gateNamesProgress '{id}'"
                );
                names_false += usize::from(!want);
                names += 1;
            }
            other => panic!("unknown row kind '{other}'"),
        }
    }

    assert!(
        gates > 0 && names > 0,
        "oracle file looks empty ({gates} gates, {names} names)"
    );
    assert!(
        held_groups >= 4,
        "only {held_groups} verdicts carry heldGroup"
    );
    assert!(
        names_false >= 2,
        "only {names_false} gateNamesProgress rows answer false"
    );
    eprintln!("OK: tool gate matched oracle ({gates} verdicts, {names} gateNamesProgress rows).");
}
