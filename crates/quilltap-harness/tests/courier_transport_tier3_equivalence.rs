//! Tier-3 differential test: the **Courier transport**
//! (`quilltap_core::services::courier_transport::dispatch_courier_transport` — v4
//! `lib/services/chat-message/courier-transport.service.ts`).
//!
//! Both sides copy the same two-DB seed fixture (two characters + vaults, one
//! chat-settings row, one file, three chats — one with a delta checkpoint) and run
//! the same four-call dispatch sequence. The dispatch calls NO model: it renders
//! the assembled `formattedMessages` as Markdown, persists a placeholder ASSISTANT
//! message, pauses the chat, and emits the `pendingExternalTurn` + `done` frames.
//!
//! Per call we compare, with the minted placeholder id normalized to `<id>`:
//!   - the returned `ProcessMessageResult`;
//!   - the ordered SSE-event trace;
//!   - the persisted placeholder row's `role`/`content`/`participantId`/`provider`/
//!     `modelName` + the three `pendingExternal*` columns (the rendered-bundle
//!     BYTES — proving the renderers + delta events byte-for-byte);
//!   - the chat's `isPaused`.
//!
//! The paste/cancel resolvers live in a Next.js route handler (not exported), so
//! their differential is deferred to Phase-4 HTTP transport; the ported service
//! functions (`resolve_external_turn` / `cancel_external_turn`) compose
//! tier-2/tier-3-proven repo ops and are Rust-unit-tested in the core module.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout; `.claude` copy):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; TMPO=/tmp/qt-oracle-w444
//!   cp -R <worktree>/harness/oracle $TMPO/oracle
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-cour-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-cour-mount.db \
//!     $N/npx tsx $TMPO/oracle/fixtures/build-courier-transport-fixture.ts
//!   QT_FIXTURE_COUR_MAIN=/tmp/qt-cour-main.db QT_FIXTURE_COUR_MOUNT=/tmp/qt-cour-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-courier.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/oracle/cases" -- courier-transport-tier3
//! Run:
//!   QT_ORACLE_COURIER=/tmp/oracle-courier.ndjson \
//!   QT_FIXTURE_COUR_MAIN=/tmp/qt-cour-main.db QT_FIXTURE_COUR_MOUNT=/tmp/qt-cour-mount.db \
//!     cargo test -p quilltap-harness --test courier_transport_tier3_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::{characters_read, chats_messages_read, chats_read};
use quilltap_core::services::chat_events::RecordingSink;
use quilltap_core::services::courier_transport::{
    dispatch_courier_transport, DispatchCourierOptions,
};
use quilltap_core::services::message_context::FormattedMsg;
use serde::Deserialize;
use serde_json::{json, Value};

