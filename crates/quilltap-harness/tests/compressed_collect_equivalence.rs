//! Real-DB differential: v5's backup collector over a v4-4.10 instance whose
//! registered columns hold brotli BLOBs (P4.D203 — the drift ledger's failure
//! (b)).
//!
//! **What the ledger measured, and what nothing else could see.** On a migrated
//! instance `services/backup/collect.rs`'s `F::Json` bind on
//! `llm_logs.{request,response}` raised `InvalidColumnType`, and
//! `backup/mod.rs`'s `.unwrap_or_default()` swallowed it — so **a backup
//! SILENTLY contained ZERO `llm_logs` rows.** No error, no warning, no failed
//! step: the operator got an archive with a partition quietly missing. The
//! `chat_messages` arm of the same collector failed LOUDLY instead (its `?`
//! propagates), which is the only reason the two are worth distinguishing.
//!
//! Every existing backup family reads a committed fixture the round's §R.12
//! forbids rebuilding at the target pin, so this lane builds the ONE new
//! committed triple §R.12 authorizes it — `chat-compressed-{main,mount,
//! llmlogs}.db`, baked at `f45a517a9` through v4's REAL repositories — and
//! compares over that.
//!
//! **The oracle is v4's own call.** `collectUserData` is module-private in v4,
//! but the call it makes for that partition is `repos.llmLogs.findAll(10000)`;
//! the case drives that exact REAL method, plus `repos.chats.getMessages` and
//! `repos.conversationChunks.findByChatId` for the other two compressed
//! surfaces.
//!
//! Regenerate (Node 24, from a pinned v4 worktree):
//!   W=<this worktree>
//!   cd /tmp/qt-v4-pin-p4d203-f45a517a9
//!   QT_FIXTURE_CZ_MAIN=$W/crates/quilltap-web/tests/fixtures/chat-compressed-main.db \
//!   QT_FIXTURE_CZ_MOUNT=$W/crates/quilltap-web/tests/fixtures/chat-compressed-mount.db \
//!   QT_FIXTURE_CZ_LLM=$W/crates/quilltap-web/tests/fixtures/chat-compressed-llmlogs.db \
//!     ~/.nvm/versions/node/v24.13.1/bin/npx tsx \
//!       $W/harness/oracle/cases/chat-compressed-collect.ts \
//!       > /tmp/p4.d203/oracle-chat-compressed.ndjson
//! Run:
//!   QT_ORACLE_CHAT_COMPRESSED=/tmp/p4.d203/oracle-chat-compressed.ndjson \
//!     cargo test -p quilltap-harness --test compressed_collect_equivalence -- --nocapture

use std::path::PathBuf;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::backup::collect_user_data;
use serde_json::{json, Value};

const ORACLE_VAR: &str = "QT_ORACLE_CHAT_COMPRESSED";
const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn spec() -> Value {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-compressed.json");
    serde_json::from_str(&std::fs::read_to_string(&p).expect("the committed spec must exist"))
        .expect("the spec must parse")
}

fn oracle_line(text: &str, kind: &str) -> Value {
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("each oracle line is JSON");
        if v["kind"] == kind {
            return v;
        }
    }
    panic!("the oracle has no `{kind}` line");
}

/// `Option<&str>`-ish: a key the v4 side emits as `null` when absent.
fn opt(v: Option<&Value>) -> Value {
    match v {
        None | Some(Value::Null) => Value::Null,
        Some(other) => other.clone(),
    }
}

