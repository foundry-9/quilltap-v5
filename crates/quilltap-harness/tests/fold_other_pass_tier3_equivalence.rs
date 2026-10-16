//! Tier-3 differential test: the fold **catch-ups** —
//! `quilltap_core::services::extraction::{catchup, fold_catchup}` and
//! `services::memory_extraction_job::handle_memory_extraction`'s pass branch —
//! against v4's `findFoldCatchupCandidates` / `runFoldCatchup`
//! (`lib/memory/extraction/catchup.ts`), `enqueueFoldCatchups`
//! (`lib/background-jobs/maintenance/fold-catchup.ts`, bug 232's in-flight
//! guard) and `handleMemoryExtraction` at the `83334b9a5` pin (work order
//! P4.D296 Tier 1 #13–#14). The family keeps its pre-refactor name (L-20); its
//! old subject (`fold-other-pass.ts`) was deleted by `ac6348c5f`.
//!
//! Both sides start from the SAME fixture pair (the shared
//! `build-memory-processor-fixture.ts` under `QT_FIXTURE_SPEC=
//! fold-other-pass-tier3.json`: eight chats — a 300-message folded chat, an
//! idle never-folded one, an active one, a help chat, a one-message chat, one
//! past the 30-day lookback, one with no seat profile, one already caught up)
//! and run the SAME op list: the two selections (incl. a limit of one and
//! `otherPass: turn`), four sweeps (the in-flight guard; a cap of zero), five
//! catch-ups (`never-folded`, `not-idle`, `ignoreIdle`, both
//! `behind-cursor`s) and nine handler jobs (a capped backlog's continuation and
//! the continuation itself; the legacy `foldEpisodeCatchup` /
//! `foldOtherCatchup` + `foldOtherIgnoreIdle` keys; an active chat; a timeout
//! failing the job; L-4's `bogus` pass run twice; a chat that is gone).
//! Compared per op: the selection / sweep summary / fold result / the job's
//! error, the `background_jobs` rows (type, payload BYTES, status, priority,
//! attempts), the cursor table, and every captured line (`[FoldCatchup]`,
//! `[MemoryExtraction]`, the sweep's un-prefixed `maintenance.fold-catchup`
//! lines with their `module` binding, and the pipeline's `[Memory]` family).
//! Then `UNKNOWN_PASS_CURSOR_ROW` (L-4, both ways) and the `memories` table.
//!
//! Generate (Node 24, from the v4 checkout or the lane's pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-fold-catchup-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/fold-other-pass-tier3.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/fold-other-pass-tier3.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_SPEC=fold-other-pass-tier3.json QT_FIXTURE_OUT=/tmp/qt-fold-catchup-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-fold-catchup-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-memory-processor-fixture.ts
//!   QT_FIXTURE_FOLD_CATCHUP_MAIN=/tmp/qt-fold-catchup-main.db \
//!   QT_FIXTURE_FOLD_CATCHUP_MOUNT=/tmp/qt-fold-catchup-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-fold-catchup.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 --roots "$PWD" --roots "$TMPO/cases" -- "fold-other-pass-tier3\\.test\\.ts$"
//! Run:
//!   QT_ORACLE_FOLD_CATCHUP=/tmp/oracle-fold-catchup.ndjson \
//!   QT_FIXTURE_FOLD_CATCHUP_MAIN=/tmp/qt-fold-catchup-main.db \
//!   QT_FIXTURE_FOLD_CATCHUP_MOUNT=/tmp/qt-fold-catchup-mount.db \
//!     cargo test -p quilltap-harness --test fold_other_pass_tier3_equivalence

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::memory_extraction_cursors::ExtractionCursorMap;
use quilltap_core::db::memory_extraction_cursors::ExtractionCursorPass;
use quilltap_core::db::runtime::Db;
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionMessage, CompletionRole, CompletionUsage,
};
use quilltap_core::model::embedding::{CannedEmbeddingProvider, EmbeddingResult};
use quilltap_core::services::admission::{
    reset_admission_state_for_tests, reset_rate_ledger_for_tests,
};
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::cost_estimation::NoMessageCost;
use quilltap_core::services::extraction::catchup::{
    find_fold_catchup_candidates, run_fold_catchup, FoldCatchupOptions, RunFoldCatchupOptions,
};
use quilltap_core::services::extraction::context::{
    build_extraction_context, BuildExtractionContextOptions,
};
use quilltap_core::services::extraction::fold::FoldResult;
use quilltap_core::services::extraction::fold_catchup::enqueue_fold_catchups;
use quilltap_core::services::extraction::run::ExtractionDeps;
use quilltap_core::services::memory_extraction_job::{
    handle_memory_extraction, MemoryExtractionPayload,
};
use quilltap_core::services::memory_time_zone::reset_memory_time_zone_cache_for_tests;
use serde_json::{json, Value};

