//! P4.82 TIER-2 differential for the one-time head-and-shoulders backfill SCAN
//! and its dedupe-on-enqueue queue helper:
//! `services::headshoulders_backfill_enqueue::enqueue_headshoulders_backfill`
//! (over `queue_service::enqueue_character_headshoulders_backfill_blocking`)
//! vs v4's REAL `enqueueHeadShouldersBackfill` (over its REAL
//! `enqueueCharacterHeadShouldersBackfill`). Both sides run over a FRESH copy
//! of the committed `headshoulders-{main,mount}.db` pair per case.
//!
//! The fixture's eleven characters put every classification arm in ONE scan, so
//! the `(scanned, enqueued, skipped)` triple is a real comparand rather than a
//! tautology: a vault-less character, an already-filled prompt, a
//! WHITESPACE-only prompt (absent → enqueued), no seed at all, a whitespace-only
//! seed (`hasSeed` is NOT trimmed, so it IS enqueued — the asymmetry with the
//! handler, which trims and then returns silently), an eligible one, and one
//! whose PENDING job makes the helper's dedupe answer `isNew:false`.
//!
//! Three cases: a fresh scan; the same instance run TWICE (the second call must
//! find the flag and do nothing — no scan line, no new rows); and the flag
//! PRE-SET before the first call. That third case is the cross-app one that
//! matters most in the field: `instance_settings['headshoulders_backfill_
//! enqueued_v1']` is v4's own row, already written on every instance v4 has
//! booted since the feature shipped, so a v5 boot there must scan nothing.
//!
//! Comparands: the result struct per run; the `background_jobs` rows (minted
//! ids placeholdered, everything else verbatim — `type`, `payload` BYTES,
//! `priority`, `maxAttempts`, `attempts`, `status`, `userId`); the
//! `instance_settings` flag; and the scan's log lines with their bags.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-headshoulders-enqueue.ndjson npx jest -- headshoulders-backfill-enqueue
//! Run:
//!   QT_ORACLE_HEADSHOULDERS_ENQUEUE=/tmp/oracle-headshoulders-enqueue.ndjson \
//!     cargo test -p quilltap-harness --test headshoulders_backfill_enqueue_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::db::Writer;
use quilltap_core::services::headshoulders_backfill_enqueue::{
    enqueue_headshoulders_backfill, flag_value, HeadShouldersBackfillEnqueueResult, FLAG_KEY,
};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Map, Value};

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CharacterRow {
    pending_backfill_job_id: Option<String>,
}

#[derive(Deserialize)]
struct Spec {
    characters: Vec<CharacterRow>,
}

/// `(name, preset flag, runs)` — the oracle's own `CASES` table.
const CASES: &[(&str, Option<&str>, usize)] = &[
    ("fresh_scan", None, 1),
    ("second_run", None, 2),
    ("flag_preset", Some("true"), 1),
];

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/headshoulders.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

/// A fresh writable PAIR, opened DIRECTLY rather than through [`Db`].
///
/// The scan takes `&Connection`s (its production caller is the boot-repair
/// write closure, which already holds the writer), and `Db::write_blocking`
/// would run it on the WRITER thread — where the thread-scoped tracing capture
/// cannot see it, so every log comparand would be vacuously empty. Driving the
/// connections directly is both the closer shape and the only one whose lines
/// are observable.
fn fresh_pair(tag: &str) -> (Writer, Writer) {
    let scratch = std::env::temp_dir().join(format!("qt-hs-enq-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("headshoulders-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("headshoulders-mount.db"), &mount).unwrap();
    (
        Writer::open_writable(&main, TEST_PEPPER).expect("open main writer"),
        Writer::open_writable(&mount, TEST_PEPPER).expect("open mount writer"),
    )
}

fn sorted(v: &Value) -> Value {
    match v {
        Value::Object(o) => {
            let mut m = Map::new();
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        other => other.clone(),
    }
}
fn norm(v: &Value) -> String {
    serde_json::to_string_pretty(&sorted(v)).unwrap()
}
/// A REAL column the way `JSON.stringify` renders what better-sqlite3 hands
/// back (an integer-valued float as a bare integer).
fn js_num(f: f64) -> Value {
    if f.is_finite() && f.fract() == 0.0 {
        Value::from(f as i64)
    } else {
        Value::from(f)
    }
}

fn result_json(r: &HeadShouldersBackfillEnqueueResult) -> Value {
    json!({
        "scanned": r.scanned,
        "enqueued": r.enqueued,
        "skipped": r.skipped,
        "alreadyDone": r.already_done,
    })
}

/// The `background_jobs` rows, in the oracle's projection and sort.
fn dump_jobs(main: &Connection, seeded_ids: &[String]) -> Value {
    let mut stmt = main
        .prepare(
            r#"SELECT "id", "userId", "type", "status", "payload", "priority", "maxAttempts", "attempts"
                 FROM "background_jobs""#,
        )
        .expect("prepare background_jobs dump");
    let mut rows: Vec<Value> = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            Ok(json!({
                "id": if seeded_ids.contains(&id) { id } else { "<minted>".to_string() },
                "userId": row.get::<_, String>(1)?,
                "type": row.get::<_, String>(2)?,
                "status": row.get::<_, String>(3)?,
                "payload": row.get::<_, String>(4)?,
                "priority": js_num(row.get::<_, f64>(5)?),
                "maxAttempts": js_num(row.get::<_, f64>(6)?),
                "attempts": js_num(row.get::<_, f64>(7)?),
            }))
        })
        .expect("query background_jobs")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect background_jobs");
    rows.sort_by_key(|v| serde_json::to_string(v).unwrap());
    Value::Array(rows)
}

