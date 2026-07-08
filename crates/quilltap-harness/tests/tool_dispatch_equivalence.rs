//! Dispatcher end-to-end differential — v4 `executeToolCallWithContext`
//! (`lib/chat/tool-executor.ts`, the built-in dispatch if-chain) vs the ported
//! [`BuiltInToolRunner`](quilltap_core::tools::executor::BuiltInToolRunner).
//!
//! Both sides open a COPY of one baked seed (a character + a rendered chat + a
//! blank chat + a seed annotation) and run the SAME mixed batch of tool calls
//! (state accumulates): a read (`read_conversation`), two writes
//! (`upsert_annotation` create + update — exercising the dispatcher's
//! character-NAME resolution from `callingParticipantId`), a pure tool
//! (`help_navigate`), a handler failure (read of a not-rendered chat), and an
//! invalid-input failure (`help_navigate` off the allowlist). Each op's full
//! `ToolResult` (`{ toolName, success, result, error? }`) is compared, then the
//! `conversation_annotations` table in the placeholdered natural-key form.
//!
//! This proves dispatch + handler + harness compose end-to-end. The unknown-tool
//! LOUD fallback (`Unknown tool: <name>`) is unit-tested in
//! `tools::executor::tests` instead of here: v4's genuine unknown path routes
//! through the (unported) plugin registry, whose presence in the tsx env would
//! change the failure shape — the documented plugin-routing deferral.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-tooldispatch-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-tooldispatch-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-tool-dispatch-fixture.ts
//!   QT_FIXTURE_TOOLDISPATCH=/tmp/qt-tooldispatch-main.db \
//!   QT_FIXTURE_TOOLDISPATCH_MOUNT=/tmp/qt-tooldispatch-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/tool-dispatch.ts \
//!     > /tmp/oracle-tooldispatch.ndjson
//! Run:
//!   QT_ORACLE_TOOLDISPATCH=/tmp/oracle-tooldispatch.ndjson \
//!   QT_FIXTURE_TOOLDISPATCH=/tmp/qt-tooldispatch-main.db \
//!   QT_FIXTURE_TOOLDISPATCH_MOUNT=/tmp/qt-tooldispatch-mount.db \
//!     cargo test -p quilltap-harness --test tool_dispatch_equivalence

use std::path::PathBuf;

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::model::stream::CannedStreamingProvider;
use quilltap_core::services::carina_query::UnavailableBrahmaConsole;
use quilltap_core::services::carina_runner::{
    CarinaRunError, PostProsperoCarinaError, ProsperoCarinaErrorArgs,
};
use quilltap_core::services::native_tool_loop::NoToolCallDetector;
use quilltap_core::services::tool_execution::{
    create_tool_context, CannedToolRunner, ToolCall, ToolResult, ToolRunner,
};
use quilltap_core::tools::ask_carina::{ErasedAskCarina, TypedAskCarina};
use quilltap_core::tools::executor::BuiltInToolRunner;
use quilltap_core::tools::self_inventory::{ClientShell, SelfInventoryEnv};
use serde::Deserialize;
use serde_json::{json, Map, Value};

/// A Clone no-op Prospero seam for the `TypedAskCarina` bound. v4's ask_carina
/// not-found path posts a Prospero announcement to `chat_messages`; the diff is
/// `conversation_annotations` only, so a no-op is invisible.
#[derive(Clone)]
struct NoProspero;
impl PostProsperoCarinaError for NoProspero {
    fn post(&mut self, _a: ProsperoCarinaErrorArgs) -> Result<(), CarinaRunError> {
        Ok(())
    }
}

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct Op {
    tool: String,
    #[serde(rename = "chatId")]
    chat_id: String,
    #[serde(default, rename = "characterId")]
    character_id: Option<String>,
    #[serde(default, rename = "callingParticipantId")]
    calling_participant_id: Option<String>,
    args: Value,
}

#[derive(Deserialize)]
struct OracleOp {
    tool: String,
    result: Value,
}

#[derive(Deserialize)]
struct Oracle {
    ops: Vec<OracleOp>,
    dump: Value,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/tool-dispatch.json")
}

/// Serialize a [`ToolResult`] to v4's `JSON.stringify` shape (`undefined` keys
/// dropped): `toolName`, `success`, `result` always; `error`/`message` only when
/// present. No op here carries `metadata`.
fn tool_result_value(r: &ToolResult) -> Value {
    let mut m = Map::new();
    m.insert("toolName".into(), json!(r.tool_name));
    m.insert("success".into(), json!(r.success));
    m.insert("result".into(), r.result.clone());
    if let Some(e) = &r.error {
        m.insert("error".into(), json!(e));
    }
    if let Some(msg) = &r.message {
        m.insert("message".into(), json!(msg));
    }
    Value::Object(m)
}

