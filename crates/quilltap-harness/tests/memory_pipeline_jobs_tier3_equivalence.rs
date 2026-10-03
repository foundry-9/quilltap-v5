//! Tier-3 differential test: the **memory-pipeline job handlers** (P4.6bj
//! units 2–3 — `quilltap_core::services::memory_extraction_job` /
//! `context_summary_job`, v4 `handleMemoryExtraction` +
//! `handleContextSummary`).
//!
//! Both sides copy the same two-DB seed fixture and run the same ten-case
//! sequence (six MEMORY_EXTRACTION + four CONTEXT_SUMMARY). The oracle drives
//! v4's REAL handlers with only the model/infra seams mocked (see the case
//! header); this test replays the RECORDED canned completions + embeddings
//! through the ported handlers — the extraction path through
//! `build_turn_transcript` + `process_turn_for_memory`, the summary path
//! through `generate_context_summary_with_seams(RealContextSummarySeams)` —
//! then diffs six tables (`memories`, `vector_indices`, `vector_entries`,
//! `chat_messages`, `chats`, `background_jobs`) in the shared-cross-table
//! minted-values remap form, and asserts each case's thrown-error string
//! matches the oracle's.
//!
//! P4.144: an `llm_logs` comparand. The oracle runs v4's REAL `logLLMCall`
//! (wrapped so every promise is drained before the dump) into a fresh
//! llm-logs DB and emits `{kind:"llmlogs"}`; this side opens a fresh
//! llm-logs partition and builds one `CheapLlmTaskExecutor::with_logging`
//! PER CASE with `chat_id: Some(case chat)`, `message_id: None` — exactly the
//! host spine's per-job construction (`spine.rs`'s MEMORY_EXTRACTION and
//! CONTEXT_SUMMARY handlers); a bare `new()` writes no rows at all. The rows
//! are diffed after the tables, through `split_ruled_failed_call_rows`
//! expecting ZERO failure rows on BOTH sides (a canned miss would add a v5
//! error row v4 never writes — the P4.13 ruled divergence — so this is a
//! second consumption guard beside the key set). Measured and neutral: the
//! per-case executor makes `profiles_without_custom_temp` per-case where v4's
//! is per-run (no MPJ call is temperature-rejected), and `with_logging` arms
//! the cheap fallback chain (the fixture's profiles carry no fallbacks, and no
//! call fails).
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout; the /tmp
//! mirror dodges jest's `/.claude/` testPathIgnorePatterns):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<this worktree>
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-mpj-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-mpj-mount.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-memory-pipeline-jobs-fixture.ts
//!   mkdir -p /tmp/qt-mpj-oracle/cases /tmp/qt-mpj-oracle/fixtures
//!   cp $V5/harness/oracle/cases/memory-pipeline-jobs-tier3.test.ts /tmp/qt-mpj-oracle/cases/
//!   cp $V5/harness/oracle/fixtures/memory-pipeline-jobs-tier3.json /tmp/qt-mpj-oracle/fixtures/
//!   TZ=UTC QT_FIXTURE_MPJ_MAIN=/tmp/qt-mpj-main.db QT_FIXTURE_MPJ_MOUNT=/tmp/qt-mpj-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-memory-pipeline-jobs.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 --roots "$PWD" --roots /tmp/qt-mpj-oracle/cases -- memory-pipeline-jobs-tier3
//! Run (TZ pinned to match the oracle's date math):
//!   TZ=UTC QT_ORACLE_MPJ=/tmp/oracle-memory-pipeline-jobs.ndjson \
//!   QT_FIXTURE_MPJ_MAIN=/tmp/qt-mpj-main.db QT_FIXTURE_MPJ_MOUNT=/tmp/qt-mpj-mount.db \
//!     cargo test -p quilltap-harness --test memory_pipeline_jobs_tier3_equivalence

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    canned_completion_key, canned_completion_key_with_attachments, CannedCompletionProvider,
    CompletionError, CompletionMessage, CompletionParams, CompletionProvider, CompletionResponse,
    CompletionRole, CompletionUsage,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::services::cheap_llm_exec::{CheapLlmLogConfig, CheapLlmTaskExecutor};
