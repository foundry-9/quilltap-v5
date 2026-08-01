//! Tier-3 differential test: **regenerate-swipe** (v4
//! `lib/services/chat-message/regenerate-swipe.service.ts` `regenerateMessageAsSwipe`,
//! ported as `quilltap_core::services::regenerate_swipe`), the sibling entry
//! point to `processMessage`.
//!
//! Both sides copy the same two-DB seed fixture and run the same four-call
//! sequence. The oracle drives v4's REAL `regenerateMessageAsSwipe` with ONLY the
//! model boundaries + buildContext feeders mocked to match the Rust seams; this
//! test composes the ported services through `regenerate_message_as_swipe`,
//! replaying the recorded canned completion. Then the five touched tables (`chats`
//! / `chat_messages` / `memories` / `vector_indices` / `vector_entries`) are
//! diffed:
//!   - `chat_messages` — the new swipe's minted id is remapped to a first-appearance
//!     token (seeded ids identical both sides → same token); every other cell
//!     (`createdAt` = the target's SEEDED stamp, the token counts from the fixed
//!     usage, the swipe-group columns) is pinned;
//!   - `chats` — the minted metadata bump (`updatedAt` / `lastMessageAt`) is
//!     placeholdered (v4's frozen+tick vs the Rust real clock);
//!   - `memories` / `vector_entries` — the DELETE cascade's survivors are pinned;
//!   - `vector_indices` — a flush that removed entries mints `updatedAt` (sentinel-
//!     aware collapse).
//!
//! Each call's throw/no-throw is also asserted (the `not_assistant` error path).
//!
//! **TZ=UTC is REQUIRED since P4.d26** (the distill TODAY line renders in the
//! SERVER-LOCAL zone, and the harness pins `server_tz` to "UTC"), so the pin below
//! is load-bearing, not decoration.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout; jest ignores
//! `.claude/` paths, so the case is staged in a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-regen-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/regenerate-swipe-tier3.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/regenerate-swipe-tier3.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-regen-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-regen-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-regenerate-swipe-fixture.ts
//!   QT_FIXTURE_REGEN_MAIN=/tmp/qt-regen-main.db QT_FIXTURE_REGEN_MOUNT=/tmp/qt-regen-mount.db \
//!   TZ=UTC QT_ORACLE_OUT=/tmp/oracle-regenerate-swipe.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- regenerate-swipe-tier3
//! Run:
//!   QT_ORACLE_REGEN=/tmp/oracle-regenerate-swipe.ndjson \
//!   QT_FIXTURE_REGEN_MAIN=/tmp/qt-regen-main.db QT_FIXTURE_REGEN_MOUNT=/tmp/qt-regen-mount.db \
//!     cargo test -p quilltap-harness --test regenerate_swipe_tier3_equivalence

use std::collections::HashMap;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::{chats_messages_read, chats_read, dump_table_json_conn};
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionMessage, CompletionRole, CompletionUsage,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::services::build_context::NoopSeams as BcNoopSeams;
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::message_context::NoopMessageContextSeams;
use quilltap_core::services::regenerate_swipe::{
    regenerate_message_as_swipe, RegenError, RegenerateSwipeOptions,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CallW {
    name: String,
    chat_id: String,
    user_id: String,
    target_message_id: String,
    #[serde(default)]
    expect_throw: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    seed_timestamp: String,
    frozen_now_ms: i64,
    #[serde(default)]
    local_offset_minutes: i64,
    calls: Vec<CallW>,
}

#[derive(Deserialize)]
struct CannedMsgW {
    role: String,
    content: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageW {
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
}
#[derive(Deserialize)]
struct CannedCompletionW {
    provider: String,
    model: String,
    temperature: Option<f64>,
    messages: Vec<CannedMsgW>,
    response: String,
    #[serde(default)]
    usage: Option<UsageW>,
}

fn to_completion_messages(m: &[CannedMsgW]) -> Vec<CompletionMessage> {
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

fn spec_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/regenerate-swipe-tier3.json")
}

/// The oracle NDJSON line for a dumped table.
fn oracle_table(oracle_text: &str, table: &str) -> Value {
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        if v.get("kind").and_then(Value::as_str) == Some("table")
            && v.get("table").and_then(Value::as_str) == Some(table)
        {
            return v;
        }
    }
    panic!("oracle ndjson missing table {table}");
}

/// The oracle's recorded call throws (name → threw).
fn oracle_calls(oracle_text: &str) -> HashMap<String, bool> {
    let mut out = HashMap::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        if v.get("kind").and_then(Value::as_str) == Some("call") {
            out.insert(
                v.get("call").and_then(Value::as_str).unwrap().to_string(),
                v.get("threw").and_then(Value::as_bool).unwrap_or(false),
            );
        }
    }
    out
}

