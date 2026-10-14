//! Tier-2 differential: the memory calendar's stateful zone read
//! (`quilltap_core::services::memory_time_zone::refresh_memory_time_zone`) vs
//! v4's REAL `refreshMemoryTimeZone` over `repos.chatSettings.findAll()` at
//! the pin (`83334b9a5`) — P4.D289, C1-A item 3, work order LR-3 / LR-4 /
//! LR-5.
//!
//! The case builds each scenario's database FRESH through v4's real
//! `initializeDatabase` + `ensureCollection` (test pepper) and leaves its
//! pre-refresh state as `$QT_FIXTURE_OUT_DIR/<scenario>.db`; this side opens a
//! COPY, replays each step's SQL plant through the writer, and calls the
//! refresh at the step's `nowMs` with `host_fallback` = the case's
//! `QUILLTAP_TIMEZONE` (v4's env rung). Per step it compares the effective
//! zone and the captured lines (level + message + fields; v5 logs on the
//! caller thread): the `[MemoryCalendar] Instance timezone refreshed`
//! DEBUG (present on every re-read, absent while the cache is fresh) and
//! `Error finding all entities {collection: chat_settings, error}`.
//!
//! Recorded, not diffed: v4's backend `SQLite find error` (unported — the
//! `UNPORTED_BACKEND_LINES` precedent), and the row the whole-row
//! `ChatSettingsSchema` refuses — `MEMORY_ZONE_ROW_UNVALIDATED` below (v4
//! drops the row with `Data validation failed` / `Safe validation failed`;
//! v5's one-column read does not re-run that schema). v4's catch DEBUG `Could
//! not read the instance timezone; using fallback` is UNREACHABLE through its
//! real repository and is pinned ABSENT on both sides.
//!
//! Generate the oracle output, then run:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT_DIR=/tmp/qt-memory-time-zone-refresh TZ=UTC \
//!     $N/npx tsx "$V5W/harness/oracle/cases/memory-time-zone-refresh-tier2.ts" > /tmp/oracle-memory-time-zone-refresh.ndjson
//!   cd "$V5W"
//!   QT_ORACLE_MEMORY_TIME_ZONE_REFRESH=/tmp/oracle-memory-time-zone-refresh.ndjson \
//!   QT_FIXTURE_MEMORY_TIME_ZONE_REFRESH=/tmp/qt-memory-time-zone-refresh \
//!     cargo test -p quilltap-harness --test memory_time_zone_refresh_tier2_equivalence -- --nocapture

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use quilltap_core::db::runtime::Db;
use quilltap_core::services::memory_time_zone::{
    refresh_memory_time_zone, reset_memory_time_zone_cache_for_tests,
};
use quilltap_core::test_support::captured_with;
use serde_json::Value;

/// Every test that touches the process-global zone cache serialises here.
static CACHE_LOCK: Mutex<()> = Mutex::new(());

/// The ruled divergence `MEMORY_ZONE_ROW_UNVALIDATED` (LR-5):
/// `(scenario, v4's effective zone, v5's effective zone)`. v4's `_findAll`
/// drops a row its whole-row `ChatSettingsSchema` refuses (`validateSafe`),
/// so a valid `timezone` on that row is never seen; v5 reads the one column
/// and takes it. Both ways: a v4 or v5 change to either value trips it.
const MEMORY_ZONE_ROW_UNVALIDATED: &[(&str, &str, &str)] =
    &[("row-unvalidated", "America/Santiago", "Asia/Kolkata")];

/// v4 lines this port does not emit, by message (see the header).
const NOT_DIFFED: &[&str] = &[
    "SQLite find error",
    "Data validation failed",
    "Safe validation failed",
];

const UNREACHABLE_CATCH: &str =
    "[MemoryCalendar] Could not read the instance timezone; using fallback";

fn pepper() -> String {
    let spec_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-settings-tier2.json");
    let spec: Value = serde_json::from_str(&std::fs::read_to_string(&spec_path).unwrap()).unwrap();
    spec["testPepperBase64"].as_str().unwrap().to_string()
}

