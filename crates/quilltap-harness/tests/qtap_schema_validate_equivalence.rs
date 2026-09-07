//! P4.86 tier-1 differential: `generators::qtap_schema::validate_qtap_export`
//! vs v4's REAL `validateQtapExport` (`lib/validation/qtap-schema-validator.ts`
//! — ajv 2020, `{allErrors: true, strict: false, validateFormats: true}` +
//! `ajv-formats`, over `public/schemas/qtap-export.schema.json`).
//!
//! The oracle emits the INSTANCE it validated in every row, so both sides
//! validate the identical bytes and this family needs no database: the seeds
//! are built oracle-side by v4's REAL exporter (`createNdjsonStream`) folded
//! back into document shape by v4's REAL importer (`assembleExportFromStream`)
//! over the committed `system-data-*` fixture family, the mutations are
//! JSON-Pointer edits of those, and the literals are hand-written shapes.
//!
//! ## What agrees, and what is RECORDED
//!
//! Two different engines (ajv vs the `jsonschema` crate) cannot be expected to
//! agree on prose, so the comparands are graded:
//!
//! * **`valid` — EXACT.** The decision is the contract.
//! * **the `errorSections` SET — EXACT.** v4's repair pass derives the sections
//!   it asks the model to fix by running `/^\/data\/(\w+)\//` over the error
//!   strings (`ai-import.service.ts`); if the two engines disagree here the
//!   repair prompt asks for different sections and the ported step is wrong.
//! * **the error COUNT, ORDER and message TEXT — RECORDED.** The count reaches
//!   the wire twice (the `step_error validation` frame's `N validation
//!   error(s)` and the `errors.validation` sentence), so the divergence is
//!   carried, not hidden. The table below is printed on every run and the
//!   round record quotes it.
//! * **the instance-path SET — EXACT, once ajv's `if/then` wrapper error is
//!   subtracted.** The whole measured divergence at the baseline is ONE shape:
//!   where a root `allOf[i].then` branch fails, ajv adds a root-level
//!   `/: must match "then" schema` alongside the real errors, and it duplicates
//!   errors across the fifteen `if/then` branches (the `manifest_removed` row:
//!   112 ajv errors over the same 46 the crate reports). The crate reports the
//!   underlying failures only. So: strike every v4 error whose message is
//!   exactly [`AJV_THEN_WRAPPER`], and the two path SETS are equal on every
//!   row — asserted — while v4's per-path COUNT is never below v5's, also
//!   asserted. Anything else is a real divergence and fails here.
//!
//! Regenerate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-qtap-schema-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/qtap-schema-validate.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/system-data.json" "$TMPO/fixtures/"
//!   cp "$V5W/harness/oracle/fixtures/qtap-schema-validate.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_SD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/system-data-main.db \
//!   QT_FIXTURE_SD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/system-data-mount.db \
//!   QT_FIXTURE_SD_LLM=$V5W/crates/quilltap-web/tests/fixtures/system-data-llmlogs.db \
//!   QT_ORACLE_OUT=/tmp/oracle-qtap-schema.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=300000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- qtap-schema-validate
//! Run:
//!   QT_ORACLE_QTAP_SCHEMA=/tmp/oracle-qtap-schema.ndjson \
//!     cargo test -p quilltap-harness --test qtap_schema_validate_equivalence -- --nocapture
//!
//! While v4 HEAD is past the baseline the regen needs a worktree pinned at the
//! baseline — the sweep driver's job (`recipe_sweep.py --run
//! qtap_schema_validate_equivalence --v4 <pin>`), never a path baked in here.

use std::collections::{BTreeMap, BTreeSet};

use quilltap_core::generators::qtap_schema::validate_qtap_export;
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

/// ajv's root wrapper for a failed `allOf[i].then` branch. The `jsonschema`
/// crate emits no counterpart, which is the entire measured path divergence.
const AJV_THEN_WRAPPER: &str = "must match \"then\" schema";

#[derive(Deserialize)]
struct OracleRow {
    name: String,
    kind: String,
    instance: Value,
    valid: bool,
    errors: Vec<String>,
}

/// v4 `ai-import.service.ts`'s `/^\/data\/(\w+)\//` over each error string —
/// the SET the repair pass asks the model to fix.
fn error_sections(errors: &[String]) -> BTreeSet<String> {
    let re = Regex::new(r"^/data/(\w+)/").unwrap();
    errors
        .iter()
        .filter_map(|e| re.captures(e).map(|c| c[1].to_string()))
        .collect()
}

/// The `<pointer>` half of `"<pointer>: <message>"`.
fn split_error(e: &str) -> (&str, &str) {
    e.split_once(": ").unwrap_or((e, ""))
}

/// Per-path error counts.
fn path_counts(errors: &[String]) -> BTreeMap<String, usize> {
    let mut m: BTreeMap<String, usize> = BTreeMap::new();
    for e in errors {
        *m.entry(split_error(e).0.to_string()).or_default() += 1;
    }
    m
}

