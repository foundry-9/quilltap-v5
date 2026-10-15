//! Tier-1 + tier-2 differential — P4.D292: the BUILTIN TF-IDF refit policy
//! (`services::builtin_refit_policy`) and the refit debounce
//! (`services::embedding_job_scheduler`) against v4's REAL
//! `builtin-refit-policy.ts` + `embedding-job-scheduler.ts` at the
//! `83334b9a5` pin.
//!
//! (a) **tier 1** — `evaluate_refit_need` / `drift_threshold` over v4's grid
//!     (`fittedAt` null / `''` / set × `textShapeVersion` null / 0 / 0.5 / 1 /
//!     2 × totals 0, 24, 25, 249, 250, 251, 10,000 × the bar ±1).
//! (b) **tier 2** — each scenario's SQL runs on a copy of the case's
//!     `base.db` (v4's `ensureCollection` built it), then ONE
//!     `check_builtin_refit` / `enqueue_refit_if_due`; compared: the result
//!     object (byte order), the captured lines (v4's `inHand` object as
//!     `inHandJson`, §R.5), and the `background_jobs` rows (minted ids masked).
//!     Both `tfidf_vocabularies` shapes (R-R) are planted: generateDDL's
//!     `REAL DEFAULT 0` and the migration's appended `INTEGER DEFAULT 0`.
//! (d) **tier 2, the DATA tick's step 10** — v4's REAL
//!     `runScheduledMaintenance` over a BUILTIN-default plant with a drifted
//!     corpus, and over a neural default: the summary's `builtinRefit` and the
//!     `EMBEDDING_REFIT` rows (the other sweeps meet tables the plant never
//!     builds; their failures are each side's own and not compared).
//! (c) **tier 2, the debounce** — v4 runs the REAL `scheduleRefit` under a
//!     virtual clock (the module's `setTimeout` / `clearTimeout` replaced
//!     before import); v5 replays each step through `schedule_refit` /
//!     `fire_due_refits` at the same virtual `now_ms`: the pending count, the
//!     fired count and the lines per step, then the jobs.
//!
//! Generate the oracle (Node 24, from the round's pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   rm -rf /tmp/qt-builtin-refit-policy /tmp/oracle-builtin-refit-policy.ndjson
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_BUILTIN_REFIT_DIR=/tmp/qt-builtin-refit-policy \
//!     $N/npx tsx $V5W/harness/oracle/cases/builtin-refit-policy.ts \
//!     > /tmp/oracle-builtin-refit-policy.ndjson
//! Run:
//!   cd $V5W
//!   QT_ORACLE_BUILTIN_REFIT_POLICY=/tmp/oracle-builtin-refit-policy.ndjson \
//!   QT_FIXTURE_BUILTIN_REFIT_DIR=/tmp/qt-builtin-refit-policy \
//!     cargo test -p quilltap-harness --test builtin_refit_policy_equivalence

mod common;

use quilltap_core::db::runtime::Db;
use quilltap_core::services::builtin_refit_policy::{
    check_builtin_refit, drift_threshold, enqueue_refit_if_due, evaluate_refit_need, RefitNeedInput,
};
use quilltap_core::services::embedding_job_scheduler::{
    clear_pending_for_tests, fire_due_refits, pending_count, schedule_refit,
};
use quilltap_core::test_support::global_capture;
use serde_json::{Map, Value};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

