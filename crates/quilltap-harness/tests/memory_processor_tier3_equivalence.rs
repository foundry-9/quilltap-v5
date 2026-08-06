//! Tier-3 differential test: the **memory processor**
//! (`quilltap_core::services::memory_processor` — v4 `processTurnForMemory`),
//! the model-dependent per-turn extraction service, verified tier-3 → tier-2.
//!
//! Both sides pin BOTH model calls identically: the completion by exact call
//! key (the jest oracle resolves each real call to a corpus rule and RECORDS
//! the exact `provider|model|temperature|messages` entry it answered; this
//! test replays those recorded entries through `CannedCompletionProvider`, so
//! a prompt/selection/temperature divergence surfaces as a canned-miss), and
//! the embedding by exact `${summary}\n\n${content}` text (as in the
//! memory-gate differential). Then:
//!
//!   1. each call's `TurnMemoryProcessingResult` is compared structurally
//!      (debug logs byte-for-byte — they carry the rate-limit, canon-source,
//!      throttle, and gate-outcome attributions; minted created-memory ids
//!      tokenized on both sides);
//!   2. the three affected tables (`memories`, `vector_indices`,
//!      `vector_entries`) are diffed in the shared-cross-table id-map remap
//!      form (as the memory-gate test).
//!
//! Also the first differential to exercise the gate's now-ported
//! `applyNamePresenceCheck` lookup branch: one OTHER candidate deliberately
//! fails the subject-name presence check and lands with `aboutCharacterId`
//! flipped to the observer.
//!
//! Generate the fixtures + oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-memory-processor-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/memory-processor-tier3.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/memory-processor-tier3.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-memory-processor-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-memory-processor-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-memory-processor-fixture.ts
//!   QT_FIXTURE_PROCESSOR_MAIN=/tmp/qt-memory-processor-main.db \
//!   QT_FIXTURE_PROCESSOR_MOUNT=/tmp/qt-memory-processor-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-memory-processor.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/cases" -- "memory-processor-tier3\\.test\\.ts$"
//! Run:
//!   QT_ORACLE_PROCESSOR=/tmp/oracle-memory-processor.ndjson \
//!   QT_FIXTURE_PROCESSOR_MAIN=/tmp/qt-memory-processor-main.db \
//!   QT_FIXTURE_PROCESSOR_MOUNT=/tmp/qt-memory-processor-mount.db \
//!     cargo test -p quilltap-harness --test memory_processor_tier3_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::cheap_llm::{CheapLlmProfile, DangerousContentSettings};
use quilltap_core::db::characters_read;
use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::jsstr::utf16_len;
use quilltap_core::memory_format::Pronouns;
use quilltap_core::memory_tasks::{TurnCharacterSlice, TurnTranscript};
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionMessage, CompletionRole, CompletionUsage,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::services::cheap_llm_exec::{CheapLlmLogConfig, CheapLlmTaskExecutor};
use quilltap_core::services::llm_logging::LogContext;
use quilltap_core::services::memory_processor::{
    process_turn_for_memory, CheapLlmSettings, MemoryExtractionLimits, TurnMemoryExtractionContext,
};
use serde::Deserialize;
use serde_json::Value;

mod common;

// ---------------------------------------------------------------------------
// Spec structures (harness/oracle/fixtures/memory-processor-tier3.json).
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct PronounsW {
    subject: String,
    object: String,
    possessive: String,
}

