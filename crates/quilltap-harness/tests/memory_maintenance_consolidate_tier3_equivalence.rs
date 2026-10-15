//! P4.D297 tier-3 differential: the maintenance run's `consolidate` + `mirror`
//! phases — `run_maintenance` (`services::memory_maintenance`) over the
//! reshaped `services::consolidation::consolidate` — against v4's REAL
//! `runMaintenance` at the `83334b9a5` pin (`lib/memory/maintenance/` +
//! `lib/memory/consolidation.ts`, `104e986c7`; bug 246 from `64743ee13`;
//! `tfidfClusterThreshold` from `4ae0c4ca6`). The reshaped successor of
//! P4.D271's `consolidation_service_tier3` (retired with v4's
//! `runConsolidation`).
//!
//! Both sides pin BOTH model calls identically: the completion by exact call
//! key (the jest oracle RECORDS each `provider|model|temperature|messages` it
//! answered — the thrown, timed-out and unanswered ones too — and this test
//! replays them through `CannedCompletionProvider`, so a prompt-byte or
//! routing divergence surfaces as a canned miss), and the embedding by the
//! exact digest text (`embeddingTextFor` on the memory calendar; recorded,
//! incl. the one that fails). The cheap-LLM executor logs nothing on either
//! side (v4's `logLLMCall` is mocked out — zero `llm_logs` rows).
//!
//! The runs (`harness/oracle/fixtures/memory-maintenance-consolidate-runs.json`)
//! run IN ORDER on ONE copy of the consolidation fixture pair on each side,
//! each at its own clock, with the SQL plants (bug 246's profile budgets, bug
//! 211's demoted rows, a Built-in default embedding profile) run identically
//! before the run. The Job-mode run reports the mirror `deferredToCommit` and
//! then each side runs its commit hook: v4's `mirrorCommonplaceAfterCommit`
//! over the batch's digest writes, v5's `mirror_commonplace_after_job` over
//! the stashed subjects (L-1 / L-2).
//!
//! Comparands, per run: the whole report (the clock fields normalized; minted
//! digest ids remapped by first appearance) and every captured line — the
//! `memory:maintenance*` and `memory:consolidation` lines, the task's root
//! `[Consolidation] Sending cluster to model` (bug 246's `maxTokens` its last
//! field) and the after-commit hook's no-bucket DEBUG (level, message, every
//! field in v4's order; arrays as `…Json`; minted ids and `durationMs`
//! masked). At the end: `memories` (minted ids remapped, `updatedAt`
//! normalized) and the `Commonplace/` documents.
//!
//! The writer-thread lines (`Digest created` / `Digest revised in place`, R-D)
//! need EVERY thread's events, so this binary installs a process-global rig
//! and holds ONE test.
//!
//! Generate the fixture + oracle output (Node 24, from the v4 checkout or the
//! round's pin — a /tmp mirror, jest ignores `.claude/` paths):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=/tmp/qt-maintenance-tier3-oracle
//!   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-maintenance-cons-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-maintenance-cons-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-consolidation-fixture.ts
//!   rm -f /tmp/qt-maintenance-tier3.ndjson
//!   QT_FIXTURE_CONSOLIDATION_MAIN=/tmp/qt-maintenance-cons-main.db \
//!   QT_FIXTURE_CONSOLIDATION_MOUNT=/tmp/qt-maintenance-cons-mount.db \
//!   QT_ORACLE_OUT=/tmp/qt-maintenance-tier3.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$M/cases" -- memory-maintenance-consolidate-tier3
//!   cd $V5W
//!   QT_ORACLE_MAINTENANCE_TIER3=/tmp/qt-maintenance-tier3.ndjson \
//!   QT_FIXTURE_CONSOLIDATION_MAIN=/tmp/qt-maintenance-cons-main.db \
//!   QT_FIXTURE_CONSOLIDATION_MOUNT=/tmp/qt-maintenance-cons-mount.db \
//!     cargo test -p quilltap-harness --test memory_maintenance_consolidate_tier3_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionMessage, CompletionRole, CompletionUsage,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::memory_maintenance::after_job::mirror_commonplace_after_job;
use quilltap_core::services::memory_maintenance::ledger::reset_maintenance_ledger_for_tests;
use quilltap_core::services::memory_maintenance::run::run_maintenance;
use quilltap_core::services::memory_maintenance::types::{
    MaintenanceContext, MaintenanceMode, MaintenanceReason, MaintenanceRequest,
    MaintenanceRunSettings,
};
use serde_json::{json, Value};

