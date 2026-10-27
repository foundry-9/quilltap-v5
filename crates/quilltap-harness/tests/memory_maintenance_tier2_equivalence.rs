//! P4.D297 tier-2 differential: the maintenance run's LLM-free phases —
//! `quilltap_core::services::memory_maintenance::run::run_maintenance` against
//! v4's REAL `runMaintenance` (`lib/memory/maintenance/run.ts`, `104e986c7`, at
//! the `83334b9a5` pin), in process (the parent: writes land as they are made,
//! the run announces itself — `MaintenanceMode::InProcess`).
//!
//! The runs (`harness/oracle/fixtures/memory-maintenance-tier2-runs.json`) run
//! IN ORDER on ONE copy of the maintenance fixture pair
//! (`build-maintenance-fixture.ts`) on each side, each at the spec's clock
//! (`MaintenanceContext.now` — v4's own seam). Covered: the fold dry / live
//! (a spent budget — `timeBudgetMs: 0` puts the fold's deadline at the run's
//! start, so every group is deferred on both sides; survivor-first,
//! non-transitive, the date guard, the episodic / MANUAL /
//! cold twins; the survivor's footnotes, entities, links and count landing
//! through admission's `apply_fold`, the absorbed rows retired), the demotion
//! dry / live / again (exactly `demotable − cap`, lowest protection first, a
//! NULL tier read hot, the mixed-case read order), the retention dry / live
//! (`supersededAt` and its `updatedAt` fallback, AUTO only), the whole
//! automatic run with consolidation off, the archived / unknown skip arms,
//! the per-phase toggles, the missing retention window, the maintenance switch
//! off for a watermark run and a manual exemption through it. INVARIANT 10 is
//! asserted on both sides: each dry run's report equals its live twin's
//! (ignoring the clock fields).
//!
//! Comparands, per run: the whole report (`finishedAt`, `durationMs` and each
//! phase's `durationMs` normalized), every `module: 'memory:maintenance*'`
//! line (level, message, every field in v4's order; arrays as `…Json`;
//! `durationMs` masked) and the façade's `[MemorySettings]` line by TAG only
//! (P4.D288's bytes). At the end: the `memories` table (`updatedAt` of a
//! written row is wall-clock — normalized; vectors as hex BLOBs).
//!
//! Generate (Node 24, from the v4 checkout or the round's pin — a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=/tmp/qt-maintenance-tier2-oracle
//!   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-maintenance-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-maintenance-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-maintenance-fixture.ts
//!   rm -f /tmp/qt-maintenance-tier2.ndjson
//!   QT_FIXTURE_MAINTENANCE_MAIN=/tmp/qt-maintenance-main.db \
//!   QT_FIXTURE_MAINTENANCE_MOUNT=/tmp/qt-maintenance-mount.db \
//!   QT_ORACLE_OUT=/tmp/qt-maintenance-tier2.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$M/cases" -- memory-maintenance-tier2
//!   cd $V5W
//!   QT_ORACLE_MAINTENANCE_TIER2=/tmp/qt-maintenance-tier2.ndjson \
//!   QT_FIXTURE_MAINTENANCE_MAIN=/tmp/qt-maintenance-main.db \
//!   QT_FIXTURE_MAINTENANCE_MOUNT=/tmp/qt-maintenance-mount.db \
//!     cargo test -p quilltap-harness --test memory_maintenance_tier2_equivalence -- --nocapture

use std::path::{Path, PathBuf};

use quilltap_core::db::instance_settings::{
    set_memory_consolidation_settings, set_memory_maintenance_settings,
};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::CannedCompletionProvider;
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::memory_maintenance::ledger::reset_maintenance_ledger_for_tests;
use quilltap_core::services::memory_maintenance::run::run_maintenance;
use quilltap_core::services::memory_maintenance::types::{
    MaintenanceContext, MaintenanceMode, MaintenanceReason, MaintenanceRequest,
    MaintenanceRunSettings,
};
use quilltap_core::test_support::global_capture;
use serde_json::{json, Value};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
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

