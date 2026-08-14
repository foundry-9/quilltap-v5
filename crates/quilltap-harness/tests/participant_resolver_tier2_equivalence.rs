//! Tier-2 differential test: the participant resolver (v4
//! `lib/services/chat-message/participant-resolver.service.ts`
//! `resolveRespondingParticipant` / `loadAllParticipantData` /
//! `getRoleplayTemplate`, plus `lib/chat/connection-resolver.ts`), ported as
//! `quilltap_core::services::participant_resolver`.
//!
//! Both sides run the SAME op sequence (`participant-resolver.json`) on a fresh
//! copy of the two-database seed fixture (a main DB with connection profiles +
//! roleplay templates + slim characters + chats, and a mount-index DB carrying a
//! real project's document store), then diff (a) each op's result projection —
//! or the thrown flag/message for the throw cases — and (b) the final `chats`
//! dump (the one write: `getRoleplayTemplate` persisting an inherited default).
//!
//! This proves: the requested-participant path (found; continue-mode not-found
//! THROW with the exact message; normal-mode fallback); the zero/one/multiple
//! LLM-candidate paths (multiple through weighted selection, RNG injected); the
//! missing-character / no-active-character throw; connection-profile resolution
//! success + the two failure throws (no-config; not-found); `loadAllParticipantData`
//! reusing the primary; and `getRoleplayTemplate`'s chat-set / project-inherited
//! (WRITE) / settings-inherited / none chain.
//!
//! P4.D65 adds the ARCHIVED pair (the banked P4.D63 unit-4 arm) over a chat
//! seating one archived character beside a live one: REQUESTED by participant
//! id, the archived seat throws `CharacterArchivedError` — the requested path is
//! the only way in, because `selectNextSpeaker` drops archived seats before they
//! can ever be picked; with nothing requested, that same chat resolves to the
//! live seat, which is what proves the selection filter rather than assuming it.
//!
//! NORMALIZATION: none. The seed pins every id + timestamp; `getRoleplayTemplate`'s
//! persist writes only `roleplayTemplateId` (updatedAt preserved — v4 re-passes
//! existing), so the final `chats` dump and every result projection are compared
//! exactly.
//!
//! INJECTION: `Math.random` is pinned to each op's `random01` on the v4 side; the
//! identical value is threaded into the Rust selection.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-partres-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-partres-mount.db \
//!     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-participant-resolver-fixture.ts
//!   QT_FIXTURE_PARTRES=/tmp/qt-partres-main.db \
//!   QT_FIXTURE_PARTRES_MOUNT=/tmp/qt-partres-mount.db \
//!     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/cases/participant-resolver.ts > /tmp/oracle-partres.ndjson
//! Run:
//!   QT_ORACLE_PARTRES=/tmp/oracle-partres.ndjson \
//!   QT_FIXTURE_PARTRES=/tmp/qt-partres-main.db \
//!   QT_FIXTURE_PARTRES_MOUNT=/tmp/qt-partres-mount.db \
//!     cargo test -p quilltap-harness --test participant_resolver_tier2_equivalence

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::{chats_read, dump_table_json_conn};
use quilltap_core::services::participant_resolver::{
    get_roleplay_template, load_all_participant_data, resolve_responding_participant,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "resolveResponding")]
    ResolveResponding {
        #[serde(rename = "chatId")]
        chat_id: String,
        #[serde(rename = "requestedRespondingParticipantId")]
        requested: Option<String>,
        #[serde(rename = "isContinueMode")]
        is_continue_mode: bool,
        #[serde(rename = "activeUserParticipantId")]
        active_user_participant_id: Option<String>,
        random01: f64,
    },
    #[serde(rename = "loadAllParticipantData")]
    LoadAllParticipantData {
        #[serde(rename = "chatId")]
        chat_id: String,
        #[serde(rename = "primaryCharacterId")]
        primary_character_id: String,
    },
    #[serde(rename = "getRoleplayTemplate")]
    GetRoleplayTemplate {
        #[serde(rename = "chatId")]
        chat_id: String,
        #[serde(rename = "settingsDefaultRoleplayTemplateId")]
        settings_default: Option<String>,
    },
}

fn load_chat(db: &Db, chat_id: &str) -> Value {
    let chat_id = chat_id.to_string();
    db.read_main(move |conn| chats_read::find_by_id(conn, &chat_id))
        .unwrap_or_else(|e| panic!("find chat: {e:?}"))
        .unwrap_or_else(|| panic!("chat not found"))
}