/// A throwaway [`SelfInventoryEnv`] (no op in this batch is `self_inventory`).
fn dummy_env() -> SelfInventoryEnv {
    SelfInventoryEnv {
        version: String::new(),
        runtime_mode: "local-dev".to_string(),
        client_shell: ClientShell::Browser,
        mount_index_degraded: false,
        release_notes: None,
        changelog: None,
        model_info: Vec::new(),
        fallback_pricing: Vec::new(),
        registry_default_context: 8192,
    }
}

/// Placeholder the minted `id`/`createdAt`/`updatedAt`, sort by the natural key.
fn normalize_dump(dump: &Value) -> Value {
    let mut rows: Vec<Value> = dump["rows"].as_array().cloned().unwrap_or_default();
    rows.sort_by(|a, b| {
        let key = |v: &Value| {
            (
                v["chatId"].as_str().unwrap_or("").to_string(),
                v["messageIndex"].as_f64().unwrap_or(0.0).to_bits(),
                v["characterName"].as_str().unwrap_or("").to_string(),
            )
        };
        key(a).cmp(&key(b))
    });
    let rows: Vec<Value> = rows
        .into_iter()
        .map(|mut r| {
            if let Some(obj) = r.as_object_mut() {
                for k in ["id", "createdAt", "updatedAt"] {
                    if obj.contains_key(k) {
                        obj.insert(k.into(), json!("<placeholder>"));
                    }
                }
            }
            r
        })
        .collect();
    json!({ "table": dump["table"], "columns": dump["columns"], "rows": rows })
}

#[tokio::test]
async fn tool_dispatch_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_TOOLDISPATCH") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_TOOLDISPATCH to the oracle NDJSON (see header).");
            return;
        }
    };
    let (fixture, fixture_mount) = match (
        std::env::var("QT_FIXTURE_TOOLDISPATCH"),
        std::env::var("QT_FIXTURE_TOOLDISPATCH_MOUNT"),
    ) {
        (Ok(a), Ok(b)) => (a, b),
        _ => {
            eprintln!(
                "SKIP: set QT_FIXTURE_TOOLDISPATCH + QT_FIXTURE_TOOLDISPATCH_MOUNT (header)."
            );
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle: Oracle = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle");

    assert_eq!(spec.ops.len(), oracle.ops.len(), "op count: spec vs oracle");

    // Fresh copies of the seed fixtures (both DBs).
    let pid = std::process::id();
    let work_main = std::env::temp_dir().join(format!("qt-tooldispatch-rust-{pid}-main.db"));
    let work_mount = std::env::temp_dir().join(format!("qt-tooldispatch-rust-{pid}-mount.db"));
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(&fixture, &work_main).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&fixture_mount, &work_mount).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture: {e}"));

    // W4.10a: wire the REAL Carina engine behind the `ask_carina` dispatch (the
    // erased seam over `run_carina_query`). The corpus's ask_carina op names a
    // NONEXISTENT answerer, so the engine resolves `not-found` and returns v4's
    // exact error WITHOUT a model call — proving the dispatch row (`fail(...)`) +
    // the not-found handler path against v4's REAL `executeAskCarinaTool`.
    let runner = BuiltInToolRunner::new(db.clone(), dummy_env()).with_ask_carina(
        ErasedAskCarina::new(TypedAskCarina {
            db: db.clone(),
            embedding: CannedEmbeddingProvider::new(),
            streaming: CannedStreamingProvider::new(),
            tool_runner: CannedToolRunner::new(),
            tool_detector: NoToolCallDetector,
            brahma: UnavailableBrahmaConsole,
            prospero: NoProspero,
            model_supports_native_tools: false,
            now_ms: 0.0,
        }),
    );

    for (i, op) in spec.ops.iter().enumerate() {
        let tc = ToolCall {
            name: op.tool.clone(),
            arguments: op.args.clone(),
            call_id: None,
        };
        // The context carries the character + calling participant the dispatcher
        // reads (character-name resolution, the participation check, etc.).
        let ctx = create_tool_context(
            op.chat_id.clone(),
            spec.user_id.clone(),
            op.character_id.clone().unwrap_or_default(),
            op.calling_participant_id.clone().unwrap_or_default(),
            None,
            None,
            None,
            None,
            None,
        );
        let result = runner.run(&tc, &ctx).await;
        let got = tool_result_value(&result);

        let oo = &oracle.ops[i];
        assert_eq!(oo.tool, op.tool, "op {i}: tool mismatch");
        assert_eq!(
            got,
            oo.result,
            "op {i} ({}): ToolResult diverged\n  rust:   {}\n  oracle: {}",
            op.tool,
            serde_json::to_string(&got).unwrap(),
            serde_json::to_string(&oo.result).unwrap()
        );
    }

    // Diff the final annotations table in the normalized form.
    let got_dump = db
        .read_main(|conn| dump_table_json_conn(conn, "conversation_annotations", "id"))
        .expect("dump conversation_annotations");
    assert_eq!(
        normalize_dump(&got_dump),
        normalize_dump(&oracle.dump),
        "conversation_annotations dump diverged"
    );

    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    eprintln!(
        "OK: tool-dispatch end-to-end matched oracle ({} ops, dispatch + handlers + harness).",
        spec.ops.len()
    );
}
