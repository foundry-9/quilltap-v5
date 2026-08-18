//! Tier-3 differential: the Post Office persona-writer POST functions + the
//! cost/system-event writers (W4.6b).
//!
//! Both sides run the SAME sequence of persona-writer posts (concierge / prospero /
//! host / aurora / suparna / commonplace / librarian) plus two cost system-event
//! writers against a COPY of the same two-DB fixture, then diff:
//!   1. `chat_messages` — every persona message ROW byte-for-byte (systemSender /
//!      systemKind / role / participantId / opaqueContent / hostEvent /
//!      targetParticipantIds / summaryAnchor / attachments), plus the two SYSTEM
//!      cost rows;
//!   2. `chats` — the token aggregate the cost events bump
//!      (totalPromptTokens / totalCompletionTokens / estimatedCostUSD).
//!
//! The persona `content` builders are already tier-1-proven; this verifies the
//! marshaling the post functions assemble + persist through `add_message`. v4's
//! REAL `post*` writers are driven by the oracle (`harness/oracle/cases/
//! post-office-writers-tier3.ts`); the ported functions are driven here.
//!
//! Normalization: `chat_messages` — the minted `id` tokenized to `<id-N>` in row
//! order (dump ordered by `content`; every persona body distinct, the SYSTEM rows'
//! NULL content ties at the front preserving insertion order), `createdAt` → `<ts>`.
//! Everything else (chatId, pinned participant ids inside targetParticipantIds /
//! hostEvent, systemKind, opaqueContent, token cells) compares exactly. `chats` —
//! the minted `updatedAt` / `lastMessageAt` → `<ts>`; the rest (token aggregates,
//! participants, messageCount) compares exactly.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_POW_MAIN=/tmp/qt-pow-main.db QT_FIXTURE_POW_MOUNT=/tmp/qt-pow-mount.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-post-office-writers-fixture.ts
//!   QT_FIXTURE_POW_MAIN=/tmp/qt-pow-main.db QT_FIXTURE_POW_MOUNT=/tmp/qt-pow-mount.db \
//!     $N/npx tsx $V5/harness/oracle/cases/post-office-writers-tier3.ts > /tmp/oracle-pow.ndjson
//! Run:
//!   QT_ORACLE_POW=/tmp/oracle-pow.ndjson \
//!   QT_FIXTURE_POW_MAIN=/tmp/qt-pow-main.db QT_FIXTURE_POW_MOUNT=/tmp/qt-pow-mount.db \
//!     cargo test -p quilltap-harness --test post_office_writers_tier3_equivalence

use std::collections::HashMap;

use quilltap_core::db::characters_read;
use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::aurora_notifications::{
    post_core_whisper, post_outfit_change_whisper,
};
use quilltap_core::services::commonplace_notifications::{
    post_commonplace_whisper, CommonplaceWhisperKind, PostCommonplaceParams,
};
use quilltap_core::services::concierge_notifications::{
    post_concierge_danger_announcement, post_concierge_manual_announcement, ConciergeCategory,
    ConciergeDangerDetails, ConciergeManualKind,
};
use quilltap_core::services::cost_events::{
    create_context_summary_event, create_title_generation_event, TokenUsage,
};
use quilltap_core::services::host_notifications::{
    post_host_add_announcement, post_host_off_scene_characters_announcement,
    post_host_scenario_announcement, post_host_status_change_announcement,
    post_host_turn_pass_announcement, HostAddAnnouncement, HostCharacter,
    HostOffSceneCharactersAnnouncement, HostScenarioAnnouncement, HostStatusChangeAnnouncement,
    HostTurnPassAnnouncement, OffSceneCharacterCard, TurnPassSource,
};
use quilltap_core::services::librarian_notifications::{
    post_librarian_open_announcement, post_librarian_summary_announcement,
    LibrarianOpenAnnouncement, LibrarianOpenKind, LibrarianScope, LibrarianSummaryAnnouncement,
};
use quilltap_core::services::prospero_notifications::{
    post_prospero_connection_profile_change_announcement,
    ProsperoConnectionProfileChangeAnnouncement,
};
use quilltap_core::services::suparna_notifications::{
    post_suparna_mail_whisper, PostSuparnaMailWhisperParams,
};
use quilltap_core::wardrobe::OutfitSlotValues;
use serde::Deserialize;
use serde_json::Value;

