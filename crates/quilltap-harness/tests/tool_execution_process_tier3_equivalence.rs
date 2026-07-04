//! Tier-3 differential test: v4's `processToolCalls` (W4.1c sub-unit c.2,
//! lib/services/chat-message/tool-execution.service.ts).
//!
//! Both sides run the SAME batch corpus (`tool-execution-process-tier3.json`)
//! through `process_tool_calls`, injecting IDENTICAL canned per-call results
//! (keyed by `name|JSON.stringify(args)|callId`). No DB — processToolCalls only
//! emits SSE frames + builds the ToolMessage slate. Per batch the ordered frame
//! trace, the returned `toolMessages`, and the `generatedImagePaths` are compared
//! (as parsed JSON Values — object key order normalized, matching the established
//! chat-events differentials).
//!
//! Banks: a success dispatch (status frame + pretty-JSON content); a failure (no
//! status frame + the `Error: <error> - <message>` text + the failure toolResult
//! frame); generate_image (image-path extraction + the count summary + metadata);
//! and a mixed batch (a metadata-bearing vault-read success + a failure).
//!
//! Generate the oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-toolexec-process.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- tool-execution-process-tier3
//! Run:
//!   QT_ORACLE_TOOLEXEC_PROCESS=/tmp/oracle-toolexec-process.ndjson \
//!     cargo test -p quilltap-harness --test tool_execution_process_tier3_equivalence

use quilltap_core::services::chat_events::RecordingSink;
use quilltap_core::services::tool_execution::{
    create_tool_context, process_tool_calls, CannedToolRunner, GeneratedImage, StatusContext,
    ToolCall, ToolMessage, ToolResult,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    batches: Vec<Batch>,
}

#[derive(Deserialize)]
struct Batch {
    name: String,
    #[serde(rename = "statusContext")]
    status_context: Option<WireStatus>,
    #[serde(rename = "toolCalls")]
    tool_calls: Vec<WireToolCall>,
    #[serde(rename = "cannedResults")]
    canned_results: Vec<WireCanned>,
}

#[derive(Deserialize)]
struct WireStatus {
    #[serde(rename = "characterName")]
    character_name: String,
    #[serde(rename = "characterId")]
    character_id: String,
}

#[derive(Deserialize, Clone)]
struct WireToolCall {
    name: String,
    arguments: Value,
    #[serde(rename = "callId", default)]
    call_id: Option<String>,
}

#[derive(Deserialize)]
struct WireCanned {
    #[serde(rename = "toolName")]
    tool_name: String,
    success: bool,
    result: Value,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    metadata: Option<WireMetadata>,
}

#[derive(Deserialize)]
struct WireMetadata {
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(rename = "expandedPrompt", default)]
    expanded_prompt: Option<String>,
}

impl WireToolCall {
    fn to_tool_call(&self) -> ToolCall {
        ToolCall {
            name: self.name.clone(),
            arguments: self.arguments.clone(),
            call_id: self.call_id.clone(),
        }
    }
}

impl WireCanned {
    fn to_tool_result(&self) -> ToolResult {
        ToolResult {
            tool_name: self.tool_name.clone(),
            success: self.success,
            result: self.result.clone(),
            error: self.error.clone(),
            message: self.message.clone(),
            metadata: self.metadata.as_ref().map(|m| {
                quilltap_core::services::tool_execution::ToolMetadata {
                    provider: m.provider.clone(),
                    model: m.model.clone(),
                    expanded_prompt: m.expanded_prompt.clone(),
                }
            }),
        }
    }
}

/// A JS-number Value (an integer-valued double renders as an integer, so the
/// re-derived image size/width/height match v4's emitted JS numbers).
fn js_num(v: f64) -> Value {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 9.007e15 {
        json!(v as i64)
    } else {
        json!(v)
    }
}