/// v4's bag keys in v5's snake_case tracing-field spelling. PANICS on an
/// unmapped key, so a v4-side bag change fails loudly rather than silently
/// dropping a field from the comparand.
fn snake(key: &str) -> &'static str {
    match key {
        "total" => "total",
        "scanned" => "scanned",
        "enqueued" => "enqueued",
        "skipped" => "skipped",
        "characterId" => "character_id",
        "error" => "error",
        other => panic!("unmapped oracle log-bag key {other}"),
    }
}

/// Render one oracle log line the way the capture layer renders v5's: `"<LEVEL>
/// <target> <message> <field>=<value> …"` — `tracing` visits the format-args
/// `message` FIRST, unquoted and with no `=`, then each declared field in
/// source order.
fn expected_log_line(entry: &Value) -> String {
    let level = entry["level"].as_str().unwrap().to_uppercase();
    let message = entry["message"].as_str().unwrap();
    let mut out = format!("{level} quilltap::boot {message}");
    if let Some(Value::Object(ctx)) = entry.get("context") {
        for (k, v) in ctx {
            let rendered = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            out.push_str(&format!(" {}={}", snake(k), rendered));
        }
    }
    out
}

#[test]
fn headshoulders_backfill_enqueue_matches_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_HEADSHOULDERS_ENQUEUE") else {
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let seeded_ids: Vec<String> = spec
        .characters
        .iter()
        .filter_map(|c| c.pending_backfill_job_id.clone())
        .collect();

    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }
    assert_eq!(
        oracle.len(),
        CASES.len(),
        "the oracle NDJSON must carry one row per case (stale oracle?)"
    );

    let mut failed: Vec<String> = Vec::new();

    for (name, preset, runs) in CASES {
        let (main_w, mount_w) = fresh_pair(name);
        let main = main_w.connection();
        let mount = mount_w.connection();
        if let Some(value) = preset {
            quilltap_core::db::instance_settings::write_instance_setting(main, FLAG_KEY, value)
                .expect("preset flag");
        }

        let mut results: Vec<Value> = Vec::new();
        let mut log_lines: Vec<String> = Vec::new();
        for _ in 0..*runs {
            let (r, lines) = quilltap_core::test_support::captured_with(|| {
                enqueue_headshoulders_backfill(main, mount)
            });
            results.push(result_json(&r));
            log_lines.extend(lines);
        }

        let want = &oracle[*name];

        let got_results = Value::Array(results);
        if norm(&got_results) != norm(&want["results"]) {
            eprintln!(
                "[{name}] RESULTS DIVERGE:\n--- got ---\n{}\n--- want ---\n{}",
                norm(&got_results),
                norm(&want["results"])
            );
            failed.push((*name).to_string());
            continue;
        }

        let got_jobs = dump_jobs(main, &seeded_ids);
        if norm(&got_jobs) != norm(&want["jobs"]) {
            eprintln!(
                "[{name}] background_jobs DIVERGE:\n--- got ---\n{}\n--- want ---\n{}",
                norm(&got_jobs),
                norm(&want["jobs"])
            );
            failed.push((*name).to_string());
            continue;
        }

        let got_flag = flag_value(main).map(Value::String).unwrap_or(Value::Null);
        if got_flag != want["flag"] {
            eprintln!(
                "[{name}] FLAG DIVERGES: got {got_flag} / want {}",
                want["flag"]
            );
            failed.push((*name).to_string());
            continue;
        }

        let want_lines: Vec<String> = want["log"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(expected_log_line)
            .collect();
        let got_lines: Vec<String> = log_lines
            .iter()
            .filter(|l| {
                l.contains("head-and-shoulders backfill") || l.contains("Head-and-shoulders")
            })
            .cloned()
            .collect();
        if got_lines != want_lines {
            eprintln!(
                "[{name}] LOG LINES DIVERGE:\n--- got ---\n{got_lines:#?}\n--- want ---\n{want_lines:#?}"
            );
            failed.push((*name).to_string());
            continue;
        }
    }

    assert!(failed.is_empty(), "cases diverged: {failed:?}");
}