#[path = "admission_support/mod.rs"]
mod admission_support;
use admission_support::{minted_ids, oracle_lines, planted_stamps, render, render_v4, same, Norm};

fn spec_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/fold-other-pass-tier3.json")
}

const PREFIXES: &[&str] = &[
    "[Memory]",
    "[ExtractionCursors]",
    "[MemorySettings]",
    "[MemoryCalendar]",
    "[Canon]",
    "[FoldCatchup]",
    "[MemoryExtraction]",
    // `maintenance.fold-catchup`'s lines carry no prefix.
    "Enqueued fold catch-up",
    "Catch-up already queued",
    "Fold catch-up capped",
    "Fold catch-up sweep done",
    "Could not read in-flight jobs",
    "Failed to enqueue fold catch-up",
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

/// The Rust fold result in the oracle's (catch-up) shape.
fn fold_value(r: &FoldResult) -> Value {
    let p = &r.result;
    json!({
        "pass": r.pass,
        "calls": p.calls,
        "timedOut": p.timed_out,
        "failed": p.failed,
        "admitted": p.admitted.len(),
        "createdIds": p.created_ids,
        "reinforcedIds": p.reinforced_ids,
        "debugLogs": p.debug_logs,
        "rateDeferred": p.rate_deferred,
        "skippedReason": r.skipped_reason.map(|s| s.as_str()),
        "slices": r.slices,
        "slicesDeferred": r.slices_deferred,
        "cursorAdvancedTo": r.cursor_advanced_to,
    })
}

/// A numeric cell as the JS side reads it (v4's REAL columns hold integers).
fn num(v: rusqlite::types::ValueRef<'_>) -> Value {
    match v {
        rusqlite::types::ValueRef::Integer(i) => Value::from(i),
        rusqlite::types::ValueRef::Real(f) if f.fract() == 0.0 => Value::from(f as i64),
        rusqlite::types::ValueRef::Real(f) => Value::from(f),
        _ => Value::Null,
    }
}

fn jobs_of(db: &Db) -> Value {
    db.read_main(|c| {
        let mut stmt = c.prepare(
            r#"SELECT "type", "payload", "status", "priority", "maxAttempts", "attempts" FROM "background_jobs" ORDER BY "payload", "status""#,
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(json!({
                    "type": r.get::<_, String>(0)?,
                    "payload": r.get::<_, String>(1)?,
                    "status": r.get::<_, String>(2)?,
                    "priority": num(r.get_ref(3)?),
                    "maxAttempts": num(r.get_ref(4)?),
                    "attempts": num(r.get_ref(5)?),
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Value::from(rows))
    })
    .unwrap()
}

/// v4's `ExtractionCursorMap` object: the passes present, by v4's names.
fn cursor_map_value(map: &ExtractionCursorMap) -> Value {
    let mut out = serde_json::Map::new();
    for (key, v) in [
        ("other-fold", &map.other_fold),
        ("episode-fold", &map.episode_fold),
        ("turn", &map.turn),
    ] {
        if let Some(id) = v {
            out.insert(key.into(), Value::from(id.clone()));
        }
    }
    Value::Object(out)
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
async fn extraction_catchup_tier3_matches_oracle() {
    let env = |k: &str| std::env::var(k).ok();
    let (Some(oracle_path), Some(main), Some(mount)) = (
        env("QT_ORACLE_FOLD_CATCHUP"),
        env("QT_FIXTURE_FOLD_CATCHUP_MAIN"),
        env("QT_FIXTURE_FOLD_CATCHUP_MOUNT"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_FOLD_CATCHUP / QT_FIXTURE_FOLD_CATCHUP_MAIN / _MOUNT (see header)."
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
        .prefix("qt-extraction-catchup-rust-")
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
    let mut got_ops: Vec<(String, Value, Value, Vec<String>)> = Vec::new();
    for op in spec["ops"].as_array().unwrap() {
        let name = op["name"].as_str().unwrap().to_string();
        let chat_id = op["chatId"].as_str().unwrap_or("").to_string();
        let empty = json!({});
        let plants = op.get("plants").unwrap_or(&empty);
        if let Some(by_chat) = plants.get("cursors").and_then(Value::as_object) {
            for (c, by_pass) in by_chat {
                for (pass, message_id) in by_pass.as_object().unwrap() {
                    match message_id.as_str() {
                        None => {
                            exec(
                                &db,
                                r#"DELETE FROM "memory_extraction_cursors" WHERE "chatId" = ?1 AND "pass" = ?2"#.into(),
                                vec![c.clone(), pass.clone()],
                            )
                            .await
                        }
                        Some(m) => {
                            let (c, m) = (c.clone(), m.to_string());
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
        }
        if let Some(v) = plants.get("mode") {
            exec(
                &db,
                r#"INSERT INTO "instance_settings" ("key", "value") VALUES ('memoryExtractionMode', ?1)"#.into(),
                vec![serde_json::to_string(v).unwrap()],
            )
            .await;
        }
        if plants.get("lastMessageAtNow").is_some() {
            exec(
                &db,
                r#"UPDATE "chats" SET "lastMessageAt" = ?1 WHERE "id" = ?2"#.into(),
                vec![
                    quilltap_core::clock::iso_from_unix_ms(quilltap_core::clock::now_unix_ms()),
                    chat_id.clone(),
                ],
            )
            .await;
        }
        let now = op
            .get("now")
            .and_then(Value::as_str)
            .map(|iso| quilltap_core::clock::iso_to_ms(iso).expect("op now") as f64);
        let limit = op.get("limit").and_then(Value::as_u64).map(|n| n as usize);
        let pass = op
            .get("pass")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        let completion = completion_from(&oracle, &name);
        let deps = ExtractionDeps {
            completion: &completion,
            embedding: &embedding,
            executor: &executor,
        };
        logs.lock().unwrap().clear();
        let result = match op["kind"].as_str().unwrap() {
            "select" => {
                match find_fold_catchup_candidates(
                    &db,
                    &pass,
                    FoldCatchupOptions {
                        now,
                        limit,
                        ..Default::default()
                    },
                ) {
                    Ok(sel) => json!({
                        "candidates": sel.candidates.iter().map(|c| json!({
                            "chatId": c.chat_id,
                            "userId": c.user_id,
                            "anchorMessageId": c.anchor_message_id,
                            "connectionProfileId": c.connection_profile_id,
                        })).collect::<Vec<_>>(),
                        "deferred": sel.deferred,
                    }),
                    Err(e) => json!({ "error": quilltap_core::db::fallback::error_text(&e) }),
                }
            }
            "sweep" => match enqueue_fold_catchups(&db, &pass, now, limit, None, None).await {
                Ok(summary) => json!({
                    "summary": { "enqueued": summary.enqueued, "deferred": summary.deferred },
                    "jobs": jobs_of(&db),
                }),
                Err(e) => json!({ "error": quilltap_core::db::fallback::error_text(&e) }),
            },
            "catchup" => match build_extraction_context(
                &db,
                &chat_id,
                &user_id,
                BuildExtractionContextOptions {
                    host_zone_fallback: "UTC".into(),
                    ..Default::default()
                },
            ) {
                Ok(ctx) => match run_fold_catchup(
                    &deps,
                    &ctx,
                    &pass,
                    RunFoldCatchupOptions {
                        ignore_idle: op["ignoreIdle"].as_bool() == Some(true),
                        now,
                        ..Default::default()
                    },
                )
                .await
                {
                    Ok(r) => fold_value(&r),
                    Err(e) => json!({ "error": quilltap_core::db::fallback::error_text(&e) }),
                },
                Err(e) => json!({ "error": e.message() }),
            },
            _ => {
                let payload = MemoryExtractionPayload::decode(&op["payload"]);
                let error = handle_memory_extraction(
                    &db,
                    &completion,
                    &embedding,
                    &executor,
                    &NoMessageCost,
                    &format!("job-{name}"),
                    &user_id,
                    &payload,
                    "UTC",
                )
                .await
                .err();
                json!({ "error": error, "jobs": jobs_of(&db) })
            }
        };
        let captured = logs.lock().unwrap().clone();

        for key in ["memoryExtractionMode"] {
            exec(
                &db,
                r#"DELETE FROM "instance_settings" WHERE "key" = ?1"#.into(),
                vec![key.into()],
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

    // UNKNOWN_PASS_CURSOR_ROW (L-4, both ways — the `FRESH_STORE_RESIDUAL`
    // idiom): the unknown pass writes v4's raw `<chatId>:bogus` row, and v4's
    // own whole-map reads DROP it (they read only the three known passes). A
    // v4 change to the enum or to the raw write trips one arm or the other.
    {
        let unknown_chat = spec["unknownPassChatId"].as_str().unwrap().to_string();
        let want = oracle
            .iter()
            .find(|l| l["kind"] == "unknown-pass")
            .expect("oracle unknown-pass line");
        let raw_row = |cursors: &Value| {
            cursors.as_array().unwrap().iter().any(|r| {
                r["id"] == format!("{unknown_chat}:bogus").as_str() && r["pass"] == "bogus"
            })
        };
        let (got_cursors, want_cursors) = (
            &got_ops.last().unwrap().2,
            &want_ops.last().unwrap()["cursors"],
        );
        assert!(
            raw_row(got_cursors) && raw_row(want_cursors),
            "UNKNOWN_PASS_CURSOR_ROW: both sides must write the raw `<chatId>:bogus` row"
        );
        let c = unknown_chat.clone();
        let by_chat = db
            .read_main(move |conn| {
                quilltap_core::db::memory_extraction_cursors::MemoryExtractionCursorsRepository::new(conn)
                    .find_by_chat_id(&c)
            })
            .unwrap();
        let all = db
            .read_main(|conn| {
                quilltap_core::db::memory_extraction_cursors::MemoryExtractionCursorsRepository::new(conn)
                    .find_all_by_chat()
            })
            .unwrap();
        let mut all_obj = serde_json::Map::new();
        for (chat, map) in &all {
            all_obj.insert(chat.clone(), cursor_map_value(map));
        }
        let got = json!({
            "findByChatId": cursor_map_value(&by_chat),
            "findAllByChat": Value::Object(all_obj),
        });
        let want = json!({
            "findByChatId": want["findByChatId"],
            "findAllByChat": want["findAllByChat"],
        });
        if !same(&got, &want) {
            failures.push(format!(
                "UNKNOWN_PASS_CURSOR_ROW: the whole-map reads diverge\n  rust:   {got:#}\n  oracle: {want:#}"
            ));
        }
        let as_map = |v: &Value| v["findAllByChat"][unknown_chat.as_str()].clone();
        assert!(
            as_map(&got).get("bogus").is_none() && as_map(&want).get("bogus").is_none(),
            "UNKNOWN_PASS_CURSOR_ROW: v4's whole-map reads drop the raw row; so must v5's"
        );
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
        "OK: extraction catch-up tier-3 matched the oracle ({} ops, {} memories).",
        got_ops.len(),
        rows_got.len()
    );
}
