//! Tier-2 differential test: the `llm_logs` repo — the **second sibling-DB
//! partition** of Phase 2 (after the mount-index family) and the **widest repo to
//! date** (18 columns, FIVE nested JSON-object columns).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture, which is the
//! llm-logs sibling DB (`quilltap-llm-logs.db`), not the main DB. The Rust
//! `Writer` is partition-agnostic — `open_writable` opens that file by path
//! exactly as it opens a main DB — so this test is shaped identically to the
//! main-DB tier-2 tests; only the fixture differs. Both run the SAME
//! create + update + delete op sequence from the committed spec, dump the
//! `llm_logs` table canonically, and assert the post-op state is identical. Ids
//! and timestamps are pinned on both sides → zero normalization.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-ll-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-llm-logs-fixture.ts
//!   QT_FIXTURE_LLM_LOGS=/tmp/qt-ll-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/llm-logs-tier2.ts \
//!     > /tmp/oracle-ll.ndjson
//! Run:
//!   QT_ORACLE_LLM_LOGS=/tmp/oracle-ll.ndjson \
//!   QT_FIXTURE_LLM_LOGS=/tmp/qt-ll-fixture.db \
//!     cargo test -p quilltap-harness --test llm_logs_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::llm_logs::{
    CreateOptions, LlCreate, LlUpdate, LlmLogCacheUsage, LlmLogMessageSummary, LlmLogRequestHashes,
    LlmLogRequestSummary, LlmLogResponseSummary, LlmLogTokenUsage,
};
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

/// The committed fixture spec — the single source driving both ports.
#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "create")]
    Create {
        data: CreateData,
        options: CreateOpts,
    },
    #[serde(rename = "update")]
    Update { id: String, data: UpdateData },
    #[serde(rename = "delete")]
    Delete { id: String },
}

// ── JSON shapes mirroring the nested structs (camelCase from the spec) ──────────

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct MessageJson {
    role: String,
    content: String,
    content_preview: Option<String>,
    content_length: i64,
    has_attachments: bool,
}