use quilltap_core::services::context_summary_job::{handle_context_summary, ContextSummaryPayload};
use quilltap_core::services::cost_estimation::MessageCostEstimator;
use quilltap_core::services::llm_logging::LogContext;
use quilltap_core::services::memory_extraction_job::{
    handle_memory_extraction, MemoryExtractionPayload,
};
use quilltap_core::services::memory_processor::MemoryExtractionLimits;
use serde::Deserialize;
use serde_json::Value;

mod common;

// ---------------------------------------------------------------------------
// Spec structures (harness/oracle/fixtures/memory-pipeline-jobs-tier3.json).
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LimitsW {
    enabled: bool,
    max_per_hour: f64,
    soft_start_fraction: f64,
    soft_floor: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MeCaseW {
    name: String,
    chat_id: String,
    turn_opener_message_id: Option<String>,
    extraction_anchor_message_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CsCaseW {
    name: String,
    chat_id: String,
    force_regenerate: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    connection_profile_id: String,
    memory_extraction_limits: LimitsW,
    me_cases: Vec<MeCaseW>,
    cs_cases: Vec<CsCaseW>,
}

// ---------------------------------------------------------------------------
// Oracle rows.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CannedMessageW {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct CannedUsageW {
    #[serde(rename = "promptTokens")]
    prompt_tokens: i64,
    #[serde(rename = "completionTokens")]
    completion_tokens: i64,
    #[serde(rename = "totalTokens")]
    total_tokens: i64,
}

#[derive(Deserialize)]
struct CannedRowW {
    provider: String,
    model: String,
    temperature: Option<f64>,
    messages: Vec<CannedMessageW>,
    response: String,
    usage: CannedUsageW,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/memory-pipeline-jobs-tier3.json")
}

/// Records the canned key of every completion call v5 makes, then delegates.
///
/// P4.D242's A10 finding / P4.138: the fold-episode pass folds a canned MISS
/// (`success: false`) and the canned `[]` reply into one silent `return result`
/// with zero tracing, so a prompt/selection/temperature divergence on that call
/// wrote the same zero rows as agreement. Asserting that the SET of keys v5 hit
/// equals the SET of keys the oracle recorded makes the divergence visible on
/// ANY canned call (memory note `a-canned-miss-in-a-no-op-run-is-invisible`).
/// Only `send_message` is implemented — the other trait methods default onto it.
struct KeyRecording<C> {
    inner: C,
    hit: Mutex<BTreeSet<String>>,
}

impl<C: CompletionProvider + Sync> CompletionProvider for KeyRecording<C> {
    fn send_message(
        &self,
        provider: &str,
        base_url: Option<&str>,
        params: &CompletionParams,
    ) -> impl std::future::Future<Output = Result<CompletionResponse, CompletionError>> + Send {
        self.hit
            .lock()
            .unwrap()
            .insert(canned_completion_key_with_attachments(
                provider,
                &params.model,
                params.temperature,
                &params.messages,
                &params.attachments,
            ));
        self.inner.send_message(provider, base_url, params)
    }
}

// The oracle's `estimateMessageCost` mock returns `{ cost: null }`.
struct NullCost;

impl MessageCostEstimator for NullCost {
    async fn estimate(
        &self,
        _provider: &str,
        _model: &str,
        _prompt_tokens: i64,
        _completion_tokens: i64,
        _user_id: &str,
    ) -> Option<f64> {
        None
    }
}

// ---------------------------------------------------------------------------
// Table normalization — the shared-cross-table id-map remap form.
// ---------------------------------------------------------------------------

struct TableSpec {
    table: &'static str,
    order_by: &'static str,
    id_columns: &'static [&'static str],
    id_array_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
    /// Free-text columns that may EMBED minted ids (the gate's "absorbed
    /// into <uuid>" debug line) — every UUID already in the shared map is
    /// replaced by its token; unknown UUIDs (pinned ids) stay literal.
    text_id_columns: &'static [&'static str],
}

