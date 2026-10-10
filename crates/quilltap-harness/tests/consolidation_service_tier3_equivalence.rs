//! P4.D271 tier-3 differential: the consolidation service (v4
//! `runConsolidation`, `lib/memory/consolidation.ts`, byte-identical at the
//! `01a83539d` pin) against `quilltap_core::services::consolidation::
//! run_consolidation`, on the `fold-episode-tier3` shape.
//!
//! Both sides pin BOTH model calls identically: the completion by exact call
//! key (the jest oracle RECORDS each `provider|model|temperature|messages` it
//! answered — incl. the thrown and timed-out ones — and this test replays them
//! through `CannedCompletionProvider`, so a prompt-byte or routing divergence
//! surfaces as a canned miss), and the embedding by the exact
//! `buildMemoryEmbeddingText` text (recorded, incl. the one that fails).
//!
//! The fifteen runs (`harness/oracle/fixtures/consolidation-service-runs.json`)
//! run IN ORDER on ONE copy of the consolidation fixture pair on each side.
//! Comparands, per run: the whole report (`finishedAt` and `durationMs`
//! normalized; minted digest ids remapped by first appearance) and every
//! captured line — the service's `memory:consolidation` lines, the task's
//! `[Consolidation] Sending cluster to model`, the repository's `Moved memories
//! between tiers`, the bridge's lines (level, message, every context field in
//! v4's order; minted ids masked; `durationMs` masked). At the end: `memories`,
//! `vector_entries`, `vector_indices` (minted ids remapped, `updatedAt`
//! normalized), the `Commonplace/` documents, and `llm_logs` row counts by type
//! (v5's ruled extra failure rows — P4.13 unit 6 — split out and asserted
//! both ways).
//!
//! The executor-side writer-thread lines (R-D) need EVERY thread's events, so
//! this binary installs a process-global rig and holds ONE test.
//!
//! Generate the fixture + oracle output (Node 24, from the v4 checkout or the
//! round's pin — a /tmp mirror, jest ignores `.claude/` paths):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=/tmp/qt-consolidation-oracle
//!   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-consolidation-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-consolidation-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-consolidation-fixture.ts
//!   rm -f /tmp/qt-consolidation-service.ndjson
//!   QT_FIXTURE_CONSOLIDATION_MAIN=/tmp/qt-consolidation-main.db \
//!   QT_FIXTURE_CONSOLIDATION_MOUNT=/tmp/qt-consolidation-mount.db \
//!   QT_ORACLE_OUT=/tmp/qt-consolidation-service.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$M/cases" -- consolidation-service-tier3
//!   cd $V5W
//!   QT_ORACLE_CONSOLIDATION_SERVICE=/tmp/qt-consolidation-service.ndjson \
//!   QT_FIXTURE_CONSOLIDATION_MAIN=/tmp/qt-consolidation-main.db \
//!   QT_FIXTURE_CONSOLIDATION_MOUNT=/tmp/qt-consolidation-mount.db \
//!     cargo test -p quilltap-harness --test consolidation_service_tier3_equivalence -- --nocapture

mod common;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionMessage, CompletionRole, CompletionUsage,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::services::cheap_llm_exec::{CheapLlmLogConfig, CheapLlmTaskExecutor};
use quilltap_core::services::consolidation::{
    run_consolidation, ConsolidationTrigger, RunConsolidationOptions,
};
use quilltap_core::services::llm_logging::LogContext;
use serde_json::{json, Value};

const CAPTURED_MESSAGES: &[&str] = &[
    "[Consolidation] Sending cluster to model",
    "Moved memories between tiers",
    "Skipping Commonplace mirror: holder not found",
    "Skipping Commonplace mirror for archived character",
    "Skipping Commonplace mirror: holder has no character vault",
    "Wrote Commonplace digest mirror",
    "Failed to write a Commonplace digest mirror",
    "Commonplace mirror failed",
    "Commonplace mirror complete",
    "Document absent from database-backed store",
];

/// A process-global capture of EVERY thread's events while armed (the
/// `fold_episode_tier3` rig) — safe because this binary holds one test.
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

