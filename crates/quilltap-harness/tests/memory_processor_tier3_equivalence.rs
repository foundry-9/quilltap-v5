//! Tier-3 differential test: the extraction **turn grain** —
//! `quilltap_core::services::extraction::{context, route, rate_limit, cursors,
//! run, passes, turn}` against v4's ONE pipeline at the `83334b9a5` pin (work
//! order P4.D296 Tier 1 #5–#11). The family keeps its pre-refactor name (the
//! order's L-20); its old subject (`processTurnForMemory`) was deleted by
//! `ac6348c5f`.
//!
//! Both sides start from the SAME fixture pair (the grown
//! `build-memory-processor-fixture.ts`: three characters, three profiles, a
//! chat-settings row, a three-seat chat and an autonomous room with stamped
//! messages, the cursor and `instance_settings` tables) and run the SAME op
//! list: plant (the `turn` cursor, the limits / mode rows, the cheap-LLM
//! block, the rate ledger), `build_extraction_context` +
//! `run_turn_extraction`. The model calls are pinned by exact call key (the
//! oracle RECORDS every `provider|model|temperature|messages` entry it
//! answered, incl. the one timed-out call; this test replays exactly those,
//! so any prompt / selection divergence is a canned-miss), the embeddings by
//! exact text (the oracle records each text's vector). Then per op:
//!
//!   1. the turn result — calls, timeouts, failures, every admitted
//!      candidate (the whole `MemoryCandidate` + its anchors + the action
//!      kind), created / reinforced ids, usage, the debug lines BYTE-FOR-BYTE
//!      (minted memory ids tokenised by row content on each side), the source
//!      message, `hadContributions`;
//!   2. the cursor table after the op;
//!   3. the captured lines (`[Memory]`, `[ExtractionCursors] Advanced`,
//!      `[MemorySettings]`, `[MemoryCalendar]`, `[Canon]` — the latter three
//!      are sibling lanes' lines, compared whole here because they ride the
//!      same unit of work). `[ExtractionCursors] Cursor set` is P4.D288's line
//!      emitted on the WRITER thread, invisible to this thread-scoped capture
//!      (memory note `capture-rig-writer-thread`): filtered from the v4 side.
//!
//! and, after the last op, the `memories` table.
//!
//! Generate (Node 24, from the v4 checkout or the lane's pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-memory-processor-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/memory-processor-tier3.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/memory-processor-tier3.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-memory-processor-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-memory-processor-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-memory-processor-fixture.ts
//!   QT_FIXTURE_PROCESSOR_MAIN=/tmp/qt-memory-processor-main.db \
//!   QT_FIXTURE_PROCESSOR_MOUNT=/tmp/qt-memory-processor-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-memory-processor.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 --roots "$PWD" --roots "$TMPO/cases" -- "memory-processor-tier3\\.test\\.ts$"
//! Run:
//!   QT_ORACLE_PROCESSOR=/tmp/oracle-memory-processor.ndjson \
//!   QT_FIXTURE_PROCESSOR_MAIN=/tmp/qt-memory-processor-main.db \
//!   QT_FIXTURE_PROCESSOR_MOUNT=/tmp/qt-memory-processor-mount.db \
//!     cargo test -p quilltap-harness --test memory_processor_tier3_equivalence

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
use quilltap_core::services::extraction::run::ExtractionDeps;
use quilltap_core::services::extraction::turn::{run_turn_extraction, TurnLocator};
use quilltap_core::services::memory_time_zone::reset_memory_time_zone_cache_for_tests;
use serde_json::{json, Value};

#[path = "admission_support/mod.rs"]
mod admission_support;
use admission_support::{minted_ids, oracle_lines, planted_stamps, render, render_v4, same, Norm};

fn spec_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/memory-processor-tier3.json")
}

const PREFIXES: &[&str] = &[
    "[Memory]",
    "[ExtractionCursors]",
    "[MemorySettings]",
    "[MemoryCalendar]",
    "[Canon]",
];

fn completion_from(oracle: &[Value]) -> CannedCompletionProvider {
    let mut completion = CannedCompletionProvider::new();
    for row in oracle.iter().filter(|l| l["kind"] == "canned") {
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
fn result_value(r: &quilltap_core::services::extraction::turn::TurnExtractionResult) -> Value {
    let p = &r.result;
    json!({
        "pass": p.pass.as_str(),
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
        "sourceMessageId": r.source_message_id,
        "hadContributions": r.had_contributions,
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

#[tokio::test(flavor = "current_thread")]
async fn extraction_turn_tier3_matches_oracle() {
    let env = |k: &str| std::env::var(k).ok();
    let (Some(oracle_path), Some(main), Some(mount)) = (
        env("QT_ORACLE_PROCESSOR"),
        env("QT_FIXTURE_PROCESSOR_MAIN"),
        env("QT_FIXTURE_PROCESSOR_MOUNT"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_PROCESSOR / QT_FIXTURE_PROCESSOR_MAIN / _MOUNT (see header)."
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
        .prefix("qt-extraction-turn-rust-")
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

    let completion = completion_from(&oracle);
    let embedding = embedding_from(&oracle);
    let executor = CheapLlmTaskExecutor::new();
    let deps = ExtractionDeps {
        completion: &completion,
        embedding: &embedding,
        executor: &executor,
    };

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
        if let Some(cursor) = plants.get("cursor") {
            match cursor.as_str() {
                None => {
                    exec(
                        &db,
                        r#"DELETE FROM "memory_extraction_cursors" WHERE "chatId" = ?1 AND "pass" = 'turn'"#.into(),
                        vec![chat_id.clone()],
                    )
                    .await
                }
                Some(m) => {
                    let (c, m) = (chat_id.clone(), m.to_string());
                    db.write(move |w| {
                        w.main()
                            .memory_extraction_cursors()
                            .set_cursor(&c, ExtractionCursorPass::Turn, &m)
                    })
                    .await
                    .unwrap();
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

        if let Some(content) = plants.get("failContent").and_then(Value::as_str) {
            exec(
                &db,
                format!(
                    r#"CREATE TRIGGER "qt_fail_admission" BEFORE INSERT ON "memories" WHEN NEW."content" = '{content}' BEGIN SELECT RAISE(ABORT, 'planted admission failure'); END"#
                ),
                vec![],
            )
            .await;
        }

        logs.lock().unwrap().clear();
        let locator = TurnLocator {
            turn_opener_message_id: s(&op["locator"], "turnOpenerMessageId"),
            extraction_anchor_message_id: s(&op["locator"], "extractionAnchorMessageId"),
            extract_after_message_id: s(&op["locator"], "extractAfterMessageId"),
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
            Ok(ctx) => match run_turn_extraction(&deps, &ctx, &locator).await {
                Ok(r) => result_value(&r),
                Err(e) => json!({ "error": quilltap_core::db::fallback::error_text(&e) }),
            },
            Err(e) => json!({ "error": e.message() }),
        };
        let captured = logs.lock().unwrap().clone();
        if plants.get("failContent").is_some() {
            exec(&db, r#"DROP TRIGGER "qt_fail_admission""#.into(), vec![]).await;
        }

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
        let got_lines: Vec<String> = captured.iter().map(|l| got_norm.ids(l)).collect();
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
        "OK: extraction turn tier-3 matched the oracle ({} ops, {} memories).",
        got_ops.len(),
        rows_got.len()
    );
}
