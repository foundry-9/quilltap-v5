//! Unit tests for the Brahma one-shot console (W4.5b). The pure helpers are
//! checked directly; the never-throws / no-profile sentinel over a bare `Db`; and
//! the 25-turn loop bound + the dup/stale stuck-loop guards over a seeded `Db`
//! driven by a call-order scripted streaming provider (the differential
//! `brahma_console_tier3_equivalence` proves the byte-exact behavior against v4).

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::sync::Mutex;

use serde_json::{json, Value};

use crate::db::runtime::Db;
use crate::model::stream::{
    StreamChunk, StreamChunkResult, StreamParams, StreamingCompletionProvider,
};
use crate::services::native_tool_loop::ToolCallDetector;
use crate::services::tool_execution::{
    CannedToolRunner, ToolCall, ToolResult, ToolRunner,
};

use super::*;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

// ---------------------------------------------------------------------------
// Pure helpers.
// ---------------------------------------------------------------------------

#[test]
fn loop_and_guard_constants_match_v4() {
    assert_eq!(MAX_AGENT_TURNS, 25);
    assert_eq!(MAX_DUPLICATE_TOOL_CALLS, 2);
}

#[test]
fn requires_api_key_defaults_true_for_unknown() {
    assert!(requires_api_key("ANTHROPIC"));
    assert!(requires_api_key("OPENAI"));
    assert!(!requires_api_key("OLLAMA")); // manifest requiresApiKey: false
    assert!(requires_api_key("NOT_A_PROVIDER")); // v4 `?? true`
}

#[test]
fn system_prompt_composes_base_sql_and_instructions() {
    let p = build_brahma_system_prompt("TOOLS_HERE", true);
    assert!(p.starts_with("You are the Brahma Console"));
    assert!(p.contains("## You can also run read-only SQL"));
    assert!(p.ends_with("TOOLS_HERE"));
    // The three parts join on a blank line.
    assert!(p.contains("\n\nTOOLS_HERE"));
    // Without the SQL section the console never runs, but the composer honours it.
    let no_sql = build_brahma_system_prompt("X", false);
    assert!(!no_sql.contains("## You can also run read-only SQL"));
}

#[test]
fn tool_call_signature_is_stable_and_string_insensitive() {
    let a = vec![ToolCall {
        name: "run_sql".into(),
        arguments: json!({ "sql": "SELECT  1", "database": "main" }),
        call_id: Some("c1".into()),
    }];
    // Same call with a DIFFERENT callId + whitespace/case in the string value →
    // same signature (callId is not part of it; strings are collapsed+lowercased).
    let b = vec![ToolCall {
        name: "run_sql".into(),
        arguments: json!({ "database": "main", "sql": "select 1" }),
        call_id: Some("c2".into()),
    }];
    assert_eq!(normalize_tool_call_signature(&a), normalize_tool_call_signature(&b));
    // A different argument value → a different signature.
    let c = vec![ToolCall {
        name: "run_sql".into(),
        arguments: json!({ "sql": "SELECT 2", "database": "main" }),
        call_id: None,
    }];
    assert_ne!(normalize_tool_call_signature(&a), normalize_tool_call_signature(&c));
}

#[test]
fn to_completion_messages_maps_roles() {
    let msgs = vec![
        plain_message("system", "s"),
        plain_message("assistant", "a"),
        plain_message("tool", "t"),
        plain_message("user", "u"),
        plain_message("weird", "w"),
    ];
    let out = to_completion_messages(&msgs);
    use crate::model::completion::CompletionRole::*;
    assert_eq!(out[0].role, System);
    assert_eq!(out[1].role, Assistant);
    assert_eq!(out[2].role, Tool);
    assert_eq!(out[3].role, User);
    assert_eq!(out[4].role, User); // unknown → user
}

// ---------------------------------------------------------------------------
// A call-order scripted streaming provider (ignores the key — returns the next
// scripted sequence). A local marker detector + echo tool runner drive the loop.
// ---------------------------------------------------------------------------

struct ScriptedStream {
    queue: Mutex<VecDeque<Vec<StreamChunkResult>>>,
}
impl ScriptedStream {
    fn new(seqs: Vec<Vec<StreamChunkResult>>) -> Self {
        Self {
            queue: Mutex::new(seqs.into_iter().collect()),
        }
    }
    fn remaining(&self) -> usize {
        self.queue.lock().unwrap().len()
    }
}
impl StreamingCompletionProvider for ScriptedStream {
    fn stream_message(
        &self,
        _provider: &str,
        _base_url: Option<&str>,
        _params: &StreamParams,
    ) -> impl Future<Output = tokio::sync::mpsc::Receiver<StreamChunkResult>> + Send {
        let seq = self
            .queue
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| vec![Ok(StreamChunk::done(None))]);
        async move {
            let (tx, rx) = tokio::sync::mpsc::channel(seq.len().max(1) + 1);
            for item in seq {
                let _ = tx.send(item).await;
            }
            rx
        }
    }
}

