//! Tier-3 differential test: the **buildContext capstone**
//! (`quilltap_core::services::build_context::build_context` — v4 `buildContext`,
//! lib/chat/context-manager.ts:477).
//!
//! Both sides run over ONE seeded two-database fixture (a responding character
//! with a real vault + `Knowledge/*.md` + a memory + vector, and a bare chat).
//! The unported feeder / post-office subsystems are the Rust `NoopSeams` and the
//! matching jest mocks on the oracle side; the model boundary is pinned
//! identically (canned embeddings keyed by exact query text; the compression
//! case's canned completion recorded + replayed). Each op's full `BuiltContext`
//! JSON is compared byte-for-byte — the read path mints nothing persistent, so
//! zero normalization (the timestamp whisper's wall clock is frozen on both
//! sides via the injected `now_ms` / the oracle's `FIXED_NOW_MS`).
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-build-context-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-build-context-mount.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-context-tier3-fixture.ts
//!   QT_FIXTURE_BC_MAIN=/tmp/qt-build-context-main.db \
//!   QT_FIXTURE_BC_MOUNT=/tmp/qt-build-context-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-build-context.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- build-context-tier3
//! Run:
//!   QT_ORACLE_BUILD_CONTEXT=/tmp/oracle-build-context.ndjson \
//!   QT_FIXTURE_BC_MAIN=/tmp/qt-build-context-main.db \
//!   QT_FIXTURE_BC_MOUNT=/tmp/qt-build-context-mount.db \
//!     cargo test -p quilltap-harness --test build_context_tier3_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::chat_timestamp::{TimestampConfig, TimestampFormat, TimestampMode};
use quilltap_core::db::characters_read;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionMessage, CompletionRole, CompletionUsage,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::services::build_context::{
    build_context, BuildContextInput, ContextCharacter, ContextChat, ExistingMessage,
    RealBuildContextSeams,
};
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::system_prompt::{Character as SysChar, UserCharacter};
use serde::Deserialize;
use serde_json::{Map, Value};

const FIXED_NOW_MS: i64 = 1_718_452_800_000; // matches the oracle's FIXED_NOW_MS