/// Sort chat_messages by a stable id-free key, then remap every `id` to a
/// first-appearance token (seeded ids identical → same token; minted swipe ids →
/// matching tokens). `createdAt` is the target's SEEDED stamp both sides, so it
/// stays literal.
fn normalize_messages(dump: &mut Value, idmap: &mut HashMap<String, String>) {
    if let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) {
        rows.sort_by_key(|r| {
            (
                str_of(r, "chatId"),
                str_of(r, "createdAt"),
                r.get("swipeIndex").and_then(Value::as_i64).unwrap_or(-1),
                str_of(r, "role"),
                str_of(r, "content"),
            )
        });
        for row in rows {
            if let Some(obj) = row.as_object_mut() {
                if let Some(id) = obj.get("id").and_then(Value::as_str) {
                    let n = idmap.len();
                    let tok = idmap
                        .entry(id.to_string())
                        .or_insert_with(|| format!("<m{n}>"))
                        .clone();
                    obj.insert("id".into(), Value::String(tok));
                }
            }
        }
    }
}

/// chats: placeholder the minted metadata bump columns.
fn normalize_chats(dump: &mut Value) {
    if let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) {
        rows.sort_by_key(|r| str_of(r, "id"));
        for row in rows {
            if let Some(obj) = row.as_object_mut() {
                for c in ["updatedAt", "lastMessageAt"] {
                    if let Some(v) = obj.get(c) {
                        if !v.is_null() {
                            obj.insert(c.to_string(), Value::String(format!("<{c}>")));
                        }
                    }
                }
            }
        }
    }
}

/// memories / vector_indices / vector_entries: sort by id; sentinel-aware
/// `updatedAt` collapse (a flush that removed entries mints via `saveMeta`).
fn normalize_id_table(dump: &mut Value, sentinel: &str) {
    if let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) {
        rows.sort_by_key(|r| str_of(r, "id"));
        for row in rows {
            if let Some(obj) = row.as_object_mut() {
                let bump = matches!(obj.get("updatedAt"), Some(Value::String(s)) if s != sentinel);
                if bump {
                    obj.insert("updatedAt".into(), Value::String("<ts>".into()));
                }
            }
        }
    }
}