/// Every 36-char UUID in `s` that is not a committed value (spec text, or the
/// deterministic `0d…` template ids) → `<id>`.
fn mask_minted(s: &str, known: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if i + 36 <= bytes.len() && s.is_char_boundary(i) && s.is_char_boundary(i + 36) {
            let cand = &s[i..i + 36];
            let is_uuid = cand.bytes().enumerate().all(|(j, c)| match j {
                8 | 13 | 18 | 23 => c == b'-',
                _ => c.is_ascii_hexdigit(),
            });
            if is_uuid {
                let committed = known.contains(cand) || cand.starts_with("0d");
                out.push_str(if committed { cand } else { "<id>" });
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

/// `LEVEL message k=v …` — the target dropped (v4 has none), `durationMs` masked.
fn strip_target(line: &str) -> String {
    let mut parts = line.splitn(3, ' ');
    let level = parts.next().unwrap_or_default();
    let _target = parts.next();
    let rest = parts.next().unwrap_or_default();
    let rest = mask_duration(rest);
    format!("{level} {rest}")
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
    if let Some(ctx) = line["context"].as_object() {
        for (k, v) in ctx {
            let rendered = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            out.push_str(&format!(" {k}={rendered}"));
        }
    }
    mask_duration(&out)
}

fn wanted_line(line: &str) -> bool {
    let rest = line.splitn(3, ' ').nth(2).unwrap_or_default();
    rest.contains(" module=memory:consolidation")
        || CAPTURED_MESSAGES.iter().any(|m| {
            rest.strip_prefix(m)
                .is_some_and(|t| t.is_empty() || t.starts_with(' '))
        })
}

/// Remap every minted UUID in a JSON value, by first appearance, onto `M<n>`.
fn remap_minted(v: &Value, known: &str, map: &mut HashMap<String, String>) -> Value {
    match v {
        Value::String(s) => {
            let masked = mask_minted(s, known);
            if !masked.contains("<id>") {
                return v.clone();
            }
            // Re-walk `s` replacing each minted id with its stable token.
            let mut out = String::new();
            let mut i = 0;
            while i < s.len() {
                if i + 36 <= s.len() && s.is_char_boundary(i) && s.is_char_boundary(i + 36) {
                    let cand = &s[i..i + 36];
                    if mask_minted(cand, known) == "<id>" {
                        let next = format!("M{}", map.len());
                        out.push_str(map.entry(cand.to_string()).or_insert(next));
                        i += 36;
                        continue;
                    }
                }
                let ch = s[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
            Value::String(out)
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

/// Structural equality, numbers at 1e-12 (REAL vs JS integer included).
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
    r["stats"]["durationMs"] = json!(0);
    r
}

fn options(o: &Value) -> RunConsolidationOptions {
    RunConsolidationOptions {
        dry_run: o["dryRun"].as_bool().unwrap(),
        trigger: ConsolidationTrigger::parse(o["trigger"].as_str().unwrap()).unwrap(),
        now_ms: Some(quilltap_core::clock::iso_to_ms(o["now"].as_str().unwrap()).unwrap()),
        user_id: o.get("userId").and_then(Value::as_str).map(str::to_string),
        settings_override: o.get("settings").cloned(),
        max_clusters_per_run: o.get("maxClustersPerRun").and_then(Value::as_f64),
        time_budget_ms: o.get("timeBudgetMs").and_then(Value::as_i64),
        ..Default::default()
    }
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

fn normalize_rows(
    rows: &[Value],
    ts_cols: &[&str],
    known: &str,
    map: &mut HashMap<String, String>,
) -> Value {
    Value::Array(
        rows.iter()
            .map(|r| {
                let mut r = remap_minted(r, known, map);
                for c in ts_cols {
                    if r.get(*c).is_some_and(|v| !v.is_null()) {
                        r[*c] = json!("<ts>");
                    }
                }
                r
            })
            .collect(),
    )
}

#[tokio::test(flavor = "current_thread")]
async fn consolidation_service_tier3_matches_oracle() {
    let (Ok(oracle_path), Ok(fixture_main), Ok(fixture_mount)) = (
        std::env::var("QT_ORACLE_CONSOLIDATION_SERVICE"),
        std::env::var("QT_FIXTURE_CONSOLIDATION_MAIN"),
        std::env::var("QT_FIXTURE_CONSOLIDATION_MOUNT"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_CONSOLIDATION_SERVICE + QT_FIXTURE_CONSOLIDATION_MAIN / _MOUNT (see header).");
        return;
    };
    let spec_text =
        std::fs::read_to_string(fixtures_dir().join("consolidation-tier3.json")).unwrap();
    let spec: Value = serde_json::from_str(&spec_text).unwrap();
    let runs: Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("consolidation-service-runs.json")).unwrap(),
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
        .prefix("qt-consolidation-service-rust-")
        .tempdir()
        .unwrap();
    let main = scratch.path().join("main.db");
    let mount = scratch.path().join("mount.db");
    let llm = scratch.path().join("llm-logs.db");
    std::fs::copy(&fixture_main, &main).unwrap();
    std::fs::copy(&fixture_mount, &mount).unwrap();
    let pepper = spec["testPepperBase64"].as_str().unwrap();
    common::materialize_llm_logs(&llm, pepper);
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: Some(llm),
        },
        pepper,
    )
    .expect("open the fixture copies");
    let _canned_key = quilltap_core::test_support::CannedCheapLlmKey::install("test-key");
    let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
        db: db.clone(),
        user_id: spec["userId"].as_str().unwrap().to_string(),
        chat_id: None,
        message_id: None,
        ctx: LogContext::none(),
    });
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
    for (run, want) in runs.iter().zip(oracle_runs) {
        let name = run["name"].as_str().unwrap();
        assert_eq!(want["name"], name, "run order");
        let (report, captured) = all_threads::capture(run_consolidation(
            &db,
            &completion,
            &embedding,
            &executor,
            run["characterId"].as_str().unwrap(),
            options(&run["opts"]),
        ))
        .await;
        let report = report.unwrap_or_else(|e| panic!("{name}: run failed: {e}"));
        let mut got_map = HashMap::new();
        let mut want_map = HashMap::new();
        let got = remap_minted(
            &normalize_report(report.to_json()),
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
        lines_compared += want_lines.len();
        if got_lines != want_lines {
            failures.push(format!(
                "{name}: lines\n  v5 {got_lines:#?}\n  v4 {want_lines:#?}"
            ));
        }
    }

    // ── tables ───────────────────────────────────────────────────────────────
    let mut got_map = HashMap::new();
    let mut want_map = HashMap::new();
    for (table, sql, ts) in [
        (
            "memories",
            "SELECT * FROM memories ORDER BY characterId, content, id",
            &["updatedAt"][..],
        ),
        (
            "vector_entries",
            "SELECT * FROM vector_entries ORDER BY characterId, embedding",
            &["createdAt"][..],
        ),
        (
            "vector_indices",
            "SELECT * FROM vector_indices ORDER BY id",
            &["createdAt", "updatedAt"][..],
        ),
    ] {
        let want = oracle
            .iter()
            .find(|o| o["kind"] == "table" && o["table"] == table)
            .unwrap_or_else(|| panic!("oracle missing table {table}"));
        let g = normalize_rows(&sql_rows(&db, sql, false), ts, &spec_text, &mut got_map);
        let w = normalize_rows(
            want["rows"].as_array().unwrap(),
            ts,
            &spec_text,
            &mut want_map,
        );
        if !approx_eq(&g, &w) {
            let ga = g.as_array().unwrap();
            let wa = w.as_array().unwrap();
            let first = ga
                .iter()
                .zip(wa.iter())
                .position(|(a, b)| !approx_eq(a, b))
                .unwrap_or(ga.len().min(wa.len()));
            failures.push(format!(
                "table {table}: {} vs {} rows, first divergence at {first}:\n  v5 {}\n  v4 {}",
                ga.len(),
                wa.len(),
                ga.get(first).cloned().unwrap_or(Value::Null),
                wa.get(first).cloned().unwrap_or(Value::Null)
            ));
        }
    }
    let docs = sql_rows(
        &db,
        "SELECT l.mountPointId, l.relativePath, d.content FROM doc_mount_file_links l \
         JOIN doc_mount_documents d ON d.fileId = l.fileId \
         WHERE l.relativePath LIKE 'Commonplace/%' ORDER BY l.mountPointId, l.relativePath",
        true,
    );
    let docs: Vec<Value> = docs
        .into_iter()
        .map(|r| json!([r["mountPointId"], r["relativePath"], r["content"]]))
        .collect();
    let want_docs = oracle
        .iter()
        .find(|o| o["kind"] == "projection")
        .expect("oracle commonplace projection");
    if Value::Array(docs.clone()) != want_docs["rows"] {
        failures.push(format!(
            "commonplace documents:\n  v5 {docs:?}\n  v4 {}",
            want_docs["rows"]
        ));
    }

    // ── llm_logs: v4's rows by type; v5's ruled extra failure rows split out ──
    let (faithful, failed) = common::split_ruled_failed_call_rows(common::dump_llm_logs(&db));
    let mut by_type: Vec<(String, i64)> = Vec::new();
    for r in &faithful {
        let t = r["type"].as_str().unwrap_or_default().to_string();
        match by_type.iter_mut().find(|(k, _)| *k == t) {
            Some((_, n)) => *n += 1,
            None => by_type.push((t, 1)),
        }
    }
    by_type.sort();
    let got_logs: Vec<Value> = by_type
        .iter()
        .map(|(t, n)| json!({"type": t, "n": n}))
        .collect();
    let want_logs = &oracle
        .iter()
        .find(|o| o["kind"] == "llmLogs")
        .expect("llmLogs row")["rows"];
    if &Value::Array(got_logs.clone()) != want_logs {
        failures.push(format!("llm_logs by type: v5 {got_logs:?} v4 {want_logs}"));
    }

    // The ruled divergence asserted BOTH ways: exactly the `failures` run's
    // three failed provider calls (`provider exploded`, and the timed-out call
    // plus its one retry) carry a v5-only error row; v4 logs none.
    if failed.len() != 3 {
        failures.push(format!(
            "ruled v5-only failure rows: {} (expected 3)",
            failed.len()
        ));
    }
    assert!(
        failures.is_empty(),
        "{} failure(s):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert!(
        canned_rows >= 30 && embedding_rows >= 8,
        "canned {canned_rows} embeddings {embedding_rows}"
    );
    assert!(lines_compared >= 100, "lines compared: {lines_compared}");
    eprintln!(
        "OK: consolidation service tier-3 matched oracle ({} runs, {lines_compared} lines, 3 tables, the Commonplace documents, llm_logs {:?}; {} ruled v5-only failure rows).",
        runs.len(),
        got_logs,
        failed.len()
    );
}