impl From<MessageJson> for LlmLogMessageSummary {
    fn from(m: MessageJson) -> Self {
        LlmLogMessageSummary {
            role: m.role,
            content: m.content,
            content_preview: m.content_preview,
            content_length: m.content_length,
            has_attachments: m.has_attachments,
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RequestJson {
    message_count: i64,
    messages: Vec<MessageJson>,
    temperature: Option<f64>,
    max_tokens: Option<i64>,
    tool_count: i64,
}

impl From<RequestJson> for LlmLogRequestSummary {
    fn from(r: RequestJson) -> Self {
        LlmLogRequestSummary {
            message_count: r.message_count,
            messages: r.messages.into_iter().map(Into::into).collect(),
            temperature: r.temperature,
            max_tokens: r.max_tokens,
            tool_count: r.tool_count,
            full_messages: None,
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ResponseJson {
    content: String,
    content_preview: Option<String>,
    content_length: i64,
    full_content: Option<String>,
    error: Option<String>,
    finish_reason: Option<String>,
}

impl From<ResponseJson> for LlmLogResponseSummary {
    fn from(r: ResponseJson) -> Self {
        LlmLogResponseSummary {
            content: r.content,
            content_preview: r.content_preview,
            content_length: r.content_length,
            full_content: r.full_content,
            error: r.error,
            finish_reason: r.finish_reason,
            tool_calls: None,
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct UsageJson {
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
}

impl From<UsageJson> for LlmLogTokenUsage {
    fn from(u: UsageJson) -> Self {
        LlmLogTokenUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CacheUsageJson {
    cache_creation_input_tokens: Option<i64>,
    cache_read_input_tokens: Option<i64>,
}

impl From<CacheUsageJson> for LlmLogCacheUsage {
    fn from(c: CacheUsageJson) -> Self {
        LlmLogCacheUsage {
            cache_creation_input_tokens: c.cache_creation_input_tokens,
            cache_read_input_tokens: c.cache_read_input_tokens,
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RequestHashesJson {
    system_block1_hash: Option<String>,
    system_block2_hash: Option<String>,
    system_block3_hash: Option<String>,
    tools_array_hash: Option<String>,
    history_tail_hash: Option<String>,
}

impl From<RequestHashesJson> for LlmLogRequestHashes {
    fn from(h: RequestHashesJson) -> Self {
        LlmLogRequestHashes {
            system_block1_hash: h.system_block1_hash,
            system_block2_hash: h.system_block2_hash,
            system_block3_hash: h.system_block3_hash,
            tools_array_hash: h.tools_array_hash,
            history_tail_hash: h.history_tail_hash,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateData {
    user_id: String,
    #[serde(rename = "type")]
    log_type: String,
    message_id: Option<String>,
    chat_id: Option<String>,
    character_id: Option<String>,
    autonomous_run_id: Option<String>,
    provider: String,
    model_name: String,
    request: RequestJson,
    response: ResponseJson,
    usage: Option<UsageJson>,
    cache_usage: Option<CacheUsageJson>,
    raw_provider_usage: Option<Value>,
    request_hashes: Option<RequestHashesJson>,
    duration_ms: Option<f64>,
}

#[derive(Deserialize)]
struct CreateOpts {
    id: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateData {
    #[serde(rename = "type")]
    log_type: Option<String>,
    message_id: Option<String>,
    chat_id: Option<String>,
    character_id: Option<String>,
    autonomous_run_id: Option<String>,
    provider: Option<String>,
    model_name: Option<String>,
    request: Option<RequestJson>,
    response: Option<ResponseJson>,
    usage: Option<UsageJson>,
    cache_usage: Option<CacheUsageJson>,
    raw_provider_usage: Option<Value>,
    request_hashes: Option<RequestHashesJson>,
    duration_ms: Option<f64>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/llm-logs-tier2.json")
}

#[test]
fn llm_logs_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_LLM_LOGS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_LLM_LOGS to the oracle NDJSON (see test header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_LLM_LOGS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_LLM_LOGS to the seed fixture .db (see test header).");
            return;
        }
    };

    // Parse the committed spec (pepper + op sequence) — same file the oracle used.
    let spec_text = std::fs::read_to_string(spec_path())
        .unwrap_or_else(|e| panic!("cannot read fixture spec: {e}"));
    let spec: Spec = serde_json::from_str(&spec_text).expect("parse fixture spec");

    // Parse the oracle's expected post-op dump (one NDJSON object).
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));
    let oracle: Value = serde_json::from_str(oracle_text.trim()).expect("parse oracle dump");

    // Work on a fresh copy of the seed fixture so the shared file stays pristine.
    let work = std::env::temp_dir().join(format!("qt-ll-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port. The Writer opens the
    // llm-logs fixture by path — no special "llm-logs writer" needed.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.llm_logs();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => {
                    let create = LlCreate {
                        user_id: data.user_id.clone(),
                        log_type: data.log_type.clone(),
                        message_id: data.message_id.clone(),
                        chat_id: data.chat_id.clone(),
                        character_id: data.character_id.clone(),
                        autonomous_run_id: data.autonomous_run_id.clone(),
                        provider: data.provider.clone(),
                        model_name: data.model_name.clone(),
                        request: data.request.clone().into(),
                        response: data.response.clone().into(),
                        usage: data.usage.clone().map(Into::into),
                        cache_usage: data.cache_usage.clone().map(Into::into),
                        raw_provider_usage: data.raw_provider_usage.clone(),
                        request_hashes: data.request_hashes.clone().map(Into::into),
                        duration_ms: data.duration_ms,
                    };
                    repo.create(
                        &create,
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .expect("llm_logs.create");
                }
                Op::Update { id, data } => {
                    let patch = LlUpdate {
                        log_type: data.log_type.clone(),
                        message_id: data.message_id.clone(),
                        chat_id: data.chat_id.clone(),
                        character_id: data.character_id.clone(),
                        autonomous_run_id: data.autonomous_run_id.clone(),
                        provider: data.provider.clone(),
                        model_name: data.model_name.clone(),
                        request: data.request.clone().map(Into::into),
                        response: data.response.clone().map(Into::into),
                        usage: data.usage.clone().map(Into::into),
                        cache_usage: data.cache_usage.clone().map(Into::into),
                        raw_provider_usage: data.raw_provider_usage.clone(),
                        request_hashes: data.request_hashes.clone().map(Into::into),
                        duration_ms: data.duration_ms,
                        updated_at: data.updated_at.clone(),
                    };
                    let found = repo.update(id, &patch).expect("llm_logs.update");
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let removed = repo.delete(id).expect("llm_logs.delete");
                    assert!(removed, "delete target {id} not found in fixture");
                }
            }
        }
    }

    let got = writer
        .dump_table_json("llm_logs", "id")
        .expect("dump llm_logs");

    let _ = std::fs::remove_file(&work);

    // Structural diff: table + columns + rows must match (ignore the oracle's
    // "case" label).
    assert_eq!(got["table"], oracle["table"], "table name");
    assert_eq!(got["columns"], oracle["columns"], "column set / order");
    assert_eq!(
        got["rows"], oracle["rows"],
        "row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["rows"]
    );

    let n = got["rows"].as_array().map(|a| a.len()).unwrap_or(0);
    assert!(n > 0, "dump looks empty");
    eprintln!("OK: llm_logs tier-2 matched oracle ({n} rows).");
}
