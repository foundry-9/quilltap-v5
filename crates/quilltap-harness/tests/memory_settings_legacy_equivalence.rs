//! P4.D288 unit 2 — tier-1 differential: the legacy memory-settings translator
//! (`db::instance_settings::{legacy_housekeeping_to_maintenance,
//! with_memory_maintenance_from_legacy}`, contract C1-5) against v4's REAL
//! `lib/memory/settings-legacy.ts` at the `83334b9a5` pin.
//!
//! - `legacy` rows: the `memoryMaintenance` BYTES the settings move stores
//!   (`JSON.stringify` of v4's answer — key order, the clamped caps, the
//!   overrides in V8's own-key order, no `phases`) or `null`.
//! - `restore` rows: the rows the restore twin answers, in order — the dead key
//!   dropped, the backup's own key winning, the FIRST candidate (not the
//!   latest) unless the single user's row exists, key PRESENCE with a `null`
//!   value deriving nothing, the derived row LAST.
//!
//! **`LEGACY_JSON_NUMBER_OVERFLOW`** (a named both-ways table, P4.D288's lane
//! record): a stored legacy STRING carrying an out-of-range number literal
//! (`1e400`) — V8's `JSON.parse` reads `Infinity` (then the clamp drops it and
//! the cap defaults); serde refuses the text (the `v8_json_parse_message`
//! class, `jsstr.rs`), so v5 translates nothing. `JSON.stringify` never writes
//! such a literal; only a hand-edited row carries one. Pinned both ways.
//!
//! Regenerate + run (a pure tsx oracle):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -f /tmp/oracle-memory-settings-legacy.ndjson
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/memory-settings-legacy.ts \
//!     > /tmp/oracle-memory-settings-legacy.ndjson
//!   cd $V5W
//!   QT_ORACLE_MEMORY_SETTINGS_LEGACY=/tmp/oracle-memory-settings-legacy.ndjson \
//!     cargo test -p quilltap-harness --test memory_settings_legacy_equivalence -- --nocapture

use quilltap_core::db::instance_settings::{
    legacy_housekeeping_to_maintenance, with_memory_maintenance_from_legacy,
};
use serde_json::{json, Value};

/// `LEGACY_JSON_NUMBER_OVERFLOW`: label → (v4's bytes, v5's answer).
const LEGACY_JSON_NUMBER_OVERFLOW: &[(&str, &str, Option<&str>)] = &[(
    "cap-huge-text",
    "{\"enabled\":false,\"perCharacterCap\":2000,\"perCharacterCapOverrides\":{}}",
    None,
)];

#[test]
fn memory_settings_legacy_matches_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_MEMORY_SETTINGS_LEGACY") else {
        eprintln!("SKIP: set QT_ORACLE_MEMORY_SETTINGS_LEGACY to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("oracle");
    let mut failures = Vec::new();
    let (mut legacy, mut restore, mut ruled) = (0usize, 0usize, 0usize);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("oracle line");
        let label = row["label"].as_str().unwrap();
        match row["fn"].as_str().unwrap() {
            "legacy" => {
                legacy += 1;
                let got = legacy_housekeeping_to_maintenance(&row["input"]).map(|v| v.to_string());
                let want = row["output"].as_str().map(str::to_string);
                if let Some((_, v4, v5)) = LEGACY_JSON_NUMBER_OVERFLOW
                    .iter()
                    .find(|(l, _, _)| *l == label)
                {
                    ruled += 1;
                    assert_eq!(
                        want.as_deref(),
                        Some(*v4),
                        "{label}: v4 moved (retire the row)"
                    );
                    assert_eq!(got.as_deref(), *v5, "{label}: v5 moved (retire the row)");
                    continue;
                }
                if got != want {
                    failures.push(format!("legacy {label}: v5 {got:?} v4 {want:?}"));
                }
            }
            "restore" => {
                restore += 1;
                let rows: Vec<(String, String)> = row["input"]["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| {
                        (
                            r["key"].as_str().unwrap().to_string(),
                            r["value"].as_str().unwrap().to_string(),
                        )
                    })
                    .collect();
                let chat_settings = row["input"]["chatSettings"].as_array().unwrap().clone();
                let got: Vec<Value> = with_memory_maintenance_from_legacy(&rows, &chat_settings)
                    .into_iter()
                    .map(|(key, value)| json!({ "key": key, "value": value }))
                    .collect();
                let got = serde_json::to_string(&got).unwrap();
                let want = serde_json::to_string(&row["output"]).unwrap();
                if got != want {
                    failures.push(format!("restore {label}:\n  v5 {got}\n  v4 {want}"));
                }
            }
            other => panic!("unknown fn {other}"),
        }
    }
    assert!(
        failures.is_empty(),
        "{} row(s) differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(
        ruled,
        LEGACY_JSON_NUMBER_OVERFLOW.len(),
        "every ruled row met"
    );
    assert!(
        legacy >= 30 && restore >= 9,
        "the corpus shrank: {legacy} / {restore}"
    );
    eprintln!("OK: legacy translator matched v4 ({legacy} legacy rows, {ruled} ruled; {restore} restore rows).");
}
