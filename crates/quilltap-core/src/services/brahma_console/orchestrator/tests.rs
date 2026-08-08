//! Unit tests for the Brahma Console orchestrator. The differential
//! (`brahma_orchestrator_tier3_equivalence`) proves the byte-exact frame + row
//! behavior against v4's real code for the plain / run_sql / text-block /
//! submit_final / dup-stuck arms; this test covers the ONE termination the
//! differential does not — the agent-turn cap, now an operator-set instance
//! setting (`resolve_brahma_max_agent_turns`, default 50; seeded small here) —
//! over a temp copy of the committed `brahma-*.db` fixture, mirroring the
//! one-shot engine's `loop_bound_forces_a_final_answer_at_the_operator_cap`.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::{json, Value};

use crate::db::runtime::{Db, DbPaths};
use crate::model::stream::{
    StreamChunk, StreamChunkResult, StreamParams, StreamingCompletionProvider,
};
use crate::services::chat_events::RecordingSink;
use crate::services::message_finalizer::NoCostTracking;
use crate::services::native_tool_loop::ToolCallDetector;
use crate::services::tool_execution::{ToolCall, ToolExecutionContext, ToolResult, ToolRunner};

use super::{handle_brahma_console_message, BrahmaConsoleSendOptions, BrahmaSendDeps};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const USER: &str = "e18e05bc-63e8-4539-8a85-719b7a508850";
const CHAT_B: &str = "c1000000-0000-4000-8000-00000000000b"; // empty brahma chat, pinned P1
const CHAT_C: &str = "c1000000-0000-4000-8000-00000000000c"; // empty brahma chat, unpinned

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

/// A call-order scripted streaming provider (ignores the key — returns the next
/// scripted sequence; the cap test threads 24 distinct results, never repeating).
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

fn tool_stream(marker: &str) -> Vec<StreamChunkResult> {
    let mut done = StreamChunk::done(None);
    done.raw_response = Some(json!({ "marker": marker }));
    vec![Ok(done)]
}
fn text_stream(text: &str) -> Vec<StreamChunkResult> {
    vec![Ok(StreamChunk::content(text)), Ok(StreamChunk::done(None))]
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
    }
}

/// Echoes the call's arguments as the result (distinct args ⇒ distinct result ⇒
/// no stale-guard trip), so distinct-per-turn tool calls run to the cap.
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

/// Open a fresh temp copy of the committed brahma fixture (never write the seed).
fn fixture_copy() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("brahma-main.db");
    let mount = dir.path().join("brahma-mount.db");
    std::fs::copy(fixtures_dir().join("brahma-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("brahma-mount.db"), &mount).unwrap();
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        PEPPER,
    )
    .unwrap();
    (dir, db)
}

#[tokio::test]
async fn loop_bound_forces_a_final_answer_at_the_operator_cap() {
    let (_dir, db) = fixture_copy();
    // Seed a small operator-set budget so the loop proves it reads
    // `resolve_brahma_max_agent_turns` (Settings → Chat → Brahma Console), not a
    // retired hardcoded constant. Created IF NOT EXISTS so the test is agnostic
    // to whether the committed fixture's vintage carries the instance_settings
    // table.
    const CAP: i64 = 6;
    db.write(|ws| {
        let conn = ws.main().connection();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS \"instance_settings\" \
             (\"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL)",
            [],
        )
        .unwrap();
        crate::db::instance_settings::set_brahma_console_settings(conn, CAP).unwrap();
        Ok::<(), crate::db::DbError>(())
    })
    .await
    .unwrap();

    // CAP-1 DISTINCT tool iterations (distinct args ⇒ no dup, distinct results ⇒
    // no stale), then the CAP-th stream (after the force-final push) returns text.
    let mut seqs = Vec::new();
    let mut by_marker = HashMap::new();
    for n in 1..CAP {
        let marker = format!("m{n}");
        seqs.push(tool_stream(&marker));
        by_marker.insert(
            marker,
            vec![ToolCall {
                name: "run_sql".into(),
                arguments: json!({ "database": "main", "sql": format!("SELECT {n}") }),
                call_id: Some(format!("c{n}")),
            }],
        );
    }
    seqs.push(text_stream("FINAL ANSWER AT THE CAP"));

    let streaming = ScriptedStream::new(seqs);
    let detector = MarkerDetector { by_marker };
    let runner = EchoRunner;
    let sink = RecordingSink::new();
    let mut cost = NoCostTracking;
    let mut deps = BrahmaSendDeps {
        db: &db,
        streaming: &streaming,
        tool_runner: &runner,
        tool_detector: &detector,
        cost: &mut cost,
        model_supports_native_tools: true,
    };
    let opts = BrahmaConsoleSendOptions {
        content: "count everything".into(),
        file_ids: Vec::new(),
    };
    let result = handle_brahma_console_message(&mut deps, &sink, USER, CHAT_B, &opts)
        .await
        .expect("send ok");

    // A final answer was persisted (the loop terminated at the cap, not by tools).
    assert!(result.message_id.is_some(), "expected a final message id");
    // All CAP scripted streams consumed — the loop ran to the operator-set cap.
    assert_eq!(streaming.remaining(), 0);

    // The persisted transcript ends with the forced final answer.
    let messages = db
        .read_main(|c| crate::db::chats_messages_read::get_messages(c, CHAT_B))
        .unwrap();
    let last = messages.last().expect("messages present");
    assert_eq!(
        last.get("content").and_then(Value::as_str),
        Some("FINAL ANSWER AT THE CAP")
    );
    // Exactly one done frame closed the run.
    let done_frames = sink
        .events_json()
        .into_iter()
        .filter(|e| e.get("done").is_some())
        .count();
    assert_eq!(done_frames, 1);
}

#[tokio::test]
async fn stale_guard_forces_a_final_when_results_repeat() {
    let (_dir, db) = fixture_copy();

    // Four DISTINCT signatures (distinct args ⇒ duplicateCount stays 0) but the
    // ConstResultRunner returns IDENTICAL content, so `staleIterations` climbs to
    // the cap and trips the STALE branch (not the dup branch); the final stream
    // returns text → done. Mirrors the one-shot engine's stale-guard test.
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
    seqs.push(text_stream("HALTED BY STALE GUARD"));

    let streaming = ScriptedStream::new(seqs);
    let detector = MarkerDetector { by_marker };
    let runner = ConstResultRunner;
    let sink = RecordingSink::new();
    let mut cost = NoCostTracking;
    let mut deps = BrahmaSendDeps {
        db: &db,
        streaming: &streaming,
        tool_runner: &runner,
        tool_detector: &detector,
        cost: &mut cost,
        model_supports_native_tools: true,
    };
    let opts = BrahmaConsoleSendOptions {
        content: "same thing over and over".into(),
        file_ids: Vec::new(),
    };
    let result = handle_brahma_console_message(&mut deps, &sink, USER, CHAT_C, &opts)
        .await
        .expect("send ok");

    assert!(result.message_id.is_some());
    assert_eq!(streaming.remaining(), 0);
    let messages = db
        .read_main(|c| crate::db::chats_messages_read::get_messages(c, CHAT_C))
        .unwrap();
    let last = messages.last().expect("messages present");
    assert_eq!(
        last.get("content").and_then(Value::as_str),
        Some("HALTED BY STALE GUARD")
    );
}
