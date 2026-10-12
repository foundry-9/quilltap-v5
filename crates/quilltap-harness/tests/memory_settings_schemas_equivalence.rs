//! P4.D265 tier-1 differential: v4's two memory-programme instance-settings
//! schemas (`MemoryConsolidationSettingsSchema`,
//! `MemoryExtractionModeSettingsSchema`, v4 `d58548051`) through their REAL
//! `safeParse`, against `db::instance_settings::parse_memory_*_settings`, and
//! v4's REAL `setMemory*Settings` merge-then-write against v5's setters.
//!
//! * `parse` rows — every bound both sides, `.int()`, `.positive()`'s exclusive
//!   zero, `.nullable()`, the enum, wrong types, non-object roots, an unknown
//!   key, multi-issue values: an accepted row compares the parsed settings'
//!   `to_json()` against v4's `data`; a refused row compares the issue list
//!   BYTE-FOR-BYTE (`serde_json::to_string` — `preserve_order` `Map` equality
//!   is order-blind, memory note `serde-json-map-eq-ignores-order`).
//! * `write` rows — a stored value (or none) + a patch through the REAL
//!   setter: the stored TEXT afterwards, the setter's answer, or the thrown
//!   `ZodError.message`.
//!
//! Regenerate + run (a pure tsx oracle, no fixture):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -f /tmp/oracle-memory-settings-schemas.ndjson
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/memory-settings-schemas.ts \
//!     > /tmp/oracle-memory-settings-schemas.ndjson
//!   cd $V5W
//!   QT_ORACLE_MEMORY_SETTINGS_SCHEMAS=/tmp/oracle-memory-settings-schemas.ndjson \
//!     cargo test -p quilltap-harness --test memory_settings_schemas_equivalence -- --nocapture

use quilltap_core::db::instance_settings as settings;
use serde_json::Value;

fn conn() -> rusqlite::Connection {
    let c = rusqlite::Connection::open_in_memory().unwrap();
    c.execute_batch(
        "CREATE TABLE \"instance_settings\" (\"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL)",
    )
    .unwrap();
    c
}

#[test]
fn memory_settings_schemas_match_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_MEMORY_SETTINGS_SCHEMAS") else {
        eprintln!("SKIP: set QT_ORACLE_MEMORY_SETTINGS_SCHEMAS to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read oracle {path}: {e}"));
    let mut failures: Vec<String> = Vec::new();
    let (mut parsed_rows, mut refused_rows, mut write_rows) = (0usize, 0usize, 0usize);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("oracle line");
        match row["kind"].as_str().unwrap() {
            "parse" => {
                let schema = row["schema"].as_str().unwrap();
                let input = &row["input"];
                let got: Result<Value, Vec<quilltap_core::api::zod_issues::ZodIssue>> =
                    if schema == "memoryConsolidation" {
                        settings::parse_memory_consolidation_settings(input).map(|s| s.to_json())
                    } else {
                        settings::parse_memory_extraction_mode_settings(input).map(|s| s.to_json())
                    };
                let label = format!("{schema} {input}");
                match (row["success"].as_bool().unwrap(), got) {
                    (true, Ok(v)) => {
                        parsed_rows += 1;
                        if serde_json::to_string(&v).unwrap()
                            != serde_json::to_string(&row["data"]).unwrap()
                        {
                            failures.push(format!("{label}: data v5 {v} v4 {}", row["data"]));
                        }
                    }
                    (false, Err(issues)) => {
                        refused_rows += 1;
                        let got = serde_json::to_string(&issues).unwrap();
                        let want = serde_json::to_string(&row["issues"]).unwrap();
                        if got != want {
                            failures.push(format!("{label}: issues\n  v5 {got}\n  v4 {want}"));
                        }
                    }
                    (true, Err(issues)) => failures.push(format!(
                        "{label}: v4 accepted, v5 refused {}",
                        serde_json::to_string(&issues).unwrap()
                    )),
                    (false, Ok(v)) => {
                        failures.push(format!("{label}: v4 refused, v5 accepted {v}"))
                    }
                }
            }
            "write" => {
                write_rows += 1;
                let key = row["key"].as_str().unwrap();
                let c = conn();
                if let Some(stored) = row["stored"].as_str() {
                    c.execute(
                        "INSERT INTO instance_settings (key, value) VALUES (?1, ?2)",
                        [key, stored],
                    )
                    .unwrap();
                }
                let got = if key == "memoryConsolidation" {
                    settings::set_memory_consolidation_settings(&c, &row["patch"])
                        .map(|s| s.to_json())
                } else {
                    settings::set_memory_extraction_mode_settings(&c, &row["patch"])
                        .map(|s| s.to_json())
                };
                let (answer, error) = match got {
                    Ok(v) => (v, Value::Null),
                    Err(e) => (Value::Null, Value::String(e.to_string())),
                };
                let after: Option<String> = c
                    .query_row(
                        "SELECT value FROM instance_settings WHERE key = ?1",
                        [key],
                        |r| r.get(0),
                    )
                    .ok();
                let label = format!("write {key} {} + {}", row["stored"], row["patch"]);
                if serde_json::to_string(&answer).unwrap()
                    != serde_json::to_string(&row["answer"]).unwrap()
                {
                    failures.push(format!("{label}: answer v5 {answer} v4 {}", row["answer"]));
                }
                if error != row["error"] {
                    failures.push(format!("{label}: error v5 {error} v4 {}", row["error"]));
                }
                let want_after = row["after"].as_str().map(str::to_string);
                // A refused write leaves v4's row as it was (here: none) — and
                // v5's too.
                if after != want_after {
                    failures.push(format!("{label}: stored v5 {after:?} v4 {want_after:?}"));
                }
            }
            other => panic!("unknown oracle row kind {other}"),
        }
    }
    assert!(
        failures.is_empty(),
        "{} row(s) differ from v4:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(
        parsed_rows >= 15 && refused_rows >= 40 && write_rows >= 8,
        "the corpus shrank: {parsed_rows} accepted / {refused_rows} refused / {write_rows} writes"
    );
    eprintln!(
        "OK: memory settings schemas matched oracle ({parsed_rows} accepted, {refused_rows} refused, {write_rows} writes)."
    );
}