const CAPTURED_MESSAGES: &[&str] = &[
    "[Consolidation] Sending cluster to model",
    "Committed maintenance batch touched no mirrored bucket",
];

/// A process-global capture of EVERY thread's events while armed — safe
/// because this binary holds one test.
mod all_threads {
    use std::sync::Mutex;

    static BUF: Mutex<Option<Vec<String>>> = Mutex::new(None);

    struct Layer;

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Layer {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let mut guard = BUF.lock().unwrap();
            let Some(buf) = guard.as_mut() else { return };
            let meta = event.metadata();
            let mut visitor = quilltap_core::test_support::FieldVisitor(format!(
                "{} {}",
                meta.level(),
                meta.target()
            ));
            event.record(&mut visitor);
            buf.push(visitor.0);
        }
    }

    pub fn install() {
        use tracing_subscriber::layer::SubscriberExt;
        tracing::subscriber::set_global_default(tracing_subscriber::registry().with(Layer))
            .expect("this binary's one global subscriber");
    }

    pub async fn capture<T>(f: impl std::future::Future<Output = T>) -> (T, Vec<String>) {
        *BUF.lock().unwrap() = Some(Vec::new());
        let out = f.await;
        let lines = BUF.lock().unwrap().take().expect("armed");
        (out, lines)
    }
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
}

/// A committed id (spec text, the `000000…` template rows) stays; any other
/// UUID is minted → `<id>`.
fn is_committed(cand: &str, known: &str) -> bool {
    known.contains(cand) || cand.starts_with("000000")
}