fn oracle() -> Option<(Vec<Value>, std::path::PathBuf)> {
    let Ok(path) = std::env::var("QT_ORACLE_BUILTIN_REFIT_POLICY") else {
        eprintln!("SKIP: set QT_ORACLE_BUILTIN_REFIT_POLICY to the oracle NDJSON (see header).");
        return None;
    };
    let Ok(dir) = std::env::var("QT_FIXTURE_BUILTIN_REFIT_DIR") else {
        eprintln!("SKIP: set QT_FIXTURE_BUILTIN_REFIT_DIR to the case's dir (see header).");
        return None;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let lines: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line"))
        .collect();
    assert!(!lines.is_empty(), "the oracle NDJSON is empty");
    Some((lines, std::path::PathBuf::from(dir)))
}

/// v4's captured line as `LEVEL message k=v …`: an object / array field takes
/// the `…Json` file-layer name (§R.5); `null` / numbers / booleans as JSON.
pub fn want_lines(lines: &Value) -> Vec<String> {
    lines
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            let mut out = format!(
                "{} {}",
                l["level"].as_str().unwrap().to_uppercase(),
                l["message"].as_str().unwrap()
            );
            for (k, v) in l["fields"].as_object().unwrap() {
                match v {
                    Value::String(s) if k == "error" => {
                        out.push_str(&format!(" {k}={}", common::normalize_v4_sqlite(s)))
                    }
                    Value::String(s) => out.push_str(&format!(" {k}={s}")),
                    Value::Object(_) | Value::Array(_) => {
                        out.push_str(&format!(" {k}Json={}", serde_json::to_string(v).unwrap()))
                    }
                    other => out.push_str(&format!(" {k}={other}")),
                }
            }
            out
        })
        .collect()
}

/// v5's captured line minus its target.
pub fn got_lines(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .map(|l| {
            let mut parts = l.splitn(3, ' ');
            let level = parts.next().unwrap().to_string();
            let _target = parts.next();
            format!("{level} {}", parts.next().unwrap_or_default())
        })
        .collect()
}