/// Serialize a [`ToolMessage`] to v4's emitted shape (`{toolName, success,
/// content, arguments, callId?, metadata?}` — processToolCalls sets no
/// anchorOffset/seq, so they are omitted).
fn tool_message_value(m: &ToolMessage) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("toolName".into(), json!(m.tool_name));
    obj.insert("success".into(), json!(m.success));
    obj.insert("content".into(), json!(m.content));
    if let Some(args) = &m.arguments {
        obj.insert("arguments".into(), args.clone());
    }
    if let Some(cid) = &m.call_id {
        obj.insert("callId".into(), json!(cid));
    }
    if let Some(md) = &m.metadata {
        let mut mo = serde_json::Map::new();
        if let Some(p) = &md.provider {
            mo.insert("provider".into(), json!(p));
        }
        if let Some(model) = &md.model {
            mo.insert("model".into(), json!(model));
        }
        if let Some(ep) = &md.expanded_prompt {
            mo.insert("expandedPrompt".into(), json!(ep));
        }
        obj.insert("metadata".into(), Value::Object(mo));
    }
    Value::Object(obj)
}

/// Serialize a [`GeneratedImage`] to v4's emitted shape (`{id, filename,
/// filepath, mimeType, size, width?, height?, sha256?}`; numeric fields as JS
/// numbers).
fn generated_image_value(g: &GeneratedImage) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("id".into(), json!(g.id));
    obj.insert("filename".into(), json!(g.filename));
    obj.insert("filepath".into(), json!(g.filepath));
    obj.insert("mimeType".into(), json!(g.mime_type));
    obj.insert("size".into(), js_num(g.size));
    if let Some(w) = g.width {
        obj.insert("width".into(), js_num(w));
    }
    if let Some(h) = g.height {
        obj.insert("height".into(), js_num(h));
    }
    if let Some(s) = &g.sha256 {
        obj.insert("sha256".into(), json!(s));
    }
    Value::Object(obj)
}

#[tokio::test]
async fn tool_execution_process_tier3_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_TOOLEXEC_PROCESS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_TOOLEXEC_PROCESS to the oracle NDJSON (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/tool-execution-process-tier3.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle");
    let oracle_batches = oracle["batches"].as_array().expect("oracle batches");

    let ctx = create_tool_context(
        "chat-1", "user-1", "char-1", "pp-1", None, None, None, None, None,
    );

    for (bi, batch) in spec.batches.iter().enumerate() {
        let calls: Vec<ToolCall> = batch.tool_calls.iter().map(|c| c.to_tool_call()).collect();
        let mut runner = CannedToolRunner::new();
        for (call, canned) in calls.iter().zip(batch.canned_results.iter()) {
            runner.register(call, canned.to_tool_result());
        }
        let sink = RecordingSink::new();
        let status = batch.status_context.as_ref().map(|s| StatusContext {
            character_name: s.character_name.clone(),
            character_id: s.character_id.clone(),
        });
        let out = process_tool_calls(&calls, &ctx, &sink, &runner, status.as_ref()).await;

        let got = json!({
            "name": batch.name,
            "frames": sink.events_json(),
            "toolMessages": out.tool_messages.iter().map(tool_message_value).collect::<Vec<_>>(),
            "generatedImagePaths": out.generated_image_paths.iter().map(generated_image_value).collect::<Vec<_>>(),
        });

        let want = &oracle_batches[bi];
        assert_eq!(
            got["name"], want["name"],
            "batch {bi} name (spec/oracle order mismatch)"
        );
        assert_eq!(
            got["frames"], want["frames"],
            "batch `{}` frame trace diverged\n  rust:   {}\n  oracle: {}",
            batch.name, got["frames"], want["frames"]
        );
        assert_eq!(
            got["toolMessages"], want["toolMessages"],
            "batch `{}` toolMessages diverged\n  rust:   {}\n  oracle: {}",
            batch.name, got["toolMessages"], want["toolMessages"]
        );
        assert_eq!(
            got["generatedImagePaths"], want["generatedImagePaths"],
            "batch `{}` generatedImagePaths diverged\n  rust:   {}\n  oracle: {}",
            batch.name, got["generatedImagePaths"], want["generatedImagePaths"]
        );
    }

    eprintln!(
        "OK: tool-execution processToolCalls tier-3 (c.2) matched oracle ({} batches).",
        spec.batches.len()
    );
}
