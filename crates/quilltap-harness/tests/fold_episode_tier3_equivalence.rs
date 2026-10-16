//! Tier-3 differential test: the extraction **fold runner** —
//! `quilltap_core::services::extraction::fold::run_fold_pass` over BOTH fold
//! passes (`episode-fold`, `other-fold`) against v4's `runFoldPass`
//! (`lib/memory/extraction/fold.ts`) at the `83334b9a5` pin (work order
//! P4.D296 Tier 1 #12). The family keeps its pre-refactor name (L-20); its old
//! subject (`fold-episode-pass.ts`) was deleted by `ac6348c5f`.
//!
//! Both sides start from the SAME fixture pair (the shared
//! `build-memory-processor-fixture.ts` under `QT_FIXTURE_SPEC=
//! fold-episode-tier3.json`: a 300-message two-seat chat, a chat whose second
//! character is absent and whose user is a character, a chat whose only
//! character was removed, three seeded episode fragments) and run the SAME op
//! list: plant (any pass's cursor, the limits / mode rows, the rate ledger, a
//! `failContains` marker that times out every call carrying it),
//! `build_extraction_context` + `run_fold_pass` over a window cut from the
//! chat's message events (or a raw window). Model calls are pinned by exact
//! recorded call key, embeddings by exact text. Per op: the fold result (the
//! pass result whole + `skippedReason` / `slices` / `slicesDeferred` /
//! `cursorAdvancedTo`), the cursor table, the captured lines (writer-thread
//! `[ExtractionCursors] Cursor set` filtered, as the turn family); after the
//! last op the `memories` table (incl. `relatedMemoryIds` — the fragment
//! links).
//!
//! Generate (Node 24, from the v4 checkout or the lane's pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-fold-episode-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/fold-episode-tier3.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/fold-episode-tier3.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_SPEC=fold-episode-tier3.json QT_FIXTURE_OUT=/tmp/qt-fold-episode-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-fold-episode-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-memory-processor-fixture.ts
//!   QT_FIXTURE_FOLD_EPISODE_MAIN=/tmp/qt-fold-episode-main.db \
//!   QT_FIXTURE_FOLD_EPISODE_MOUNT=/tmp/qt-fold-episode-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-fold-episode.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 --roots "$PWD" --roots "$TMPO/cases" -- "fold-episode-tier3\\.test\\.ts$"
//! Run:
//!   QT_ORACLE_FOLD_EPISODE=/tmp/oracle-fold-episode.ndjson \
//!   QT_FIXTURE_FOLD_EPISODE_MAIN=/tmp/qt-fold-episode-main.db \
//!   QT_FIXTURE_FOLD_EPISODE_MOUNT=/tmp/qt-fold-episode-mount.db \
//!     cargo test -p quilltap-harness --test fold_episode_tier3_equivalence

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::memory_extraction_cursors::ExtractionCursorPass;
use quilltap_core::db::runtime::Db;
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionMessage, CompletionRole, CompletionUsage,
};
use quilltap_core::model::embedding::{CannedEmbeddingProvider, EmbeddingResult};
use quilltap_core::services::admission::{
    record_admission_insert, reset_admission_state_for_tests, reset_rate_ledger_for_tests,
};
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::extraction::context::{
    build_extraction_context, BuildExtractionContextOptions,
};
use quilltap_core::services::extraction::fold::{run_fold_pass, FoldResult, RunFoldInput};
use quilltap_core::services::extraction::run::ExtractionDeps;
use quilltap_core::services::memory_time_zone::reset_memory_time_zone_cache_for_tests;
use quilltap_core::services::turn_transcript::load_chat_message_events;
use serde_json::{json, Value};

#[path = "admission_support/mod.rs"]
mod admission_support;
use admission_support::{minted_ids, oracle_lines, planted_stamps, render, render_v4, same, Norm};

fn spec_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/fold-episode-tier3.json")
}

const PREFIXES: &[&str] = &[
    "[Memory]",
    "[ExtractionCursors]",
    "[MemorySettings]",
    "[MemoryCalendar]",
    "[Canon]",
];

/// The op's recorded calls (op-scoped: a slice timed out in one op is the
/// same call key when a later op re-reads it and succeeds).
fn completion_from(oracle: &[Value], op: &str) -> CannedCompletionProvider {
    let mut completion = CannedCompletionProvider::new();
    for row in oracle
        .iter()
        .filter(|l| l["kind"] == "canned" && l["op"] == op)
    {
        let messages: Vec<CompletionMessage> = row["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| CompletionMessage {
                role: CompletionRole::from_v4_wire(m["role"].as_str().unwrap()).unwrap(),
                content: m["content"].as_str().unwrap().to_string(),
            })
            .collect();
        let (provider, model) = (
            row["provider"].as_str().unwrap(),
            row["model"].as_str().unwrap(),
        );
        let temperature = row["temperature"].as_f64();
        completion = match row["failure"].as_str() {
            Some(message) => {
                completion.with_failure(provider, model, temperature, &messages, message)
            }
            None => completion.with_response(
                provider,
                model,
                temperature,
                &messages,
                row["response"].as_str().unwrap(),
                Some(CompletionUsage {
                    prompt_tokens: row["usage"]["promptTokens"].as_i64().unwrap(),
                    completion_tokens: row["usage"]["completionTokens"].as_i64().unwrap(),
                    total_tokens: row["usage"]["totalTokens"].as_i64().unwrap(),
                }),
            ),
        };
    }
    completion
}

