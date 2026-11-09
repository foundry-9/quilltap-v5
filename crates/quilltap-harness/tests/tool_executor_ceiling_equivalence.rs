//! Tier-3 differential (P4.D343): the execution-time effect ceiling —
//! `BuiltInToolRunner::run`'s refusal (v5) vs v4's REAL
//! `executeToolCallWithContext` (`lib/chat/tool-executor.ts:343-371`) at the pin
//! `236c04e35` (v4 `b15fe9404`).
//!
//! The check sits after the mountPool / operatorSurface refusal and before any
//! dispatch on both sides, so a refused call reaches no handler. Per row: for a
//! REFUSED call the whole result object, compared as BYTES in v4's key order
//! (`{toolName, success, result: {formattedText}, error}`), and the INFO line;
//! for a call WITHIN the ceiling the DEBUG line only (the handler's own result
//! is not part of this family — v5's runner reaches a recording fallback for an
//! unknown name, v4 its registry); with NO ceiling, neither line (the silence
//! leg). The five seed rows are v4's `tool-executor.effect-ceiling.test.ts`.
//!
//! Generate the oracle (Node 24; jest ignores `.claude/`, so mirror the case):
//!   cd ~/source/quilltap-server
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   M=/tmp/qt-tool-executor-ceiling-mirror; rm -rf $M; mkdir -p $M/cases
//!   cp $V5W/harness/oracle/cases/tool-executor-ceiling.test.ts $M/cases/
//!   QT_ORACLE_OUT=/tmp/oracle-tool-executor-ceiling.ndjson \
//!     npx jest --silent --roots "$PWD" --roots "$M/cases" --testTimeout=180000 --runTestsByPath "$M/cases/tool-executor-ceiling.test.ts"
//! Run:
//!   QT_ORACLE_TOOL_EXECUTOR_CEILING=/tmp/oracle-tool-executor-ceiling.ndjson \
//!     cargo test -p quilltap-harness --test tool_executor_ceiling_equivalence -- --nocapture

use std::sync::{Arc, Mutex};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::tool_execution::{
    create_tool_context, ToolCall, ToolExecutionContext, ToolResult, ToolRunner,
};
use quilltap_core::tools::effects::ToolEffect;
use quilltap_core::tools::executor::BuiltInToolRunner;
use quilltap_core::tools::self_inventory::{ClientShell, SelfInventoryEnv};
use serde_json::{json, Map, Value};

mod common;

const PEPPER: &str = "cXVpbGx0YXAtdGVzdC1wZXBwZXItMzItYnl0ZXMhIQ==";

/// The fallback a non-built-in name reaches: records the call and answers a
/// fixed failure (never compared).
#[derive(Clone, Default)]
struct RecordingFallback(Arc<Mutex<Vec<String>>>);

impl ToolRunner for RecordingFallback {
    #[allow(clippy::manual_async_fn)]
    fn run(
        &self,
        tool_call: &ToolCall,
        _ctx: &ToolExecutionContext,
    ) -> impl std::future::Future<Output = ToolResult> + Send {
        self.0.lock().unwrap().push(tool_call.name.clone());
        let name = tool_call.name.clone();
        async move {
            ToolResult {
                tool_name: name,
                success: false,
                result: Value::Null,
                error: Some("recorded".into()),
                message: None,
                metadata: None,
            }
        }
    }
}