/// A done chunk carrying a `{ marker }` raw response.
fn tool_stream(marker: &str) -> Vec<StreamChunkResult> {
    let mut done = StreamChunk::done(None);
    done.raw_response = Some(json!({ "marker": marker }));
    vec![Ok(done)]
}
/// A plain text stream (content + a bare done).
fn text_stream(text: &str) -> Vec<StreamChunkResult> {
    vec![
        Ok(StreamChunk::content(text)),
        Ok(StreamChunk::done(None)),
    ]
}

struct MarkerDetector {
    by_marker: HashMap<String, Vec<ToolCall>>,
}
impl ToolCallDetector for MarkerDetector {
    fn detect(&self, raw_response: &Value, _provider: &str) -> Vec<ToolCall> {
        raw_response
            .get("marker")
            .and_then(Value::as_str)
            .and_then(|m| self.by_marker.get(m))
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|c| ToolCall {
                name: c.name,
                arguments: c.arguments,
                call_id: c.call_id,
            })
            .collect()
    }
}

/// Returns `success` with the call's arguments echoed as the result (so distinct
/// args ⇒ distinct result content ⇒ no stale; identical args ⇒ identical content).
struct EchoRunner;
impl ToolRunner for EchoRunner {
    fn run(
        &self,
        tool_call: &ToolCall,
        _ctx: &ToolExecutionContext,
    ) -> impl Future<Output = ToolResult> + Send {
        let r = ToolResult {
            tool_name: tool_call.name.clone(),
            success: true,
            result: tool_call.arguments.clone(),
            error: None,
            message: None,
            metadata: None,
        };
        async move { r }
    }
}

/// Returns the SAME result content for every call (so distinct signatures still
/// produce identical fingerprints — the stale path).
struct ConstResultRunner;
impl ToolRunner for ConstResultRunner {
    fn run(
        &self,
        tool_call: &ToolCall,
        _ctx: &ToolExecutionContext,
    ) -> impl Future<Output = ToolResult> + Send {
        let name = tool_call.name.clone();
        async move {
            ToolResult {
                tool_name: name,
                success: true,
                result: json!({ "same": "result" }),
                error: None,
                message: None,
                metadata: None,
            }
        }
    }
}

/// Build an in-memory `Db` seeded with one OLLAMA default profile (OLLAMA
/// requires no api key, so the api-key step is skipped). Returns the temp dir
/// guard + the `Db`. Uses the async writer (a `#[tokio::test]` cannot
/// `write_blocking`).
async fn seeded_db() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("brahma-main.db");
    let db = Db::open_main(&path, PEPPER).unwrap();
    db.write(|ws| {
        let conn = ws.main().connection();
        conn.execute(
            "CREATE TABLE connection_profiles \
               (id, userId, name, provider, transport, courierDeltaMode, apiKeyId, baseUrl, \
                modelName, parameters, isDefault, isCheap, allowWebSearch, useNativeWebSearch, \
                allowToolUse, pseudoToolMode, modelClass, maxContext, maxTokens, \
                isDangerousCompatible, supportsImageUpload, tags, sortIndex, totalTokens, \
                totalPromptTokens, totalCompletionTokens, messageCount, createdAt, updatedAt)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO connection_profiles VALUES \
               ('p1', 'u1', 'Default', 'OLLAMA', 'default', 1, NULL, NULL, 'llama3', '{}', \
                1, 0, 0, 0, 1, 'auto', NULL, NULL, NULL, 0, 0, '[]', 0, 0, 0, 0, 0, \
                '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z')",
            [],
        )
        .unwrap();
        Ok::<(), crate::db::DbError>(())
    })
    .await
    .unwrap();
    (dir, db)
}

fn one_call(marker: &str, name: &str, args: Value, call_id: Option<&str>) -> (String, Vec<ToolCall>) {
    (
        marker.to_string(),
        vec![ToolCall {
            name: name.to_string(),
            arguments: args,
            call_id: call_id.map(str::to_string),
        }],
    )
}

#[tokio::test]
async fn no_profile_sentinel_and_never_throws() {
    // A bare Db (no connection_profiles table) → resolution fails → "no-profile".
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open_main(dir.path().join("bare.db"), PEPPER).unwrap();
    let streaming = crate::model::stream::CannedStreamingProvider::new();
    let runner = CannedToolRunner::new();
    let detector = crate::services::native_tool_loop::NoToolCallDetector;
    let deps = BrahmaQueryDeps {
        db: &db,
        streaming: &streaming,
        tool_runner: &runner,
        tool_detector: &detector,
        model_supports_native_tools: true,
    };
    let r = run_brahma_query(&deps, "u1", "c1", "hi").await;
    assert!(!r.ok);
    assert_eq!(r.detail.as_deref(), Some("no-profile"));
}

