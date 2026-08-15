//! Tier-3 differential test: the **answer-confirmation service** (W4.3;
//! `quilltap_core::services::answer_confirmation` +
//! `quilltap_core::services::message_finalizer` — v4 `finalizeMessageResponse` +
//! `answer-confirmation.service.ts`), verified tier-3 → tier-2.
//!
//! Both sides copy the same two-DB seed fixture (one character + vault; 17 chats
//! each with a seeded Commonplace whisper / tool slate — the three `scene_*`
//! chats also carry seeded prior dialogue; a project with an
//! `answerConfirmationOverride`) and run the same 17-call finalize sequence with
//! the answer-confirmation feature ON. The model boundary is canned: the Rust
//! [`RealAnswerConfirmation`] runner replays the oracle-recorded
//! `provider|model|temperature|messages` keys (the check on the cheap profile,
//! the re-affirmation on the character's OWN profile, the danger case's cheap
//! profile ESCALATED to the uncensored one — the recorded keys prove the prompt
//! bytes, the 24 K reference truncation, the a7b1398d re-affirmation scene block
//! [`buildRecentConversationContext` — the 20-message cap, the 8 K-UTF-16
//! tail-slice truncation with a non-ASCII boundary, the null no-dialogue path]
//! + name anchor, and the profile switch). Then:
//!
//!   1. each call's `ProcessMessageResult` is compared (v4's rich
//!      `sceneTrackingContext` collapsed to its `characterIds`);
//!   2. each call's ordered SSE-event trace (the `confirming` / `affirming` /
//!      `confirmationResult` / `done` frames) is compared byte-for-byte (all ids
//!      pinned);
//!   3. the `chats` / `chat_messages` dumps are compared in a sentinel-aware
//!      minted-values form (a timestamp cell differing from its pre-run value
//!      collapses to `<ts>`; a new `chat_messages` row placeholders only
//!      `createdAt`; the `confirmation*` columns are diffed exactly).
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-ac-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-ac-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-answer-confirmation-fixture.ts
//!   TMPO=/tmp/qt-ac-oracle; mkdir -p $TMPO/cases $TMPO/fixtures
//!   cp $V5W/harness/oracle/cases/answer-confirmation-tier3.test.ts $TMPO/cases/
//!   cp $V5W/harness/oracle/fixtures/answer-confirmation-tier3.json $TMPO/fixtures/
//!   QT_FIXTURE_AC_MAIN=/tmp/qt-ac-main.db QT_FIXTURE_AC_MOUNT=/tmp/qt-ac-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-answer-confirmation.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- answer-confirmation-tier3
//! Run:
//!   QT_ORACLE_ANSWER_CONFIRMATION=/tmp/oracle-answer-confirmation.ndjson \
//!   QT_FIXTURE_AC_MAIN=/tmp/qt-ac-main.db QT_FIXTURE_AC_MOUNT=/tmp/qt-ac-mount.db \
//!     cargo test -p quilltap-harness --test answer_confirmation_tier3_equivalence

use std::collections::HashMap;

use quilltap_core::cheap_llm::{CheapLlmProfile, CheapLlmSelection, DangerousContentSettings};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::{characters_read, chats_read};
use quilltap_core::model::completion::{
    canned_completion_key, CompletionError, CompletionMessage, CompletionParams,
    CompletionProvider, CompletionResponse, CompletionRole,
};
use quilltap_core::services::answer_confirmation::ReaffirmationProfile;
use quilltap_core::services::chat_events::RecordingSink;
use quilltap_core::services::cheap_llm_exec::{CheapLlmLogConfig, CheapLlmTaskExecutor};
use quilltap_core::services::llm_logging::LogContext;
use quilltap_core::services::message_finalizer::{
    finalize_message_response, ClosureProspero, FinalizeOptions, FinalizerCharacter, FinalizerChat,
    FinalizerChatSettings, FinalizerCompression, FinalizerConfirmationInputs, FinalizerParticipant,
    FinalizerProfile, FinalizerStreaming, NoAsyncCompression, NoCostTracking, ParticipantCharacter,
    ProcessMessageResult, RealAnswerConfirmation,
};
use quilltap_core::services::tool_execution::ToolMessage;
use quilltap_core::tools::rng::FixedBytes;
use serde::Deserialize;
use serde_json::{json, Value};