fn str_of(r: &Value, k: &str) -> String {
    r.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

fn assert_rows_eq(name: &str, got: &Value, want: &Value) {
    if got.get("rows") != want.get("rows") {
        let g = serde_json::to_string_pretty(got.get("rows").unwrap()).unwrap();
        let w = serde_json::to_string_pretty(want.get("rows").unwrap()).unwrap();
        panic!("table {name} mismatch\n--- got ---\n{g}\n--- want ---\n{w}");
    }
}

#[test]
fn regenerate_swipe_tier3_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_REGEN") else {
        eprintln!("QT_ORACLE_REGEN not set; skipping");
        return;
    };
    let Ok(fixture_main) = std::env::var("QT_FIXTURE_REGEN_MAIN") else {
        eprintln!("QT_FIXTURE_REGEN_MAIN not set; skipping");
        return;
    };
    let Ok(fixture_mount) = std::env::var("QT_FIXTURE_REGEN_MOUNT") else {
        eprintln!("QT_FIXTURE_REGEN_MOUNT not set; skipping");
        return;
    };

    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("spec readable"))
            .expect("spec parses");
    let oracle_text = std::fs::read_to_string(&oracle_path).expect("oracle readable");
    let want_calls = oracle_calls(&oracle_text);

    // Canned completions.
    let mut canned_completions: Vec<CannedCompletionW> = Vec::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).unwrap();
        if v.get("kind").and_then(Value::as_str) == Some("cannedCompletion") {
            canned_completions.push(serde_json::from_value(v).expect("cannedCompletion"));
        }
    }

    let scratch = std::env::temp_dir().join(format!("qt-regen-harness-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let work_main = scratch.join("regen-main.db");
    let work_mount = scratch.join("regen-mount.db");
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(&fixture_main, &work_main).expect("copy main");
    std::fs::copy(&fixture_mount, &work_mount).expect("copy mount");

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open fixture");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let mut completion = CannedCompletionProvider::new();
    for row in &canned_completions {
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
    let embedding = CannedEmbeddingProvider::new();
    let executor = CheapLlmTaskExecutor::new();
    let bc_seams = BcNoopSeams;
    let mc_seams = NoopMessageContextSeams;

    for call in &spec.calls {
        let chat_id = call.chat_id.clone();
        let chat = db
            .read_main(move |c| chats_read::find_by_id(c, &chat_id))
            .expect("chat read")
            .expect("chat present");
        let chat_id2 = call.chat_id.clone();
        let all_messages = db
            .read_main(move |c| chats_messages_read::get_messages(c, &chat_id2))
            .expect("messages read");
        let target = all_messages
            .iter()
            .find(|m| m.get("id").and_then(Value::as_str) == Some(call.target_message_id.as_str()))
            .expect("target present")
            .clone();

        let result = rt.block_on(regenerate_message_as_swipe(
            &db,
            &embedding,
            &completion,
            &executor,
            &bc_seams,
            &mc_seams,
            RegenerateSwipeOptions {
                user_id: call.user_id.clone(),
                chat,
                target_message: target,
                all_messages,
                active_user_participant_id: None,
                model_context_limit: 200_000,
                timestamp_config: None,
                timezone: Some("UTC".to_string()),
                server_tz: Some("UTC".to_string()),
                now_ms: spec.frozen_now_ms,
                local_offset_minutes: spec.local_offset_minutes,
                random01: 0.0,
            },
        ));

        let threw = match &result {
            Ok(_) => false,
            Err(RegenError::NotAssistant) | Err(RegenError::StaffMessage) => true,
            Err(e) => panic!("regenerate {} unexpected error: {e}", call.name),
        };
        let want = *want_calls
            .get(&call.name)
            .unwrap_or_else(|| panic!("oracle missing call {}", call.name));
        // The oracle's recorded throw must match the spec's declared expectation
        // (a corpus-consistency check) AND the Rust side.
        assert_eq!(
            want, call.expect_throw,
            "spec/oracle throw disagree for {}",
            call.name
        );
        assert_eq!(threw, want, "call {} throw mismatch", call.name);
    }

    // Dump + diff.
    let dump = |t: &str| -> Value {
        let t = t.to_string();
        db.read_main(move |c| dump_table_json_conn(c, &t, "id"))
            .expect("dump")
    };
    let mut got_chats = dump("chats");
    let mut got_msgs = dump("chat_messages");
    let mut got_mem = dump("memories");
    let mut got_vi = dump("vector_indices");
    let mut got_ve = dump("vector_entries");
    drop(db);
    let _ = std::fs::remove_dir_all(&scratch);

    let mut want_chats = oracle_table(&oracle_text, "chats");
    let mut want_msgs = oracle_table(&oracle_text, "chat_messages");
    let mut want_mem = oracle_table(&oracle_text, "memories");
    let mut want_vi = oracle_table(&oracle_text, "vector_indices");
    let mut want_ve = oracle_table(&oracle_text, "vector_entries");

    normalize_chats(&mut got_chats);
    normalize_chats(&mut want_chats);
    let mut idmap_g = HashMap::new();
    let mut idmap_w = HashMap::new();
    normalize_messages(&mut got_msgs, &mut idmap_g);
    normalize_messages(&mut want_msgs, &mut idmap_w);
    for (g, w) in [
        (&mut got_mem, &mut want_mem),
        (&mut got_vi, &mut want_vi),
        (&mut got_ve, &mut want_ve),
    ] {
        normalize_id_table(g, &spec.seed_timestamp);
        normalize_id_table(w, &spec.seed_timestamp);
    }

    assert_rows_eq("chats", &got_chats, &want_chats);
    assert_rows_eq("chat_messages", &got_msgs, &want_msgs);
    assert_rows_eq("memories", &got_mem, &want_mem);
    assert_rows_eq("vector_indices", &got_vi, &want_vi);
    assert_rows_eq("vector_entries", &got_ve, &want_ve);

    // Sanity: two memory survivors (targetC's), two vector entries.
    assert_eq!(
        got_mem["rows"].as_array().unwrap().len(),
        2,
        "memory survivors"
    );
    assert_eq!(
        got_ve["rows"].as_array().unwrap().len(),
        2,
        "vector survivors"
    );

    eprintln!("OK: regenerate-swipe tier-3 matched oracle (5 tables + call throws).");
}