const NOW_MS: i64 = 1_625_659_200_000; // 2021-07-07T12:00:00.000Z

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AttachmentSpec {
    id: String,
    #[serde(default)]
    filename: Option<String>,
    #[serde(default)]
    mime_type: Option<String>,
    #[serde(default)]
    size: Option<f64>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct FormattedMsgSpec {
    role: String,
    content: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    attachments: Vec<AttachmentSpec>,
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
    participant_characters: Vec<String>,
    courier_delta_mode: bool,
    provider: String,
    model_name: String,
    formatted_messages: Vec<FormattedMsgSpec>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    resolved_identity_name: String,
    calls: Vec<CallW>,
}

#[derive(Deserialize)]
struct OracleLine {
    kind: String,
    call: String,
    #[serde(flatten)]
    rest: Value,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/courier-transport-tier3.json")
}

fn to_formatted_msg(m: &FormattedMsgSpec) -> FormattedMsg {
    let attachments = if m.attachments.is_empty() {
        None
    } else {
        Some(
            m.attachments
                .iter()
                .map(|a| {
                    json!({
                        "id": a.id,
                        "filename": a.filename,
                        "mimeType": a.mime_type,
                        "size": a.size,
                    })
                })
                .collect(),
        )
    };
    FormattedMsg {
        role: m.role.clone(),
        content: m.content.clone(),
        name: m.name.clone(),
        thought_signature: None,
        attachments,
    }
}

/// Normalize the minted placeholder id (and only that id) to `<id>` inside a
/// `messageId` field.
fn norm_message_id(mut v: Value, minted: &str) -> Value {
    if let Some(obj) = v.as_object_mut() {
        if obj.get("messageId").and_then(Value::as_str) == Some(minted) {
            obj.insert("messageId".into(), json!("<id>"));
        }
    }
    v
}

#[test]
fn courier_transport_tier3_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_COURIER") else {
        eprintln!("QT_ORACLE_COURIER not set; skipping");
        return;
    };
    let Ok(fixture_main) = std::env::var("QT_FIXTURE_COUR_MAIN") else {
        eprintln!("QT_FIXTURE_COUR_MAIN not set; skipping");
        return;
    };
    let Ok(fixture_mount) = std::env::var("QT_FIXTURE_COUR_MOUNT") else {
        eprintln!("QT_FIXTURE_COUR_MOUNT not set; skipping");
        return;
    };

    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("spec file readable"))
            .expect("spec parses");

    // Parse the oracle NDJSON, keyed by (call, kind).
    let oracle_text = std::fs::read_to_string(&oracle_path).expect("oracle readable");
    let mut want: HashMap<(String, String), Value> = HashMap::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let parsed: OracleLine = serde_json::from_str(line).expect("oracle line parses");
        want.insert((parsed.call.clone(), parsed.kind.clone()), parsed.rest);
    }

    // Copy the fixture DBs to a scratch dir.
    let scratch = std::env::temp_dir().join(format!("qt-cour-harness-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let work_main = scratch.join("cour-main.db");
    let work_mount = scratch.join("cour-mount.db");
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

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    for call in &spec.calls {
        let chat_id = call.chat_id.clone();
        let chat = db
            .read_main(move |c| chats_read::find_by_id(c, &chat_id))
            .expect("read chat")
            .unwrap_or_else(|| panic!("chat not found: {}", call.chat_id));
        let responder = call.responder_character.clone();
        let character = db
            .read_main(move |c| characters_read::find_by_id_raw(c, &responder))
            .expect("read character")
            .unwrap_or_else(|| panic!("character not found: {}", call.responder_character));
        let character_participant = chat
            .get("participants")
            .and_then(Value::as_array)
            .and_then(|ps| {
                ps.iter()
                    .find(|p| p.get("id").and_then(Value::as_str) == Some(&call.responder_participant))
            })
            .cloned()
            .unwrap_or_else(|| panic!("responder participant not found: {}", call.responder_participant));

        let mut participant_characters: HashMap<String, Value> = HashMap::new();
        for cid in &call.participant_characters {
            let cid_owned = cid.clone();
            if let Some(c) = db
                .read_main(move |conn| characters_read::find_by_id_raw(conn, &cid_owned))
                .expect("read participant character")
            {
                participant_characters.insert(cid.clone(), c);
            }
        }

        let sink = RecordingSink::new();
        let formatted_messages: Vec<FormattedMsg> =
            call.formatted_messages.iter().map(to_formatted_msg).collect();

        let result = rt
            .block_on(dispatch_courier_transport(
                &db,
                &sink,
                DispatchCourierOptions {
                    chat_id: call.chat_id.clone(),
                    chat: chat.clone(),
                    character: character.clone(),
                    character_participant: character_participant.clone(),
                    user_participant_id: call.user_participant_id.clone(),
                    is_multi_character: call.is_multi_character,
                    participant_characters,
                    resolved_identity_name: spec.resolved_identity_name.clone(),
                    formatted_messages,
                    effective_provider: Some(call.provider.clone()),
                    effective_model_name: Some(call.model_name.clone()),
                    courier_delta_mode: call.courier_delta_mode,
                    now_ms: NOW_MS,
                },
            ))
            .unwrap_or_else(|e| panic!("dispatch {} failed: {e:?}", call.name));

        let minted = result.message_id.clone();

        // --- result ---
        let got_result = norm_message_id(
            json!({
                "isMultiCharacter": result.is_multi_character,
                "hasContent": result.has_content,
                "messageId": result.message_id,
                "userParticipantId": result.user_participant_id,
                "isPaused": result.is_paused,
            }),
            &minted,
        );
        let want_result = want
            .get(&(call.name.clone(), "result".to_string()))
            .and_then(|v| v.get("result"))
            .cloned()
            .unwrap_or_else(|| panic!("oracle result missing for {}", call.name));
        assert_eq!(got_result, want_result, "result mismatch for {}", call.name);

        // --- events ---
        let got_events: Vec<Value> = sink
            .events_json()
            .into_iter()
            .map(|e| norm_message_id(e, &minted))
            .collect();
        let want_events = want
            .get(&(call.name.clone(), "events".to_string()))
            .and_then(|v| v.get("events"))
            .cloned()
            .unwrap_or_else(|| panic!("oracle events missing for {}", call.name));
        assert_eq!(
            Value::Array(got_events),
            want_events,
            "event trace mismatch for {}",
            call.name
        );

        // --- placeholder row ---
        let chat_id2 = call.chat_id.clone();
        let messages = db
            .read_main(move |c| chats_messages_read::get_messages(c, &chat_id2))
            .expect("read messages");
        let placeholder = messages
            .iter()
            .find(|m| m.get("id").and_then(Value::as_str) == Some(minted.as_str()))
            .unwrap_or_else(|| panic!("placeholder not persisted: {minted}"));
        let got_row = json!({
            "role": placeholder.get("role").cloned().unwrap_or(Value::Null),
            "content": placeholder.get("content").cloned().unwrap_or(Value::Null),
            "participantId": placeholder.get("participantId").cloned().unwrap_or(Value::Null),
            "provider": placeholder.get("provider").cloned().unwrap_or(Value::Null),
            "modelName": placeholder.get("modelName").cloned().unwrap_or(Value::Null),
            "pendingExternalPrompt": placeholder.get("pendingExternalPrompt").cloned().unwrap_or(Value::Null),
            "pendingExternalPromptFull": placeholder.get("pendingExternalPromptFull").cloned().unwrap_or(Value::Null),
            "pendingExternalAttachments": placeholder.get("pendingExternalAttachments").cloned().unwrap_or(Value::Null),
        });
        let want_row = want
            .get(&(call.name.clone(), "placeholder".to_string()))
            .and_then(|v| v.get("row"))
            .cloned()
            .unwrap_or_else(|| panic!("oracle placeholder missing for {}", call.name));
        assert_eq!(got_row, want_row, "placeholder row mismatch for {}", call.name);

        // --- chat isPaused ---
        let chat_id3 = call.chat_id.clone();
        let fresh = db
            .read_main(move |c| chats_read::find_by_id(c, &chat_id3))
            .expect("re-read chat");
        let is_paused = fresh
            .as_ref()
            .and_then(|c| c.get("isPaused").and_then(Value::as_bool))
            .unwrap_or(false);
        let want_paused = want
            .get(&(call.name.clone(), "chat".to_string()))
            .and_then(|v| v.get("isPaused").and_then(Value::as_bool))
            .unwrap_or_else(|| panic!("oracle chat missing for {}", call.name));
        assert_eq!(is_paused, want_paused, "isPaused mismatch for {}", call.name);
    }
}
