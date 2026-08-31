//! Tier-3 differential test: the **compression cache** (v4
//! `lib/services/chat-message/compression-cache.service.ts`
//! `triggerAsyncCompression` / `getCachedCompression` / `invalidateCompressionCache`,
//! ported as `quilltap_core::services::compression_cache`).
//!
//! Both sides copy the same seed fixture (three empty chats) and run the same op
//! sequence. The oracle drives v4's REAL cache functions with ONLY the cheap-LLM
//! (`createLLMProvider`) mocked; this test composes the ported functions,
//! replaying the recorded canned completion. Each op's observable output is
//! compared:
//!   - `trigger` — the persisted `chats.compressionCache` column (the entry's
//!     minted `createdAt` normalized; everything else — the compression result,
//!     `messageCount`, `systemPromptHash` — pinned);
//!   - `trigger` (too-few messages) — the column stays `null` (the guard);
//!   - `get` — the `getCachedCompression` return (`{result, cachedMessageCount,
//!     isFallback}`), read from the DB after clearing the in-memory cache;
//!   - `get` (no cache) — `null` (the miss);
//!   - `invalidate` — the column cleared to `null`.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout). jest ignores
//! `.claude/` paths, so the case + its spec are staged into a per-family /tmp
//! mirror first — the old header named `/tmp/qt-oracle-run` without ever
//! creating it, so the run died on `Directory /tmp/qt-oracle-run/cases in the
//! roots[1] option was not found` (P4.34 phase 1):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-compcache-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/compression-cache-tier3.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/compression-cache-tier3.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-compcache.db \
//!     $N/npx tsx "$V5W/harness/oracle/fixtures/build-compression-cache-fixture.ts"
//!   QT_FIXTURE_COMPCACHE=/tmp/qt-compcache.db QT_ORACLE_OUT=/tmp/oracle-compcache.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- compression-cache-tier3
//! Run:
//!   QT_ORACLE_COMPCACHE=/tmp/oracle-compcache.ndjson QT_FIXTURE_COMPCACHE=/tmp/qt-compcache.db \
//!     cargo test -p quilltap-harness --test compression_cache_tier3_equivalence