/// The report with its wall-clock fields normalized.
fn normalize_report(mut r: Value) -> Value {
    r["finishedAt"] = json!("<ts>");
    r["durationMs"] = json!(0);
    if let Some(phases) = r["phases"].as_array_mut() {
        for p in phases {
            p["durationMs"] = json!(0);
        }
    }
    r
}

fn mask_duration(s: &str) -> String {
    match s.find("durationMs=") {
        Some(i) => {
            let tail = &s[i + "durationMs=".len()..];
            let end = tail.find(' ').unwrap_or(tail.len());
            format!("{}durationMs=<ms>{}", &s[..i], &tail[end..])
        }
        None => s.to_string(),
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
            other => out.push_str(&format!(" {k}={other}")),
        }
    }
    mask_duration(&out)
}

fn strip_target(line: &str) -> String {
    let mut parts = line.splitn(3, ' ');
    let level = parts.next().unwrap_or_default();
    let _ = parts.next();
    mask_duration(&format!("{level} {}", parts.next().unwrap_or_default()))
}

const SETTINGS_TAG: &str = "[MemorySettings] Resolved memory settings";

fn sql_rows(db: &Db, sql: &str) -> Vec<Value> {
    db.read_main(|conn| {
        let mut stmt = conn.prepare(sql)?;
        let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let rows = stmt.query_map([], |row| {
            let mut m = serde_json::Map::new();
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

fn strings(v: Option<&Value>) -> Option<Vec<String>> {
    v.and_then(Value::as_array)
        .map(|a| a.iter().map(|x| x.as_str().unwrap().to_string()).collect())
}

/// P4.168 (P-5, the planner's ruling) — `MAINTENANCE_BATCH_LOADER_STRICT`,
/// pinned both ways: with every `memories` row read failing, v4's
/// `findByCharacterIdInBatches` page falls back to `[]` (`Error finding
/// entities by filter`) and the run plans from an empty — in general a
/// TRUNCATED — corpus; v5 logs v4's line and fails the run.
const MAINTENANCE_BATCH_LOADER_STRICT: &str = "corpus_read_fails_strict";

/// P4.168: run a run's `sqlBefore` / `sqlAfter` plant statements.
async fn exec_all(db: &Db, statements: Option<&Value>) {
    for sql in statements.and_then(Value::as_array).into_iter().flatten() {
        let sql = sql.as_str().unwrap().to_string();
        db.write(move |w| {
            w.main()
                .connection()
                .execute_batch(&sql)
                .map_err(quilltap_core::db::DbError::from)
        })
        .await
        .expect("plant");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn memory_maintenance_tier2_matches_oracle() {
    let (Ok(oracle_path), Ok(fixture_main), Ok(fixture_mount)) = (
        std::env::var("QT_ORACLE_MAINTENANCE_TIER2"),
        std::env::var("QT_FIXTURE_MAINTENANCE_MAIN"),
        std::env::var("QT_FIXTURE_MAINTENANCE_MOUNT"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_MAINTENANCE_TIER2 + QT_FIXTURE_MAINTENANCE_MAIN / _MOUNT (see header).");
        return;
    };
    global_capture::install();
    reset_maintenance_ledger_for_tests();
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("memory-maintenance-tier2.json")).unwrap(),
    )
    .unwrap();
    let runs: Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("memory-maintenance-tier2-runs.json"))
            .unwrap(),
    )
    .unwrap();
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle {oracle_path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();

    let mut embedding = CannedEmbeddingProvider::new();
    let mut embedding_rows = 0;
    for row in oracle.iter().filter(|o| o["kind"] == "embedding") {
        embedding_rows += 1;
        embedding = embedding.with_vector(
            row["text"].as_str().unwrap().to_string(),
            row["vector"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap() as f32)
                .collect(),
        );
    }
    let completion = CannedCompletionProvider::new();
    let executor = CheapLlmTaskExecutor::new();

    let scratch = tempfile::Builder::new()
        .prefix("qt-maintenance-tier2-rust-")
        .tempdir()
        .unwrap();
    let main = scratch.path().join("main.db");
    let mount = scratch.path().join("mount.db");
    std::fs::copy(&fixture_main, &main).unwrap();
    std::fs::copy(&fixture_mount, &mount).unwrap();
    let pepper = spec["testPepperBase64"].as_str().unwrap();
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        pepper,
    )
    .expect("open the fixture copies");
    let now_ms = quilltap_core::clock::iso_to_ms(spec["now"].as_str().unwrap()).unwrap();

    let oracle_runs: Vec<&Value> = oracle.iter().filter(|o| o["kind"] == "run").collect();
    let runs = runs["runs"].as_array().unwrap();
    assert_eq!(
        runs.len(),
        oracle_runs.len(),
        "run count — regenerate the oracle"
    );
    let mut failures: Vec<String> = Vec::new();
    let mut lines_compared = 0usize;
    let mut reports: Vec<(String, Value, Value)> = Vec::new();
    for (run, want) in runs.iter().zip(oracle_runs) {
        let name = run["name"].as_str().unwrap();
        assert_eq!(want["name"], name, "run order");
        if let Some(plant) = run.get("plant") {
            if let Some(m) = plant.get("maintenance").cloned() {
                db.write(move |w| set_memory_maintenance_settings(w.main().connection(), &m))
                    .await
                    .unwrap();
            }
            if let Some(c) = plant.get("consolidation").cloned() {
                db.write(move |w| set_memory_consolidation_settings(w.main().connection(), &c))
                    .await
                    .unwrap();
            }
            exec_all(&db, plant.get("sqlBefore")).await;
        }
        let holder = run["holder"].as_str().unwrap();
        let character_id = if holder == "unknown" {
            spec["unknownId"].as_str().unwrap()
        } else {
            spec["holders"][holder]["id"].as_str().unwrap()
        };
        let r = &run["request"];
        let request = MaintenanceRequest {
            character_id: character_id.to_string(),
            phases: strings(r.get("phases")),
            reason: MaintenanceReason::parse(r["reason"].as_str().unwrap()).unwrap(),
            exempt_phases: strings(r.get("exemptPhases")),
            dry_run: r.get("dryRun") == Some(&json!(true)),
            settings: r
                .get("settings")
                .and_then(MaintenanceRunSettings::from_value),
        };
        let ctx = MaintenanceContext {
            user_id: None,
            time_budget_ms: run["ctx"]["timeBudgetMs"].as_i64(),
            now_ms: Some(now_ms),
            mode: MaintenanceMode::InProcess,
            elapsed: None,
            host_zone_fallback: "UTC".to_string(),
        };
        let (run_result, captured) = global_capture::capture_async(Box::pin(run_maintenance(
            &db,
            &completion,
            &embedding,
            &executor,
            request,
            ctx,
        )))
        .await;
        exec_all(&db, run.get("plant").and_then(|p| p.get("sqlAfter"))).await;
        if name == MAINTENANCE_BATCH_LOADER_STRICT {
            // Both ways: v4 answers the failed page `[]` (its inner
            // `findByFilter` line) and the run completes over an EMPTY
            // corpus; v5 logs v4's line and fails the run.
            let line = |l: &str| {
                l.starts_with("ERROR Error finding entities by filter collection=memories ")
            };
            let v4_line = want["repoLines"]
                .as_array()
                .is_some_and(|ls| ls.iter().map(render_oracle_line).any(|l| line(&l)));
            let v4_completed = want["report"].is_object();
            let v5_line = captured.iter().map(|l| strip_target(l)).any(|l| line(&l));
            match (&run_result, v4_completed && v4_line && v5_line) {
                (Err(e), true) => eprintln!(
                    "[{name}] MAINTENANCE_BATCH_LOADER_STRICT holds (v4 ran on an empty corpus; v5 failed: {e})."
                ),
                (r, _) => failures.push(format!(
                    "{name}: MAINTENANCE_BATCH_LOADER_STRICT moved\n  v4 completed {v4_completed} / line {v4_line}\n  v5 {:?} / line {v5_line}",
                    r.as_ref().map(|_| "completed")
                )),
            }
            continue;
        }
        let run_result = run_result.unwrap_or_else(|e| panic!("{name}: run failed: {e}"));
        let got = normalize_report(run_result.report.to_json());
        let expected = normalize_report(want["report"].clone());
        if !approx_eq(&got, &expected) {
            failures.push(format!("{name}: report\n  v5 {got}\n  v4 {expected}"));
        }
        reports.push((name.to_string(), got, expected));
        let got_lines: Vec<String> = captured
            .iter()
            .map(|l| strip_target(l))
            .filter(|l| l.contains(" module=memory:maintenance"))
            .collect();
        let want_lines: Vec<String> = want["lines"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|l| l["message"] != SETTINGS_TAG)
            .map(render_oracle_line)
            .collect();
        lines_compared += want_lines.len();
        if got_lines != want_lines {
            failures.push(format!(
                "{name}: lines\n  v5 {got_lines:#?}\n  v4 {want_lines:#?}"
            ));
        }
        // The façade's line by TAG (P4.D288's bytes): present iff v4 logged it.
        let got_tag = captured.iter().filter(|l| l.contains(SETTINGS_TAG)).count();
        let want_tag = want["lines"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|l| l["message"] == SETTINGS_TAG)
            .count();
        if got_tag != want_tag {
            failures.push(format!(
                "{name}: [MemorySettings] tag v5 {got_tag} v4 {want_tag}"
            ));
        }
    }

    // ── invariant 10: a dry run reports what its live twin does ────────────
    for (dry, live) in [
        ("fold_dry", "fold_live"),
        ("demote_dry", "demote_live"),
        ("retire_dry", "retire_live"),
        ("mixed_dry", "mixed_live"),
    ] {
        let find = |n: &str| reports.iter().find(|(name, _, _)| name == n).unwrap();
        for side in [1, 2] {
            let pick = |t: &(String, Value, Value)| -> Value {
                let mut v = if side == 1 { t.1.clone() } else { t.2.clone() };
                v["dryRun"] = json!(null);
                v
            };
            let (d, l) = (pick(find(dry)), pick(find(live)));
            if !approx_eq(&d, &l) {
                failures.push(format!(
                    "invariant 10 ({}): {dry} ≠ {live}\n  dry  {d}\n  live {l}",
                    if side == 1 { "v5" } else { "v4" }
                ));
            }
        }
    }

    // ── the memories table after every run ─────────────────────────────────
    let want_rows = oracle
        .iter()
        .find(|o| o["kind"] == "table")
        .expect("oracle memories table")["rows"]
        .clone();
    let got_rows = Value::Array(sql_rows(
        &db,
        "SELECT * FROM memories ORDER BY characterId, id",
    ));
    let norm = |rows: &Value| -> Value {
        Value::Array(
            rows.as_array()
                .unwrap()
                .iter()
                .map(|r| {
                    let mut r = r.clone();
                    r["updatedAt"] = json!("<ts>");
                    r
                })
                .collect(),
        )
    };
    let (g, w) = (norm(&got_rows), norm(&want_rows));
    if !approx_eq(&g, &w) {
        let (ga, wa) = (g.as_array().unwrap(), w.as_array().unwrap());
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

    reset_maintenance_ledger_for_tests();
    assert!(
        failures.is_empty(),
        "{} failure(s):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert!(embedding_rows >= 2, "embeddings {embedding_rows}");
    assert!(lines_compared >= 50, "lines compared: {lines_compared}");
    eprintln!(
        "OK: memory maintenance tier-2 matched oracle ({} runs, {lines_compared} lines, {embedding_rows} re-embeds, the memories table; invariant 10 both sides).",
        runs.len()
    );
}