/// v4's `{level, message, fields}` as v5's capture rig renders it, minus the
/// target: `LEVEL message key=value …` (fields in v4's key order; JSON null
/// renders `null`, the tree's `unwrap_or("null")` convention).
fn expected_line(log: &Value) -> String {
    let mut out = format!(
        "{} {}",
        log["level"].as_str().unwrap().to_uppercase(),
        log["message"].as_str().unwrap()
    );
    for (k, v) in log["fields"].as_object().unwrap() {
        let rendered = match v {
            Value::Null => "null".to_string(),
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        out.push_str(&format!(" {k}={rendered}"));
    }
    out
}

/// A captured v5 line without its target token.
fn strip_target(line: &str) -> String {
    let mut parts = line.splitn(3, ' ');
    let level = parts.next().unwrap_or_default();
    let _target = parts.next();
    format!("{level} {}", parts.next().unwrap_or_default())
}

#[test]
fn refresh_memory_time_zone_matches_oracle() {
    let _serial = CACHE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (path, dir) = match (
        std::env::var("QT_ORACLE_MEMORY_TIME_ZONE_REFRESH"),
        std::env::var("QT_FIXTURE_MEMORY_TIME_ZONE_REFRESH"),
    ) {
        (Ok(p), Ok(d)) => (p, PathBuf::from(d)),
        _ => {
            eprintln!(
                "SKIP: set QT_ORACLE_MEMORY_TIME_ZONE_REFRESH and QT_FIXTURE_MEMORY_TIME_ZONE_REFRESH \
                 (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let pepper = pepper();

    let mut scenarios = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let sc: Value = serde_json::from_str(line).unwrap();
        let name = sc["scenario"].as_str().unwrap().to_string();
        let host_fallback = sc["hostFallback"].as_str().unwrap().to_string();
        let ruled = MEMORY_ZONE_ROW_UNVALIDATED
            .iter()
            .find(|(s, _, _)| *s == name);

        let work = tempfile::tempdir().unwrap();
        let file = work.path().join(format!("{name}.db"));
        std::fs::copy(dir.join(format!("{name}.db")), &file)
            .unwrap_or_else(|e| panic!("{name}: copy the case's base file: {e}"));
        let db = Db::open_main(&file, &pepper).unwrap_or_else(|e| panic!("{name}: open: {e}"));

        reset_memory_time_zone_cache_for_tests();
        for (i, step) in sc["steps"].as_array().unwrap().iter().enumerate() {
            if let Some(plant) = step["plant"].as_array() {
                let sql: Vec<String> = plant
                    .iter()
                    .map(|s| s.as_str().unwrap().to_string())
                    .collect();
                db.write_blocking(move |w| {
                    for stmt in &sql {
                        w.main().connection().execute_batch(stmt)?;
                    }
                    Ok(())
                })
                .unwrap_or_else(|e| panic!("{name} step {i}: plant: {e}"));
            }
            let now_ms = step["nowMs"].as_f64().unwrap();
            let (effective, lines) =
                captured_with(|| refresh_memory_time_zone(&db, &host_fallback, now_ms));

            let v4_effective = step["effective"].as_str().unwrap();
            match ruled {
                Some((_, v4_ruled, v5_ruled)) => {
                    assert_eq!(
                        v4_effective, *v4_ruled,
                        "{name} step {i}: MEMORY_ZONE_ROW_UNVALIDATED v4 side moved"
                    );
                    assert_eq!(
                        effective, *v5_ruled,
                        "{name} step {i}: MEMORY_ZONE_ROW_UNVALIDATED v5 side moved"
                    );
                }
                None => assert_eq!(effective, v4_effective, "{name} step {i}: effective zone"),
            }

            let v4_logs: Vec<&Value> = step["logs"].as_array().unwrap().iter().collect();
            assert!(
                v4_logs.iter().all(|l| l["message"] != UNREACHABLE_CATCH),
                "{name} step {i}: v4's catch DEBUG fired — LR-5's unreachability premise moved"
            );
            assert!(
                lines.iter().all(|l| !l.contains(UNREACHABLE_CATCH)),
                "{name} step {i}: v5 emitted the unreachable catch line"
            );
            let mut want: Vec<String> = v4_logs
                .iter()
                .filter(|l| !NOT_DIFFED.contains(&l["message"].as_str().unwrap()))
                .map(|l| expected_line(l))
                .collect();
            if let Some((_, _, v5_ruled)) = ruled {
                // v5 took the row v4 dropped, so its DEBUG names the zone.
                for w in &mut want {
                    if w.starts_with("DEBUG [MemoryCalendar] Instance timezone refreshed") {
                        *w = format!(
                            "DEBUG [MemoryCalendar] Instance timezone refreshed instanceTimeZone={v5_ruled} effective={v5_ruled}"
                        );
                    }
                }
            }
            let got: Vec<String> = lines.iter().map(|l| strip_target(l)).collect();
            assert_eq!(got, want, "{name} step {i}: captured lines");
        }
        scenarios.push(name);
    }

    assert_eq!(
        scenarios,
        [
            "no-rows",
            "first-invalid-second-valid",
            "null-then-folded-link",
            "ttl",
            "row-unvalidated",
            "read-failure",
            "read-failure-after-success",
        ],
        "scenario set drifted from the case"
    );
    eprintln!(
        "OK: memory-time-zone refresh matched oracle ({} scenarios).",
        scenarios.len()
    );
}