// ---------------------------------------------------------------------------
// Corpus spec (harness/oracle/fixtures/build-context-tier3.json).
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpecOp {
    name: String,
    character_id: String,
    #[serde(default)]
    new_user_message: Option<String>,
    #[serde(default)]
    existing_messages: Vec<SpecMsg>,
    #[serde(default)]
    existing_messages_repeat: Option<SpecRepeat>,
    #[serde(default)]
    chat_overrides: Option<SpecChatOverrides>,
    #[serde(default)]
    responding_participant_id: Option<String>,
    #[serde(default)]
    active_user_participant_id: Option<String>,
    #[serde(default)]
    participants: Vec<SpecParticipant>,
    #[serde(default)]
    messages_with_participants: Vec<SpecMwp>,
    skip_memories: bool,
    compression_enabled: bool,
    #[serde(default)]
    timestamp_mode: Option<String>,
    /// W4.4a4: the async pre-compression cache the spine would pass into buildContext.
    /// When present AND phase-1 fires, buildContext uses it verbatim (no sync
    /// compression call — proven by the completion provider recording no compression key).
    #[serde(default)]
    cached_compression: Option<SpecCachedCompression>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SpecCachedCompression {
    compressed_history: String,
    message_count: i64,
    #[serde(default)]
    warnings: Vec<String>,
}

#[derive(Deserialize, Clone)]
struct SpecMsg {
    #[serde(default)]
    id: Option<String>,
    role: String,
    content: String,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SpecChatOverrides {
    #[serde(default)]
    summary_anchor_message_ids: Vec<String>,
    #[serde(default)]
    context_summary: Option<String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SpecParticipant {
    id: String,
    #[serde(rename = "type")]
    participant_type: String,
    character_id: String,
    controlled_by: String,
    status: String,
    has_history_access: bool,
    created_at: String,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SpecMwp {
    id: String,
    role: String,
    content: String,
    participant_id: String,
    created_at: String,
    #[serde(default)]
    target_participant_ids: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpecRepeat {
    count: usize,
    user_content: String,
    assistant_content: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpecChat {
    id: String,
    #[serde(default)]
    compaction_generation: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    canned_embeddings: std::collections::HashMap<String, Vec<f32>>,
    chat: SpecChat,
    ops: Vec<SpecOp>,
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
        .join("../../harness/oracle/fixtures/build-context-tier3.json")
}

/// Map the overlaid character JSON (from `characters_read::find_by_id`) into the
/// system-prompt `Character` subset (the corpus characters carry no pronouns /
/// scenarios / systemPrompts, so those default empty).
fn sys_char_from_json(v: &Value) -> SysChar {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    SysChar {
        name: v
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        title: s("title"),
        identity: s("identity"),
        description: s("description"),
        manifesto: s("manifesto"),
        personality: s("personality"),
        ..Default::default()
    }
}

fn build_existing(op: &SpecOp) -> Vec<ExistingMessage> {
    if let Some(rep) = &op.existing_messages_repeat {
        let mut out = Vec::with_capacity(rep.count * 2);
        for _ in 0..rep.count {
            out.push(ExistingMessage {
                role: "user".to_string(),
                content: rep.user_content.clone(),
                ..Default::default()
            });
            out.push(ExistingMessage {
                role: "assistant".to_string(),
                content: rep.assistant_content.clone(),
                ..Default::default()
            });
        }
        out
    } else {
        op.existing_messages
            .iter()
            .map(|m| ExistingMessage {
                role: m.role.clone(),
                content: m.content.clone(),
                id: m.id.clone(),
                ..Default::default()
            })
            .collect()
    }
}

#[tokio::test]
async fn build_context_tier3_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_BUILD_CONTEXT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_BUILD_CONTEXT to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture_main = match std::env::var("QT_FIXTURE_BC_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_BC_MAIN to the seed main .db (see header).");
            return;
        }
    };
    let fixture_mount = match std::env::var("QT_FIXTURE_BC_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_BC_MOUNT to the seed mount-index .db (see header).");
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
    for line in oracle_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("result") => {
                oracle_results.push((v["op"].as_str().unwrap().to_string(), v["result"].clone()))
            }
            Some("canned") => oracle_canned.push(serde_json::from_value(v).expect("parse canned")),
            other => panic!("unknown oracle row kind {other:?}"),
        }
    }

    // Canned embeddings (exact query text → vector).
    let mut embedding = CannedEmbeddingProvider::new();
    for (text, vec) in &spec.canned_embeddings {
        embedding = embedding.with_vector(text.clone(), vec.clone());
    }

    // Canned completions (compression case).
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

    // Fresh copies so the shared seed fixture stays pristine.
    let pid = std::process::id();
    let work_main = std::env::temp_dir().join(format!("qt-bc-rust-main-{pid}.db"));
    let work_mount = std::env::temp_dir().join(format!("qt-bc-rust-mount-{pid}.db"));
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(&fixture_main, &work_main).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&fixture_mount, &work_mount).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    let executor = CheapLlmTaskExecutor::new();
    // Round-3 unification (Group 2): the W4.6b whisper writers run LIVE (the oracle
    // un-mocks the same writers). buildContext diffs only the returned BuiltContext;
    // the POSTs are proven in orchestrator_tier3's table dump.
    let seams = RealBuildContextSeams { db: &db };

    assert_eq!(
        spec.ops.len(),
        oracle_results.len(),
        "op-count mismatch — regenerate the oracle NDJSON"
    );

    // Cheap-LLM selection for the compression case (matches the oracle's).
    let selection = quilltap_core::cheap_llm::CheapLlmSelection {
        provider: "ANTHROPIC".to_string(),
        model_name: "claude-cheap".to_string(),
        base_url: None,
        connection_profile_id: None,
        is_local: false,
        profile_parameters: None,
    };

    for (op, (oracle_op, oracle_result)) in spec.ops.iter().zip(&oracle_results) {
        assert_eq!(&op.name, oracle_op, "op order mismatch");

        // Read the overlaid character + its vault mount id.
        let cid = op.character_id.clone();
        let (char_json, mount_id) = db
            .read_main(|main| {
                db.read_mount_index(|mount| {
                    let c = characters_read::find_by_id(main, mount, &cid)?
                        .unwrap_or_else(|| panic!("character {cid} not found"));
                    let mount_id = c
                        .get("characterDocumentMountPointId")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    Ok((c, mount_id))
                })
            })
            .expect("read character");

        let sys = sys_char_from_json(&char_json);
        let character = ContextCharacter {
            id: op.character_id.clone(),
            name: sys.name.clone(),
            character_document_mount_point_id: mount_id,
            sys,
        };

        let timestamp_config = op.timestamp_mode.as_deref().map(|mode| TimestampConfig {
            mode: match mode {
                "START_ONLY" => TimestampMode::StartOnly,
                "EVERY_MESSAGE" => TimestampMode::EveryMessage,
                "EVERY_N_MINUTES" => TimestampMode::EveryNMinutes,
                _ => TimestampMode::None,
            },
            format: TimestampFormat::Iso8601,
            custom_format: None,
            use_fictional_time: false,
            fictional_base_timestamp: None,
            fictional_base_real_time: None,
            auto_prepend: true,
            timezone: None,
            interval_minutes: 0,
        });

        // Multi-character inputs (op 7): participants + attribution messages +
        // the per-character map, each character read overlaid from the fixture.
        let is_multi = op.responding_participant_id.is_some()
            && !op.participants.is_empty()
            && !op.messages_with_participants.is_empty();
        let (responding_participant, all_participants, participant_characters, mwp) = if is_multi {
            let rp_id = op.responding_participant_id.clone().unwrap();
            let responding = Some(
                quilltap_core::services::build_context::RespondingParticipant {
                    id: rp_id,
                    selected_system_prompt_id: None,
                },
            );
            let all: Vec<quilltap_core::services::build_context::FullParticipant> = op
                .participants
                .iter()
                .map(
                    |p| quilltap_core::services::build_context::FullParticipant {
                        id: p.id.clone(),
                        participant_type: p.participant_type.clone(),
                        character_id: Some(p.character_id.clone()),
                        controlled_by: p.controlled_by.clone(),
                        status: p.status.clone(),
                        created_at: Some(p.created_at.clone()),
                        has_history_access: p.has_history_access,
                    },
                )
                .collect();
            let mut map = std::collections::HashMap::new();
            for p in &op.participants {
                let pcid = p.character_id.clone();
                let (pc_json, pc_mount) = db
                    .read_main(|main| {
                        db.read_mount_index(|mount| {
                            let c = characters_read::find_by_id(main, mount, &pcid)?
                                .unwrap_or_else(|| {
                                    panic!("participant character {pcid} not found")
                                });
                            let mount_id = c
                                .get("characterDocumentMountPointId")
                                .and_then(Value::as_str)
                                .map(str::to_string);
                            Ok((c, mount_id))
                        })
                    })
                    .expect("read participant character");
                let pc_sys = sys_char_from_json(&pc_json);
                map.insert(
                    p.character_id.clone(),
                    ContextCharacter {
                        id: p.character_id.clone(),
                        name: pc_sys.name.clone(),
                        character_document_mount_point_id: pc_mount,
                        sys: pc_sys,
                    },
                );
            }
            let mwp: Vec<quilltap_core::services::build_context::MessageWithParticipant> = op
                .messages_with_participants
                .iter()
                .map(
                    |m| quilltap_core::services::build_context::MessageWithParticipant {
                        id: Some(m.id.clone()),
                        role: m.role.clone(),
                        content: m.content.clone(),
                        participant_id: Some(m.participant_id.clone()),
                        thought_signature: None,
                        created_at: Some(m.created_at.clone()),
                        target_participant_ids: m.target_participant_ids.clone(),
                        host_event: None,
                    },
                )
                .collect();
            (responding, Some(all), Some(map), Some(mwp))
        } else {
            (None, None, None, None)
        };

        let (connection_profile, compression_settings, cheap_llm) = if op.compression_enabled {
            (
                Some(
                    quilltap_core::services::build_context::ConnectionProfileInput {
                        max_context: Some(8000),
                        max_tokens: Some(1000),
                        model_class: None,
                    },
                ),
                Some(
                    quilltap_core::services::build_context::ContextCompressionSettingsInput {
                        enabled: true,
                        window_size: 5,
                        compression_threshold_ratio: 0.5,
                        system_prompt_target_tokens: 1000,
                    },
                ),
                Some(selection.clone()),
            )
        } else {
            (None, None, None)
        };

        // W4.4a4 warm-cache: the pre-computed compression result the spine would pass.
        let (cached_compression, cached_compression_count) = match &op.cached_compression {
            Some(c) => (
                Some(
                    quilltap_core::services::compression::ContextCompressionResult {
                        compression_applied: true,
                        compressed_history: Some(c.compressed_history.clone()),
                        compressed_system_prompt: None,
                        compression_details: None,
                        warnings: c.warnings.clone(),
                    },
                ),
                Some(c.message_count),
            ),
            None => (None, None),
        };

        let input = BuildContextInput {
            model_context_limit: model_limit(),
            user_id: spec.user_id.clone(),
            character,
            user_character: Some(UserCharacter {
                name: "Charlie".to_string(),
                description: "The operator.".to_string(),
            }),
            chat: ContextChat {
                id: spec.chat.id.clone(),
                compaction_generation: spec.chat.compaction_generation,
                commonplace_recall_history: Value::Null,
                summary_anchor_message_ids: op
                    .chat_overrides
                    .as_ref()
                    .map(|o| o.summary_anchor_message_ids.clone())
                    .unwrap_or_default(),
                context_summary: op
                    .chat_overrides
                    .as_ref()
                    .and_then(|o| o.context_summary.clone()),
                ..Default::default()
            },
            existing_messages: build_existing(op),
            new_user_message: op.new_user_message.clone(),
            active_user_participant_id: op.active_user_participant_id.clone(),
            roleplay_template: None,
            embedding_profile_id: None,
            skip_memories: op.skip_memories,
            min_memory_importance: 0.5,
            responding_participant,
            all_participants,
            participant_characters,
            messages_with_participants: mwp,
            tool_instructions: None,
            timestamp_config,
            is_initial_message: false,
            timezone: None,
            connection_profile,
            context_compression_settings: compression_settings,
            cheap_llm_selection: cheap_llm,
            bypass_compression: false,
            cached_compression_result: cached_compression.clone(),
            cached_compression_message_count: cached_compression_count,
            generate_memory_recap: false,
            uncensored_fallback: None,
            is_continue_mode: false,
            now_ms: FIXED_NOW_MS,
            local_offset_minutes: 0,
            minutes_since_last_timestamp_announcement: None,
        };

        let built = build_context(&db, &embedding, &completion, &executor, &seams, &input)
            .await
            .unwrap_or_else(|e| panic!("{}: build_context failed: {e}", op.name));

        // Serialize then RE-PARSE the Rust result so both sides pass through the
        // same serde_json float parse. Without the `float_roundtrip` feature
        // serde_json's number parsing can land 1 ULP off the shortest-repr
        // decimal (e.g. the text-fallback score `0.043750000000000004` parses to
        // the neighbouring f64) — the oracle NDJSON is parsed that way, so the
        // Rust value must be canonicalized identically. The serialized STRING is
        // still the exact shortest repr of the computed f64, so a real numeric
        // divergence (> 1 decimal-parse ULP) still fails.
        let got_text = serde_json::to_string(&built).expect("serialize built context");
        let mut got: Value = serde_json::from_str(&got_text).expect("reparse built context");
        strip_recall_debug(&mut got);
        let want = normalize_oracle(oracle_result);
        assert_eq!(
            got,
            want,
            "{}: built context diverges\n got={}\nwant={}",
            op.name,
            serde_json::to_string_pretty(&got).unwrap(),
            serde_json::to_string_pretty(&want).unwrap(),
        );
    }

    drop(db);
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
}