#[tokio::test]
async fn loop_bound_forces_a_final_answer_at_the_cap() {
    let (_dir, db) = seeded_db().await;

    // 24 DISTINCT tool iterations (distinct args ⇒ no dup, distinct results ⇒ no
    // stale), then the 25th stream (after the force-final push) returns text.
    let mut seqs = Vec::new();
    let mut by_marker = HashMap::new();
    for n in 1..MAX_AGENT_TURNS {
        let marker = format!("m{n}");
        seqs.push(tool_stream(&marker));
        let (_m, calls) =
            one_call(&marker, "run_sql", json!({ "sql": format!("q{n}") }), Some(&format!("c{n}")));
        by_marker.insert(marker, calls);
    }
    seqs.push(text_stream("FINAL ANSWER"));

    let streaming = ScriptedStream::new(seqs);
    let detector = MarkerDetector { by_marker };
    let runner = EchoRunner;
    let deps = BrahmaQueryDeps {
        db: &db,
        streaming: &streaming,
        tool_runner: &runner,
        tool_detector: &detector,
        model_supports_native_tools: true,
    };
    let r = run_brahma_query(&deps, "u1", "c1", "count things").await;
    assert!(r.ok, "expected ok, got {r:?}");
    assert_eq!(r.answer, "FINAL ANSWER");
    // All 25 scripted streams consumed — the loop ran to the cap.
    assert_eq!(streaming.remaining(), 0);
}

#[tokio::test]
async fn dup_guard_forces_a_final_after_repeated_calls() {
    let (_dir, db) = seeded_db().await;

    // Three IDENTICAL tool calls; the 3rd trips the duplicate guard (duplicateCount
    // >= 2), so it is NOT executed — a nudge is threaded and the 4th stream returns
    // the final text.
    let (marker, calls) = one_call("same", "run_sql", json!({ "sql": "SELECT 1" }), Some("cc"));
    let mut by_marker = HashMap::new();
    by_marker.insert(marker.clone(), calls);
    let seqs = vec![
        tool_stream(&marker),
        tool_stream(&marker),
        tool_stream(&marker),
        text_stream("STOPPED"),
    ];
    let streaming = ScriptedStream::new(seqs);
    let detector = MarkerDetector { by_marker };
    let runner = EchoRunner;
    let deps = BrahmaQueryDeps {
        db: &db,
        streaming: &streaming,
        tool_runner: &runner,
        tool_detector: &detector,
        model_supports_native_tools: true,
    };
    let r = run_brahma_query(&deps, "u1", "c1", "loop please").await;
    assert!(r.ok, "expected ok, got {r:?}");
    assert_eq!(r.answer, "STOPPED");
    assert_eq!(streaming.remaining(), 0);
}

#[tokio::test]
async fn stale_guard_forces_a_final_when_results_repeat() {
    let (_dir, db) = seeded_db().await;

    // Three DISTINCT signatures (distinct args ⇒ duplicateCount stays 0) but the
    // ConstResultRunner returns IDENTICAL content, so `staleIterations` climbs to
    // the cap on the 3rd and trips the STALE branch (not the dup branch).
    // iter1 exec→stale0, iter2 exec→stale1, iter3 exec→stale2, iter4 sees stale>=2
    // → the stuck guard fires (no exec + nudge), iter5 text → done.
    let mut by_marker = HashMap::new();
    let mut seqs = Vec::new();
    for n in 1..=4 {
        let marker = format!("s{n}");
        seqs.push(tool_stream(&marker));
        by_marker.insert(
            marker,
            vec![ToolCall {
                name: "run_sql".into(),
                arguments: json!({ "sql": format!("distinct-{n}") }),
                call_id: Some(format!("id{n}")),
            }],
        );
    }
    seqs.push(text_stream("HALTED"));
    let streaming = ScriptedStream::new(seqs);
    let detector = MarkerDetector { by_marker };
    let runner = ConstResultRunner;
    let deps = BrahmaQueryDeps {
        db: &db,
        streaming: &streaming,
        tool_runner: &runner,
        tool_detector: &detector,
        model_supports_native_tools: true,
    };
    let r = run_brahma_query(&deps, "u1", "c1", "same thing").await;
    assert!(r.ok, "expected ok, got {r:?}");
    assert_eq!(r.answer, "HALTED");
    assert_eq!(streaming.remaining(), 0);
}
