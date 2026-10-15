//! P4.D297 tier-2 differential: the memory graph repair —
//! `quilltap_core::services::memory_repair::repair_memory_graph` through
//! `api::memory_maintenance_routes::memory_repair` against v4's REAL
//! `repairMemoryGraph` (`lib/memory/repair.ts`) through its route handler
//! (POST `/api/v1/memories?action=repair`, `app/api/v1/memories/route.ts`), at
//! the `83334b9a5` pin.
//!
//! Over a COPY of the maintenance fixture pair with the dangling-link plants
//! of `harness/oracle/fixtures/memory-graph-repair-steps.json` — every
//! dangling column: a missing `relatedMemoryIds` target (one row naming it
//! twice; a live cross-holder link KEPT), a missing `consolidatedFrom` member
//! (a partial digest → its members cut), a digest whose every member is gone
//! (→ the `graph-repair` retirement), a missing `supersededById` on a cold and
//! a hot row (each restored to its tier through the index), and a
//! `relatedMemoryIds` that is not JSON (skipped by the scan's `json_valid`).
//! The steps run IN ORDER on ONE copy: whole-instance and scoped dry runs, a
//! scoped live run (the other holders' links untouched), a whole-instance
//! live run, the clean re-scan, an absent body, the 404, and a failed scan —
//! the memories table renamed away: v4's `findDanglingLinks` has NO fallback
//! ("a scan that failed must not read as a clean graph"), so the route
//! answers 500. The fixture holds no embedding profile: a live run's backfill
//! answers `embeddingSkippedReason: 'no-embedding-profile'` (the routes family
//! covers the queued-jobs arm).
//!
//! Comparands, per step: status; the body (errors as `error` only — R-K); the
//! `memory:repair` and `[Memories API]` lines in full (L-8: `foundJson`,
//! `repairedJson`, `null` when absent); every other captured line —
//! the retirement's, the index's, the mirror's, the repository's scan — by
//! TAG (their owners' bytes: C1-3 / C1-7 / the repository). Before the failed
//! scan: the `memories` table (`updatedAt` normalized) and `background_jobs`.
//!
//! Generate (Node 24, from the v4 checkout or the round's pin — a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=${M:-/tmp/qt-graph-repair-oracle}
//!   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=${QT_MAIN:-/tmp/qt-maintenance-main.db} \
//!   QT_FIXTURE_MOUNT_OUT=${QT_MOUNT:-/tmp/qt-maintenance-mount.db} \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-maintenance-fixture.ts
//!   rm -f ${QT_OUT:-/tmp/qt-graph-repair.ndjson}
//!   QT_FIXTURE_MAINTENANCE_MAIN=${QT_MAIN:-/tmp/qt-maintenance-main.db} \
//!   QT_FIXTURE_MAINTENANCE_MOUNT=${QT_MOUNT:-/tmp/qt-maintenance-mount.db} \
//!   QT_ORACLE_OUT=${QT_OUT:-/tmp/qt-graph-repair.ndjson} \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$M/cases" -- memory-graph-repair-tier2
//!   cd $V5W
//!   QT_ORACLE_GRAPH_REPAIR=${QT_OUT:-/tmp/qt-graph-repair.ndjson} \
//!   QT_FIXTURE_MAINTENANCE_MAIN=${QT_MAIN:-/tmp/qt-maintenance-main.db} \
//!   QT_FIXTURE_MAINTENANCE_MOUNT=${QT_MOUNT:-/tmp/qt-maintenance-mount.db} \
//!     cargo test -p quilltap-harness --test memory_graph_repair_tier2_equivalence -- --nocapture

use std::path::{Path, PathBuf};

use quilltap_core::api::memory_maintenance_routes::memory_repair;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::test_support::global_capture;
use serde_json::{json, Map, Value};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
}

fn status_and_body(r: &Response) -> (u16, Value) {
    match r {
        Response::Error(e) => (
            match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::NotFound => 404,
                ErrorKind::Internal => 500,
                _ => 0,
            },
            json!({ "error": e.message }),
        ),
        other => {
            let v = serde_json::to_value(other).unwrap();
            (200, v.get("data").cloned().unwrap_or(Value::Null))
        }
    }
}

fn approx_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            x == y || (x - y).abs() <= 1e-12
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| approx_eq(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y.iter())
                    .all(|((k1, v1), (k2, v2))| k1 == k2 && approx_eq(v1, v2))
        }
        _ => a == b,
    }
}

fn render_oracle_line(line: &Value) -> String {
    let mut out = format!(
        "{} {}",
        line["level"].as_str().unwrap().to_uppercase(),
        line["message"].as_str().unwrap()
    );
    for (k, v) in line["context"].as_object().unwrap() {
        match v {
            Value::String(s) => out.push_str(&format!(" {k}={s}")),
            Value::Array(_) | Value::Object(_) => {
                out.push_str(&format!(" {k}Json={}", serde_json::to_string(v).unwrap()))
            }
            // L-8: `repaired` is object-valued — `repairedJson` on every arm.
            Value::Null if k == "repaired" => out.push_str(" repairedJson=null"),
            other => out.push_str(&format!(" {k}={other}")),
        }
    }
    out
}