const TABLES: &[TableSpec] = &[
    TableSpec {
        table: "memories",
        order_by: "content",
        id_columns: &["id", "sourceMessageId"],
        id_array_columns: &["relatedMemoryIds"],
        ts_columns: &[
            "createdAt",
            "updatedAt",
            "lastReinforcedAt",
            "lastAccessedAt",
        ],
        text_id_columns: &[],
    },
    TableSpec {
        table: "vector_entries",
        order_by: "embedding",
        id_columns: &["id"],
        id_array_columns: &[],
        ts_columns: &["createdAt"],
        text_id_columns: &[],
    },
    TableSpec {
        table: "vector_indices",
        order_by: "id",
        id_columns: &[],
        id_array_columns: &[],
        ts_columns: &["createdAt", "updatedAt"],
        text_id_columns: &[],
    },
    TableSpec {
        table: "chat_messages",
        order_by: "content",
        // Seeded message ids are pinned (tokenized consistently both sides);
        // the librarian whisper + system-event rows carry minted ids.
        id_columns: &["id"],
        id_array_columns: &[],
        ts_columns: &["createdAt"],
        text_id_columns: &["debugMemoryLogs"],
    },
    TableSpec {
        table: "chats",
        order_by: "id",
        id_columns: &[],
        id_array_columns: &[],
        ts_columns: &["updatedAt", "lastMessageAt"],
        text_id_columns: &[],
    },
    TableSpec {
        table: "background_jobs",
        order_by: "type",
        // The chained CHAT_DANGER_CLASSIFICATION rows mint fresh job ids; the
        // payloads carry only pinned chat/profile ids.
        id_columns: &["id"],
        id_array_columns: &[],
        ts_columns: &["createdAt", "updatedAt", "scheduledAt"],
        text_id_columns: &[],
    },
];

fn token_for(id_map: &mut HashMap<String, String>, raw: &str) -> String {
    let next = format!("ID_{}", id_map.len());
    id_map.entry(raw.to_string()).or_insert(next).clone()
}

