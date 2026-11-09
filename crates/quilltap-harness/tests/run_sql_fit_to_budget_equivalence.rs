//! Tier-1 differential (P4.D374): `run_sql`'s result size budget —
//! `quilltap_core::tools::run_sql::fit_to_budget` vs v4's REAL exported
//! `fitToBudget` (`lib/tools/handlers/run-sql-handler.ts`, `c539c82e0`).
//!
//! Each oracle line carries `inputJson` (the rows as V8 wrote them) and
//! `outJson` (`JSON.stringify(fitToBudget(rows))`). The Rust side parses the
//! input, runs the port, assembles `{rows, clippedCells, droppedRows, note?}` in
//! v4's key order and renders it with `pascal::js_value::json_stringify` — the
//! JS-number twin (ruling L-4; `serde_json::to_string` writes `1e20` where V8
//! writes `100000000000000000000` for `1e20`; MEASURED: both write `1e+21`, so
//! the `fit_1e20_boundary` rows, not the `1e21` ones, are the L-4 mutation
//! guard) — then compares the BYTES.
//!
//! `RUN_SQL_LONE_SURROGATE_CLIP` (ruling L-3, a NAMED both-ways table): v4's
//! `value.slice(0, 600)` can split a surrogate pair and KEEP the lone half
//! (JSON `\ud83d`); a Rust `String` cannot hold one, so v5 decodes U+FFFD
//! there. For the rows in the table the comparison maps v4's lone-surrogate
//! escape to U+FFFD AND asserts the raw bytes DIFFER (so a row that stops
//! splitting fails here); every other row must be byte-equal unmapped. The
//! budget's size is measured BEFORE the clip, so the drop decision is
//! unaffected — `clippedCells` / `droppedRows` are compared exactly on every row.
//!
//! Generate the oracle output (Node 24, from the pinned v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/run-sql-fit-to-budget.ts \
//!     > /tmp/oracle-run-sql-fit-to-budget.ndjson
//! Run:
//!   QT_ORACLE_RUN_SQL_BUDGET=/tmp/oracle-run-sql-fit-to-budget.ndjson \
//!     cargo test -p quilltap-harness --test run_sql_fit_to_budget_equivalence -- --nocapture

use quilltap_core::pascal::js_value::json_stringify;
use quilltap_core::tools::run_sql::fit_to_budget;
use serde_json::{Map, Value};

/// L-3's named rows: the clip splits an astral pair at unit 600.
const RUN_SQL_LONE_SURROGATE_CLIP: &[&str] = &["fit_astral_600"];

/// Replace every `\udXXX` escape of a LONE surrogate half with U+FFFD — what a
/// Rust `String` decodes the same split to. (No row in the corpus carries a
/// well-formed escaped pair; V8 writes astral characters raw.)
fn map_lone_surrogates(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && i + 5 < b.len()
            && b[i + 1] == b'u'
            && (b[i + 2] == b'd' || b[i + 2] == b'D')
            && matches!(b[i + 3], b'8'..=b'9' | b'a'..=b'f' | b'A'..=b'F')
        {
            out.push('\u{FFFD}');
            i += 6;
            continue;
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

#[test]
fn fit_to_budget_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_RUN_SQL_BUDGET") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_RUN_SQL_BUDGET to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).expect("read oracle NDJSON");
    let mut red: Vec<String> = Vec::new();
    let mut n = 0;
    let mut seen_table = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("oracle line");
        let id = row["id"].as_str().unwrap().to_string();
        let input: Vec<Value> =
            serde_json::from_str(row["inputJson"].as_str().unwrap()).expect("input rows");
        let expected = row["outJson"].as_str().unwrap().to_string();
        let fit = fit_to_budget(input);
        let mut out = Map::new();
        out.insert("rows".into(), Value::Array(fit.rows));
        out.insert("clippedCells".into(), Value::from(fit.clipped_cells as u64));
        out.insert("droppedRows".into(), Value::from(fit.dropped_rows as u64));
        if let Some(note) = fit.note {
            out.insert("note".into(), Value::String(note));
        }
        let actual = json_stringify(&Value::Object(out));
        n += 1;
        let ok = if RUN_SQL_LONE_SURROGATE_CLIP.contains(&id.as_str()) {
            seen_table += 1;
            // Both ways: v4's raw bytes carry a lone surrogate v5 cannot.
            actual != expected && actual == map_lone_surrogates(&expected)
        } else {
            actual == expected
        };
        if !ok {
            let at = actual
                .chars()
                .zip(expected.chars())
                .take_while(|(a, b)| a == b)
                .count();
            let ctx = |s: &str| {
                s.chars()
                    .skip(at.saturating_sub(40))
                    .take(140)
                    .collect::<String>()
            };
            red.push(format!(
                "{id}: differs at char {at}\n   v5: {}\n   v4: {}",
                ctx(&actual),
                ctx(&expected)
            ));
        }
    }
    assert!(n >= 18, "oracle has only {n} rows");
    assert_eq!(
        seen_table,
        RUN_SQL_LONE_SURROGATE_CLIP.len(),
        "every RUN_SQL_LONE_SURROGATE_CLIP row must be in the oracle"
    );
    if !red.is_empty() {
        panic!("{} of {n} rows RED:\n{}", red.len(), red.join("\n"));
    }
    println!("OK: fit_to_budget byte-exact for {n} rows ({seen_table} L-3 table row).");
}
