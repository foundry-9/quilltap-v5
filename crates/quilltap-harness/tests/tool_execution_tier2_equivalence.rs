//! Tier-2 differential test: v4's `saveToolMessages` (W4.1c sub-unit c.1,
//! lib/services/chat-message/tool-execution.service.ts).
//!
//! Both sides run the SAME call sequence (`tool-execution-tier2.json`) on a fresh
//! copy of the seed fixture (one chat + one GENERATED image file, `chat_messages`
//! materialized empty), then the `chat_messages` table (the TOOL-row marshaling +
//! content JSON), the `chats` table (the minted-timestamp side-effect), the
//! `files` table (the addLink/addTag image plumbing), and each call's return
//! `{ firstToolMessageId, generatedImageIds }` are compared.
//!
//! Banks: the whisper matrix (public / ALWAYS_PRIVATE `search`/`read_conversation`
//! whispered to the user participant / VAULT_READ `doc_read_file`/`doc_grep`
//! whispered unless `allowCrossCharacterVaultReads`); the content field order +
//! omission (anchorOffset/seq/callId + provider/model/prompt present vs absent);
//! a multi-message batch (order + `firstToolMessageId` = the first-created row);
//! the generated-image link + character tag + attachment id.
//!
//! NORMALIZATION (applied identically both sides): saveToolMessages mints the TOOL
//! row `id` + `createdAt`, the chat's `lastMessageAt`/`updatedAt` (a TOOL row is
//! `type:'message'`), and the file's `updatedAt` (addLink/addTag). Minted message
//! ids are remapped to first-seen tokens by a CONTENT-sorted walk (content is
//! pinned + distinct across the corpus); `files.linkedTo` + the returned
//! `firstToolMessageId` resolve through that shared map. Minted timestamps become
//! `<ts>` (sentinel-aware on chats/files: a cell equal to the seed sentinel stays
//! pinned, so a stray mint is caught). `tags`/`generatedImageIds` are the pinned
//! seeded file id.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-toolexec-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-tool-execution-fixture.ts
//!   QT_FIXTURE_TOOLEXEC=/tmp/qt-toolexec-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/tool-execution-tier2.ts > /tmp/oracle-toolexec.ndjson
//! Run:
//!   QT_ORACLE_TOOLEXEC=/tmp/oracle-toolexec.ndjson \
//!   QT_FIXTURE_TOOLEXEC=/tmp/qt-toolexec-fixture.db \
//!     cargo test -p quilltap-harness --test tool_execution_tier2_equivalence

use std::collections::HashMap;

use quilltap_core::db::Writer;
use quilltap_core::services::tool_execution::{
    save_tool_messages, GeneratedImage, ToolMessage, ToolMetadata, ToolWhisperContext,
};
use serde::Deserialize;
use serde_json::Value;

/// Minted-timestamp columns collapsed to `<ts>` (non-sentinel only).
const CHAT_TS_COLUMNS: &[&str] = &["lastMessageAt", "updatedAt"];
const FILE_TS_COLUMNS: &[&str] = &["updatedAt"];

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    sentinel: String,
    #[serde(rename = "userId")]
    user_id: String,
    calls: Vec<Call>,
}

#[derive(Deserialize)]
struct Call {
    #[serde(rename = "chatId")]
    chat_id: String,
    #[serde(rename = "characterId")]
    character_id: Option<String>,
    #[serde(rename = "participantId")]
    participant_id: Option<String>,
    #[serde(rename = "whisperContext")]
    whisper_context: Option<WireWhisper>,
    #[serde(rename = "generatedImagePaths")]
    generated_image_paths: Vec<WireImage>,
    #[serde(rename = "toolMessages")]
    tool_messages: Vec<WireToolMessage>,
}

#[derive(Deserialize)]
struct WireWhisper {
    #[serde(rename = "userParticipantId")]
    user_participant_id: Option<String>,
    #[serde(rename = "allowCrossCharacterVaultReads")]
    allow_cross_character_vault_reads: bool,
}

#[derive(Deserialize)]
struct WireImage {
    id: String,
    filename: String,
    filepath: String,
    #[serde(rename = "mimeType")]
    mime_type: String,
    size: f64,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    height: Option<f64>,
    #[serde(default)]
    sha256: Option<String>,
}