fn effect_map(row: &Value, key: &str) -> std::collections::BTreeMap<String, ToolEffect> {
    row.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|p| {
                    (
                        p[0].as_str().unwrap().to_string(),
                        ToolEffect::from_str(p[1].as_str().unwrap()).unwrap(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// v4's refusal object in its key order.
fn result_json(r: &ToolResult) -> Value {
    let mut m = Map::new();
    m.insert("toolName".into(), json!(r.tool_name));
    m.insert("success".into(), json!(r.success));
    m.insert("result".into(), r.result.clone());
    m.insert("error".into(), json!(r.error));
    Value::Object(m)
}

/// A captured line → `{level, message, fields}` in v4's capture shape.
fn line_json(line: &str) -> Option<Value> {
    // "<LEVEL> <target> <message> k=v k=v"
    let (level, rest) = line.split_once(' ')?;
    let (_target, rest) = rest.split_once(' ')?;
    let msg_end = [" context="]
        .iter()
        .filter_map(|p| rest.find(p))
        .min()
        .unwrap_or(rest.len());
    let message = rest[..msg_end].trim();
    if !message.contains("effect ceiling") {
        return None;
    }
    let mut fields = Map::new();
    for (k, v) in common::capture_fields(&rest[msg_end..]) {
        fields.insert(k.to_string(), Value::String(v.to_string()));
    }
    Some(json!({ "level": level.to_lowercase(), "message": message, "fields": fields }))
}

#[test]
fn tool_executor_ceiling_matches_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_TOOL_EXECUTOR_CEILING") else {
        eprintln!("SKIP: QT_ORACLE_TOOL_EXECUTOR_CEILING not set");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let dpath = dir.path().to_path_buf();
    quilltap_core::services::provisioning::provision_fresh_instance(&dpath, PEPPER).unwrap();
    let db = Db::open(
        DbPaths {
            main: dpath.join("quilltap.db"),
            mount_index: Some(dpath.join("quilltap-mount-index.db")),
            llm_logs: None,
        },
        PEPPER,
    )
    .unwrap();
    let env = SelfInventoryEnv {
        version: String::new(),
        runtime_mode: "local-dev".to_string(),
        client_shell: ClientShell::Unknown,
        mount_index_degraded: false,
        release_notes: None,
        changelog: None,
        model_info: Vec::new(),
        fallback_pricing: Vec::new(),
        registry_default_context: 8192,
    };
    let fallback = RecordingFallback::default();
    let runner = BuiltInToolRunner::new(db, env).with_fallback(fallback.clone());

    let mut failures = Vec::new();
    let mut rows = 0;
    let (mut refused_rows, mut silent_rows) = (0, 0);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let id = row["id"].as_str().unwrap().to_string();
        let call = ToolCall {
            name: row["call"]["name"].as_str().unwrap().to_string(),
            arguments: row["call"]["arguments"].clone(),
            call_id: None,
        };
        let mut ctx = create_tool_context("chat-1", "u1", "", "", None, None, None, None, None);
        // v4's context carries no character; the ceiling lines name none.
        ctx.character_id = None;
        ctx.calling_participant_id = None;
        ctx.effect_ceiling = row["effectCeiling"]
            .as_str()
            .map(|c| ToolEffect::from_str(c).unwrap());
        ctx.custom_tool_effects = effect_map(&row, "customToolEffects");
        ctx.plugin_tool_effects = effect_map(&row, "pluginToolEffects");
        let before = fallback.0.lock().unwrap().len();
        let (result, lines) =
            quilltap_core::test_support::captured_with(|| rt.block_on(runner.run(&call, &ctx)));
        let got_lines: Vec<Value> = lines.iter().filter_map(|l| line_json(l)).collect();
        let refused = got_lines.iter().any(|l| {
            l["message"]
                .as_str()
                .unwrap_or_default()
                .starts_with("Tool call refused")
        });
        let got = json!({
            "refused": refused,
            "result": if refused { result_json(&result) } else { Value::Null },
            "lines": got_lines,
        });
        let want = json!({
            "refused": row["refused"],
            "result": row["result"],
            "lines": row["lines"],
        });
        if refused {
            refused_rows += 1;
            // The refusal precedes any dispatch: the fallback saw nothing.
            assert_eq!(
                fallback.0.lock().unwrap().len(),
                before,
                "{id}: dispatched past a refusal"
            );
        }
        if row["lines"].as_array().is_some_and(Vec::is_empty) {
            silent_rows += 1;
        }
        let (g, w) = (
            serde_json::to_string(&got).unwrap(),
            serde_json::to_string(&want).unwrap(),
        );
        if g != w {
            failures.push(format!("{id}:\n  v5: {g}\n  v4: {w}"));
        }
        rows += 1;
    }
    eprintln!("tool_executor_ceiling_equivalence: {rows} rows ({refused_rows} refused, {silent_rows} silent)");
    assert!(
        rows >= 19 && refused_rows >= 8 && silent_rows >= 1,
        "stale or truncated oracle"
    );
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