/// The model context limit v4's `getModelContextLimit('ANTHROPIC', 'claude-test')`
/// resolves (the ANTHROPIC hardcoded provider default, 200000). Injected on the
/// Rust side — registry resolution is a separate concern (see
/// `crate::model_context`), and the corpus pins the model so the value is fixed.
fn model_limit() -> i64 {
    200_000
}

/// The oracle emits v4's `BuiltContext` with `undefined` optionals dropped by
/// `JSON.stringify`. The one place the ported code legitimately diverges is the
/// dynamic-head `debugMemories` **recall-adjustment** diagnostics
/// (`recallMultiplier`/`recallFired`/`blendedBefore`/`blendedAfter`): v4's
/// `searchMemoriesSemantic` attaches a `recallAdjustment` record (a no-op with
/// multiplier 1 / nothing fired for this corpus), while the ported
/// `search_memories_semantic` defers the recallContext re-rank (a documented
/// deferral in that unit), so it carries no adjustment. These four fields are
/// purely diagnostic (they never reach the rendered memory block) — strip them
/// from BOTH the oracle rows and the Rust output so the comparison is on the
/// load-bearing shape. Everything else compares byte-for-byte.
const RECALL_DEBUG_KEYS: &[&str] = &[
    "recallMultiplier",
    "recallFired",
    "blendedBefore",
    "blendedAfter",
];

fn strip_recall_debug(v: &mut Value) {
    if let Some(dm) = v.get_mut("debugMemories").and_then(Value::as_array_mut) {
        for entry in dm.iter_mut() {
            if let Some(obj) = entry.as_object_mut() {
                for k in RECALL_DEBUG_KEYS {
                    obj.remove(*k);
                }
            }
        }
    }
}

fn normalize_oracle(v: &Value) -> Value {
    let mut obj = Value::Object(v.as_object().cloned().unwrap_or_else(Map::new));
    strip_recall_debug(&mut obj);
    obj
}