fn is_uuid(cand: &str) -> bool {
    cand.len() == 36
        && cand.bytes().enumerate().all(|(j, c)| match j {
            8 | 13 | 18 | 23 => c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

fn mask_minted(s: &str, known: &str) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < s.len() {
        if i + 36 <= s.len() && s.is_char_boundary(i) && s.is_char_boundary(i + 36) {
            let cand = &s[i..i + 36];
            if is_uuid(cand) {
                out.push_str(if is_committed(cand, known) {
                    cand
                } else {
                    "<id>"
                });
                i += 36;
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Remap every minted UUID in a JSON value, by first appearance, onto `M<n>`.
fn remap_minted(v: &Value, known: &str, map: &mut HashMap<String, String>) -> Value {
    match v {
        Value::String(s) => {
            let mut out = String::new();
            let mut i = 0;
            let mut changed = false;
            while i < s.len() {
                if i + 36 <= s.len() && s.is_char_boundary(i) && s.is_char_boundary(i + 36) {
                    let cand = &s[i..i + 36];
                    if is_uuid(cand) && !is_committed(cand, known) {
                        let next = format!("M{}", map.len());
                        out.push_str(map.entry(cand.to_string()).or_insert(next));
                        i += 36;
                        changed = true;
                        continue;
                    }
                }
                let ch = s[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
            if changed {
                Value::String(out)
            } else {
                v.clone()
            }
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| remap_minted(x, known, map)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| (k.clone(), remap_minted(x, known, map)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Structural equality, numbers at 1e-12, objects in KEY ORDER.
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

fn normalize_report(mut r: Value) -> Value {
    r["finishedAt"] = json!("<ts>");
    r["durationMs"] = json!(0);
    if let Some(phases) = r["phases"].as_array_mut() {
        for p in phases {
            p["durationMs"] = json!(0);
        }
    }
    if r.get("consolidation").is_some() {
        r["consolidation"]["finishedAt"] = json!("<ts>");
        r["consolidation"]["stats"]["durationMs"] = json!(0);
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

fn strip_target(line: &str) -> String {
    let mut parts = line.splitn(3, ' ');
    let level = parts.next().unwrap_or_default();
    let _target = parts.next();
    mask_duration(&format!("{level} {}", parts.next().unwrap_or_default()))
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

fn wanted_line(line: &str) -> bool {
    let rest = line.splitn(3, ' ').nth(2).unwrap_or_default();
    rest.contains(" module=memory:maintenance")
        || rest.contains(" module=memory:consolidation ")
        || rest.ends_with(" module=memory:consolidation")
        || CAPTURED_MESSAGES.iter().any(|m| {
            rest.strip_prefix(m)
                .is_some_and(|t| t.is_empty() || t.starts_with(' '))
        })
}

fn sql_rows(db: &Db, sql: &str, mount: bool) -> Vec<Value> {
    let run = |conn: &rusqlite::Connection| -> Result<Vec<Value>, quilltap_core::db::DbError> {
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
    };
    if mount {
        db.read_mount_index(run).expect("mount query")
    } else {
        db.read_main(run).expect("main query")
    }
}

fn strings(v: Option<&Value>) -> Option<Vec<String>> {
    v.and_then(Value::as_array)
        .map(|a| a.iter().map(|x| x.as_str().unwrap().to_string()).collect())
}

#[tokio::test(flavor = "current_thread")]
async fn memory_maintenance_consolidate_tier3_matches_oracle() {
    let (Ok(oracle_path), Ok(fixture_main), Ok(fixture_mount)) = (
        std::env::var("QT_ORACLE_MAINTENANCE_TIER3"),
        std::env::var("QT_FIXTURE_CONSOLIDATION_MAIN"),
        std::env::var("QT_FIXTURE_CONSOLIDATION_MOUNT"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_MAINTENANCE_TIER3 + QT_FIXTURE_CONSOLIDATION_MAIN / _MOUNT (see header).");
        return;
    };
    reset_maintenance_ledger_for_tests();
    let spec_text =
        std::fs::read_to_string(fixtures_dir().join("consolidation-tier3.json")).unwrap();
    let spec: Value = serde_json::from_str(&spec_text).unwrap();
    let runs: Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("memory-maintenance-consolidate-runs.json"))
            .unwrap(),
    )
    .unwrap();
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle {oracle_path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();

    let mut completion = CannedCompletionProvider::new();
    let mut canned_rows = 0;
    for row in oracle.iter().filter(|o| o["kind"] == "canned") {
        canned_rows += 1;
        let messages: Vec<CompletionMessage> = row["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| CompletionMessage {
                role: CompletionRole::from_v4_wire(m["role"].as_str().unwrap()).unwrap(),
                content: m["content"].as_str().unwrap().to_string(),
            })
            .collect();
        let provider = row["provider"].as_str().unwrap();
        let model = row["model"].as_str().unwrap();
        let temperature = row["temperature"].as_f64();
        completion = match row.get("fail").and_then(Value::as_str) {
            Some(fail) => completion.with_failure(provider, model, temperature, &messages, fail),
            None => completion.with_response(
                provider,
                model,
                temperature,
                &messages,
                row["response"].as_str().unwrap().to_string(),
                Some(CompletionUsage {
                    prompt_tokens: row["usage"]["promptTokens"].as_i64().unwrap(),
                    completion_tokens: row["usage"]["completionTokens"].as_i64().unwrap(),
                    total_tokens: row["usage"]["totalTokens"].as_i64().unwrap(),
                }),
            ),
        };
    }
    let mut embedding = CannedEmbeddingProvider::new();
    let mut embedding_rows = 0;
    for row in oracle.iter().filter(|o| o["kind"] == "embedding") {
        embedding_rows += 1;
        let text = row["text"].as_str().unwrap().to_string();
        embedding = match row["vector"].as_array() {
            Some(v) => {
                embedding.with_vector(text, v.iter().map(|x| x.as_f64().unwrap() as f32).collect())
            }
            None => embedding.with_failure(text),
        };
    }

    let scratch = tempfile::Builder::new()
        .prefix("qt-maintenance-tier3-rust-")
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
    let _canned_key = quilltap_core::test_support::CannedCheapLlmKey::install("test-key");
    let executor = CheapLlmTaskExecutor::new();
    all_threads::install();

    let oracle_runs: Vec<&Value> = oracle.iter().filter(|o| o["kind"] == "run").collect();
    let runs = runs["runs"].as_array().unwrap();
    assert_eq!(
        runs.len(),
        oracle_runs.len(),
        "run count — regenerate the oracle"
    );
    let mut failures: Vec<String> = Vec::new();
    let mut lines_compared = 0usize;
    let mut max_tokens_seen: Vec<String> = Vec::new();
    for (run, want) in runs.iter().zip(oracle_runs) {
        let name = run["name"].as_str().unwrap();
        assert_eq!(want["name"], name, "run order");
        for sql in run["plantSql"].as_array().cloned().unwrap_or_default() {
            let sql = sql.as_str().unwrap().to_string();
            db.write(move |w| Ok(w.main().connection().execute_batch(&sql)?))
                .await
                .unwrap();
        }
        let character_id = run["characterId"].as_str().unwrap().to_string();
        let r = &run["request"];
        let request = MaintenanceRequest {
            character_id: character_id.clone(),
            phases: strings(r.get("phases")),
            reason: MaintenanceReason::parse(r["reason"].as_str().unwrap()).unwrap(),
            exempt_phases: strings(r.get("exemptPhases")),
            dry_run: r.get("dryRun") == Some(&json!(true)),
            settings: r
                .get("settings")
                .and_then(MaintenanceRunSettings::from_value),
        };
        let job = run.get("mode").and_then(Value::as_str) == Some("job");
        let ctx = MaintenanceContext {
            user_id: run
                .get("userId")
                .and_then(Value::as_str)
                .map(str::to_string),
            time_budget_ms: None,
            now_ms: Some(quilltap_core::clock::iso_to_ms(run["now"].as_str().unwrap()).unwrap()),
            mode: if job {
                MaintenanceMode::Job
            } else {
                MaintenanceMode::InProcess
            },
            elapsed: None,
            host_zone_fallback: "UTC".to_string(),
        };
        let payload = {
            let mut p = serde_json::Map::new();
            p.insert("characterId".into(), json!(character_id));
            if let Value::Object(o) = r {
                for (k, v) in o {
                    p.insert(k.clone(), v.clone());
                }
            }
            Value::Object(p)
        };
        let (result, captured) = all_threads::capture(async {
            let run = Box::pin(run_maintenance(
                &db,
                &completion,
                &embedding,
                &executor,
                request,
                ctx,
            ))
            .await;
            if let (true, Ok(done)) = (job, &run) {
                let touched: Vec<(String, Vec<String>)> = if done.touched_subjects.is_empty() {
                    Vec::new()
                } else {
                    vec![(character_id.clone(), done.touched_subjects.clone())]
                };
                mirror_commonplace_after_job(&db, "MEMORY_MAINTENANCE", &payload, &touched).await;
            }
            run
        })
        .await;
        let result = result.unwrap_or_else(|e| panic!("{name}: run failed: {e}"));
        let mut got_map = HashMap::new();
        let mut want_map = HashMap::new();
        let got = remap_minted(
            &normalize_report(result.report.to_json()),
            &spec_text,
            &mut got_map,
        );
        let expected = remap_minted(
            &normalize_report(want["report"].clone()),
            &spec_text,
            &mut want_map,
        );
        if !approx_eq(&got, &expected) {
            failures.push(format!("{name}: report\n  v5 {got}\n  v4 {expected}"));
        }
        let got_lines: Vec<String> = captured
            .iter()
            .filter(|l| wanted_line(l))
            .map(|l| mask_minted(&strip_target(l), &spec_text))
            .collect();
        let want_lines: Vec<String> = want["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| mask_minted(&render_oracle_line(l), &spec_text))
            .collect();
        for l in &got_lines {
            if let Some(i) = l.find(" maxTokens=") {
                max_tokens_seen.push(l[i + 1..].to_string());
            }
        }
        lines_compared += want_lines.len();
        if got_lines != want_lines {
            failures.push(format!(
                "{name}: lines\n  v5 {got_lines:#?}\n  v4 {want_lines:#?}"
            ));
        }
    }

    // ── the memories table ──────────────────────────────────────────────────
    let mut got_map = HashMap::new();
    let mut want_map = HashMap::new();
    let want = oracle
        .iter()
        .find(|o| o["kind"] == "table")
        .expect("oracle memories table");
    let norm = |rows: &[Value], map: &mut HashMap<String, String>| -> Value {
        Value::Array(
            rows.iter()
                .map(|r| {
                    let mut r = remap_minted(r, &spec_text, map);
                    r["updatedAt"] = json!("<ts>");
                    // The repository's `setTier` stamps `supersededAt` with its
                    // own wall clock on both sides (bug 225), not the run's.
                    if !r["supersededAt"].is_null() {
                        r["supersededAt"] = json!("<ts>");
                    }
                    r
                })
                .collect(),
        )
    };
    let g = norm(
        &sql_rows(
            &db,
            "SELECT * FROM memories ORDER BY characterId, content, id",
            false,
        ),
        &mut got_map,
    );
    let w = norm(want["rows"].as_array().unwrap(), &mut want_map);
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

    // ── the Commonplace/ documents ──────────────────────────────────────────
    let docs: Vec<Value> = sql_rows(
        &db,
        "SELECT l.mountPointId, l.relativePath, d.content FROM doc_mount_file_links l \
         JOIN doc_mount_documents d ON d.fileId = l.fileId \
         WHERE l.relativePath LIKE 'Commonplace/%' ORDER BY l.relativePath, d.content",
        true,
    )
    .into_iter()
    .map(|r| json!([r["mountPointId"], r["relativePath"], r["content"]]))
    .collect();
    let want_docs = &oracle
        .iter()
        .find(|o| o["kind"] == "projection")
        .expect("oracle commonplace projection")["rows"];
    // The after-commit hook mirrors at the wall clock (v4 `new Date()`), the
    // in-process runs at their own clock: every frontmatter `updatedAt` masked.
    let mask = |v: &Value| -> Value {
        let text = mask_minted(&v.to_string(), &spec_text);
        let re = regex::Regex::new(r#"updatedAt: \\"[0-9T:.\-Z]+\\""#).unwrap();
        serde_json::from_str(&re.replace_all(&text, "updatedAt: <ts>")).unwrap()
    };
    if mask(&Value::Array(docs.clone())) != mask(want_docs) {
        failures.push(format!(
            "commonplace documents:\n  v5 {docs:?}\n  v4 {want_docs}"
        ));
    }

    reset_maintenance_ledger_for_tests();
    assert!(
        failures.is_empty(),
        "{} failure(s):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    // Bug 246's three budgets reached the line (the profile's own, the class
    // default, the floor).
    for budget in ["maxTokens=12000", "maxTokens=8000", "maxTokens=4000"] {
        assert!(
            max_tokens_seen.iter().any(|m| m == budget),
            "{budget} never sent: {max_tokens_seen:?}"
        );
    }
    assert!(
        canned_rows >= 30 && embedding_rows >= 8,
        "canned {canned_rows} embeddings {embedding_rows}"
    );
    assert!(lines_compared >= 150, "lines compared: {lines_compared}");
    eprintln!(
        "OK: memory maintenance consolidate tier-3 matched oracle ({} runs, {lines_compared} lines, {canned_rows} canned calls, {embedding_rows} embeddings, the memories table, the Commonplace documents).",
        runs.len()
    );
}