#[tokio::test]
async fn participant_resolver_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_PARTRES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PARTRES to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_PARTRES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_PARTRES to the main seed fixture .db (see header).");
            return;
        }
    };
    let fixture_mount = match std::env::var("QT_FIXTURE_PARTRES_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_PARTRES_MOUNT to the mount seed fixture .db.");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/participant-resolver.json"),
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
    let oracle_results = oracle["results"].as_array().expect("oracle results array");

    let pid = std::process::id();
    let work_main = std::env::temp_dir().join(format!("qt-partres-rust-main-{pid}.db"));
    let work_mount = std::env::temp_dir().join(format!("qt-partres-rust-mount-{pid}.db"));
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(&fixture, &work_main).unwrap_or_else(|e| panic!("copy main fixture: {e}"));
    std::fs::copy(&fixture_mount, &work_mount)
        .unwrap_or_else(|e| panic!("copy mount fixture: {e}"));

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copies: {e}"));

    let mut got_results: Vec<Value> = Vec::new();

    for op in &spec.ops {
        match op {
            Op::ResolveResponding {
                chat_id,
                requested,
                is_continue_mode,
                active_user_participant_id,
                random01,
            } => {
                let chat = load_chat(&db, chat_id);
                match resolve_responding_participant(
                    &db,
                    &chat,
                    &spec.user_id,
                    requested.as_deref(),
                    *is_continue_mode,
                    active_user_participant_id.as_deref(),
                    *random01,
                )
                .await
                {
                    Ok(r) => got_results.push(json!({
                        "op": "resolveResponding",
                        "chatId": chat_id,
                        "threw": false,
                        "characterParticipantId": r.character_participant.get("id").and_then(Value::as_str),
                        "characterId": r.character.get("id").and_then(Value::as_str),
                        "characterName": r.character.get("name").and_then(Value::as_str),
                        "connectionProfileId": r.connection_profile.get("id").and_then(Value::as_str),
                        "imageProfileId": r.image_profile_id,
                        "userParticipantId": r.user_participant_id,
                        "isMultiCharacter": r.is_multi_character,
                    })),
                    Err(e) => got_results.push(json!({
                        "op": "resolveResponding",
                        "chatId": chat_id,
                        "threw": true,
                        "message": e.to_string(),
                    })),
                }
            }
            Op::LoadAllParticipantData {
                chat_id,
                primary_character_id,
            } => {
                let chat = load_chat(&db, chat_id);
                let primary = {
                    let cid = primary_character_id.clone();
                    db.read_main(|main| {
                        db.read_mount_index(|mount| {
                            quilltap_core::db::characters_read::find_by_id(main, mount, &cid)
                        })
                    })
                }
                .unwrap_or_else(|e| panic!("find primary: {e:?}"))
                .unwrap_or_else(|| panic!("primary character not found"));

                let data = load_all_participant_data(&db, &chat, &primary)
                    .unwrap_or_else(|e| panic!("loadAllParticipantData: {e:?}"));
                let mut ids: Vec<String> = data.keys().cloned().collect();
                ids.sort();
                got_results.push(json!({
                    "op": "loadAllParticipantData",
                    "chatId": chat_id,
                    "characterIds": ids,
                }));
            }
            Op::GetRoleplayTemplate {
                chat_id,
                settings_default,
            } => {
                let chat = load_chat(&db, chat_id);
                let rt = get_roleplay_template(&db, &chat, settings_default.as_deref())
                    .await
                    .unwrap_or_else(|e| panic!("getRoleplayTemplate: {e:?}"));
                got_results.push(json!({
                    "op": "getRoleplayTemplate",
                    "chatId": chat_id,
                    "systemPrompt": rt.map(|r| r.system_prompt),
                }));
            }
        }
    }

    assert_eq!(
        got_results.len(),
        oracle_results.len(),
        "op-count mismatch — regenerate the oracle NDJSON"
    );
    for (i, (got, want)) in got_results.iter().zip(oracle_results.iter()).enumerate() {
        assert_eq!(
            got, want,
            "op {i} result diverged\n  rust:   {got}\n  oracle: {want}"
        );
    }

    let got_dump = db
        .read_main(|conn| dump_table_json_conn(conn, "chats", "id"))
        .expect("dump chats");
    let oracle_dump = &oracle["dump"];

    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);

    assert_eq!(got_dump["table"], oracle_dump["table"], "table name");
    assert_eq!(
        got_dump["columns"], oracle_dump["columns"],
        "column set / order"
    );
    assert_eq!(
        got_dump["rows"], oracle_dump["rows"],
        "chats row state diverged\n  rust:   {}\n  oracle: {}",
        got_dump["rows"], oracle_dump["rows"]
    );

    eprintln!(
        "OK: participant-resolver tier-2 matched oracle ({} ops, {} chats).",
        got_results.len(),
        got_dump["rows"].as_array().map(|a| a.len()).unwrap_or(0)
    );
}
