//! Tier-3 differential test: the **message finalizer**
//! (`quilltap_core::services::message_finalizer` — v4
//! `message-finalizer.service.ts` `finalizeMessageResponse` /
//! `calculateNextSpeaker`), verified tier-3 → tier-2.
//!
//! Both sides copy the same two-DB seed fixture (characters + vaults, chats +
//! opener messages, chat settings, one generated-image file) and run the same
//! ten-call finalize sequence. The out-of-scope subsystems are pinned
//! identically: the jest oracle stubs `triggerAsyncCompression` /
//! `detectAndConvertRngPatterns` / `estimateMessageCost` / `runCarinaMarkupQuery`
//! to recorders matching the Rust injected seams (the carina case emits a FIXED
//! `carinaAnswer` message on both sides; the posting path is separately verified
//! by `carina_runner_tier3_equivalence`). Then:
//!
//!   1. each call's `ProcessMessageResult` is compared (v4's rich
//!      `sceneTrackingContext` collapsed to its `characterIds` — the rest is
//!      host-side context the port does not carry);
//!   2. each call's ordered SSE-event trace (decoded to JSON) is compared
//!      byte-for-byte (all ids pinned — the corpus pre-generates every message
//!      id);
//!   3. the recorded compression-trigger + cost-tracking seam calls are compared;
//!   4. the `chats` / `chat_messages` / `background_jobs` / `files` dumps are
//!      compared in a sentinel-aware minted-values form: a timestamp cell that
//!      differs from its PRE-RUN value (both sides share the same fixture bytes)
//!      collapses to `<ts>` — so a missing bump (cell still at the pre-run
//!      value) diverges from the oracle's `<ts>` and is caught; new
//!      `chat_messages` rows placeholder only `createdAt`; `background_jobs`
//!      rows (all minted) are re-sorted by (type, payload) and placeholder
//!      `id` + timestamps.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-mf-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-mf-mount.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-message-finalizer-fixture.ts
//!   QT_FIXTURE_MF_MAIN=/tmp/qt-mf-main.db QT_FIXTURE_MF_MOUNT=/tmp/qt-mf-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-message-finalizer.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- message-finalizer-tier3
//! Run:
//!   QT_ORACLE_MESSAGE_FINALIZER=/tmp/oracle-message-finalizer.ndjson \
//!   QT_FIXTURE_MF_MAIN=/tmp/qt-mf-main.db QT_FIXTURE_MF_MOUNT=/tmp/qt-mf-mount.db \
//!     cargo test -p quilltap-harness --test message_finalizer_tier3_equivalence

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::{characters_read, chats_read};
use quilltap_core::model::stream::StreamUsage;
use quilltap_core::services::carina_runner::{
    CarinaResult, CarinaRunError, ClosureProspero, PostedCarinaMessage, RunCarinaQuery,
    RunCarinaQueryOptions,
};
use quilltap_core::services::chat_events::{ChatEvent, EventSink, RecordingSink};
use quilltap_core::services::message_finalizer::{
    finalize_message_response, AsyncCompressionTrigger, CostTrackArgs, CostTracker,
    FinalizeOptions, FinalizerCharacter, FinalizerChat, FinalizerChatSettings,
    FinalizerCompression, FinalizerParticipant, FinalizerProfile, FinalizerStreaming,
    NoAnswerConfirmation, NoRng, ParticipantCharacter,
};
use quilltap_core::services::primary_stream::ReasoningSegment;
use serde::Deserialize;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Spec.
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CarinaW {
    character_name: String,
    answerer_participant_id: String,
    answerer_character_id: String,
    answerer_name: String,
    message_id: String,
    answer: String,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct UsageW {
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SegW {
    anchor_offset: usize,
    content: String,
    seq: u64,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CompressionW {
    enabled: bool,
    selection: bool,
    system_prompt: bool,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CallW {
    name: String,
    chat_id: String,
    responder_character: String,
    responder_participant: String,
    user_participant_id: Option<String>,
    is_multi_character: bool,
    pre_generated_assistant_message_id: String,
    full_response: String,
    usage: UsageW,
    reasoning_content: Option<String>,
    reasoning_segments: Vec<SegW>,
    generated_image_ids: Vec<String>,
    compression: CompressionW,
    chat_settings: bool,
    participant_characters: Vec<String>,
    #[serde(default)]
    carina: Option<CarinaW>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatSettingsW {
    auto_detect_rng: bool,
    answer_confirmation_enabled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    chat_settings: ChatSettingsW,
    calls: Vec<CallW>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/message-finalizer-tier3.json")
}

// ---------------------------------------------------------------------------
// Seam recorders.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct RecordingCompression {
    calls: Vec<Value>,
}
impl AsyncCompressionTrigger for RecordingCompression {
    fn trigger(&mut self, chat_id: &str, participant_id: Option<&str>) {
        // Mirror the oracle's record: `{chatId, participantId}` where
        // `participantId` is absent when v4 passes `undefined`.
        let mut obj = serde_json::Map::new();
        obj.insert("chatId".into(), json!(chat_id));
        if let Some(p) = participant_id {
            obj.insert("participantId".into(), json!(p));
        }
        self.calls.push(Value::Object(obj));
    }
}

#[derive(Default)]
struct RecordingCost {
    calls: Vec<Value>,
}
impl CostTracker for RecordingCost {
    fn estimate(
        &mut self,
        args: &CostTrackArgs,
    ) -> quilltap_core::services::message_finalizer::CostEstimate {
        // The oracle records `estimateMessageCost`'s args (no profileId — that
        // rides the .then() into trackMessageTokenUsage) and returns a null cost
        // with source 'unavailable' — the aggregate write then increments the
        // token counters only, both sides.
        self.calls.push(json!({
            "provider": args.provider,
            "modelName": args.model_name,
            "promptTokens": args.prompt_tokens,
            "completionTokens": args.completion_tokens,
            "userId": args.user_id,
        }));
        quilltap_core::services::message_finalizer::CostEstimate {
            cost: None,
            source: Some("unavailable".to_string()),
        }
    }
}

/// The fixed carinaAnswer message both sides emit for a `carina` corpus case
/// (must match the oracle's `carinaMessage()` byte-for-byte as parsed JSON).
fn carina_message_json(c: &CarinaW) -> Value {
    json!({
        "id": c.message_id,
        "type": "message",
        "role": "ASSISTANT",
        "content": c.answer,
        "participantId": c.answerer_participant_id,
        "systemSender": "carina",
    })
}

/// The harness `runCarinaQuery` seam: emits the fixed `carinaAnswer` (the seam
/// owns `onPosted`, as v4's real `runCarinaQuery` does) and returns the success
/// result. Writes nothing (the posting path is separately verified).
struct HarnessCarina {
    sink: RecordingSink,
    spec: Option<CarinaW>,
}
impl RunCarinaQuery for HarnessCarina {
    fn run(&mut self, opts: RunCarinaQueryOptions) -> Result<CarinaResult, CarinaRunError> {
        let Some(c) = self.spec.clone() else {
            return Err(CarinaRunError(format!(
                "unexpected carina query for {}",
                opts.character_name
            )));
        };
        assert_eq!(opts.character_name, c.character_name, "carina parse name");
        assert!(!opts.whisper, "corpus carina case is public");
        let msg = carina_message_json(&c);
        self.sink.emit(ChatEvent::carina_answer(msg.clone()));
        Ok(CarinaResult::Ok {
            answer: c.answer.clone(),
            message_id: c.message_id.clone(),
            message: PostedCarinaMessage {
                id: c.message_id.clone(),
                message: msg,
                target_participant_ids: None,
            },
            answerer_id: c.answerer_character_id.clone(),
            answerer_name: c.answerer_name.clone(),
        })
    }
}

// ---------------------------------------------------------------------------
// Marshaling: chat row (chats_read) → FinalizerChat.
// ---------------------------------------------------------------------------

fn str_of(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(String::from)
}

fn to_finalizer_chat(row: &Value) -> FinalizerChat {
    FinalizerChat {
        id: str_of(row, "id").unwrap_or_default(),
        is_paused: row
            .get("isPaused")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        chat_type: str_of(row, "chatType"),
        project_id: str_of(row, "projectId"),
        answer_confirmation_override: str_of(row, "answerConfirmationOverride"),
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
    }
}

fn read_character(db: &Db, id: &str) -> Option<Value> {
    let id = id.to_string();
    db.read_main(|main| db.read_mount_index(|mount| characters_read::find_by_id(main, mount, &id)))
        .expect("character read")
}

fn to_participant_character(row: &Value) -> ParticipantCharacter {
    ParticipantCharacter {
        id: str_of(row, "id").unwrap_or_default(),
        name: str_of(row, "name").unwrap_or_default(),
        aliases: row
            .get("aliases")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

// ---------------------------------------------------------------------------
// Oracle NDJSON.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct OracleLine {
    kind: String,
    #[serde(default)]
    call: Option<String>,
    #[serde(flatten)]
    rest: Value,
}

// ---------------------------------------------------------------------------
// Normalization.
// ---------------------------------------------------------------------------

/// Collapse `sceneTrackingContext` to its `characterIds` (v4's rich object; the
/// port carries only the ids — the rest is host-side context).
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

/// The Rust `ProcessMessageResult` in the same normalized shape.
fn rust_result_to_value(
    r: &quilltap_core::services::message_finalizer::ProcessMessageResult,
) -> Value {
    json!({
        "isMultiCharacter": r.is_multi_character,
        "hasContent": r.has_content,
        "messageId": r.message_id,
        "userParticipantId": r.user_participant_id,
        "isPaused": r.is_paused,
        "sceneTrackingCharacterIds": r.scene_tracking_character_ids,
    })
}

/// Snapshot `{row id → {col → value}}` for the timestamp columns of a dump.
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

/// Sentinel-aware placeholdering: a timestamp cell that differs from its pre-run
/// value collapses to `<ts>`; a cell still at the pre-run value stays literal
/// (so a missing bump is caught). A row absent from the pre-run snapshot (a new
/// row) placeholders every listed column.
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

/// background_jobs rows are all minted: re-sort by (type, payload) — payloads are
/// deterministic (pinned chat/message ids) — then placeholder `id` and the
/// timestamp columns.
fn normalize_background_jobs(dump: &mut Value) {
    if let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) {
        rows.sort_by_key(|r| {
            (
                r.get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                r.get("payload")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            )
        });
        for row in rows {
            if let Some(obj) = row.as_object_mut() {
                obj.insert("id".into(), Value::String("<id>".into()));
                for c in ["scheduledAt", "createdAt", "updatedAt"] {
                    if !obj.get(c).map(Value::is_null).unwrap_or(true) {
                        obj.insert(c.to_string(), Value::String("<ts>".into()));
                    }
                }
            }
        }
    }
}

fn dump_table(db: &Db, table: &str, order_by: &str) -> Value {
    let table = table.to_string();
    let order_by = order_by.to_string();
    db.read_main(move |c| quilltap_core::db::dump_table_json_conn(c, &table, &order_by))
        .expect("table dump")
}

// ---------------------------------------------------------------------------
// The test.
// ---------------------------------------------------------------------------

#[test]
fn message_finalizer_tier3_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_MESSAGE_FINALIZER") else {
        eprintln!("QT_ORACLE_MESSAGE_FINALIZER not set; skipping");
        return;
    };
    let Ok(fixture_main) = std::env::var("QT_FIXTURE_MF_MAIN") else {
        eprintln!("QT_FIXTURE_MF_MAIN not set; skipping");
        return;
    };
    let Ok(fixture_mount) = std::env::var("QT_FIXTURE_MF_MOUNT") else {
        eprintln!("QT_FIXTURE_MF_MOUNT not set; skipping");
        return;
    };

    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("spec file readable"))
            .expect("spec parses");

    // Parse the oracle NDJSON.
    let oracle_text = std::fs::read_to_string(&oracle_path).expect("oracle readable");
    let mut want_results: HashMap<String, Value> = HashMap::new();
    let mut want_events: HashMap<String, Value> = HashMap::new();
    let mut want_compression: Vec<Value> = Vec::new();
    let mut want_cost: Vec<Value> = Vec::new();
    let mut want_tables: HashMap<String, Value> = HashMap::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let parsed: OracleLine = serde_json::from_str(line).expect("oracle line parses");
        match parsed.kind.as_str() {
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
            "compression" => {
                let mut obj = parsed.rest.as_object().cloned().unwrap_or_default();
                obj.remove("kind");
                // v4 records `participantId: undefined` as an absent key already
                // (JSON.stringify drops it); nothing further to strip.
                want_compression.push(Value::Object(obj));
            }
            "cost" => {
                let mut obj = parsed.rest.as_object().cloned().unwrap_or_default();
                obj.remove("kind");
                want_cost.push(Value::Object(obj));
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
    let scratch = std::env::temp_dir().join(format!("qt-mf-harness-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let work_main = scratch.join("mf-main.db");
    let work_mount = scratch.join("mf-mount.db");
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(&fixture_main, &work_main).expect("copy main fixture");
    std::fs::copy(&fixture_mount, &work_mount).expect("copy mount fixture");

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open fixture instance");

    // Pre-run snapshots for the sentinel-aware normalization.
    let pre_chats = snapshot_cells(
        &dump_table(&db, "chats", "id"),
        &["updatedAt", "lastMessageAt"],
    );
    let pre_files = snapshot_cells(&dump_table(&db, "files", "id"), &["updatedAt"]);
    let pre_msg_ids: HashSet<String> = dump_table(&db, "chat_messages", "id")
        .get("rows")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|r| r.get("id").and_then(Value::as_str).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let profile = FinalizerProfile {
        id: "profile-mf-0000-0000-000000000001".into(),
        provider: "ANTHROPIC".into(),
        model_name: "claude-sonnet".into(),
    };

    let mut compression_rec = RecordingCompression::default();
    let mut cost_rec = RecordingCost::default();

    let mut got_results: Vec<(String, Value)> = Vec::new();
    let mut got_events: Vec<(String, Value)> = Vec::new();

    for call in &spec.calls {
        // Read the chat + characters fresh (as the v4 oracle does per call).
        let chat_id = call.chat_id.clone();
        let chat_row = db
            .read_main(move |c| chats_read::find_by_id(c, &chat_id))
            .expect("chat read")
            .expect("chat exists");
        let chat = to_finalizer_chat(&chat_row);

        let character_row =
            read_character(&db, &call.responder_character).expect("responder character");
        let character = FinalizerCharacter {
            id: str_of(&character_row, "id").unwrap(),
            name: str_of(&character_row, "name").unwrap(),
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

        let participant_status = chat
            .participants
            .iter()
            .find(|p| {
                p.get("id").and_then(Value::as_str) == Some(call.responder_participant.as_str())
            })
            .and_then(|p| p.get("status").and_then(Value::as_str))
            .map(String::from);
        let character_participant = FinalizerParticipant {
            id: call.responder_participant.clone(),
            status: participant_status,
        };

        let participant_characters: Vec<ParticipantCharacter> = call
            .participant_characters
            .iter()
            .filter_map(|cid| read_character(&db, cid))
            .map(|row| to_participant_character(&row))
            .collect();

        let streaming = FinalizerStreaming {
            full_response: call.full_response.clone(),
            usage: Some(StreamUsage {
                prompt_tokens: call.usage.prompt_tokens,
                completion_tokens: call.usage.completion_tokens,
                total_tokens: call.usage.total_tokens,
            }),
            cache_usage: None,
            attachment_results: None,
            raw_response: None,
            thought_signature: None,
            reasoning_content: call.reasoning_content.clone(),
            reasoning_segments: call
                .reasoning_segments
                .iter()
                .map(|s| ReasoningSegment {
                    anchor_offset: s.anchor_offset,
                    content: s.content.clone(),
                    seq: s.seq,
                })
                .collect(),
        };

        let compression = FinalizerCompression {
            compression_enabled: call.compression.enabled,
            cheap_llm_selection_present: call.compression.selection,
            original_system_prompt_present: call.compression.system_prompt,
        };

        let chat_settings = if call.chat_settings {
            Some(FinalizerChatSettings {
                cheap_llm_settings_present: true,
                auto_detect_rng: Some(spec.chat_settings.auto_detect_rng),
                answer_confirmation_global_enabled: spec.chat_settings.answer_confirmation_enabled,
            })
        } else {
            None
        };

        let sink = RecordingSink::new();
        let mut carina_seam = HarnessCarina {
            sink: sink.clone(),
            spec: call.carina.clone(),
        };
        let mut prospero = ClosureProspero(|_a| Ok(()));
        let mut confirmation = NoAnswerConfirmation;
        let mut rng = NoRng;

        let opts = FinalizeOptions {
            chat_id: call.chat_id.clone(),
            user_id: spec.user_id.clone(),
            chat,
            character,
            character_participant,
            user_participant_id: call.user_participant_id.clone(),
            is_multi_character: call.is_multi_character,
            generated_image_ids: call.generated_image_ids.clone(),
            pre_generated_assistant_message_id: Some(
                call.pre_generated_assistant_message_id.clone(),
            ),
            profile: profile.clone(),
            streaming,
            compression,
            participant_characters,
            chat_settings,
            is_dangerous_chat: false,
        };

        let result = rt
            .block_on(finalize_message_response(
                &db,
                &sink,
                opts,
                &mut confirmation,
                &mut compression_rec,
                &mut rng,
                &mut cost_rec,
                &mut carina_seam,
                &mut prospero,
            ))
            .unwrap_or_else(|e| panic!("finalize {} failed: {e:?}", call.name));

        got_results.push((call.name.clone(), rust_result_to_value(&result)));
        got_events.push((call.name.clone(), Value::Array(sink.events_json())));
    }

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

    // --- 3. seam records ---
    assert_eq!(
        compression_rec.calls, want_compression,
        "compression-trigger seam records mismatch"
    );
    assert_eq!(
        cost_rec.calls, want_cost,
        "cost-tracking seam records mismatch"
    );

    // --- 4. table dumps ---
    // chats: sentinel-aware minted updatedAt / lastMessageAt.
    let mut got_chats = dump_table(&db, "chats", "id");
    let mut want_chats = want_tables.remove("chats").expect("oracle chats dump");
    normalize_vs_prerun(&mut got_chats, &pre_chats, &["updatedAt", "lastMessageAt"]);
    normalize_vs_prerun(&mut want_chats, &pre_chats, &["updatedAt", "lastMessageAt"]);
    assert_table_eq("chats", &got_chats, &want_chats);

    // chat_messages: new rows placeholder createdAt only (ids all pinned).
    let mut got_msgs = dump_table(&db, "chat_messages", "id");
    let mut want_msgs = want_tables
        .remove("chat_messages")
        .expect("oracle chat_messages dump");
    let pre_msgs: HashMap<String, HashMap<String, Value>> = pre_msg_ids
        .iter()
        .map(|id| {
            // Seed rows: mark createdAt as "unchanged" by snapshotting the actual
            // current value — the seed rows are pinned so both sides carry the
            // fixture value literally; only rows ABSENT from this set placeholder.
            (id.clone(), HashMap::new())
        })
        .collect();
    normalize_new_row_created_at(&mut got_msgs, &pre_msgs);
    normalize_new_row_created_at(&mut want_msgs, &pre_msgs);
    assert_table_eq("chat_messages", &got_msgs, &want_msgs);

    // background_jobs: all minted — re-sort + placeholder ids/timestamps.
    let mut got_jobs = dump_table(&db, "background_jobs", "id");
    let mut want_jobs = want_tables
        .remove("background_jobs")
        .expect("oracle background_jobs dump");
    normalize_background_jobs(&mut got_jobs);
    normalize_background_jobs(&mut want_jobs);
    assert_table_eq("background_jobs", &got_jobs, &want_jobs);

    // files: sentinel-aware minted updatedAt (the addLink bump).
    let mut got_files = dump_table(&db, "files", "id");
    let mut want_files = want_tables.remove("files").expect("oracle files dump");
    normalize_vs_prerun(&mut got_files, &pre_files, &["updatedAt"]);
    normalize_vs_prerun(&mut want_files, &pre_files, &["updatedAt"]);
    assert_table_eq("files", &got_files, &want_files);

    // Clean up scratch.
    drop(db);
    let _ = std::fs::remove_dir_all(&scratch);
}

/// Placeholder `createdAt` on rows whose id is NOT in the pre-run set (i.e. rows
/// this run inserted, whose `createdAt` is clock-minted).
fn normalize_new_row_created_at(dump: &mut Value, pre: &HashMap<String, HashMap<String, Value>>) {
    if let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) {
        for row in rows {
            let id = row
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if !pre.contains_key(&id) {
                if let Some(obj) = row.as_object_mut() {
                    obj.insert("createdAt".into(), Value::String("<ts>".into()));
                }
            }
        }
    }
}

fn assert_table_eq(name: &str, got: &Value, want: &Value) {
    let got_rows = got.get("rows").cloned().unwrap_or(Value::Null);
    let want_rows = want.get("rows").cloned().unwrap_or(Value::Null);
    if got_rows != want_rows {
        let g = serde_json::to_string_pretty(&got_rows).unwrap();
        let w = serde_json::to_string_pretty(&want_rows).unwrap();
        panic!("table {name} mismatch\n--- got ---\n{g}\n--- want ---\n{w}");
    }
}