impl PronounsW {
    fn into_core(self) -> Pronouns {
        Pronouns {
            subject: self.subject,
            object: self.object,
            possessive: self.possessive,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SliceW {
    character_id: String,
    character_name: String,
    #[serde(default)]
    character_pronouns: Option<PronounsW>,
    text: String,
    contributing_message_ids: Vec<String>,
    #[serde(default)]
    is_user_controlled: bool,
    #[serde(default)]
    last_message_created_at: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptW {
    #[serde(default)]
    turn_opener_message_id: Option<String>,
    #[serde(default)]
    user_message: Option<String>,
    #[serde(default)]
    user_character_id: Option<String>,
    #[serde(default)]
    user_character_name: Option<String>,
    #[serde(default)]
    user_character_pronouns: Option<PronounsW>,
    character_slices: Vec<SliceW>,
    #[serde(default)]
    latest_assistant_message_id: Option<String>,
    #[serde(default)]
    turn_timestamp: Option<String>,
}

impl TranscriptW {
    fn into_core(self) -> TurnTranscript {
        TurnTranscript {
            turn_opener_message_id: self.turn_opener_message_id,
            user_message: self.user_message,
            user_character_id: self.user_character_id,
            user_character_name: self.user_character_name,
            user_character_pronouns: self.user_character_pronouns.map(PronounsW::into_core),
            character_slices: self
                .character_slices
                .into_iter()
                .map(|s| TurnCharacterSlice {
                    character_id: s.character_id,
                    character_name: s.character_name,
                    character_pronouns: s.character_pronouns.map(PronounsW::into_core),
                    text: s.text,
                    contributing_message_ids: s.contributing_message_ids,
                    is_user_controlled: s.is_user_controlled,
                    last_message_created_at: s.last_message_created_at,
                })
                .collect(),
            latest_assistant_message_id: self.latest_assistant_message_id,
            turn_timestamp: self.turn_timestamp,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileW {
    id: String,
    provider: String,
    model_name: String,
    #[serde(default)]
    base_url: Option<String>,
    is_cheap: bool,
    is_dangerous_compatible: bool,
    #[serde(default)]
    parameters: Option<Value>,
    #[serde(default)]
    max_tokens: Option<f64>,
    #[serde(default)]
    model_class: Option<String>,
}

impl ProfileW {
    fn into_core(self) -> CheapLlmProfile {
        CheapLlmProfile {
            id: self.id,
            provider: self.provider,
            model_name: self.model_name,
            base_url: self.base_url,
            is_cheap: self.is_cheap,
            is_dangerous_compatible: self.is_dangerous_compatible,
            parameters: self.parameters,
            max_tokens: self.max_tokens,
            model_class: self.model_class,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DangerW {
    mode: String,
    #[serde(default)]
    uncensored_text_profile_id: Option<String>,
}

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
struct SettingsW {
    strategy: String,
    #[serde(default)]
    user_defined_profile_id: Option<String>,
    fallback_to_local: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CallW {
    name: String,
    chat_id: String,
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    project_description: Option<String>,
    #[serde(default)]
    chat_context_summary: Option<String>,
    #[serde(rename = "cheapLLMSettings")]
    cheap_llm_settings: SettingsW,
    #[serde(default)]
    danger_settings: Option<DangerW>,
    is_dangerous_chat: bool,
    #[serde(default)]
    memory_extraction_limits: Option<LimitsW>,
    #[serde(default)]
    source_message_timestamp: Option<String>,
    #[serde(default)]
    timeline_mode: Option<String>,
    dry_run: bool,
    in_autonomous_room: bool,
    participant_ids: Vec<String>,
    transcript: TranscriptW,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    profiles: Vec<ProfileW>,
    current_profile_id: String,
    canned_embeddings: HashMap<String, Vec<f32>>,
    canned_failures: Vec<String>,
    calls: Vec<CallW>,
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
        .join("../../harness/oracle/fixtures/memory-processor-tier3.json")
}

// ---------------------------------------------------------------------------
// Table normalization — the memory-gate shared-cross-table id-map remap form.
// ---------------------------------------------------------------------------

struct TableSpec {
    table: &'static str,
    order_by: &'static str,
    id_columns: &'static [&'static str],
    id_array_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
}

const TABLES: &[TableSpec] = &[
    TableSpec {
        table: "memories",
        order_by: "content",
        id_columns: &["id"],
        id_array_columns: &["relatedMemoryIds"],
        ts_columns: &[
            "createdAt",
            "updatedAt",
            "lastReinforcedAt",
            "lastAccessedAt",
        ],
    },
    TableSpec {
        table: "vector_entries",
        order_by: "embedding",
        id_columns: &["id"],
        id_array_columns: &[],
        ts_columns: &["createdAt"],
    },
    TableSpec {
        table: "vector_indices",
        order_by: "id",
        // id == characterId (pinned) — left literal.
        id_columns: &[],
        id_array_columns: &[],
        ts_columns: &["createdAt", "updatedAt"],
    },
];

fn token_for(id_map: &mut HashMap<String, String>, raw: &str) -> String {
    let next = format!("ID_{}", id_map.len());
    id_map.entry(raw.to_string()).or_insert(next).clone()
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

/// Tokenize the minted created-memory ids inside a result object (both sides),
/// leaving the pinned reinforced ids literal.
fn normalize_result(result: &mut Value) {
    if let Some(Value::Array(ids)) = result.get_mut("createdMemoryIds") {
        for (i, id) in ids.iter_mut().enumerate() {
            *id = Value::String(format!("CREATED_{i}"));
        }
    }
}

#[tokio::test]
async fn memory_processor_tier3_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_PROCESSOR") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PROCESSOR to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture_main = match std::env::var("QT_FIXTURE_PROCESSOR_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_PROCESSOR_MAIN to the seed main .db (see header).");
            return;
        }
    };
    let fixture_mount = match std::env::var("QT_FIXTURE_PROCESSOR_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_PROCESSOR_MOUNT to the seed mount .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    let mut oracle_results: Vec<(String, Value)> = Vec::new();
    let mut oracle_canned: Vec<CannedRowW> = Vec::new();
    let mut oracle_tables: HashMap<String, Value> = HashMap::new();
    let mut oracle_llm_logs: Option<Vec<Value>> = None;
    for line in oracle_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("result") => {
                oracle_results.push((v["call"].as_str().unwrap().to_string(), v["result"].clone()))
            }
            Some("canned") => {
                oracle_canned.push(serde_json::from_value(v).expect("parse canned row"))
            }
            Some("table") => {
                oracle_tables.insert(v["table"].as_str().unwrap().to_string(), v);
            }
            Some("llmlogs") => oracle_llm_logs = Some(common::oracle_llm_logs(&v)),
            other => panic!("unknown oracle row kind {other:?}"),
        }
    }

    // Fresh copies so the shared seed fixtures stay pristine.
    let pid = std::process::id();
    let work_main = std::env::temp_dir().join(format!("qt-memproc-main-rust-{pid}.db"));
    let work_mount = std::env::temp_dir().join(format!("qt-memproc-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(&fixture_main, &work_main).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&fixture_mount, &work_mount).unwrap_or_else(|e| panic!("copy mount: {e}"));

    // The completion provider replays EXACTLY the entries the oracle recorded —
    // an input the oracle never sent is a surfaced canned-miss, not an answer.
    let mut completion = CannedCompletionProvider::new();
    for row in &oracle_canned {
        let messages: Vec<CompletionMessage> = row
            .messages
            .iter()
            .map(|m| CompletionMessage {
                role: match m.role.as_str() {
                    "system" => CompletionRole::System,
                    "user" => CompletionRole::User,
                    "assistant" => CompletionRole::Assistant,
                    other => panic!("unexpected role {other}"),
                },
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

    // The same canned embeddings both sides inject. The failure message mirrors
    // the oracle's text-length-parameterized message (the gate's retry reason
    // reaches the debug logs), so it must match per failing input — the corpus
    // carries exactly one.
    let mut embedding = CannedEmbeddingProvider::new();
    for (text, vec) in &spec.canned_embeddings {
        embedding = embedding.with_vector(text.clone(), vec.clone());
    }
    for text in &spec.canned_failures {
        embedding = embedding
            .with_failure(text.clone())
            .with_failure_message(format!(
                "canned failure for input ({} chars)",
                utf16_len(text)
            ));
    }

    // W4.10b: a fresh llm-logs partition for the MEMORY_EXTRACTION rows the SELF/
    // OTHER extraction cheap calls write.
    let work_ll = work_main.with_file_name(format!("qt-memproc-ll-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work_ll);
    common::materialize_llm_logs(&work_ll, &spec.test_pepper_base64);

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: Some(work_ll.clone()),
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copies: {e}"));

    let profiles: Vec<CheapLlmProfile> =
        spec.profiles.into_iter().map(ProfileW::into_core).collect();
    let current_profile = profiles
        .iter()
        .find(|p| p.id == spec.current_profile_id)
        .expect("current profile in spec")
        .clone();

    assert_eq!(
        spec.calls.len(),
        oracle_results.len(),
        "call-count mismatch — regenerate the oracle NDJSON"
    );

    for (call, (oracle_name, oracle_result)) in spec.calls.into_iter().zip(oracle_results) {
        assert_eq!(call.name, oracle_name, "call order mismatch");
        let name = call.name.clone();

        // Hydrated participant characters (v4 `repos.characters.findByIds`).
        let mut participant_characters: HashMap<String, Value> = HashMap::new();
        if !call.participant_ids.is_empty() {
            let ids = call.participant_ids.clone();
            let chars = db
                .read_main(|main| {
                    db.read_mount_index(|mount| characters_read::find_by_ids(main, mount, &ids))
                })
                .unwrap_or_else(|e| panic!("{name}: findByIds: {e:?}"));
            for c in chars {
                let id = c["id"].as_str().unwrap().to_string();
                participant_characters.insert(id, c);
            }
        }

        // Per-call executor with logging: the SELF/OTHER extraction cheap calls
        // write MEMORY_EXTRACTION rows carrying this call's chatId + the extracted
        // characterId (passed per-execute-call); v4's memory extraction sends no
        // messageId. The corpus has no temperature rejection, so a fresh executor
        // per call is equivalent to v4's module-global no-temp cache.
        let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
            db: db.clone(),
            user_id: spec.user_id.clone(),
            chat_id: Some(call.chat_id.clone()),
            message_id: None,
            ctx: LogContext::none(),
        });

        let ctx = TurnMemoryExtractionContext {
            transcript: call.transcript.into_core(),
            participant_characters,
            chat_id: call.chat_id,
            project_id: call.project_id,
            project_description: call.project_description,
            chat_context_summary: call.chat_context_summary,
            user_id: spec.user_id.clone(),
            connection_profile: current_profile.clone(),
            cheap_llm_settings: CheapLlmSettings {
                strategy: call.cheap_llm_settings.strategy,
                user_defined_profile_id: call.cheap_llm_settings.user_defined_profile_id,
                fallback_to_local: call.cheap_llm_settings.fallback_to_local,
            },
            available_profiles: Some(profiles.clone()),
            danger_settings: call.danger_settings.map(|d| DangerousContentSettings {
                mode: d.mode,
                uncensored_text_profile_id: d.uncensored_text_profile_id,
            }),
            is_dangerous_chat: call.is_dangerous_chat,
            memory_extraction_limits: call.memory_extraction_limits.map(|l| {
                MemoryExtractionLimits {
                    enabled: l.enabled,
                    max_per_hour: l.max_per_hour,
                    soft_start_fraction: l.soft_start_fraction,
                    soft_floor: l.soft_floor,
                }
            }),
            source_message_timestamp: call.source_message_timestamp,
            timeline_mode: call.timeline_mode,
            dry_run: call.dry_run,
            in_autonomous_room: call.in_autonomous_room,
            registry_cheapest_for_current: None,
        };

        let result = process_turn_for_memory(&db, &completion, &embedding, &executor, &ctx).await;
        let mut got = serde_json::to_value(&result).expect("serialize result");
        let mut want = oracle_result;
        normalize_result(&mut got);
        normalize_result(&mut want);
        assert_eq!(got, want, "{name}: result object diverges");
    }

    // W4.10b: the MEMORY_EXTRACTION rows the extraction cheap calls wrote.
    let got_logs = common::dump_llm_logs(&db);

    // Dump + diff the three tables in the shared-id-map remap form.
    let mut got: Vec<Value> = TABLES
        .iter()
        .map(|t| {
            db.read_main(|conn| dump_table_json_conn(conn, t.table, t.order_by))
                .unwrap_or_else(|e| panic!("dump {}: {e:?}", t.table))
        })
        .collect();
    drop(db);
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    let _ = std::fs::remove_file(&work_ll);

    let want_logs = oracle_llm_logs.expect("oracle emitted no llmlogs row");
    assert_eq!(
        got_logs,
        want_logs,
        "llm_logs MEMORY_EXTRACTION rows diverge (got {} vs oracle {})",
        got_logs.len(),
        want_logs.len()
    );

    let mut want: Vec<Value> = TABLES
        .iter()
        .map(|t| {
            let mut v = oracle_tables
                .remove(t.table)
                .unwrap_or_else(|| panic!("oracle missing table {}", t.table));
            // Drop the envelope key so the shapes align with the Rust dump.
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
        assert_eq!(got[i]["rows"], want[i]["rows"], "{} rows diverge", t.table);
    }
}