#[test]
fn the_collector_reads_every_compressed_partition() {
    let Ok(oracle_path) = std::env::var(ORACLE_VAR) else {
        println!("SKIP: {ORACLE_VAR} unset — see this test's header for the regen recipe");
        return;
    };
    let oracle_text = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("cannot read the oracle at {oracle_path}: {e}"));
    assert!(
        !oracle_text.trim().is_empty(),
        "the oracle NDJSON at {oracle_path} is EMPTY"
    );

    let spec = spec();
    let user_id = spec["userId"].as_str().expect("spec userId").to_string();
    let chat_id = spec["chatId"].as_str().expect("spec chatId").to_string();

    // Work on COPIES — the committed bytes are never touched.
    let scratch = tempfile::tempdir().expect("scratch");
    let f = fixtures_dir();
    let main = scratch.path().join("main.db");
    let mount = scratch.path().join("mount.db");
    let llm = scratch.path().join("llm.db");
    std::fs::copy(f.join("chat-compressed-main.db"), &main).expect("copy main");
    std::fs::copy(f.join("chat-compressed-mount.db"), &mount).expect("copy mount");
    std::fs::copy(f.join("chat-compressed-llmlogs.db"), &llm).expect("copy llm");

    let db = Db::open(
        DbPaths {
            main: main.clone(),
            mount_index: Some(mount.clone()),
            llm_logs: Some(llm.clone()),
        },
        TEST_PEPPER,
    )
    .expect("open the compressed instance");

    let collected = collect_user_data(&db, &user_id).expect("collect_user_data");

    // --- llm_logs: the SILENT failure ---------------------------------------
    let want = oracle_line(&oracle_text, "llm_logs");
    let want_rows = want["rows"].as_array().expect("oracle llm_logs rows");
    assert!(
        !want_rows.is_empty(),
        "the fixture must carry llm_logs rows or this family proves nothing"
    );
    let mut got_rows: Vec<Value> = collected
        .llm_logs
        .iter()
        .map(|l| {
            json!({
                "id": opt(l.get("id")),
                "type": opt(l.get("type")),
                "provider": opt(l.get("provider")),
                "modelName": opt(l.get("modelName")),
                "request": opt(l.get("request")),
                "response": opt(l.get("response")),
            })
        })
        .collect();
    got_rows.sort_by_key(|r| r["id"].as_str().unwrap_or_default().to_string());
    assert_eq!(
        collected.llm_logs.len(),
        want["count"].as_u64().expect("oracle count") as usize,
        "llm_logs row COUNT diverged — zero here is the ledger's silent backup"
    );
    assert_eq!(
        &Value::Array(got_rows),
        &want["rows"],
        "llm_logs rows diverged"
    );

    // --- chat_messages: the four columns through the hydrating read ----------
    let want_msgs = oracle_line(&oracle_text, "chat_messages");
    let chat = collected
        .chats
        .iter()
        .find(|c| c.get("id").and_then(Value::as_str) == Some(chat_id.as_str()))
        .unwrap_or_else(|| panic!("the collected backup has no chat {chat_id}"));
    let got_msgs: Vec<Value> = chat
        .get("messages")
        .and_then(Value::as_array)
        .expect("the collected chat carries messages")
        .iter()
        .map(|m| {
            json!({
                "id": opt(m.get("id")),
                "type": opt(m.get("type")),
                "content": opt(m.get("content")),
                "opaqueContent": opt(m.get("opaqueContent")),
                "context": opt(m.get("context")),
                "description": opt(m.get("description")),
            })
        })
        .collect();
    // ⚠ v5's collector filters `type == 'message'` (v4's `collectUserData`
    // does the same), so the context-summary and system rows are NOT here.
    // Compare only the ids both sides carry, and assert the filter itself.
    let want_all = want_msgs["rows"].as_array().expect("oracle message rows");
    let want_messages_only: Vec<&Value> =
        want_all.iter().filter(|m| m["type"] == "message").collect();
    assert_eq!(
        got_msgs.len(),
        want_messages_only.len(),
        "chat message COUNT diverged (v4 read {} of {} events as type=message)",
        want_messages_only.len(),
        want_all.len()
    );
    for (got, want) in got_msgs.iter().zip(want_messages_only.iter()) {
        assert_eq!(got["id"], want["id"], "message order diverged");
        assert_eq!(
            got["content"], want["content"],
            "message {} content diverged",
            got["id"]
        );
        assert_eq!(
            got["opaqueContent"], want["opaqueContent"],
            "message {} opaqueContent diverged",
            got["id"]
        );
    }
    // The two rows the filter drops carry `context` and `description` — the
    // other two registered columns. They are proven by the read family
    // (`chats_messages_read_equivalence`); assert here only that the oracle
    // really held them, so a corpus that lost them names itself.
    assert!(
        want_all.iter().any(|m| m["context"] != Value::Null),
        "the fixture lost its compressed `context` row"
    );
    assert!(
        want_all.iter().any(|m| m["description"] != Value::Null),
        "the fixture lost its compressed `description` row"
    );

    // --- conversation_chunks.content ----------------------------------------
    let want_chunks = oracle_line(&oracle_text, "conversation_chunks");
    let got_chunks: Vec<Value> = collected
        .conversation_chunks
        .iter()
        .map(|c| {
            json!({
                "id": opt(c.get("id")),
                "interchangeIndex": opt(c.get("interchangeIndex")),
                "content": opt(c.get("content")),
            })
        })
        .collect();
    assert_eq!(
        &Value::Array(got_chunks),
        &want_chunks["rows"],
        "conversation_chunks rows diverged"
    );

    println!(
        "compressed_collect_equivalence: {} llm_logs / {} messages / {} chunks compared",
        collected.llm_logs.len(),
        got_msgs.len(),
        collected.conversation_chunks.len()
    );
}