#[derive(Deserialize)]
struct WireToolMessage {
    #[serde(rename = "toolName")]
    tool_name: String,
    success: bool,
    content: String,
    #[serde(default)]
    arguments: Option<Value>,
    #[serde(rename = "callId", default)]
    call_id: Option<String>,
    #[serde(rename = "anchorOffset", default)]
    anchor_offset: Option<f64>,
    #[serde(default)]
    seq: Option<f64>,
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

impl WireToolMessage {
    fn to_tool_message(&self) -> ToolMessage {
        ToolMessage {
            tool_name: self.tool_name.clone(),
            success: self.success,
            content: self.content.clone(),
            arguments: self.arguments.clone(),
            call_id: self.call_id.clone(),
            anchor_offset: self.anchor_offset,
            seq: self.seq,
            metadata: self.metadata.as_ref().map(|m| ToolMetadata {
                provider: m.provider.clone(),
                model: m.model.clone(),
                expanded_prompt: m.expanded_prompt.clone(),
            }),
        }
    }
}

impl WireImage {
    fn to_generated_image(&self) -> GeneratedImage {
        GeneratedImage {
            id: self.id.clone(),
            filename: self.filename.clone(),
            filepath: self.filepath.clone(),
            mime_type: self.mime_type.clone(),
            size: self.size,
            width: self.width,
            height: self.height,
            sha256: self.sha256.clone(),
        }
    }
}

/// Build the minted-message-id → token map by sorting `chat_messages` rows on
/// their (pinned, distinct) `content` and enumerating.
fn build_id_map(messages: &Value) -> HashMap<String, String> {
    let mut pairs: Vec<(String, String)> = messages["rows"]
        .as_array()
        .expect("messages rows")
        .iter()
        .map(|r| {
            (
                r["content"].as_str().unwrap_or_default().to_string(),
                r["id"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    pairs
        .into_iter()
        .enumerate()
        .map(|(i, (_content, id))| (id, format!("m{i}")))
        .collect()
}

fn placeholder_ts(obj: &mut serde_json::Map<String, Value>, cols: &[&str], sentinel: &str) {
    for col in cols {
        let mint = obj
            .get(*col)
            .and_then(Value::as_str)
            .is_some_and(|s| s != sentinel);
        if mint {
            obj.insert((*col).to_string(), Value::String("<ts>".into()));
        }
    }
}

/// Remap message ids to tokens (re-sorting rows by the token) and placeholder
/// minted `createdAt`.
fn normalize_messages(messages: &mut Value, id_map: &HashMap<String, String>) {
    let rows = messages["rows"].as_array_mut().expect("messages rows");
    for row in rows.iter_mut() {
        let obj = row.as_object_mut().unwrap();
        if let Some(id) = obj.get("id").and_then(Value::as_str) {
            if let Some(tok) = id_map.get(id) {
                obj.insert("id".into(), Value::String(tok.clone()));
            }
        }
        obj.insert("createdAt".into(), Value::String("<ts>".into()));
    }
    rows.sort_by(|a, b| {
        a["id"]
            .as_str()
            .unwrap_or_default()
            .cmp(b["id"].as_str().unwrap_or_default())
    });
}

fn normalize_chats(chats: &mut Value, sentinel: &str) {
    for row in chats["rows"].as_array_mut().expect("chats rows") {
        placeholder_ts(row.as_object_mut().unwrap(), CHAT_TS_COLUMNS, sentinel);
    }
}

/// Placeholder the file `updatedAt` and remap message ids inside `linkedTo`.
fn normalize_files(files: &mut Value, id_map: &HashMap<String, String>, sentinel: &str) {
    for row in files["rows"].as_array_mut().expect("files rows") {
        let obj = row.as_object_mut().unwrap();
        placeholder_ts(obj, FILE_TS_COLUMNS, sentinel);
        if let Some(linked) = obj.get("linkedTo").and_then(Value::as_str) {
            if let Ok(ids) = serde_json::from_str::<Vec<String>>(linked) {
                let remapped: Vec<String> = ids
                    .into_iter()
                    .map(|id| id_map.get(&id).cloned().unwrap_or(id))
                    .collect();
                obj.insert(
                    "linkedTo".into(),
                    Value::String(serde_json::to_string(&remapped).unwrap()),
                );
            }
        }
    }
}

/// Remap `firstToolMessageId` in each return object through the shared id-map.
fn normalize_returns(returns: &mut Value, id_map: &HashMap<String, String>) {
    for r in returns.as_array_mut().expect("returns array") {
        let obj = r.as_object_mut().unwrap();
        if let Some(first) = obj.get("firstToolMessageId").and_then(Value::as_str) {
            if let Some(tok) = id_map.get(first) {
                obj.insert("firstToolMessageId".into(), Value::String(tok.clone()));
            }
        }
    }
}

fn assert_eq_labeled(got: &Value, oracle: &Value, label: &str) {
    assert_eq!(
        got, oracle,
        "{label} diverged\n  rust:   {got}\n  oracle: {oracle}"
    );
}

#[test]
fn tool_execution_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_TOOLEXEC") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_TOOLEXEC to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_TOOLEXEC") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_TOOLEXEC to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/tool-execution-tier2.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle dump");

    let work = std::env::temp_dir().join(format!("qt-toolexec-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    let mut returns: Vec<Value> = Vec::new();
    for call in &spec.calls {
        let tool_messages: Vec<ToolMessage> = call
            .tool_messages
            .iter()
            .map(|m| m.to_tool_message())
            .collect();
        let images: Vec<GeneratedImage> = call
            .generated_image_paths
            .iter()
            .map(|i| i.to_generated_image())
            .collect();
        let whisper = call.whisper_context.as_ref().map(|w| ToolWhisperContext {
            user_participant_id: w.user_participant_id.clone(),
            allow_cross_character_vault_reads: w.allow_cross_character_vault_reads,
        });
        let result = save_tool_messages(
            &writer,
            &call.chat_id,
            &spec.user_id,
            &tool_messages,
            &images,
            call.character_id.as_deref(),
            call.participant_id.as_deref(),
            whisper.as_ref(),
        )
        .expect("save_tool_messages");
        returns.push(serde_json::json!({
            "firstToolMessageId": result.first_tool_message_id,
            "generatedImageIds": result.generated_image_ids,
        }));
    }

    let mut got_messages = writer
        .dump_table_json("chat_messages", "id")
        .expect("dump chat_messages");
    let mut got_chats = writer.dump_table_json("chats", "id").expect("dump chats");
    let mut got_files = writer.dump_table_json("files", "id").expect("dump files");
    let _ = std::fs::remove_file(&work);

    // Build the shared id-map from each side's OWN messages (same content → same
    // tokens), then normalize both.
    let id_map_got = build_id_map(&got_messages);
    let mut oracle_messages = oracle["messages"].clone();
    let id_map_oracle = build_id_map(&oracle_messages);

    normalize_messages(&mut got_messages, &id_map_got);
    normalize_chats(&mut got_chats, &spec.sentinel);
    normalize_files(&mut got_files, &id_map_got, &spec.sentinel);
    let mut got_returns = Value::Array(returns);
    normalize_returns(&mut got_returns, &id_map_got);

    normalize_messages(&mut oracle_messages, &id_map_oracle);
    let mut oracle_chats = oracle["chats"].clone();
    normalize_chats(&mut oracle_chats, &spec.sentinel);
    let mut oracle_files = oracle["files"].clone();
    normalize_files(&mut oracle_files, &id_map_oracle, &spec.sentinel);
    // Oracle returns carry a `name` label; strip it to the compared shape.
    let mut oracle_returns = Value::Array(
        oracle["returns"]
            .as_array()
            .expect("oracle returns")
            .iter()
            .map(|r| {
                serde_json::json!({
                    "firstToolMessageId": r["firstToolMessageId"],
                    "generatedImageIds": r["generatedImageIds"],
                })
            })
            .collect(),
    );
    normalize_returns(&mut oracle_returns, &id_map_oracle);

    assert_eq_labeled(&got_messages, &oracle_messages, "chat_messages");
    assert_eq_labeled(&got_chats, &oracle_chats, "chats");
    assert_eq_labeled(&got_files, &oracle_files, "files");
    assert_eq_labeled(&got_returns, &oracle_returns, "returns");

    let nm = got_messages["rows"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    assert!(nm >= 7, "expected >= 7 tool message rows, got {nm}");
    eprintln!("OK: tool-execution tier-2 (c.1) matched oracle ({nm} tool rows).");
}