fn embedding_from(oracle: &[Value]) -> CannedEmbeddingProvider {
    let mut provider = CannedEmbeddingProvider::new();
    for line in oracle.iter().filter(|l| l["kind"] == "embedding") {
        let embedding: Vec<f32> = line["embedding"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_f64().unwrap() as f32)
            .collect();
        provider = provider.with_result(
            line["text"].as_str().unwrap(),
            EmbeddingResult {
                dimensions: embedding.len(),
                embedding,
                model: "canned".into(),
                provider: "canned".into(),
            },
        );
    }
    provider
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

async fn exec(db: &Db, sql: String, args: Vec<String>) {
    db.write(move |w| {
        w.main()
            .connection()
            .execute(&sql, rusqlite::params_from_iter(args.iter()))
            .map_err(quilltap_core::db::DbError::from)
    })
    .await
    .expect("plant");
}

/// The Rust result in the oracle's shape.
fn result_value(r: &FoldResult) -> Value {
    let p = &r.result;
    json!({
        "pass": r.pass,
        "calls": p.calls,
        "timedOut": p.timed_out,
        "failed": p.failed,
        "admitted": p.admitted.iter().map(|a| json!({
            "candidate": serde_json::to_value(&a.candidate).unwrap(),
            "occurredAt": a.occurred_at,
            "narrativeTime": a.narrative_time,
            "holderName": a.holder_name,
            "aboutName": a.about_name,
            "action": a.outcome.action.kind().as_str(),
        })).collect::<Vec<_>>(),
        "createdIds": p.created_ids,
        "reinforcedIds": p.reinforced_ids,
        "usage": {
            "promptTokens": p.usage.prompt_tokens,
            "completionTokens": p.usage.completion_tokens,
            "totalTokens": p.usage.total_tokens,
        },
        "debugLogs": p.debug_logs,
        "rateDeferred": p.rate_deferred,
        "skippedReason": r.skipped_reason.map(|s| s.as_str()),
        "slices": r.slices,
        "slicesDeferred": r.slices_deferred,
        "cursorAdvancedTo": r.cursor_advanced_to,
    })
}

fn cursors_of(db: &Db) -> Value {
    db.read_main(|c| {
        let mut stmt = c.prepare(
            r#"SELECT "id", "chatId", "pass", "messageId" FROM "memory_extraction_cursors" ORDER BY "id""#,
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "chatId": r.get::<_, String>(1)?,
                    "pass": r.get::<_, String>(2)?,
                    "messageId": r.get::<_, String>(3)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Value::from(rows))
    })
    .unwrap()
}

/// The oracle case stubs `@/lib/memory/maintenance/triggers` to
/// `maybeEnqueueMaintenance: async () => []`, so v4's admission fires a trigger
/// that reads nothing and says nothing. v5 runs the real one (wired at the
/// `83334b9a5` unification, v4 `admit.ts:405-409`); with both switches off its
/// only observable is `resolveMemorySettings()`'s line with NO user — dropped
/// here as the stub's twin. The trigger's own lines are the maintenance
/// families' business.
fn is_stubbed_maintenance_trigger_line(line: &str) -> bool {
    line.starts_with("DEBUG [MemorySettings] Resolved memory settings userId=null")
}

#[tokio::test(flavor = "current_thread")]
async fn extraction_fold_tier3_matches_oracle() {
    let env = |k: &str| std::env::var(k).ok();
    let (Some(oracle_path), Some(main), Some(mount)) = (
        env("QT_ORACLE_FOLD_EPISODE"),
        env("QT_FIXTURE_FOLD_EPISODE_MAIN"),
        env("QT_FIXTURE_FOLD_EPISODE_MOUNT"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_FOLD_EPISODE / QT_FIXTURE_FOLD_EPISODE_MAIN / _MOUNT (see header)."
        );
        return;
    };
    let spec: Value =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("read spec")).unwrap();
    let oracle = oracle_lines(&oracle_path);
    let want_ops: Vec<&Value> = oracle.iter().filter(|l| l["kind"] == "op").collect();
    assert_eq!(
        want_ops.len(),
        spec["ops"].as_array().unwrap().len(),
        "op-count mismatch — regenerate the oracle"
    );

    let dir = tempfile::Builder::new()
        .prefix("qt-extraction-fold-rust-")
        .tempdir()
        .unwrap();
    let paths = admission_support::copy_pair(&main, &mount, dir.path());
    let db =
        Db::open(paths, spec["testPepperBase64"].as_str().unwrap()).expect("open fixture copy");
    // The builder's fresh tables predate three migration-only column pairs
    // the v5 chat reads name (the repaired-at-boot idiom).
    db.write(|w| {
        let conn = w.main().connection();
        quilltap_core::test_support::ensure_p4d171_columns(conn);
        quilltap_core::test_support::ensure_p4d182_columns(conn);
        quilltap_core::test_support::ensure_p4d225_columns(conn);
        Ok(())
    })
    .await
    .unwrap();

    let embedding = embedding_from(&oracle);
    let executor = CheapLlmTaskExecutor::new();

    use tracing_subscriber::layer::SubscriberExt;
    let logs = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(admission_support::Capture {
        lines: logs.clone(),
        owned: &[],
        prefixes: PREFIXES,
    });
    let _guard = tracing::subscriber::set_default(subscriber);

    reset_admission_state_for_tests();
    reset_rate_ledger_for_tests();
    reset_memory_time_zone_cache_for_tests();

    let user_id = spec["userId"].as_str().unwrap().to_string();
    let base_cheap = serde_json::to_string(&spec["chatSettings"]["cheapLLMSettings"]).unwrap();
    let mut got_ops: Vec<(String, Value, Value, Vec<String>)> = Vec::new();
    for op in spec["ops"].as_array().unwrap() {
        let name = op["name"].as_str().unwrap().to_string();
        let chat_id = op["chatId"].as_str().unwrap().to_string();
        let empty = json!({});
        let plants = op.get("plants").unwrap_or(&empty);
        let opts = op.get("opts").unwrap_or(&empty);
        if let Some(cursors) = plants.get("cursor").and_then(Value::as_object) {
            for (pass, message_id) in cursors {
                match message_id.as_str() {
                    None => {
                        exec(
                            &db,
                            r#"DELETE FROM "memory_extraction_cursors" WHERE "chatId" = ?1 AND "pass" = ?2"#.into(),
                            vec![chat_id.clone(), pass.clone()],
                        )
                        .await
                    }
                    Some(m) => {
                        let (c, m) = (chat_id.clone(), m.to_string());
                        let pass = ExtractionCursorPass::parse(pass).expect("cursor pass");
                        db.write(move |w| {
                            w.main().memory_extraction_cursors().set_cursor(&c, pass, &m)
                        })
                        .await
                        .unwrap();
                    }
                }
            }
        }
        for (key, field) in [
            ("memoryExtractionLimits", "limits"),
            ("memoryExtractionMode", "mode"),
        ] {
            if let Some(v) = plants.get(field) {
                exec(
                    &db,
                    r#"INSERT INTO "instance_settings" ("key", "value") VALUES (?1, ?2)"#.into(),
                    vec![key.into(), serde_json::to_string(v).unwrap()],
                )
                .await;
            }
        }
        if let Some(v) = plants.get("cheapLLMSettings") {
            exec(
                &db,
                r#"UPDATE "chat_settings" SET "cheapLLMSettings" = ?1"#.into(),
                vec![serde_json::to_string(v).unwrap()],
            )
            .await;
        }
        if let Some(ledger) = plants.get("ledger").and_then(Value::as_object) {
            for (character_id, n) in ledger {
                for _ in 0..n.as_u64().unwrap() {
                    record_admission_insert(
                        character_id,
                        quilltap_core::clock::now_unix_ms() as f64,
                    );
                }
            }
        }

        let completion = completion_from(&oracle, &name);
        let deps = ExtractionDeps {
            completion: &completion,
            embedding: &embedding,
            executor: &executor,
        };
        logs.lock().unwrap().clear();
        let window: Vec<Value> = match op.get("windowRaw") {
            Some(raw) => raw.as_array().unwrap().clone(),
            None => {
                let all = load_chat_message_events(&db, &chat_id).expect("message events");
                let (a, b) = (
                    op["window"][0].as_u64().unwrap() as usize,
                    op["window"][1].as_u64().unwrap() as usize,
                );
                all[a.min(all.len())..b.min(all.len())].to_vec()
            }
        };
        let max_slices = match &op["maxSlices"] {
            Value::String(s) if s == "Infinity" => Some(f64::INFINITY),
            v => v.as_f64(),
        };
        let result = match build_extraction_context(
            &db,
            &chat_id,
            &user_id,
            BuildExtractionContextOptions {
                dry_run: opts["dryRun"].as_bool() == Some(true),
                backfill: opts["backfill"].as_bool() == Some(true),
                host_zone_fallback: "UTC".into(),
                ..Default::default()
            },
        ) {
            Ok(ctx) => {
                let r = run_fold_pass(
                    &deps,
                    &ctx,
                    op["pass"].as_str().unwrap(),
                    RunFoldInput {
                        window_messages: window,
                        cover_through_message_id: s(op, "coverThrough"),
                        max_slices,
                    },
                )
                .await;
                result_value(&r)
            }
            Err(e) => json!({ "error": e.message() }),
        };
        let captured = logs.lock().unwrap().clone();

        for key in ["memoryExtractionLimits", "memoryExtractionMode"] {
            exec(
                &db,
                r#"DELETE FROM "instance_settings" WHERE "key" = ?1"#.into(),
                vec![key.into()],
            )
            .await;
        }
        if plants.get("cheapLLMSettings").is_some() {
            exec(
                &db,
                r#"UPDATE "chat_settings" SET "cheapLLMSettings" = ?1"#.into(),
                vec![base_cheap.clone()],
            )
            .await;
        }
        got_ops.push((name, result, cursors_of(&db), captured));
    }

    // ---- normalisation: minted ids tokenised per side by row content ------
    let mut seeded: HashSet<String> = HashSet::new();
    for m in spec["seedMemories"].as_array().unwrap() {
        seeded.insert(m["id"].as_str().unwrap().to_string());
    }
    let mut planted: HashSet<String> = HashSet::new();
    planted_stamps(&spec, &mut planted);
    let got_dump = db
        .read_main(|c| dump_table_json_conn(c, "memories", "content"))
        .unwrap();
    let want_dump = oracle
        .iter()
        .find(|l| l["kind"] == "table" && l["table"] == "memories")
        .expect("oracle memories dump")
        .clone();
    // describeAdmission's neighbour notes print `id.slice(0, 8)`: tokenise the
    // prefixes too, after the whole ids (memory note `unify-review-finding-shapes`
    // — prefix pins).
    let with_prefixes = |mut minted: Vec<(String, String)>| {
        let prefixes: Vec<(String, String)> = minted
            .iter()
            .map(|(raw, token)| (raw[..8].to_string(), format!("{token}#8")))
            .collect();
        minted.extend(prefixes);
        minted
    };
    let got_norm = Norm {
        minted: with_prefixes(minted_ids(&got_dump, &seeded)),
        planted: planted.clone(),
    };
    let want_norm = Norm {
        minted: with_prefixes(minted_ids(&want_dump, &seeded)),
        planted: planted.clone(),
    };

    let mut failures: Vec<String> = Vec::new();
    for ((name, result, cursors, captured), want) in got_ops.iter().zip(&want_ops) {
        assert_eq!(
            want["op"].as_str(),
            Some(name.as_str()),
            "op order mismatch"
        );
        let g = got_norm.value(result, None);
        let w = want_norm.value(&want["result"], None);
        if !same(&g, &w) {
            failures.push(format!(
                "{name}: result diverges\n  rust:   {g:#}\n  oracle: {w:#}"
            ));
        }
        if cursors != &want["cursors"] {
            failures.push(format!(
                "{name}: cursors diverge\n  rust:   {cursors}\n  oracle: {}",
                want["cursors"]
            ));
        }
        let want_lines: Vec<String> = want["lines"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|l| l["message"] != "[ExtractionCursors] Cursor set")
            .map(|l| want_norm.ids(&render_v4(l)))
            .collect();
        let got_lines: Vec<String> = captured
            .iter()
            .filter(|l| !is_stubbed_maintenance_trigger_line(l))
            .map(|l| got_norm.ids(l))
            .collect();
        if got_lines != want_lines {
            failures.push(format!(
                "{name}: captured lines diverge\n  rust:\n    {}\n  oracle:\n    {}",
                got_lines.join("\n    "),
                want_lines.join("\n    ")
            ));
        }
    }

    let rows_got: Vec<Value> = got_dump["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| got_norm.row(r))
        .collect();
    let rows_want: Vec<Value> = want_dump["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| want_norm.row(r))
        .collect();
    if rows_got.len() != rows_want.len() {
        failures.push(format!(
            "memories: row count {} vs oracle {}",
            rows_got.len(),
            rows_want.len()
        ));
    } else {
        for (g, w) in rows_got.iter().zip(&rows_want) {
            if !same(g, w) {
                failures.push(format!(
                    "memories row diverges\n  rust:   {g}\n  oracle: {w}"
                ));
            }
        }
    }
    let _ = render; // the shared renderer is the capture layer's
    assert!(
        failures.is_empty(),
        "{} divergence(s):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    eprintln!(
        "OK: extraction fold tier-3 matched the oracle ({} ops, {} memories).",
        got_ops.len(),
        rows_got.len()
    );
}
