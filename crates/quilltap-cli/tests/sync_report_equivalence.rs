//! Tier-1 differential: the `quilltap sync` REPORT RENDERER (v4
//! `packages/quilltap/lib/sync-report.js`, `23da0b322`) vs
//! `quilltap-cli`'s `sync_report`.
//!
//! The renderer is pure, so the whole surface is a table: the fixed 8/6 column
//! widths, the path column capped at 44, the em dash a side-less action takes,
//! the trailing slash on a folder, the `sha …, size` tail, the summary's
//! conditional clauses and their ORDER (skipped before conflicts, failed last),
//! and the 0/1/2 exit code.
//!
//! **Colour is compared both ways, and that is the point.** v4 pads INSIDE the
//! ANSI escapes, so the coloured line is not the plain line with escapes wrapped
//! round it: the escape bytes count toward `String#length` in `pad`, which means
//! the two forms differ in more than colour. A port that reasoned about the
//! widths instead of reproducing `pad` would be right in the half nobody reads
//! and wrong in the half a human sees.
//!
//! `quilltap-cli` is a bin-only crate, so the module under test is pulled in by
//! `#[path]` (the `mount_common` idiom) rather than imported — `nodefmt` comes
//! with it because `format_bytes` lives there.
//!
//! Regenerate the oracle (from the v4 checkout):
//!   cd ~/source/quilltap-server
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   npx tsx $V5W/harness/oracle/cases/sync-report.ts > /tmp/oracle-sync-report.ndjson
//! Run:
//!   QT_ORACLE_SYNC_REPORT=/tmp/oracle-sync-report.ndjson cargo test -p quilltap-cli --test sync_report_equivalence -- --nocapture

// Only `format_bytes` is reached from here; the rest of `nodefmt` rides along
// because the module is compiled whole into this test binary.
#[allow(dead_code)]
#[path = "../src/nodefmt.rs"]
mod nodefmt;
#[allow(dead_code)]
#[path = "../src/sync_report.rs"]
mod sync_report;

use std::collections::BTreeMap;

use serde_json::Value;
use sync_report::{
    detail_for, display_path, exit_code_for, format_action_line, format_action_lines, format_bytes,
    format_summary, ReportAction, ReportSummary,
};

/// The corpus rows are read as plain JSON: `quilltap-cli` links `serde_json`
/// and not `serde`, and the renderer under test is itself a JSON-shaped reader.
fn s_at(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn strings_at(row: &Value, key: &str) -> Vec<String> {
    row.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|v| v.as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn sync_report_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_SYNC_REPORT") else {
        eprintln!("SKIP: set QT_ORACLE_SYNC_REPORT to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("cannot read oracle {oracle_path}: {e}"));

    let mut mismatches: Vec<String> = Vec::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut n = 0usize;
    let mut coloured_lines = 0usize;

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("parse oracle row");
        n += 1;
        let kind = s_at(&row, "kind");
        let id = s_at(&row, "id");
        *seen.entry(kind.clone()).or_default() += 1;

        let mut differ = |label: &str, want: String, got: String| {
            if got != want {
                mismatches.push(format!(
                    "[{kind}/{id}] {label}\n      v4: {want:?}\n      v5: {got:?}"
                ));
            }
        };

        match kind.as_str() {
            "format-bytes" => {
                let bytes = row.get("n").and_then(Value::as_f64);
                differ("", s_at(&row, "out"), format_bytes(bytes));
            }
            "action-line" => {
                let action = ReportAction::from_value(row.get("action").expect("action"));
                let colour = row.get("colour").and_then(Value::as_bool).expect("colour");
                if colour {
                    coloured_lines += 1;
                }
                let path_width = row
                    .get("pathWidth")
                    .and_then(Value::as_u64)
                    .expect("pathWidth") as usize;
                differ(
                    "",
                    s_at(&row, "out"),
                    format_action_line(&action, colour, path_width),
                );
            }
            "action-parts" => {
                let action = ReportAction::from_value(row.get("action").expect("action"));
                differ("detailFor", s_at(&row, "detailFor"), detail_for(&action));
                differ(
                    "displayPath",
                    s_at(&row, "displayPath"),
                    display_path(&action),
                );
            }
            "action-lines" => {
                let actions: Vec<ReportAction> = row
                    .get("actions")
                    .and_then(Value::as_array)
                    .expect("actions")
                    .iter()
                    .map(ReportAction::from_value)
                    .collect();
                let colour = row.get("colour").and_then(Value::as_bool).expect("colour");
                let want = strings_at(&row, "out");
                let got = format_action_lines(&actions, colour);
                if got != want {
                    mismatches.push(format!(
                        "[{kind}/{id}]\n      v4: {want:?}\n      v5: {got:?}"
                    ));
                }
            }
            "summary" => {
                let summary = ReportSummary::from_value(row.get("summary").expect("summary"));
                let elapsed_ms = row
                    .get("elapsedMs")
                    .and_then(Value::as_f64)
                    .expect("elapsedMs");
                let dry_run = row.get("dryRun").and_then(Value::as_bool).expect("dryRun");
                differ(
                    "",
                    s_at(&row, "out"),
                    format_summary(&summary, elapsed_ms, dry_run),
                );
            }
            "exit-code" => {
                let summary = ReportSummary::from_value(row.get("summary").expect("summary"));
                let want = row.get("out").and_then(Value::as_i64).expect("out") as i32;
                let got = exit_code_for(&summary);
                if got != want {
                    mismatches.push(format!("[{kind}/{id}] v4 {want}, v5 {got}"));
                }
            }
            other => panic!("unknown corpus row kind `{other}`"),
        }
    }

    assert!(
        mismatches.is_empty(),
        "{} disagreements over {n} rows:\n  {}",
        mismatches.len(),
        mismatches.join("\n  ")
    );
    assert_eq!(
        n, 176,
        "expected the full corpus (176 rows), saw {n} — stale oracle?"
    );
    for kind in [
        "format-bytes",
        "action-line",
        "action-parts",
        "action-lines",
        "summary",
        "exit-code",
    ] {
        assert!(
            seen.get(kind).copied().unwrap_or(0) > 0,
            "no `{kind}` rows in the corpus ({seen:?})"
        );
    }
    // The coloured half is where `pad`-inside-the-escapes bites; a corpus that
    // lost it would still look complete.
    assert!(
        coloured_lines >= 50,
        "only {coloured_lines} coloured action lines — the ANSI half is under-measured"
    );
    eprintln!("OK: sync-report matched v4 on {n} rows; kinds: {seen:?}");
}
