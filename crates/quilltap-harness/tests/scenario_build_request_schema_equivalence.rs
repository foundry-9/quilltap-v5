//! Tier-1 differential: the Scenario Builder request body — v4
//! `scenarioBuildRequestSchema` (`lib/scenario-builder/request-schema.ts`,
//! `d1c06cd9d`) against v5's hand-written Zod twin
//! `quilltap_core::services::scenario_builder::request_schema`.
//!
//! Exact, per row, on BOTH halves of `safeParse`: the parsed OUTPUT (key order
//! included — absent optionals omitted, explicit nulls kept, the trims and the
//! two defaults applied) or the ISSUE LIST (each issue in Zod's own per-code
//! key order, the list in Zod's order). The corpus asks every field absent /
//! null / wrong type / at its boundary ±1, the refine's four quadrants plus
//! which field issues ABORT it, JS-`trim()` vs Rust-`trim()` characters, Zod
//! ≥ 4.5's code-point length rule, non-object bodies and stripped unknown keys.
//!
//! ⚠ PIN REQUIRED at the TARGET `d1c06cd9d`: the schema module does not exist
//! at the `00c290c9a` baseline, so a baseline-pinned run fails to IMPORT — that
//! failure is the pin verification. Regen from the checkout once the baseline
//! has moved past `d1c06cd9d`; until then point the driver at a pin
//! (`--v4 "$PIN"`).
//!
//! Regenerate + run (self-contained; a pure tsx oracle, no fixture):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -f /tmp/oracle-scenario-build-request-schema.ndjson
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/scenario-build-request-schema.ts \
//!     > /tmp/oracle-scenario-build-request-schema.ndjson
//!   cd $V5W
//!   QT_ORACLE_SCENARIO_BUILD_REQUEST_SCHEMA=/tmp/oracle-scenario-build-request-schema.ndjson \
//!     cargo test -p quilltap-harness --test scenario_build_request_schema_equivalence -- --nocapture

use quilltap_core::services::scenario_builder::request_schema::parse_scenario_build_request;
use serde_json::Value;

#[test]
fn scenario_build_request_schema_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_SCENARIO_BUILD_REQUEST_SCHEMA") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_SCENARIO_BUILD_REQUEST_SCHEMA to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    assert!(
        !text.trim().is_empty(),
        "{path} is EMPTY — a failed regen truncates its redirect"
    );

    let (mut ok_rows, mut issue_rows, mut failures) = (0usize, 0usize, Vec::<String>::new());
    // The shapes the corpus must still ask, so a trimmed regen cannot go green.
    let mut saw_custom_with_field_issue = false;
    let mut saw_aborted_refine = false;
    let mut saw_array_too_big = false;
    let mut saw_root_type = false;

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("oracle row is JSON");
        let id = row["id"].as_str().expect("id").to_string();
        let body = &row["body"];
        let got = parse_scenario_build_request(body);
        match (row.get("data"), row.get("issues")) {
            (Some(want), None) => {
                ok_rows += 1;
                match got {
                    Ok(parsed) => {
                        let (g, w) = (parsed.to_value().to_string(), want.to_string());
                        if g != w {
                            failures.push(format!("{id}: OUTPUT differs\n  v4: {w}\n  v5: {g}"));
                        }
                    }
                    Err(issues) => failures.push(format!(
                        "{id}: v4 PARSED, v5 refused with {}",
                        serde_json::to_string(&issues).unwrap()
                    )),
                }
            }
            (None, Some(want)) => {
                issue_rows += 1;
                let codes: Vec<&str> = want
                    .as_array()
                    .expect("issues array")
                    .iter()
                    .map(|i| i["code"].as_str().unwrap_or(""))
                    .collect();
                if codes.len() > 1 && codes.last() == Some(&"custom") {
                    saw_custom_with_field_issue = true;
                }
                if id.starts_with("refine-fails-plus-invalid-type") && !codes.contains(&"custom") {
                    saw_aborted_refine = true;
                }
                if want
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|i| i["origin"] == "array")
                {
                    saw_array_too_big = true;
                }
                if want.as_array().unwrap().iter().any(|i| {
                    i["path"].as_array().is_some_and(|p| p.is_empty())
                        && i["code"] == "invalid_type"
                }) {
                    saw_root_type = true;
                }
                match got {
                    Err(issues) => {
                        let (g, w) = (serde_json::to_string(&issues).unwrap(), want.to_string());
                        if g != w {
                            failures.push(format!("{id}: ISSUES differ\n  v4: {w}\n  v5: {g}"));
                        }
                    }
                    Ok(parsed) => {
                        failures.push(format!("{id}: v4 REFUSED, v5 parsed {}", parsed.to_value()))
                    }
                }
            }
            _ => panic!("{id}: a row carries exactly one of `data` / `issues`"),
        }
    }

    eprintln!("scenario_build_request_schema: {ok_rows} parsed rows, {issue_rows} refused rows");
    assert!(
        ok_rows + issue_rows >= 30,
        "the corpus shrank below the order's 30 bodies"
    );
    assert!(
        saw_custom_with_field_issue,
        "no row pins a continuable field issue beside the refine"
    );
    assert!(
        saw_aborted_refine,
        "no row pins an ABORTING issue skipping the refine"
    );
    assert!(saw_array_too_big, "no row pins the array's own size issue");
    assert!(saw_root_type, "no row pins a non-object body");
    assert!(
        failures.is_empty(),
        "{} row(s) differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
