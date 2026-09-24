//! Tier-2 differential (P4.113 Tier 2): v4's `readJsonSetting`
//! (`lib/instance-settings/index.ts:130-145`, `d1c06cd9d`) through its five
//! REAL getters — `getMemoryRecallSettings`, `getDataRetentionSettings`,
//! `getBrahmaConsoleSettings`, `getTabooSettings`, `getMemoryExtractionLimits`
//! — against `quilltap_core::db::instance_settings`.
//!
//! Both sides run the SAME cases (`harness/oracle/fixtures/instance-settings-
//! json-warns.json`): the `instance_settings` row for `key` deleted, then
//! (unless `raw` is null) written with the case's raw text, then the getter.
//! **Comparand, per case:** the returned value (v4's object; v5's typed return
//! rendered into v4's shape) and every WARN the getter logged — message bytes
//! exact (`[InstanceSettings] ${key} failed to parse — using defaults`), the
//! `error` field by PRESENCE (V8's `JSON.parse` / Zod wording is not serde's —
//! never transcribed). A malformed value per key warns; a valid value, a
//! `{}` (every field `.default`ed) and an absent row are SILENT.
//!
//! v4's side needs a real DB (its `rawQuery`); v5's runs on an in-memory
//! connection with the same one-table DDL — the getters touch nothing else.
//!
//!     N=~/.nvm/versions/node/v24.13.1/bin ; W=${V5W:-$(git rev-parse --show-toplevel)}
//!     STAGE=/tmp/qt-oracle-stage-is-json-warns
//!     rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
//!     cp $W/harness/oracle/cases/instance-settings-json-warns.test.ts $STAGE/harness/oracle/cases/
//!     cp $W/harness/oracle/fixtures/instance-settings-json-warns.json $STAGE/harness/oracle/fixtures/
//!     cd ~/source/quilltap-server
//!     QT_ORACLE_OUT=/tmp/oracle-instance-settings-json-warns.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=240000 \
//!     --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "instance-settings-json-warns\.test\.ts$"
//!
//! Then:
//!
//!     QT_ORACLE_ISJSONWARNS=/tmp/oracle-instance-settings-json-warns.ndjson \
//!     cargo test -p quilltap-harness --test instance_settings_json_warns_equivalence -- --nocapture

use quilltap_core::db::instance_settings as settings;
use serde_json::{json, Value};

const TARGET: &str = "quilltap_core::db::instance_settings";

/// v5's getter for `key`, its typed return rendered into v4's object shape.
fn get(conn: &rusqlite::Connection, key: &str) -> Value {
    match key {
        "memoryRecall" => settings::get_memory_recall_settings(conn)
            .expect("get_memory_recall_settings")
            .to_json(),
        "dataRetention" => json!({
            "staleChatDays": settings::get_data_retention_settings(conn)
                .expect("get_data_retention_settings"),
        }),
        "brahmaConsole" => json!({
            "maxAgentTurns": settings::get_brahma_console_settings(conn)
                .expect("get_brahma_console_settings"),
        }),
        "taboo" => json!({
            "phrases": settings::get_taboo_settings(conn).expect("get_taboo_settings"),
        }),
        "memoryExtractionLimits" => {
            settings::get_memory_extraction_limits(conn).expect("get_memory_extraction_limits")
        }
        other => panic!("no getter for key {other}"),
    }
}

/// A captured `WARN <target> <message> error=…` line as `{ message, hasError }`.
fn warn_record(line: &str) -> Value {
    let rest = line
        .strip_prefix(&format!("WARN {TARGET} "))
        .unwrap_or_else(|| panic!("a WARN off an unexpected target: {line}"));
    let (message, has_error) = match rest.find(" error=") {
        Some(i) => (&rest[..i], rest.len() > i + " error=".len()),
        None => (rest, false),
    };
    json!({ "message": message, "hasError": has_error })
}

#[test]
fn instance_settings_json_warns_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_ISJSONWARNS") else {
        eprintln!("SKIP: set QT_ORACLE_ISJSONWARNS to the oracle NDJSON (see header).");
        return;
    };
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/instance-settings-json-warns.json"),
        )
        .expect("read spec"),
    )
    .expect("parse spec");
    let cases = spec["cases"].as_array().expect("cases");
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle row"))
        .collect();
    assert_eq!(
        oracle.len(),
        cases.len(),
        "oracle case count != spec case count"
    );

    let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
    conn.execute_batch(
        "CREATE TABLE \"instance_settings\" (\"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL)",
    )
    .expect("instance_settings DDL");

    let mut reds: Vec<String> = Vec::new();
    let (mut warned, mut silent) = (0usize, 0usize);
    let mut keys_warned = std::collections::BTreeSet::new();
    for (c, want) in cases.iter().zip(&oracle) {
        let name = c["name"].as_str().unwrap();
        let key = c["key"].as_str().unwrap();
        assert_eq!(want["name"].as_str(), Some(name), "case order");
        conn.execute(
            "DELETE FROM \"instance_settings\" WHERE \"key\" = ?1",
            [key],
        )
        .expect("delete");
        if let Some(raw) = c["raw"].as_str() {
            conn.execute(
                "INSERT INTO \"instance_settings\" (\"key\", \"value\") VALUES (?1, ?2)",
                [key, raw],
            )
            .expect("insert");
        }
        let (value, lines) = quilltap_core::test_support::captured_with(|| get(&conn, key));
        let warns: Vec<Value> = lines
            .iter()
            .filter(|l| l.starts_with("WARN "))
            .map(|l| warn_record(l))
            .collect();
        if value != want["value"] {
            reds.push(format!("{name}: VALUE — v4 {} v5 {value}", want["value"]));
        }
        let want_warns = want["warns"].as_array().expect("warns");
        if &warns != want_warns {
            reds.push(format!("{name}: WARNs — v4 {want_warns:?} v5 {warns:?}"));
        }
        if want_warns.is_empty() {
            silent += 1;
        } else {
            warned += 1;
            keys_warned.insert(key.to_string());
        }
    }
    eprintln!(
        "instance_settings_json_warns: {} cases ({warned} warning, {silent} silent)",
        cases.len()
    );
    assert!(
        reds.is_empty(),
        "{} case(s) differ from v4:\n{}",
        reds.len(),
        reds.join("\n")
    );
    // Every one of v4's five `readJsonSetting` keys must be armed on the warning
    // side, and the corpus must keep silence legs, or a trimmed spec goes green.
    assert_eq!(
        keys_warned.len(),
        5,
        "every readJsonSetting key needs a warning arm: {keys_warned:?}"
    );
    assert!(silent >= 5, "every key needs a silence leg");
}