fn strip_target(line: &str) -> String {
    let mut parts = line.splitn(3, ' ');
    let level = parts.next().unwrap_or_default();
    let _ = parts.next();
    format!("{level} {}", parts.next().unwrap_or_default())
}

/// C1-7's scrub runs whole inside ONE writer closure (`db.write`): its own
/// line, the graph-repair retirement's, and the index's tier moves / evictions
/// log on the writer's thread — invisible to a per-thread capture (memory note
/// `capture-rig-writer-thread`). Their effects are pinned by the body's
/// `repaired` counts and the `memories` table; the post-commit announcements
/// (the Commonplace mirror refresh) ARE captured and tag-compared.
const WRITER_THREAD_TAGS: [&str; 5] = [
    "Scrubbed dangling memory links",
    "Scrubbed digest links to retired memories",
    "Evicted retired memories from the index",
    "Retired memories",
    "Moved memories between tiers through the index",
];

/// The lines this lane owns (compared in full).
fn owned(message: &str, module: &str) -> bool {
    module == "memory:repair" || message.starts_with("[Memories API]")
}

fn sql_rows(db: &Db, sql: &str) -> Vec<Value> {
    db.read_main(|conn| {
        let mut stmt = conn.prepare(sql)?;
        let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let rows = stmt.query_map([], |row| {
            let mut m = Map::new();
            for (i, name) in names.iter().enumerate() {
                let v: rusqlite::types::Value = row.get(i)?;
                m.insert(
                    name.clone(),
                    match v {
                        rusqlite::types::Value::Null => Value::Null,
                        rusqlite::types::Value::Integer(i) => json!(i),
                        rusqlite::types::Value::Real(f) => json!(f),
                        rusqlite::types::Value::Text(s) => json!(s),
                        rusqlite::types::Value::Blob(b) => {
                            json!(b.iter().map(|x| format!("{x:02x}")).collect::<String>())
                        }
                    },
                );
            }
            Ok(Value::Object(m))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
    .unwrap()
}

fn resolve(spec: &Value, v: &Value) -> Value {
    match v {
        Value::String(s) => match s.strip_prefix('@') {
            Some("unknown") => spec["unknownId"].clone(),
            Some(k) => spec["holders"][k]["id"].clone(),
            None => v.clone(),
        },
        Value::Array(a) => Value::Array(a.iter().map(|x| resolve(spec, x)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| (k.clone(), resolve(spec, x)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn memory_graph_repair_tier2_matches_oracle() {
    let (Ok(oracle_path), Ok(fixture_main), Ok(fixture_mount)) = (
        std::env::var("QT_ORACLE_GRAPH_REPAIR"),
        std::env::var("QT_FIXTURE_MAINTENANCE_MAIN"),
        std::env::var("QT_FIXTURE_MAINTENANCE_MOUNT"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_GRAPH_REPAIR + QT_FIXTURE_MAINTENANCE_MAIN / _MOUNT (see header)."
        );
        return;
    };
    global_capture::install();
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("memory-maintenance-tier2.json")).unwrap(),
    )
    .unwrap();
    let corpus: Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("memory-graph-repair-steps.json")).unwrap(),
    )
    .unwrap();
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle {oracle_path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();

    let scratch = tempfile::Builder::new()
        .prefix("qt-graph-repair-rust-")
        .tempdir()
        .unwrap();
    let main = scratch.path().join("main.db");
    let mount = scratch.path().join("mount.db");
    std::fs::copy(&fixture_main, &main).unwrap();
    std::fs::copy(&fixture_mount, &mount).unwrap();
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        spec["testPepperBase64"].as_str().unwrap(),
    )
    .expect("open the fixture copies");
    let user_id = spec["userId"].as_str().unwrap();
    for sql in corpus["plants"].as_array().unwrap() {
        let sql = sql.as_str().unwrap().to_string();
        db.write(move |w| Ok(w.main().connection().execute_batch(&sql)?))
            .await
            .unwrap();
    }

    let mut failures: Vec<String> = Vec::new();
    let mut owned_lines = 0usize;
    let mut tag_lines = 0usize;
    let oracle_steps: Vec<&Value> = oracle.iter().filter(|o| o["kind"] == "step").collect();
    let steps = corpus["steps"].as_array().unwrap();
    assert_eq!(steps.len(), oracle_steps.len(), "step count — regenerate");
    for (raw, want) in steps.iter().zip(oracle_steps) {
        let step = resolve(&spec, raw);
        let name = step["name"].as_str().unwrap();
        assert_eq!(want["name"], name, "step order");
        if let Some(sql) = step.get("sql").and_then(Value::as_str) {
            // ── the tables, before the scan fails ─────────────────────────
            let norm = |rows: Vec<Value>| -> Value {
                Value::Array(
                    rows.into_iter()
                        .map(|mut r| {
                            if r.get("updatedAt").is_some() {
                                r["updatedAt"] = json!("<ts>");
                            }
                            r
                        })
                        .collect(),
                )
            };
            let got = norm(sql_rows(
                &db,
                "SELECT * FROM memories ORDER BY characterId, id",
            ));
            let want_rows = norm(
                oracle.iter().find(|o| o["kind"] == "table").unwrap()["rows"]
                    .as_array()
                    .unwrap()
                    .clone(),
            );
            if !approx_eq(&got, &want_rows) {
                let (ga, wa) = (got.as_array().unwrap(), want_rows.as_array().unwrap());
                let first = ga
                    .iter()
                    .zip(wa.iter())
                    .position(|(a, b)| !approx_eq(a, b))
                    .unwrap_or(ga.len().min(wa.len()));
                failures.push(format!(
                    "memories: {} vs {} rows, first divergence at {first}:\n  v5 {}\n  v4 {}",
                    ga.len(),
                    wa.len(),
                    ga.get(first).cloned().unwrap_or(Value::Null),
                    wa.get(first).cloned().unwrap_or(Value::Null)
                ));
            }
            let got_jobs = Value::Array(sql_rows(
                &db,
                "SELECT userId, type, status, payload, maxAttempts FROM background_jobs ORDER BY type, payload",
            ));
            let want_jobs = &oracle.iter().find(|o| o["kind"] == "jobs").unwrap()["rows"];
            if !approx_eq(&got_jobs, want_jobs) {
                failures.push(format!(
                    "background_jobs:\n  v5 {got_jobs}\n  v4 {want_jobs}"
                ));
            }
            let sql = sql.to_string();
            db.write(move |w| Ok(w.main().connection().execute_batch(&sql)?))
                .await
                .unwrap();
        }
        let body = step.get("body").cloned().unwrap_or(Value::Null);
        let (resp, captured) =
            global_capture::capture_async(Box::pin(memory_repair(&db, user_id, body))).await;
        let (status, got_body) = status_and_body(&resp);
        let want_status = want["status"].as_u64().unwrap();
        if u64::from(status) != want_status {
            failures.push(format!(
                "{name}: status v5 {status} v4 {want_status} ({got_body})"
            ));
        }
        let want_body = if want_status == 200 {
            want["body"].clone()
        } else {
            json!({ "error": want["body"]["error"] })
        };
        if !approx_eq(&got_body, &want_body) {
            failures.push(format!("{name}: body\n  v5 {got_body}\n  v4 {want_body}"));
        }
        let got_owned: Vec<String> = captured
            .iter()
            .map(|l| strip_target(l))
            .filter(|l| {
                let message = l.split_once(' ').map_or("", |(_, m)| m);
                message.starts_with("[Memories API]") || l.contains(" module=memory:repair")
            })
            .collect();
        let want_lines = want["lines"].as_array().unwrap();
        let want_owned: Vec<String> = want_lines
            .iter()
            .filter(|l| {
                owned(
                    l["message"].as_str().unwrap(),
                    l["context"]["module"].as_str().unwrap_or_default(),
                )
            })
            .map(render_oracle_line)
            .collect();
        owned_lines += want_owned.len();
        if got_owned != want_owned {
            failures.push(format!(
                "{name}: lines\n  v5 {got_owned:#?}\n  v4 {want_owned:#?}"
            ));
        }
        // Every other line by TAG: v4's count of each message, present in v5.
        let mut tags: Vec<&str> = want_lines
            .iter()
            .filter(|l| {
                !owned(
                    l["message"].as_str().unwrap(),
                    l["context"]["module"].as_str().unwrap_or_default(),
                )
            })
            .map(|l| l["message"].as_str().unwrap())
            .collect();
        tags.sort_unstable();
        tags.dedup();
        tags.retain(|t| !WRITER_THREAD_TAGS.contains(t));
        for tag in tags {
            tag_lines += 1;
            let w = want_lines.iter().filter(|l| l["message"] == tag).count();
            let g = captured
                .iter()
                .filter(|l| {
                    strip_target(l)
                        .split_once(' ')
                        .is_some_and(|(_, m)| m.starts_with(tag))
                })
                .count();
            if g != w {
                failures.push(format!("{name}: tag {tag:?} v5 {g} v4 {w}"));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} failure(s):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert!(owned_lines >= 8, "owned lines: {owned_lines}");
    eprintln!(
        "OK: memory graph repair tier-2 matched oracle ({} steps, {owned_lines} owned lines + {tag_lines} tags, the memories table, the jobs).",
        steps.len()
    );
}