// ---------------------------------------------------------------------------
// Spec structures (harness/oracle/fixtures/post-office-writers-tier3.json).
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatSpec {
    category: String,
    score: f64,
    #[serde(default)]
    label: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DangerDetailsSpec {
    score: f64,
    threshold: f64,
    categories: Vec<CatSpec>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    provider_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PronounsSpec {
    subject: String,
    object: String,
    possessive: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OffSceneSpec {
    id: String,
    name: String,
    #[serde(default)]
    aliases: Option<Vec<String>>,
    #[serde(default)]
    pronouns: Option<PronounsSpec>,
    #[serde(default)]
    identity: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OutfitSpec {
    top: Vec<String>,
    bottom: Vec<String>,
    footwear: Vec<String>,
    accessories: Vec<String>,
    /// Absent on pre-hair fixture rows (reads as empty, matching v4's parse).
    #[serde(default)]
    hair: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageSpec {
    #[serde(default)]
    prompt_tokens: Option<f64>,
    #[serde(default)]
    completion_tokens: Option<f64>,
    #[serde(default)]
    total_tokens: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    kind: String,
    #[serde(default)]
    details: Option<DangerDetailsSpec>,
    #[serde(default)]
    manual_kind: Option<String>,
    #[serde(default)]
    character_name: Option<String>,
    #[serde(default)]
    old_profile_label: Option<String>,
    #[serde(default)]
    new_profile_label: Option<String>,
    #[serde(default)]
    participant_ref: Option<String>,
    #[serde(default)]
    character_ref: Option<String>,
    #[serde(default)]
    old_status: Option<String>,
    #[serde(default)]
    new_status: Option<String>,
    #[serde(default)]
    initial_status: Option<String>,
    #[serde(default)]
    off_scene_characters: Option<Vec<OffSceneSpec>>,
    #[serde(default)]
    scenario_text: Option<String>,
    #[serde(default)]
    target_ref: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    opaque_content: Option<String>,
    #[serde(default)]
    whisper_kind: Option<String>,
    #[serde(default)]
    outfit: Option<OutfitSpec>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    target_refs: Option<Vec<String>>,
    #[serde(default)]
    summary_anchor: Option<i64>,
    #[serde(default)]
    display_title: Option<String>,
    #[serde(default)]
    file_path: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    mount_point: Option<String>,
    #[serde(default)]
    is_new: Option<bool>,
    #[serde(default)]
    origin_by: Option<String>,
    #[serde(default)]
    origin_character_name: Option<String>,
    #[serde(default)]
    turn_pass_source: Option<String>,
    #[serde(default)]
    usage: Option<UsageSpec>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    model_name: Option<String>,
    #[serde(rename = "estimatedCostUSD", default)]
    estimated_cost_usd: Option<f64>,
}

#[derive(Deserialize)]
struct CharSeed {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatSpec {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    chat: ChatSpec,
    ada: CharSeed,
    p1_id: String,
    p2_id: String,
    cases: Vec<Case>,
}

// ---------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------

fn commonplace_kind(s: &str) -> CommonplaceWhisperKind {
    match s {
        "memory-recap" => CommonplaceWhisperKind::MemoryRecap,
        "relevant-memories" => CommonplaceWhisperKind::RelevantMemories,
        "inter-character-memories" => CommonplaceWhisperKind::InterCharacterMemories,
        "consolidated" => CommonplaceWhisperKind::Consolidated,
        "relevant-conversations" => CommonplaceWhisperKind::RelevantConversations,
        other => panic!("unknown commonplace whisper kind {other}"),
    }
}

/// Tokenize each row's `id` to `<id-N>` in row order (dump already sorted by
/// `content`), placeholder `createdAt` → `<ts>`.
fn normalize_messages(dump: &mut Value) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .expect("rows array");
    let mut id_map: HashMap<String, String> = HashMap::new();
    for row in rows.iter_mut() {
        let obj = row.as_object_mut().expect("row object");
        if let Some(Value::String(id)) = obj.get("id") {
            let next = format!("<id-{}>", id_map.len());
            let token = id_map.entry(id.clone()).or_insert(next).clone();
            obj.insert("id".to_string(), Value::String(token));
        }
        if obj.get("createdAt").map(|v| !v.is_null()).unwrap_or(false) {
            obj.insert("createdAt".to_string(), Value::String("<ts>".to_string()));
        }
    }
}

/// Placeholder the minted `updatedAt` / `lastMessageAt` on the single chats row.
fn normalize_chats(dump: &mut Value) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .expect("chats rows array");
    for row in rows.iter_mut() {
        let obj = row.as_object_mut().expect("chats row object");
        for col in ["updatedAt", "lastMessageAt"] {
            if obj.get(col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert(col.to_string(), Value::String("<ts>".to_string()));
            }
        }
    }
}

fn assert_dump_eq(got: &Value, oracle: &Value, label: &str) {
    assert_eq!(
        got.get("columns"),
        oracle.get("columns"),
        "{label}: column set differs"
    );
    let g = got.get("rows").and_then(Value::as_array).expect("got rows");
    let o = oracle
        .get("rows")
        .and_then(Value::as_array)
        .expect("oracle rows");
    assert_eq!(g.len(), o.len(), "{label}: row count differs");
    for (i, (gr, or)) in g.iter().zip(o.iter()).enumerate() {
        assert_eq!(
            gr, or,
            "{label}: row {i} differs\n  rust:   {gr}\n  oracle: {or}"
        );
    }
}

// ---------------------------------------------------------------------------
// Case dispatch.
// ---------------------------------------------------------------------------

async fn run_case(db: &Db, spec: &Spec, c: &Case) {
    let chat_id = &spec.chat.id;
    let participant = |r: &Option<String>| -> String {
        match r.as_deref() {
            Some("p1") => spec.p1_id.clone(),
            Some("p2") => spec.p2_id.clone(),
            other => panic!("case {}: unknown participant ref {:?}", c.name, other),
        }
    };

    match c.kind.as_str() {
        "conciergeDanger" => {
            let d = c.details.as_ref().expect("details");
            let details = ConciergeDangerDetails {
                score: d.score,
                threshold: d.threshold,
                categories: d
                    .categories
                    .iter()
                    .map(|cat| ConciergeCategory {
                        category: cat.category.clone(),
                        score: cat.score,
                        label: cat.label.clone(),
                    })
                    .collect(),
                source: d.source.clone(),
                provider_name: d.provider_name.clone(),
            };
            post_concierge_danger_announcement(db, chat_id, Some(&details)).await;
        }
        "conciergeManual" => {
            let kind =
                ConciergeManualKind::from_wire(c.manual_kind.as_deref().expect("manualKind"))
                    .expect("valid manual kind");
            post_concierge_manual_announcement(db, chat_id, kind).await;
        }
        "prosperoConnProfile" => {
            post_prospero_connection_profile_change_announcement(
                db,
                ProsperoConnectionProfileChangeAnnouncement {
                    chat_id: chat_id.clone(),
                    character_name: c.character_name.clone().expect("characterName"),
                    old_profile_label: c.old_profile_label.clone(),
                    new_profile_label: c.new_profile_label.clone(),
                },
            )
            .await;
        }
        "hostStatusChange" => {
            post_host_status_change_announcement(
                db,
                HostStatusChangeAnnouncement {
                    chat_id: chat_id.clone(),
                    character_name: c.character_name.clone().expect("characterName"),
                    participant_id: participant(&c.participant_ref),
                    old_status: c.old_status.clone().expect("oldStatus"),
                    new_status: c.new_status.clone().expect("newStatus"),
                },
            )
            .await;
        }
        "hostAdd" => {
            assert_eq!(
                c.character_ref.as_deref(),
                Some("ada"),
                "only ada supported"
            );
            let ada_id = spec.ada.id.clone();
            let raw = db
                .read_main(move |main| characters_read::find_by_id_raw(main, &ada_id))
                .expect("read ada")
                .expect("ada present");
            let character = HostCharacter {
                name: raw
                    .get("name")
                    .and_then(Value::as_str)
                    .expect("ada name")
                    .to_string(),
                description: raw
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                character_document_mount_point_id: raw
                    .get("characterDocumentMountPointId")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            };
            post_host_add_announcement(
                db,
                HostAddAnnouncement {
                    chat_id: chat_id.clone(),
                    character,
                    participant_id: participant(&c.participant_ref),
                    initial_status: c.initial_status.clone(),
                },
            )
            .await;
        }
        "hostOffScene" => {
            let characters = c
                .off_scene_characters
                .as_ref()
                .expect("offSceneCharacters")
                .iter()
                .map(|o| OffSceneCharacterCard {
                    id: o.id.clone(),
                    name: o.name.clone(),
                    aliases: o.aliases.clone().unwrap_or_default(),
                    pronouns: o
                        .pronouns
                        .as_ref()
                        .map(|p| (p.subject.clone(), p.object.clone(), p.possessive.clone())),
                    identity: o.identity.clone(),
                    description: o.description.clone(),
                })
                .collect();
            post_host_off_scene_characters_announcement(
                db,
                HostOffSceneCharactersAnnouncement {
                    chat_id: chat_id.clone(),
                    characters,
                },
            )
            .await;
        }
        "hostTurnPass" => {
            post_host_turn_pass_announcement(
                db,
                HostTurnPassAnnouncement {
                    chat_id: chat_id.clone(),
                    character_name: c.character_name.clone().expect("characterName"),
                    participant_id: participant(&c.participant_ref),
                    source: match c.turn_pass_source.as_deref() {
                        Some("user") => TurnPassSource::User,
                        _ => TurnPassSource::Llm,
                    },
                },
            )
            .await;
        }
        "hostScenario" => {
            post_host_scenario_announcement(
                db,
                HostScenarioAnnouncement {
                    chat_id: chat_id.clone(),
                    scenario_text: c.scenario_text.clone().expect("scenarioText"),
                },
            )
            .await;
        }
        "auroraCoreWhisper" => {
            post_core_whisper(
                db,
                chat_id,
                &participant(&c.target_ref),
                c.content.as_deref().expect("content"),
                c.opaque_content.as_deref().expect("opaqueContent"),
            )
            .await;
        }
        "auroraOutfitChange" => {
            let o = c.outfit.as_ref().expect("outfit");
            let outfit = OutfitSlotValues {
                top: o.top.clone(),
                bottom: o.bottom.clone(),
                footwear: o.footwear.clone(),
                accessories: o.accessories.clone(),
                hair: o.hair.clone(),
            };
            post_outfit_change_whisper(
                db,
                chat_id,
                c.character_name.as_deref().expect("characterName"),
                &outfit,
            )
            .await;
        }
        "suparnaMail" => {
            post_suparna_mail_whisper(
                db,
                PostSuparnaMailWhisperParams {
                    chat_id: chat_id.clone(),
                    target_participant_id: Some(participant(&c.target_ref)),
                    content: c.content.clone().expect("content"),
                },
            )
            .await;
        }
        "commonplaceTargeted" => {
            post_commonplace_whisper(
                db,
                PostCommonplaceParams {
                    chat_id: chat_id.clone(),
                    target_participant_id: Some(participant(&c.target_ref)),
                    content: c.content.clone().expect("content"),
                    opaque_content: c.opaque_content.clone(),
                    kind: commonplace_kind(c.whisper_kind.as_deref().expect("whisperKind")),
                },
            )
            .await;
        }
        "commonplacePublicNullOpaque" => {
            post_commonplace_whisper(
                db,
                PostCommonplaceParams {
                    chat_id: chat_id.clone(),
                    target_participant_id: None,
                    content: c.content.clone().expect("content"),
                    opaque_content: None,
                    kind: commonplace_kind(c.whisper_kind.as_deref().expect("whisperKind")),
                },
            )
            .await;
        }
        "librarianSummary" => {
            let targets: Vec<String> = c
                .target_refs
                .as_ref()
                .map(|refs| refs.iter().map(|r| participant(&Some(r.clone()))).collect())
                .unwrap_or_default();
            post_librarian_summary_announcement(
                db,
                &LibrarianSummaryAnnouncement {
                    chat_id: chat_id.clone(),
                    summary: c.summary.clone().expect("summary"),
                    target_participant_ids: if targets.is_empty() {
                        None
                    } else {
                        Some(targets)
                    },
                    summary_anchor: c.summary_anchor,
                },
            )
            .await;
        }
        "librarianOpen" => {
            let scope = match c.scope.as_deref().expect("scope") {
                "project" => LibrarianScope::Project,
                "document_store" => LibrarianScope::DocumentStore,
                "general" => LibrarianScope::General,
                other => panic!("unknown scope {other}"),
            };
            let origin = match c.origin_by.as_deref() {
                Some("character") => LibrarianOpenKind::ByCharacter {
                    character_name: c
                        .origin_character_name
                        .clone()
                        .expect("originCharacterName"),
                },
                _ => LibrarianOpenKind::ByUser,
            };
            post_librarian_open_announcement(
                db,
                &LibrarianOpenAnnouncement {
                    chat_id: chat_id.clone(),
                    display_title: c.display_title.clone().expect("displayTitle"),
                    file_path: c.file_path.clone().expect("filePath"),
                    scope,
                    mount_point: c.mount_point.clone(),
                    is_new: c.is_new.unwrap_or(false),
                    origin,
                    hidden_from_characters: false,
                },
            )
            .await;
        }
        "costContextSummary" => {
            create_context_summary_event(
                db,
                chat_id,
                c.usage.as_ref().map(usage_to_token),
                c.provider.clone(),
                c.model_name.clone(),
                c.estimated_cost_usd,
            )
            .await;
        }
        "costTitleGeneration" => {
            create_title_generation_event(
                db,
                chat_id,
                c.usage.as_ref().map(usage_to_token),
                c.provider.clone(),
                c.model_name.clone(),
                c.estimated_cost_usd,
            )
            .await;
        }
        other => panic!("unknown case kind {other}"),
    }
}

fn usage_to_token(u: &UsageSpec) -> TokenUsage {
    TokenUsage {
        prompt_tokens: u.prompt_tokens,
        completion_tokens: u.completion_tokens,
        total_tokens: u.total_tokens,
    }
}

// ---------------------------------------------------------------------------
// Test.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn post_office_writers_tier3_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_POW") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_POW to the oracle NDJSON (see header).");
            return;
        }
    };
    let main_fixture = match std::env::var("QT_FIXTURE_POW_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_POW_MAIN to the main fixture .db (header).");
            return;
        }
    };
    let mount_fixture = match std::env::var("QT_FIXTURE_POW_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_POW_MOUNT to the mount fixture .db (header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/post-office-writers-tier3.json"),
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

    // Fresh copies so the shared seed fixtures stay pristine.
    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-pow-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-pow-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let paths = DbPaths {
        main: main_work.clone(),
        mount_index: Some(mount_work.clone()),
        llm_logs: None,
    };
    let db = Db::open(paths, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copies: {e}"));

    for case in &spec.cases {
        run_case(&db, &spec, case).await;
    }

    let mut got_messages = db
        .read_main(|conn| dump_table_json_conn(conn, "chat_messages", "content"))
        .expect("dump chat_messages");
    let mut got_chats = db
        .read_main(|conn| dump_table_json_conn(conn, "chats", "id"))
        .expect("dump chats");
    drop(db);
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);

    let mut oracle_messages = oracle
        .get("chatMessages")
        .cloned()
        .expect("oracle chatMessages");
    let mut oracle_chats = oracle.get("chats").cloned().expect("oracle chats");

    normalize_messages(&mut got_messages);
    normalize_messages(&mut oracle_messages);
    normalize_chats(&mut got_chats);
    normalize_chats(&mut oracle_chats);

    assert_dump_eq(&got_messages, &oracle_messages, "chat_messages");
    assert_dump_eq(&got_chats, &oracle_chats, "chats");

    let nm = got_messages["rows"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    assert_eq!(nm, 21, "expected 19 persona + 2 cost SYSTEM rows");
    eprintln!("OK: post-office-writers tier-3 matched oracle ({nm} rows, chats aggregate).");
}