/// Replace every UUID-shaped substring that is already in the shared id map
/// with its token (embedded minted ids in free-text cells). Unknown UUIDs stay
/// literal — pinned ids are identical on both sides.
fn replace_mapped_uuids(raw: &str, id_map: &HashMap<String, String>) -> String {
    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    let is_uuid_char = |c: u8| c.is_ascii_hexdigit() || c == b'-';
    while i < bytes.len() {
        if bytes[i].is_ascii_hexdigit() {
            let mut j = i;
            while j < bytes.len() && is_uuid_char(bytes[j]) {
                j += 1;
            }
            let cand = &raw[i..j];
            if cand.len() == 36 {
                if let Some(token) = id_map.get(cand) {
                    out.push_str(token);
                    i = j;
                    continue;
                }
            }
            out.push_str(cand);
            i = j;
        } else {
            let ch = raw[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn normalize_table(dump: &mut Value, spec: &TableSpec, id_map: &mut HashMap<String, String>) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{}: dump has no rows array", spec.table));

    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{}: row is not an object", spec.table));

        for col in spec.id_columns {
            if let Some(Value::String(raw)) = obj.get(*col) {
                let token = token_for(id_map, raw);
                obj.insert((*col).to_string(), Value::String(token));
            }
        }

        for col in spec.id_array_columns {
            if let Some(Value::String(raw)) = obj.get(*col) {
                if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(raw) {
                    let mapped: Vec<Value> = items
                        .iter()
                        .map(|v| match v.as_str() {
                            Some(s) => Value::String(token_for(id_map, s)),
                            None => v.clone(),
                        })
                        .collect();
                    let re = serde_json::to_string(&Value::Array(mapped)).unwrap();
                    obj.insert((*col).to_string(), Value::String(re));
                }
            }
        }

        for col in spec.text_id_columns {
            if let Some(Value::String(raw)) = obj.get(*col) {
                let replaced = replace_mapped_uuids(raw, id_map);
                obj.insert((*col).to_string(), Value::String(replaced));
            }
        }

        for col in spec.ts_columns {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
    }
}

fn normalize_all(dumps: &mut [Value]) {
    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        normalize_table(&mut dumps[i], spec, &mut id_map);
    }
}

// ---------------------------------------------------------------------------
// P4.144: the `llm_logs` comparand's compressed cells.
// ---------------------------------------------------------------------------

/// ESCALATED, not ruled (P4.144): the number of `llm_logs` rows whose STORED
/// compressed bytes differ from v4's while decoding to the identical text.
/// `db::text_compression`'s module doc records encoder byte parity with
/// Node's brotli (35 rows to 262,293 bytes); this family's corpus refutes it on
/// all ten MEMORY_EXTRACTION `request` payloads (8,919–11,702 raw bytes; two
/// store 1–2 bytes LONGER on v5), while the six ≤ 2,516-byte rows match.
/// Measured out of tree against the same `brotli` 8.0.4 crate: the
/// `CompressorWriter` (any buffer size) and the one-shot `BrotliCompress` both
/// differ from Node 24.13.1's bundled C brotli at `QUALITY 5` + `SIZE_HINT`,
/// which the oracle's bytes equal exactly — the divergence is inside the
/// encoder, not the call shape. The codec is not this lane's file, so the
/// family compares the cells DECODED and pins this count in both directions:
/// a codec fix trips it (retire the pin), and so does any new divergence.
const STORED_BYTE_DIVERGENCES: usize = 10;

/// A dumped cell holding a compressed text BLOB arrives as hex (the shared
/// dump's convention, both sides). Decode it to the text it stores.
fn decoded_cell(v: &Value) -> Option<String> {
    let s = v.as_str()?;
    let bytes = hex::decode(s).ok()?;
    quilltap_core::db::text_compression::is_compressed_text_blob(&bytes)
        .then(|| quilltap_core::db::text_compression::decode_blob(&bytes))
}

/// Each row paired with its decoded twin (every compressed cell replaced by its
/// text), sorted by the decoded twin's canonical JSON.
fn decode_llm_log_rows(rows: Vec<Value>) -> Vec<(Value, Value)> {
    let mut pairs: Vec<(Value, Value)> = rows
        .into_iter()
        .map(|row| {
            let mut decoded = row.clone();
            if let Some(obj) = decoded.as_object_mut() {
                for v in obj.values_mut() {
                    if let Some(text) = decoded_cell(v) {
                        *v = Value::String(text);
                    }
                }
            }
            (decoded, row)
        })
        .collect();
    pairs.sort_by_key(|(decoded, _)| serde_json::to_string(decoded).unwrap());
    pairs
}

#[tokio::test]
async fn memory_pipeline_jobs_tier3_matches_oracle() {
    // P4.144: the oracle mocks `getApiKeyForCheapLLMSelection` to a constant;
    // a `with_logging` executor resolves the selection's own key through the
    // DB (P4.133) where a bare `new()` never asked, so the twin is needed now —
    // without it every extraction call refused with `No API key available for
    // cheap LLM provider` (measured: 15 of v4's 16 keys never called).
    let _canned_key = quilltap_core::test_support::CannedCheapLlmKey::install("test-key");
    let oracle_path = match std::env::var("QT_ORACLE_MPJ") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_MPJ to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture_main = match std::env::var("QT_FIXTURE_MPJ_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_MPJ_MAIN to the seed main .db (see header).");
            return;
        }
    };
    let fixture_mount = match std::env::var("QT_FIXTURE_MPJ_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_MPJ_MOUNT to the seed mount .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    let mut oracle_canned: Vec<CannedRowW> = Vec::new();
    let mut oracle_embeddings: Vec<(String, Vec<f32>)> = Vec::new();
    let mut oracle_tables: HashMap<String, Value> = HashMap::new();
    let mut oracle_errors: HashMap<String, Option<String>> = HashMap::new();
    let mut oracle_llmlogs: Option<Value> = None;
    for line in oracle_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("ran") => {
                oracle_errors.insert(
                    v["call"].as_str().unwrap().to_string(),
                    v.get("error").and_then(Value::as_str).map(str::to_string),
                );
            }
            Some("canned") => {
                oracle_canned.push(serde_json::from_value(v).expect("parse canned row"))
            }
            Some("cannedEmbedding") => {
                let text = v["text"].as_str().expect("embedding text").to_string();
                let vec: Vec<f32> = v["vector"]
                    .as_array()
                    .expect("embedding vector")
                    .iter()
                    .map(|x| x.as_f64().unwrap() as f32)
                    .collect();
                oracle_embeddings.push((text, vec));
            }
            Some("table") => {
                oracle_tables.insert(v["table"].as_str().unwrap().to_string(), v);
            }
            Some("llmlogs") => oracle_llmlogs = Some(v),
            other => panic!("unknown oracle row kind {other:?}"),
        }
    }
    assert_eq!(
        oracle_errors.len(),
        spec.me_cases.len() + spec.cs_cases.len(),
        "oracle ran a different number of cases — regenerate the oracle NDJSON"
    );

    // Fresh copies so the shared seed fixtures stay pristine.
    let pid = std::process::id();
    let work_main = std::env::temp_dir().join(format!("qt-mpj-main-rust-{pid}.db"));
    let work_mount = std::env::temp_dir().join(format!("qt-mpj-mount-rust-{pid}.db"));
    let work_llm_logs = std::env::temp_dir().join(format!("qt-mpj-llm-logs-rust-{pid}.db"));
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    let _ = std::fs::remove_file(&work_llm_logs);
    common::materialize_llm_logs(&work_llm_logs, &spec.test_pepper_base64);
    std::fs::copy(&fixture_main, &work_main).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&fixture_mount, &work_mount).unwrap_or_else(|e| panic!("copy mount: {e}"));

    // Replay EXACTLY the oracle-recorded entries — an input the oracle never
    // sent surfaces as a canned-miss, not an answer.
    let mut completion = CannedCompletionProvider::new();
    for row in &oracle_canned {
        let messages: Vec<CompletionMessage> = row
            .messages
            .iter()
            .map(|m| CompletionMessage {
                role: CompletionRole::from_v4_wire(m.role.as_str())
                    .unwrap_or_else(|| panic!("unexpected role {}", m.role.as_str())),
                content: m.content.clone(),
            })
            .collect();
        completion = completion.with_response(
            &row.provider,
            &row.model,
            row.temperature,
            &messages,
            row.response.clone(),
            Some(CompletionUsage {
                prompt_tokens: row.usage.prompt_tokens,
                completion_tokens: row.usage.completion_tokens,
                total_tokens: row.usage.total_tokens,
            }),
        );
    }

    // The keys the oracle recorded (v4 dedups by key, first write wins).
    let want_keys: BTreeSet<String> = oracle_canned
        .iter()
        .map(|row| {
            let messages: Vec<CompletionMessage> = row
                .messages
                .iter()
                .map(|m| CompletionMessage {
                    role: CompletionRole::from_v4_wire(m.role.as_str())
                        .unwrap_or_else(|| panic!("unexpected role {}", m.role.as_str())),
                    content: m.content.clone(),
                })
                .collect();
            canned_completion_key(&row.provider, &row.model, row.temperature, &messages)
        })
        .collect();
    let completion = KeyRecording {
        inner: completion,
        hit: Mutex::new(BTreeSet::new()),
    };

    let mut embedding = CannedEmbeddingProvider::new();
    for (text, vec) in &oracle_embeddings {
        embedding = embedding.with_vector(text.clone(), vec.clone());
    }

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: Some(work_llm_logs.clone()),
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copies: {e}"));

    // One logging executor per case, as the spine builds one per job.
    let executor_for = |chat_id: &str| {
        CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
            db: db.clone(),
            user_id: spec.user_id.clone(),
            chat_id: Some(chat_id.to_string()),
            message_id: None,
            ctx: LogContext::none(),
        })
    };
    let cost = NullCost;
    let limits = MemoryExtractionLimits {
        enabled: spec.memory_extraction_limits.enabled,
        max_per_hour: spec.memory_extraction_limits.max_per_hour,
        soft_start_fraction: spec.memory_extraction_limits.soft_start_fraction,
        soft_floor: spec.memory_extraction_limits.soft_floor,
    };

    let assert_case_error = |name: &str, got: Option<String>| {
        let want = oracle_errors
            .get(name)
            .unwrap_or_else(|| panic!("{name}: missing from oracle — regenerate"));
        assert_eq!(&got, want, "{name}: thrown-error divergence");
    };

    for case in &spec.me_cases {
        let payload = MemoryExtractionPayload {
            chat_id: case.chat_id.clone(),
            turn_opener_message_id: case.turn_opener_message_id.clone(),
            extraction_anchor_message_id: case.extraction_anchor_message_id.clone(),
            connection_profile_id: spec.connection_profile_id.clone(),
        };
        let executor = executor_for(&case.chat_id);
        let got = handle_memory_extraction(
            &db,
            &completion,
            &embedding,
            &executor,
            &cost,
            &spec.user_id,
            &payload,
            Some(limits.clone()),
        )
        .await
        .err();
        assert_case_error(&case.name, got);
    }

    for case in &spec.cs_cases {
        let payload = ContextSummaryPayload {
            chat_id: case.chat_id.clone(),
            connection_profile_id: spec.connection_profile_id.clone(),
            force_regenerate: case.force_regenerate.unwrap_or(false),
        };
        let executor = executor_for(&case.chat_id);
        let got = handle_context_summary(
            &db,
            &completion,
            &embedding,
            &executor,
            &spec.user_id,
            &payload,
        )
        .await
        .err();
        assert_case_error(&case.name, got);
    }

    // Consumption: v5 must have called exactly the completions v4 did. A canned
    // miss is swallowed by the executor, so this is the only place a divergent
    // call (the fold-episode pass above all) shows.
    let got_keys = completion.hit.lock().unwrap().clone();
    // Non-vacuity (found at unification): two EMPTY sets are equal, so a regen
    // that recorded no canned call, or no fold-episode call, would pass
    // silently. The order's target is the episode pass, so at least one
    // recorded key must carry its system prompt.
    assert!(
        want_keys
            .iter()
            .any(|k| k.contains("You are consolidating a batch")),
        "the oracle recorded no fold-episode call ({} keys) — the consumption \
         assert below would measure nothing for the pass it exists to see",
        want_keys.len()
    );
    let never_built: Vec<&String> = want_keys.difference(&got_keys).collect();
    let never_sent_by_v4: Vec<&String> = got_keys.difference(&want_keys).collect();
    assert!(
        never_built.is_empty() && never_sent_by_v4.is_empty(),
        "canned-key consumption diverges from the oracle.\n\
         recorded by v4 but never called by v5 ({}):\n{:#?}\n\
         called by v5 but never recorded by v4 ({}):\n{:#?}",
        never_built.len(),
        never_built,
        never_sent_by_v4.len(),
        never_sent_by_v4,
    );

    // Dump + diff the six tables in the shared-id-map remap form.
    let mut got: Vec<Value> = TABLES
        .iter()
        .map(|t| {
            db.read_main(|conn| dump_table_json_conn(conn, t.table, t.order_by))
                .unwrap_or_else(|e| panic!("dump {}: {e:?}", t.table))
        })
        .collect();
    let got_llm_logs = common::dump_llm_logs(&db);
    drop(db);
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    let _ = std::fs::remove_file(&work_llm_logs);

    let mut want: Vec<Value> = TABLES
        .iter()
        .map(|t| {
            let mut v = oracle_tables
                .remove(t.table)
                .unwrap_or_else(|| panic!("oracle missing table {}", t.table));
            v.as_object_mut().unwrap().remove("kind");
            v
        })
        .collect();

    normalize_all(&mut got);
    normalize_all(&mut want);

    for (i, t) in TABLES.iter().enumerate() {
        assert_eq!(
            got[i]["columns"], want[i]["columns"],
            "{} column set / order",
            t.table
        );
        if got[i]["rows"] != want[i]["rows"] {
            // Row-level report: dump both sides to temp files, then panic with
            // the first differing row (the full arrays overflow the terminal).
            let gp = std::env::temp_dir().join(format!("qt-mpj-got-{}.json", t.table));
            let wp = std::env::temp_dir().join(format!("qt-mpj-want-{}.json", t.table));
            let _ = std::fs::write(&gp, serde_json::to_string_pretty(&got[i]["rows"]).unwrap());
            let _ = std::fs::write(&wp, serde_json::to_string_pretty(&want[i]["rows"]).unwrap());
            let ga = got[i]["rows"].as_array().unwrap();
            let wa = want[i]["rows"].as_array().unwrap();
            for (j, (g, w)) in ga.iter().zip(wa.iter()).enumerate() {
                if g != w {
                    panic!(
                        "{} rows diverge at index {j} (full dumps: {} / {})\n got: {g:#}\nwant: {w:#}",
                        t.table,
                        gp.display(),
                        wp.display()
                    );
                }
            }
            panic!(
                "{} row-count diverges: got {} vs want {} (full dumps: {} / {})",
                t.table,
                ga.len(),
                wa.len(),
                gp.display(),
                wp.display()
            );
        }
    }
    // P4.144: the `llm_logs` comparand (see the header). Zero failed-call rows
    // on EITHER side: v5's would be a canned miss (the P4.13 ruled-divergence
    // signature), v4's would mean v4 started logging failures.
    let want_llm_logs = common::oracle_llm_logs(
        oracle_llmlogs
            .as_ref()
            .expect("oracle ndjson has no llmlogs row — regenerate (P4.144)"),
    );
    let (got_llm_logs, got_failed) = common::split_ruled_failed_call_rows(got_llm_logs);
    let (want_llm_logs, want_failed) = common::split_ruled_failed_call_rows(want_llm_logs);
    assert!(
        got_failed.is_empty() && want_failed.is_empty(),
        "failed cheap-call rows (v5 {}, v4 {}) — a canned miss or a converged \
         failure log; none is expected in this corpus: {got_failed:#?} {want_failed:#?}",
        got_failed.len(),
        want_failed.len(),
    );
    assert!(
        !want_llm_logs.is_empty(),
        "the oracle logged no llm_logs rows — the comparand would measure nothing"
    );
    let got_pairs = decode_llm_log_rows(got_llm_logs);
    let want_pairs = decode_llm_log_rows(want_llm_logs);
    let got_llm_logs: Vec<Value> = got_pairs.iter().map(|(d, _)| d.clone()).collect();
    let want_llm_logs: Vec<Value> = want_pairs.iter().map(|(d, _)| d.clone()).collect();
    if got_llm_logs != want_llm_logs {
        let gp = std::env::temp_dir().join("qt-mpj-got-llm_logs.json");
        let wp = std::env::temp_dir().join("qt-mpj-want-llm_logs.json");
        let _ = std::fs::write(&gp, serde_json::to_string_pretty(&got_llm_logs).unwrap());
        let _ = std::fs::write(&wp, serde_json::to_string_pretty(&want_llm_logs).unwrap());
        for (j, (g, w)) in got_llm_logs.iter().zip(want_llm_logs.iter()).enumerate() {
            if g != w {
                panic!(
                    "llm_logs rows diverge at index {j} (full dumps: {} / {})\n got: {g:#}\nwant: {w:#}",
                    gp.display(),
                    wp.display()
                );
            }
        }
        panic!(
            "llm_logs row-count diverges: got {} vs want {} (full dumps: {} / {})",
            got_llm_logs.len(),
            want_llm_logs.len(),
            gp.display(),
            wp.display()
        );
    }
    let stored_byte_divergences = got_pairs
        .iter()
        .zip(want_pairs.iter())
        .filter(|((_, got_stored), (_, want_stored))| got_stored != want_stored)
        .count();
    assert_eq!(
        stored_byte_divergences, STORED_BYTE_DIVERGENCES,
        "llm_logs rows whose stored brotli bytes differ from v4's (decoded text identical) \
         moved from the escalated count — see `STORED_BYTE_DIVERGENCES`: a codec fix \
         retires the pin, a rise is a new encoder divergence"
    );

    println!(
        "memory_pipeline_jobs: {} cases OK",
        spec.me_cases.len() + spec.cs_cases.len()
    );
}