use quilltap_core::cheap_llm::CheapLlmSelection;
use quilltap_core::context_compression::CompressibleMessage;
use quilltap_core::db::chats_read;
use quilltap_core::db::runtime::Db;
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionMessage, CompletionRole, CompletionUsage,
};
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::compression::ContextCompressionOptions;
use quilltap_core::services::compression_cache::{
    clear_compression_cache, get_cached_compression, hash_string, invalidate_compression_cache,
    trigger_async_compression,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectionW {
    provider: String,
    model_name: String,
    is_local: bool,
}
#[derive(Deserialize)]
struct MsgW {
    role: String,
    content: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpW {
    kind: String,
    chat_id: String,
    #[serde(default)]
    participant_id: Option<String>,
    #[serde(default)]
    few: bool,
    #[serde(default)]
    message_count: i64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    frozen_now_ms: i64,
    system_prompt: String,
    selection: SelectionW,
    window_size: i64,
    compression_target_tokens: i64,
    character_name: String,
    user_name: String,
    messages: Vec<MsgW>,
    few_messages: Vec<MsgW>,
    ops: Vec<OpW>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageW {
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
}
#[derive(Deserialize)]
struct CannedW {
    provider: String,
    model: String,
    temperature: Option<f64>,
    messages: Vec<MsgW>,
    response: String,
    #[serde(default)]
    usage: Option<UsageW>,
}

fn to_completion_messages(m: &[MsgW]) -> Vec<CompletionMessage> {
    m.iter()
        .map(|m| CompletionMessage {
            role: match m.role.as_str() {
                "system" => CompletionRole::System,
                "assistant" => CompletionRole::Assistant,
                _ => CompletionRole::User,
            },
            content: m.content.clone(),
        })
        .collect()
}

fn to_compressible(m: &[MsgW]) -> Vec<CompressibleMessage> {
    m.iter()
        .map(|m| CompressibleMessage {
            role: m.role.clone(),
            content: m.content.clone(),
        })
        .collect()
}

fn spec_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/compression-cache-tier3.json")
}

/// The oracle op outputs, keyed by (name, chatId) → the recorded Value (column or
/// `{return}`).
fn read_oracle(oracle_text: &str) -> Vec<(String, String, Value)> {
    let mut out = Vec::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        if v.get("kind").and_then(Value::as_str) == Some("op") {
            out.push((
                v.get("name").and_then(Value::as_str).unwrap().to_string(),
                v.get("chatId").and_then(Value::as_str).unwrap().to_string(),
                v.clone(),
            ));
        }
    }
    out
}

/// Normalize the persisted entry's minted `createdAt` to `<ts>`.
fn normalize_column(mut v: Value) -> Value {
    if let Some(obj) = v.as_object_mut() {
        if obj.contains_key("createdAt") {
            obj.insert("createdAt".into(), Value::String("<ts>".into()));
        }
    }
    v
}

#[test]
fn compression_cache_tier3_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_COMPCACHE") else {
        eprintln!("QT_ORACLE_COMPCACHE not set; skipping");
        return;
    };
    let Ok(fixture) = std::env::var("QT_FIXTURE_COMPCACHE") else {
        eprintln!("QT_FIXTURE_COMPCACHE not set; skipping");
        return;
    };

    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("spec readable"))
            .expect("spec parses");
    let oracle_text = std::fs::read_to_string(&oracle_path).expect("oracle readable");
    let want_ops = read_oracle(&oracle_text);

    let mut canned: Vec<CannedW> = Vec::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).unwrap();
        if v.get("kind").and_then(Value::as_str) == Some("canned") {
            canned.push(serde_json::from_value(v).expect("canned"));
        }
    }

    let work = std::env::temp_dir().join(format!("qt-compcache-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).expect("copy fixture");

    let db = Db::open_main(&work, &spec.test_pepper_base64).expect("open fixture");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let mut completion = CannedCompletionProvider::new();
    for row in &canned {
        let messages = to_completion_messages(&row.messages);
        let usage = row.usage.as_ref().map(|u| CompletionUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });
        completion = completion.with_response(
            &row.provider,
            &row.model,
            row.temperature,
            &messages,
            &row.response,
            usage,
        );
    }
    let executor = CheapLlmTaskExecutor::new();

    // Start from a clean in-memory cache (the process-global persists across tests).
    clear_compression_cache();

    let selection = CheapLlmSelection {
        provider: spec.selection.provider.clone(),
        model_name: spec.selection.model_name.clone(),
        base_url: None,
        connection_profile_id: None,
        is_local: spec.selection.is_local,
        profile_parameters: None,
    };
    let options = ContextCompressionOptions {
        window_size: spec.window_size,
        compression_target_tokens: spec.compression_target_tokens,
        selection,
        character_name: spec.character_name.clone(),
        user_name: spec.user_name.clone(),
        uncensored_fallback: None,
        // v4's `triggerAsyncCompression` spreads `latency: 'background'` over
        // the caller's options (`a1d88aa3a`, bug 107) — this family drives that
        // path, so `Background` is what v4 uses here too.
        latency: quilltap_core::services::cheap_llm_exec::CheapLlmLatencyClass::Background,
    };
    let sys_hash = hash_string(&spec.system_prompt);
    let full = to_compressible(&spec.messages);
    let few = to_compressible(&spec.few_messages);

    let read_column = |chat_id: &str| -> Value {
        let cid = chat_id.to_string();
        db.read_main(move |c| chats_read::find_by_id(c, &cid))
            .expect("chat read")
            .and_then(|chat| chat.get("compressionCache").cloned())
            .unwrap_or(Value::Null)
    };

    let mut got_ops: Vec<(String, String, Value)> = Vec::new();
    for op in &spec.ops {
        let pid = op.participant_id.as_deref();
        match op.kind.as_str() {
            "trigger" => {
                let msgs = if op.few { &few } else { &full };
                rt.block_on(trigger_async_compression(
                    &db,
                    &completion,
                    &executor,
                    &op.chat_id,
                    pid,
                    msgs,
                    &spec.system_prompt,
                    &options,
                    spec.frozen_now_ms,
                ))
                .expect("trigger");
                let col = normalize_column(read_column(&op.chat_id));
                got_ops.push((
                    op.kind.clone(),
                    op.chat_id.clone(),
                    json!({ "column": col }),
                ));
            }
            "get" => {
                clear_compression_cache();
                let resp = rt
                    .block_on(get_cached_compression(
                        &db,
                        &op.chat_id,
                        op.message_count,
                        pid,
                        Some(&sys_hash),
                    ))
                    .expect("get");
                let ret = match resp {
                    Some(r) => json!({
                        "result": serde_json::to_value(&r.result).unwrap(),
                        "cachedMessageCount": r.cached_message_count,
                        "isFallback": r.is_fallback,
                    }),
                    None => Value::Null,
                };
                got_ops.push((
                    op.kind.clone(),
                    op.chat_id.clone(),
                    json!({ "return": ret }),
                ));
            }
            "invalidate" => {
                rt.block_on(invalidate_compression_cache(&db, &op.chat_id, pid))
                    .expect("invalidate");
                let col = normalize_column(read_column(&op.chat_id));
                got_ops.push((
                    op.kind.clone(),
                    op.chat_id.clone(),
                    json!({ "column": col }),
                ));
            }
            other => panic!("unknown op kind {other}"),
        }
    }
    drop(db);
    let _ = std::fs::remove_file(&work);

    assert_eq!(got_ops.len(), want_ops.len(), "op count");
    for (i, ((gk, gc, gv), (wk, wc, wraw))) in got_ops.iter().zip(want_ops.iter()).enumerate() {
        assert_eq!((gk, gc), (wk, wc), "op[{i}] name/chat");
        // The oracle line carries `column` or `return`; normalize its `column`.
        let want = if wraw.get("column").is_some() {
            json!({ "column": normalize_column(wraw.get("column").cloned().unwrap()) })
        } else {
            json!({ "return": wraw.get("return").cloned().unwrap_or(Value::Null) })
        };
        if gv != &want {
            panic!(
                "op[{i}] {gk}/{gc} mismatch\n--- got ---\n{}\n--- want ---\n{}",
                serde_json::to_string_pretty(gv).unwrap(),
                serde_json::to_string_pretty(&want).unwrap()
            );
        }
    }

    eprintln!(
        "OK: compression-cache tier-3 matched oracle ({} ops).",
        got_ops.len()
    );
}