#[test]
fn qtap_schema_validate_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_QTAP_SCHEMA") else {
        eprintln!("SKIP: set QT_ORACLE_QTAP_SCHEMA (see the test header).");
        return;
    };
    let text = std::fs::read_to_string(&oracle_path).unwrap();
    assert!(
        !text.trim().is_empty(),
        "{oracle_path} is EMPTY — the regen truncated it before failing (ledger §5.1)"
    );
    let rows: Vec<OracleRow> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("an oracle row"))
        .collect();

    let mut failures: Vec<String> = Vec::new();
    let mut count_diffs: Vec<(String, usize, usize)> = Vec::new();
    let mut path_diffs: Vec<String> = Vec::new();
    let (mut valid_rows, mut invalid_rows, mut sectioned_rows) = (0usize, 0usize, 0usize);
    let mut wrapper_rows = 0usize;
    let mut total_v4_errors = 0usize;
    let mut total_v5_errors = 0usize;
    let mut kinds: BTreeSet<&str> = BTreeSet::new();

    for row in &rows {
        kinds.insert(row.kind.as_str());
        let got = validate_qtap_export(&row.instance);
        total_v4_errors += row.errors.len();
        total_v5_errors += got.errors.len();
        if row.valid {
            valid_rows += 1;
        } else {
            invalid_rows += 1;
        }

        // 1. The decision — EXACT.
        if got.valid != row.valid {
            failures.push(format!(
                "{}: valid v5={} v4={}\n  v5 errors: {:?}\n  v4 errors: {:?}",
                row.name, got.valid, row.valid, got.errors, row.errors
            ));
            continue;
        }
        // A `valid` result carries no errors on either side (v4 returns `[]`).
        if row.valid && !got.errors.is_empty() {
            failures.push(format!("{}: valid but v5 reported errors", row.name));
        }

        // 2. The repair pass's section SET — EXACT.
        let (want_sections, got_sections) =
            (error_sections(&row.errors), error_sections(&got.errors));
        if !want_sections.is_empty() {
            sectioned_rows += 1;
        }
        if want_sections != got_sections {
            failures.push(format!(
                "{}: errorSections v5={got_sections:?} v4={want_sections:?}\n  v5 errors: {:?}\n  v4 errors: {:?}",
                row.name, got.errors, row.errors
            ));
        }

        // 3. The count — RECORDED (it reaches the wire in the two frames).
        if got.errors.len() != row.errors.len() {
            count_diffs.push((row.name.clone(), row.errors.len(), got.errors.len()));
        }

        // 4. The instance-path SET, ajv's `if/then` wrapper struck — EXACT;
        //    and v4's per-path count never below v5's.
        let want_real: Vec<String> = row
            .errors
            .iter()
            .filter(|e| split_error(e).1 != AJV_THEN_WRAPPER)
            .cloned()
            .collect();
        wrapper_rows += usize::from(want_real.len() != row.errors.len());
        let (want_counts, got_counts) = (path_counts(&want_real), path_counts(&got.errors));
        let (want_set, got_set): (BTreeSet<&String>, BTreeSet<&String>) =
            (want_counts.keys().collect(), got_counts.keys().collect());
        if want_set != got_set {
            path_diffs.push(format!(
                "{}: instance-path SETs differ\n    only-v4: {:?}\n    only-v5: {:?}\n    v4 errors: {:?}\n    v5 errors: {:?}",
                row.name,
                want_set.difference(&got_set).collect::<Vec<_>>(),
                got_set.difference(&want_set).collect::<Vec<_>>(),
                row.errors,
                got.errors,
            ));
        }
        for (path, n) in &got_counts {
            let v4n = want_counts.get(path).copied().unwrap_or(0);
            if *n > v4n {
                path_diffs.push(format!(
                    "{}: v5 reported {n} error(s) at {path} where v4 reported {v4n} — v5 must \
                     never invent an error ajv does not have",
                    row.name
                ));
            }
        }
    }

    // The measured engine diff, printed on every run (the lane record quotes it).
    eprintln!(
        "qtap_schema_validate_equivalence: {} rows ({} valid, {} invalid, {} carrying /data/<section>/ errors); \
         v4 reported {total_v4_errors} errors, v5 {total_v5_errors}",
        rows.len(),
        valid_rows,
        invalid_rows,
        sectioned_rows
    );
    if count_diffs.is_empty() {
        eprintln!("  error COUNTS agree on every row.");
    } else {
        eprintln!(
            "  RECORDED count divergence (v4 ajv → v5 jsonschema); {wrapper_rows} row(s) carry \
             ajv's {AJV_THEN_WRAPPER:?} root wrapper:"
        );
        for (name, v4, v5) in &count_diffs {
            eprintln!("    {name:<44} v4={v4:<4} v5={v5}");
        }
    }

    // Coverage floors — a corpus that stopped exercising the engine must fail.
    assert!(
        rows.len() >= 40,
        "the oracle carried only {} rows",
        rows.len()
    );
    assert_eq!(
        kinds,
        BTreeSet::from(["literal", "mutation", "seed"]),
        "all three corpus kinds must be present"
    );
    assert!(valid_rows >= 12, "only {valid_rows} valid rows");
    assert!(invalid_rows >= 25, "only {invalid_rows} invalid rows");
    assert!(
        sectioned_rows >= 10,
        "only {sectioned_rows} rows exercised the `/data/<section>/` derivation"
    );
    assert!(
        total_v4_errors >= 250,
        "the oracle recorded only {total_v4_errors} errors"
    );

    assert!(
        failures.is_empty(),
        "{} row(s) DIFFER:\n{}",
        failures.len(),
        failures.join("\n")
    );

    assert!(
        wrapper_rows >= 10,
        "only {wrapper_rows} row(s) exercised ajv's `if/then` wrapper — the one measured \
         divergence must stay measured"
    );
    assert!(
        path_diffs.is_empty(),
        "the instance paths diverged on {} row(s):\n  {}",
        path_diffs.len(),
        path_diffs.join("\n  ")
    );
}