/// A `background_jobs` dump in v4's column set and order; ids the plants did
/// not name (the `e2920000-…` prefix) are masked as minted.
fn jobs_dump(db: &Db) -> Value {
    db.read_main(|c| {
        let mut stmt = c.prepare(
            r#"SELECT "id","userId","type","status","payload","priority","attempts","maxAttempts","lastError","startedAt","completedAt" FROM "background_jobs" ORDER BY "createdAt", "id""#,
        )?;
        let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let rows = stmt
            .query_map([], |r| {
                let mut row = Map::new();
                for (i, name) in names.iter().enumerate() {
                    let v = match r.get_ref(i)? {
                        rusqlite::types::ValueRef::Null => Value::Null,
                        rusqlite::types::ValueRef::Integer(n) => n.into(),
                        rusqlite::types::ValueRef::Real(f) if f.fract() == 0.0 => (f as i64).into(),
                        rusqlite::types::ValueRef::Real(f) => f.into(),
                        rusqlite::types::ValueRef::Text(t) => {
                            String::from_utf8_lossy(t).into_owned().into()
                        }
                        rusqlite::types::ValueRef::Blob(_) => "<blob>".into(),
                    };
                    row.insert(name.clone(), v);
                }
                Ok(Value::Object(row))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    // A scenario that renamed the jobs table dumps `[]`, as the oracle does.
    .map_or(Value::Array(Vec::new()), mask_ids)
}

fn mask_ids(rows: Vec<Value>) -> Value {
    Value::Array(
        rows.into_iter()
            .map(|mut r| {
                if r["id"]
                    .as_str()
                    .is_some_and(|id| !id.starts_with("e2920000"))
                {
                    r["id"] = "<minted>".into();
                }
                r
            })
            .collect(),
    )
}

/// v4's dump, its minted ids masked the same way.
fn want_jobs(v: &Value) -> Value {
    mask_ids(v.as_array().unwrap().clone())
}

async fn open_planted(dir: &std::path::Path, scratch: &std::path::Path, sql: &Value) -> Db {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let work = scratch.join(format!("work-{n}.db"));
    std::fs::copy(dir.join("base.db"), &work).expect("copy base.db");
    let db = Db::open_main(&work, PEPPER).expect("open");
    let statements: Vec<String> = sql
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect();
    db.write(move |ws| {
        for s in &statements {
            ws.main().connection().execute_batch(s)?;
        }
        Ok(())
    })
    .await
    .expect("plant");
    db
}

#[test]
fn the_pure_rule_matches_v4() {
    let Some((oracle, _)) = oracle() else { return };
    let tier1 = oracle
        .iter()
        .find(|l| l["kind"] == "tier1")
        .expect("tier1 line");
    let mut failures = Vec::new();
    let rows = tier1["rows"].as_array().unwrap();
    for row in rows {
        let input = &row["input"];
        let got_threshold = drift_threshold(input["totalMemories"].as_f64().unwrap());
        let got = evaluate_refit_need(&RefitNeedInput {
            fitted_at: input["fittedAt"].as_str().map(str::to_string),
            text_shape_version: input["textShapeVersion"].as_f64(),
            new_since_fit: input["newSinceFit"].as_f64().unwrap(),
            total_memories: input["totalMemories"].as_f64().unwrap(),
        });
        let got = got.map(|r| r.as_str());
        if got != row["reason"].as_str() || Some(got_threshold) != row["threshold"].as_f64() {
            failures.push(format!(
                "{input}: v5 {got:?} / {got_threshold}, v4 {} / {}",
                row["reason"], row["threshold"]
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} tier-1 rows diverged:\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!(
        "OK: {} tier-1 rows matched v4's evaluateRefitNeed / driftThreshold.",
        rows.len()
    );
}

#[tokio::test]
async fn the_policy_matches_v4_over_planted_instances() {
    let Some((oracle, dir)) = oracle() else {
        return;
    };
    global_capture::install();
    let scratch = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();
    let mut seen = 0usize;
    for sc in oracle.iter().filter(|l| l["kind"] == "scenario") {
        seen += 1;
        let name = sc["name"].as_str().unwrap();
        let db = open_planted(&dir, scratch.path(), &sc["sql"]).await;
        let op = &sc["op"];
        let (got, lines) = if op["kind"] == "check" {
            let (r, l) = global_capture::capture_async(check_builtin_refit(&db)).await;
            (r.to_json(), l)
        } else {
            let (r, l) = global_capture::capture_async(enqueue_refit_if_due(
                &db,
                op["profileId"].as_str().unwrap(),
                op["userId"].as_str().unwrap(),
                op["force"].as_bool().unwrap(),
                op["source"].as_str().unwrap(),
            ))
            .await;
            (r.expect("enqueue_refit_if_due").to_json(), l)
        };
        let got_s = serde_json::to_string(&got).unwrap();
        let want_s = serde_json::to_string(&sc["result"]).unwrap();
        if got_s != want_s {
            failures.push(format!("{name}: result\n  v5 {got_s}\n  v4 {want_s}"));
        }
        let (gl, wl) = (got_lines(lines), want_lines(&sc["lines"]));
        if gl != wl {
            failures.push(format!("{name}: lines\n  v5 {gl:#?}\n  v4 {wl:#?}"));
        }
        let (gj, wj) = (jobs_dump(&db), want_jobs(&sc["jobs"]));
        if gj != wj {
            failures.push(format!("{name}: background_jobs\n  v5 {gj}\n  v4 {wj}"));
        }
    }
    assert!(seen >= 20, "expected the oracle's 20 scenarios, saw {seen}");
    assert!(
        failures.is_empty(),
        "{} divergences:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    eprintln!("OK: {seen} refit-policy scenarios matched v4 (result, lines, jobs).");
}

#[tokio::test]
async fn the_debounce_matches_v4_under_a_virtual_clock() {
    let Some((oracle, dir)) = oracle() else {
        return;
    };
    global_capture::install();
    let scratch = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();
    let mut seen = 0usize;
    for sc in oracle.iter().filter(|l| l["kind"] == "debounce") {
        seen += 1;
        let name = sc["name"].as_str().unwrap();
        clear_pending_for_tests();
        let db = open_planted(&dir, scratch.path(), &sc["sql"]).await;
        for (i, step) in sc["steps"].as_array().unwrap().iter().enumerate() {
            let at = step["at"].as_i64().unwrap();
            let (fired, lines) = if let Some(s) = step.get("schedule") {
                let (r, l) = global_capture::capture_async(schedule_refit(
                    &db,
                    s["userId"].as_str().unwrap(),
                    s["profileId"].as_str(),
                    s["force"].as_bool().unwrap_or(false),
                    at,
                ))
                .await;
                r.expect("schedule_refit");
                (None, l)
            } else {
                let (n, l) = global_capture::capture_async(fire_due_refits(&db, at)).await;
                (Some(n), l)
            };
            if let Some(n) = fired {
                if Some(n as u64) != step["fired"].as_u64() {
                    failures.push(format!(
                        "{name} step {i}: fired v5 {n}, v4 {}",
                        step["fired"]
                    ));
                }
            }
            if Some(pending_count() as u64) != step["pending"].as_u64() {
                failures.push(format!(
                    "{name} step {i}: pending v5 {}, v4 {}",
                    pending_count(),
                    step["pending"]
                ));
            }
            let (gl, wl) = (got_lines(lines), want_lines(&step["lines"]));
            if gl != wl {
                failures.push(format!(
                    "{name} step {i}: lines\n  v5 {gl:#?}\n  v4 {wl:#?}"
                ));
            }
        }
        let (gj, wj) = (jobs_dump(&db), want_jobs(&sc["jobs"]));
        if gj != wj {
            failures.push(format!("{name}: background_jobs\n  v5 {gj}\n  v4 {wj}"));
        }
    }
    clear_pending_for_tests();
    assert!(
        seen >= 6,
        "expected the oracle's 6 debounce scenarios, saw {seen}"
    );
    assert!(
        failures.is_empty(),
        "{} divergences:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    eprintln!("OK: {seen} debounce scenarios matched v4's scheduleRefit.");
}

#[tokio::test]
async fn the_data_tick_step_ten_matches_v4() {
    let Some((oracle, dir)) = oracle() else {
        return;
    };
    global_capture::install();
    let scratch = tempfile::tempdir().unwrap();
    let files = scratch.path().join("files");
    std::fs::create_dir_all(&files).unwrap();
    let backend = quilltap_host::files_store::LocalStorageBackend::new(files);
    let mut failures = Vec::new();
    let mut seen = 0usize;
    for sc in oracle.iter().filter(|l| l["kind"] == "maintenance") {
        seen += 1;
        let name = sc["name"].as_str().unwrap();
        let db = open_planted(&dir, scratch.path(), &sc["sql"]).await;
        let (summary, lines) = global_capture::capture_async(
            quilltap_core::services::scheduled_maintenance::run_scheduled_maintenance(
                &db,
                quilltap_core::clock::now_unix_ms(),
                &quilltap_core::services::scheduled_maintenance::NoTranscriptStore,
                &backend,
            ),
        )
        .await;
        let got = serde_json::to_string(&summary.builtin_refit.to_json()).unwrap();
        let want = serde_json::to_string(&sc["builtinRefit"]).unwrap();
        if got != want {
            failures.push(format!("{name}: builtinRefit\n  v5 {got}\n  v4 {want}"));
        }
        // The policy's own lines, from the whole pass's capture.
        let gl: Vec<String> = got_lines(lines)
            .into_iter()
            .filter(|l| l.contains("module=embedding:builtin-refit-policy"))
            .collect();
        let wl = want_lines(&sc["lines"]);
        if gl != wl {
            failures.push(format!("{name}: lines\n  v5 {gl:#?}\n  v4 {wl:#?}"));
        }
        let gj: Vec<Value> = jobs_dump(&db)
            .as_array()
            .unwrap()
            .iter()
            .filter(|j| j["type"] == "EMBEDDING_REFIT")
            .cloned()
            .collect();
        if Value::Array(gj.clone()) != want_jobs(&sc["jobs"]) {
            failures.push(format!(
                "{name}: refit jobs\n  v5 {gj:?}\n  v4 {}",
                sc["jobs"]
            ));
        }
    }
    assert!(
        seen >= 2,
        "expected the oracle's 2 maintenance rows, saw {seen}"
    );
    assert!(
        failures.is_empty(),
        "{} divergences:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    eprintln!("OK: {seen} DATA-tick step-10 rows matched v4's runScheduledMaintenance.");
}