mod common;

// ---------------------------------------------------------------------------
// Spec.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "cheapSelection")]
    cheap_selection: CheapSelectionSpec,
    #[serde(rename = "connectionProfile")]
    connection_profile: ConnectionProfileSpec,
    #[serde(rename = "uncensoredProfile")]
    uncensored_profile: UncensoredProfileSpec,
    calls: Vec<CallSpec>,
}

#[derive(Deserialize)]
struct CheapSelectionSpec {
    provider: String,
    #[serde(rename = "modelName")]
    model_name: String,
    #[serde(rename = "connectionProfileId")]
    connection_profile_id: String,
    #[serde(rename = "isLocal")]
    is_local: bool,
}

#[derive(Deserialize)]
struct ConnectionProfileSpec {
    id: String,
    provider: String,
    #[serde(rename = "modelName")]
    model_name: String,
    #[serde(rename = "baseUrl")]
    base_url: Option<String>,
    parameters: Value,
    /// P4.D79: the Max Context, for the shared `profileParams` helper the
    /// re-affirmation call now goes through. Absent in every corpus row so far.
    #[serde(rename = "maxContext", default)]
    max_context: Option<f64>,
}

#[derive(Deserialize)]
struct UncensoredProfileSpec {
    id: String,
    provider: String,
    #[serde(rename = "modelName")]
    model_name: String,
    #[serde(rename = "isDangerousCompatible")]
    is_dangerous_compatible: bool,
}

#[derive(Deserialize)]
struct ToolMessageSpec {
    #[serde(rename = "toolName")]
    tool_name: String,
    success: bool,
    content: String,
    arguments: Value,
}

#[derive(Deserialize)]
struct CallSpec {
    name: String,
    #[serde(rename = "chatId")]
    chat_id: String,
    #[serde(rename = "participantId")]
    participant_id: String,
    #[serde(rename = "assistantMessageId")]
    assistant_message_id: String,
    #[serde(rename = "projectId")]
    project_id: Option<String>,
    #[serde(rename = "globalEnabled")]
    global_enabled: bool,
    #[serde(default)]
    dangerous: bool,
    #[serde(rename = "toolMessages")]
    tool_messages: Vec<ToolMessageSpec>,
    reply: String,
}

fn spec_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/answer-confirmation-tier3.json")
}

// ---------------------------------------------------------------------------
// Canned completion provider (replays the oracle-recorded keys).
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CannedRow {
    provider: String,
    model: String,
    temperature: Option<f64>,
    messages: Vec<CannedMsg>,
    response: String,
}
#[derive(Deserialize)]
struct CannedMsg {
    role: String,
    content: String,
}

struct CannedCompletion(HashMap<String, CompletionResponse>);
impl CompletionProvider for CannedCompletion {
    fn send_message(
        &self,
        provider: &str,
        _base_url: Option<&str>,
        params: &CompletionParams,
    ) -> impl std::future::Future<Output = Result<CompletionResponse, CompletionError>> + Send {
        let key = canned_completion_key(
            provider,
            &params.model,
            params.temperature,
            &params.messages,
        );
        let result = self
            .0
            .get(&key)
            .cloned()
            .ok_or_else(|| CompletionError::new(format!("no canned completion for key {key}")));
        async move { result }
    }
}

fn role_of(s: &str) -> CompletionRole {
    match s {
        "system" => CompletionRole::System,
        "user" => CompletionRole::User,
        "assistant" => CompletionRole::Assistant,
        "tool" => CompletionRole::Tool,
        other => panic!("unknown role {other}"),
    }
}

// ---------------------------------------------------------------------------
// Marshaling.
// ---------------------------------------------------------------------------

fn str_of(row: &Value, key: &str) -> Option<String> {
    row.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|_| !row.get(key).map(Value::is_null).unwrap_or(true))
}

fn to_finalizer_chat(row: &Value, project_override: Option<&str>) -> FinalizerChat {
    FinalizerChat {
        id: str_of(row, "id").unwrap_or_default(),
        is_paused: row
            .get("isPaused")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        chat_type: str_of(row, "chatType"),
        project_id: str_of(row, "projectId"),
        answer_confirmation_override: str_of(row, "answerConfirmationOverride"),
        answer_confirmation_project_override: project_override.map(str::to_string),
        impersonating_participant_ids: row
            .get("impersonatingParticipantIds")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        participants: row
            .get("participants")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        spoken_this_cycle_participant_ids: str_of(row, "spokenThisCycleParticipantIds"),
        allow_cross_character_vault_reads: row
            .get("allowCrossCharacterVaultReads")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

fn read_character(db: &Db, id: &str) -> Value {
    let id = id.to_string();
    db.read_main(|main| db.read_mount_index(|mount| characters_read::find_by_id(main, mount, &id)))
        .expect("character read")
        .expect("character exists")
}

fn dump_table(db: &Db, table: &str, order_by: &str) -> Value {
    let table = table.to_string();
    let order_by = order_by.to_string();
    db.read_main(move |c| quilltap_core::db::dump_table_json_conn(c, &table, &order_by))
        .expect("dump table")
}

// ---------------------------------------------------------------------------
// Normalization.
// ---------------------------------------------------------------------------

fn normalize_result(v4_result: &Value) -> Value {
    let mut out = serde_json::Map::new();
    for k in [
        "isMultiCharacter",
        "hasContent",
        "messageId",
        "userParticipantId",
        "isPaused",
    ] {
        out.insert(
            k.to_string(),
            v4_result.get(k).cloned().unwrap_or(Value::Null),
        );
    }
    let ids = v4_result
        .get("sceneTrackingContext")
        .and_then(|c| c.get("characterIds"))
        .cloned();
    out.insert(
        "sceneTrackingCharacterIds".into(),
        ids.unwrap_or(Value::Null),
    );
    Value::Object(out)
}

fn rust_result_to_value(r: &ProcessMessageResult) -> Value {
    json!({
        "isMultiCharacter": r.is_multi_character,
        "hasContent": r.has_content,
        "messageId": r.message_id,
        "userParticipantId": r.user_participant_id,
        "isPaused": r.is_paused,
        "sceneTrackingCharacterIds": r.scene_tracking_character_ids,
    })
}

/// Snapshot `{row id → {col → value}}` for the given columns of a dump.
fn snapshot_cells(dump: &Value, cols: &[&str]) -> HashMap<String, HashMap<String, Value>> {
    let mut map = HashMap::new();
    if let Some(rows) = dump.get("rows").and_then(Value::as_array) {
        for row in rows {
            let id = row.get("id").and_then(Value::as_str).unwrap_or_default();
            let mut cells = HashMap::new();
            for c in cols {
                cells.insert(c.to_string(), row.get(*c).cloned().unwrap_or(Value::Null));
            }
            map.insert(id.to_string(), cells);
        }
    }
    map
}

/// Sentinel-aware placeholdering: a cell that differs from its pre-run value
/// collapses to `<ts>`; a cell still at the pre-run value stays literal (so a
/// missing bump is caught). A row absent from the pre-run snapshot (a new row)
/// placeholders every listed column.
fn normalize_vs_prerun(
    dump: &mut Value,
    pre: &HashMap<String, HashMap<String, Value>>,
    cols: &[&str],
) {
    if let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) {
        for row in rows {
            let id = row
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let pre_cells = pre.get(&id);
            if let Some(obj) = row.as_object_mut() {
                for c in cols {
                    let current = obj.get(*c).cloned().unwrap_or(Value::Null);
                    let changed = match pre_cells {
                        Some(cells) => cells.get(*c) != Some(&current),
                        None => true, // new row → minted
                    };
                    if changed && !current.is_null() {
                        obj.insert((*c).to_string(), Value::String("<ts>".into()));
                    }
                }
            }
        }
    }
}

fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if n.is_f64() {
                if let Some(f) = n.as_f64() {
                    if f.fract() == 0.0 && f.abs() < 9.007e15 {
                        *v = Value::Number(serde_json::Number::from(f as i64));
                    }
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.values_mut().for_each(canon_numbers),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// The test.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct OracleLine {
    kind: String,
    #[serde(default)]
    call: Option<String>,
    #[serde(flatten)]
    rest: Value,
}

#[test]
fn answer_confirmation_tier3_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_ANSWER_CONFIRMATION") else {
        eprintln!("QT_ORACLE_ANSWER_CONFIRMATION not set; skipping");
        return;
    };
    let Ok(fixture_main) = std::env::var("QT_FIXTURE_AC_MAIN") else {
        eprintln!("QT_FIXTURE_AC_MAIN not set; skipping");
        return;
    };
    let Ok(fixture_mount) = std::env::var("QT_FIXTURE_AC_MOUNT") else {
        eprintln!("QT_FIXTURE_AC_MOUNT not set; skipping");
        return;
    };

    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("spec file readable"))
            .expect("spec parses");

    // Parse the oracle NDJSON.
    let oracle_text = std::fs::read_to_string(&oracle_path).expect("oracle readable");
    let mut want_results: HashMap<String, Value> = HashMap::new();
    let mut want_events: HashMap<String, Value> = HashMap::new();
    let mut want_tables: HashMap<String, Value> = HashMap::new();
    let mut want_llm_logs: Option<Vec<Value>> = None;
    let mut completions: HashMap<String, CompletionResponse> = HashMap::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let parsed: OracleLine = serde_json::from_str(line).expect("oracle line parses");
        match parsed.kind.as_str() {
            "llmlogs" => want_llm_logs = Some(common::oracle_llm_logs(&parsed.rest)),
            "result" => {
                want_results.insert(
                    parsed.call.clone().unwrap(),
                    parsed.rest.get("result").cloned().unwrap(),
                );
            }
            "events" => {
                want_events.insert(
                    parsed.call.clone().unwrap(),
                    parsed.rest.get("events").cloned().unwrap(),
                );
            }
            "canned" => {
                let row: CannedRow = serde_json::from_value(parsed.rest).expect("canned row");
                let messages: Vec<CompletionMessage> = row
                    .messages
                    .iter()
                    .map(|m| CompletionMessage {
                        role: role_of(&m.role),
                        content: m.content.clone(),
                    })
                    .collect();
                let key =
                    canned_completion_key(&row.provider, &row.model, row.temperature, &messages);
                completions.insert(
                    key,
                    CompletionResponse {
                        content: row.response,
                        usage: None,
                        finish_reason: None,
                        attachment_results: None,
                    },
                );
            }
            "table" => {
                let table = parsed
                    .rest
                    .get("table")
                    .and_then(Value::as_str)
                    .unwrap()
                    .to_string();
                want_tables.insert(table, parsed.rest);
            }
            other => panic!("unknown oracle line kind: {other}"),
        }
    }

    // Copy the fixture DBs to a scratch dir.
    let scratch = std::env::temp_dir().join(format!("qt-ac-harness-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let work_main = scratch.join("ac-main.db");
    let work_mount = scratch.join("ac-mount.db");
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(&fixture_main, &work_main).expect("copy main fixture");
    std::fs::copy(&fixture_mount, &work_mount).expect("copy mount fixture");

    // W4.10b: a fresh llm-logs partition so the check + re-affirmation calls'
    // ANSWER_CONFIRMATION `logLLMCall` rows land where we can dump them.
    let work_ll = scratch.join("ac-llm-logs.db");
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
    .expect("open fixture instance");

    // Pre-run snapshots for the sentinel-aware normalization.
    let pre_chats = snapshot_cells(
        &dump_table(&db, "chats", "id"),
        &["updatedAt", "lastMessageAt"],
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    // Available profiles (for the uncensored escalation lookup).
    let available_profiles = vec![CheapLlmProfile {
        id: spec.uncensored_profile.id.clone(),
        provider: spec.uncensored_profile.provider.clone(),
        model_name: spec.uncensored_profile.model_name.clone(),
        is_dangerous_compatible: spec.uncensored_profile.is_dangerous_compatible,
        ..Default::default()
    }];

    let profile = FinalizerProfile {
        id: spec.connection_profile.id.clone(),
        provider: spec.connection_profile.provider.clone(),
        model_name: spec.connection_profile.model_name.clone(),
    };

    // The real answer-confirmation runner over the canned completion provider.
    // The executor + runner are built PER CALL (below) so each ANSWER_CONFIRMATION
    // `logLLMCall` row carries that call's chatId/messageId; the session no-temp
    // cache is empty in this corpus (no temperature rejection), so a fresh
    // executor per call is equivalent to v4's module-global one.
    let completion = CannedCompletion(completions);

    let compression = NoAsyncCompression;
    let mut cost = NoCostTracking;
    let mut rng_bytes = FixedBytes::new(vec![]);

    let mut got_results: Vec<(String, Value)> = Vec::new();
    let mut got_events: Vec<(String, Value)> = Vec::new();

    for call in &spec.calls {
        let chat_id = call.chat_id.clone();
        let chat_row = db
            .read_main(move |c| chats_read::find_by_id(c, &chat_id))
            .expect("chat read")
            .expect("chat exists");
        // The project override is resolved above the seam (v4
        // `repos.projects.findById(chat.projectId)?.answerConfirmationOverride`).
        // Only the two project-tier cases carry a projectId; the seeded project's
        // override is `ON`.
        let project_override = call.project_id.as_deref().map(|_| "ON");
        let chat = to_finalizer_chat(&chat_row, project_override);

        let participant = chat_row
            .get("participants")
            .and_then(Value::as_array)
            .and_then(|ps| {
                ps.iter()
                    .find(|p| p.get("id").and_then(Value::as_str) == Some(&call.participant_id))
            })
            .cloned()
            .expect("participant exists");
        let character_id = participant
            .get("characterId")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        let character_row = read_character(&db, &character_id);
        let character = FinalizerCharacter {
            id: character_id.clone(),
            name: character_row
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            aliases: character_row
                .get("aliases")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        };
        let finalizer_participant = FinalizerParticipant {
            id: call.participant_id.clone(),
            status: participant
                .get("status")
                .and_then(Value::as_str)
                .map(str::to_string),
        };

        let streaming = FinalizerStreaming {
            full_response: call.reply.clone(),
            usage: Some(quilltap_core::model::stream::StreamUsage {
                prompt_tokens: 5,
                completion_tokens: 7,
                total_tokens: 12,
            }),
            cache_usage: None,
            attachment_results: None,
            raw_response: None,
            thought_signature: None,
            reasoning_content: None,
            reasoning_segments: Vec::new(),
        };

        let chat_settings = Some(FinalizerChatSettings {
            cheap_llm_settings_present: true,
            auto_detect_rng: Some(false),
            answer_confirmation_global_enabled: call.global_enabled,
            danger_mode_off: false,
        });

        // W4.3 confirmation inputs (feature ON).
        let cheap_selection = CheapLlmSelection {
            provider: spec.cheap_selection.provider.clone(),
            model_name: spec.cheap_selection.model_name.clone(),
            base_url: None,
            connection_profile_id: Some(spec.cheap_selection.connection_profile_id.clone()),
            is_local: spec.cheap_selection.is_local,
            profile_parameters: None,
        };
        let reaff_profile = ReaffirmationProfile {
            id: spec.connection_profile.id.clone(),
            provider: spec.connection_profile.provider.clone(),
            model_name: spec.connection_profile.model_name.clone(),
            base_url: spec.connection_profile.base_url.clone(),
            parameters: if spec.connection_profile.parameters.is_null() {
                None
            } else {
                Some(spec.connection_profile.parameters.clone())
            },
            max_context: spec.connection_profile.max_context,
        };
        let confirmation_inputs = FinalizerConfirmationInputs {
            cheap_llm_selection: Some(cheap_selection),
            connection_profile: reaff_profile,
            danger_settings: Some(DangerousContentSettings {
                mode: "AUTO_ROUTE".into(),
                uncensored_text_profile_id: Some(spec.uncensored_profile.id.clone()),
            }),
            available_profiles: available_profiles.clone(),
        };

        let tool_messages: Vec<ToolMessage> = call
            .tool_messages
            .iter()
            .map(|tm| ToolMessage {
                tool_name: tm.tool_name.clone(),
                success: tm.success,
                content: tm.content.clone(),
                arguments: if tm.arguments.is_null() {
                    None
                } else {
                    Some(tm.arguments.clone())
                },
                ..Default::default()
            })
            .collect();

        // Mirrors the oracle's `triggers.participantCharacters` (the responder —
        // a7b1398d: buildRecentConversationContext resolves its name).
        let participant_characters = vec![ParticipantCharacter {
            id: character.id.clone(),
            name: character.name.clone(),
            aliases: character.aliases.clone(),
        }];

        let opts = FinalizeOptions {
            chat_id: call.chat_id.clone(),
            user_id: spec.user_id.clone(),
            chat,
            character,
            character_participant: finalizer_participant,
            user_participant_id: None,
            is_multi_character: false,
            generated_image_paths: Vec::new(),
            tool_messages,
            pre_generated_assistant_message_id: Some(call.assistant_message_id.clone()),
            profile: profile.clone(),
            streaming,
            compression: FinalizerCompression::default(),
            participant_characters,
            chat_settings,
            is_dangerous_chat: call.dangerous,
            connection_profile_id: spec.connection_profile.id.clone(),
            confirmation: confirmation_inputs,
        };

        let sink = RecordingSink::new();
        let mut prospero = ClosureProspero(
            |_a: quilltap_core::services::carina_runner::ProsperoCarinaErrorArgs| Ok(()),
        );
        let mut carina = NoChainCarina;

        // Per-call executor with logging: the check (answer-confirmation) + the
        // re-affirmation (answer-reaffirmation) each write one ANSWER_CONFIRMATION
        // row carrying this call's chatId + messageId (the finalizer passes
        // messageId = the assistant message id, characterId = the responder id).
        let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
            db: db.clone(),
            user_id: spec.user_id.clone(),
            chat_id: Some(call.chat_id.clone()),
            message_id: Some(call.assistant_message_id.clone()),
            ctx: LogContext::none(),
        });
        let confirmation = RealAnswerConfirmation {
            completion: &completion,
            executor: &executor,
        };

        let result = rt
            .block_on(finalize_message_response(
                &db,
                &sink,
                opts,
                &confirmation,
                &compression,
                &mut rng_bytes,
                &mut cost,
                &mut carina,
                &mut prospero,
            ))
            .unwrap_or_else(|e| panic!("finalize {}: {e:?}", call.name));

        got_results.push((call.name.clone(), rust_result_to_value(&result)));
        got_events.push((call.name.clone(), Value::Array(sink.events_json())));
    }

    // The check + re-affirmation calls each wrote one ANSWER_CONFIRMATION row.
    let got_logs = common::dump_llm_logs(&db);

    drop(db);

    // --- 1. results ---
    for (name, got) in &got_results {
        let want_raw = want_results
            .get(name)
            .unwrap_or_else(|| panic!("oracle result missing for {name}"));
        let want = normalize_result(want_raw);
        assert_eq!(got, &want, "result mismatch for call {name}");
    }

    // --- 2. events ---
    for (name, got) in &got_events {
        let want = want_events
            .get(name)
            .unwrap_or_else(|| panic!("oracle events missing for {name}"));
        assert_eq!(got, want, "event trace mismatch for call {name}");
    }

    // --- 3. tables ---
    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("reopen for dump");

    let mut got_chats = dump_table(&db, "chats", "id");
    let mut want_chats = want_tables.get("chats").cloned().expect("oracle chats");
    normalize_vs_prerun(&mut got_chats, &pre_chats, &["updatedAt", "lastMessageAt"]);
    normalize_vs_prerun(&mut want_chats, &pre_chats, &["updatedAt", "lastMessageAt"]);
    canon_numbers(&mut got_chats);
    canon_numbers(&mut want_chats);
    assert_eq!(
        got_chats.get("rows"),
        want_chats.get("rows"),
        "chats rows diverge"
    );

    let mut got_msgs = dump_table(&db, "chat_messages", "id");
    let mut want_msgs = want_tables
        .get("chat_messages")
        .cloned()
        .expect("oracle chat_messages");
    // The assistant + seeded-whisper rows carry PINNED ids (the corpus
    // pre-generates every one); only the TOOL rows persisted by `saveToolMessages`
    // mint a fresh id on each side, so their `id` is placeholdered — the row is
    // identified by its (chatId, role, content). `createdAt` is minted on every
    // new row → placeholdered; the `confirmation*` columns diff exactly. Rows are
    // re-sorted by (chatId, role, content) so the placeholdered-id ordering agrees.
    for dump in [&mut got_msgs, &mut want_msgs] {
        if let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) {
            for row in rows.iter_mut() {
                if let Some(obj) = row.as_object_mut() {
                    if obj.get("role").and_then(Value::as_str) == Some("TOOL") {
                        obj.insert("id".into(), Value::String("<tool-id>".into()));
                    }
                    if obj.get("createdAt").map(|v| !v.is_null()) == Some(true) {
                        obj.insert("createdAt".into(), Value::String("<ts>".into()));
                    }
                }
            }
            rows.sort_by_key(|r| {
                (
                    r.get("chatId")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    r.get("role")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    r.get("content")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                )
            });
        }
        canon_numbers(dump);
    }
    assert_eq!(
        got_msgs.get("rows"),
        want_msgs.get("rows"),
        "chat_messages rows diverge"
    );

    // --- 5. llm_logs (ANSWER_CONFIRMATION rows: one per check + re-affirmation) ---
    let want_logs = want_llm_logs.expect("oracle emitted no llmlogs row");
    assert_eq!(
        got_logs,
        want_logs,
        "llm_logs ANSWER_CONFIRMATION rows diverge (got {} vs oracle {})",
        got_logs.len(),
        want_logs.len()
    );

    drop(db);
    eprintln!(
        "OK: answer-confirmation results + events + chats + chat_messages + llm_logs matched oracle."
    );
}

// ---------------------------------------------------------------------------
// No-op carina query seam (no corpus reply carries carina markup).
// ---------------------------------------------------------------------------

struct NoChainCarina;
impl quilltap_core::services::carina_runner::RunCarinaQuery for NoChainCarina {
    #[allow(clippy::manual_async_fn)]
    fn run(
        &mut self,
        _opts: quilltap_core::services::carina_runner::RunCarinaQueryOptions,
    ) -> impl std::future::Future<
        Output = Result<
            quilltap_core::services::carina_runner::CarinaResult,
            quilltap_core::services::carina_runner::CarinaRunError,
        >,
    > + Send {
        // Never called: no corpus reply parses as carina markup (the finalizer's
        // `run_assistant_carina` only invokes the seam when the parse succeeds).
        async { panic!("carina query seam invoked but no corpus reply carries markup") }
    }
}
