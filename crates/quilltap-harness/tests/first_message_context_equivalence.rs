//! Read-differential test: the **first-message-context builder**
//! (`quilltap_core::services::first_message_context` — v4
//! `loadParticipantMemories` / `loadProjectContext` / `buildFirstMessageContext`,
//! plus the `searchMemoriesSemantic` it composes).
//!
//! Both sides READ the same two-DB fixture (main + mount-index: full-vault
//! characters, a real project store, the speaker's memories with vector entries) and
//! run the same case corpus with the SAME canned embeddings injected (the oracle via
//! a `jest.mock` of `generateEmbeddingForUser`, the Rust side via a
//! `CannedEmbeddingProvider`). Read-only → **zero normalization**: each case's result
//! object is compared as JSON.
//!
//! Corpus banks: recent + semantic dedupe with general-memory inclusion (Alice), the
//! text-fallback firing when semantic is empty (Speaker Two → Bob), a fully-empty
//! participant (Carol), project found / missing, and `buildFirstMessageContext`
//! end-to-end with and without a project.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-fmc-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-fmc-mount.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-first-message-context-fixture.ts
//!   QT_FIXTURE_FMC_MAIN=/tmp/qt-fmc-main.db QT_FIXTURE_FMC_MOUNT=/tmp/qt-fmc-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-first-message-context.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- first-message-context
//! Run:
//!   QT_ORACLE_FMC=/tmp/oracle-first-message-context.ndjson \
//!   QT_FIXTURE_FMC_MAIN=/tmp/qt-fmc-main.db QT_FIXTURE_FMC_MOUNT=/tmp/qt-fmc-mount.db \
//!     cargo test -p quilltap-harness --test first_message_context_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::services::first_message_context::{
    build_first_message_context, load_participant_memories, load_project_context,
    ChatParticipantInput, FirstMessageContextOptions, ParticipantInfo,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "nowMs")]
    now_ms: f64,
    #[serde(rename = "cannedEmbeddings")]
    canned_embeddings: HashMap<String, Vec<f32>>,
    #[serde(rename = "cannedFailures")]
    canned_failures: Vec<String>,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    label: String,
    kind: String,
    #[serde(default, rename = "speakingCharacterId")]
    speaking_character_id: Option<String>,
    #[serde(default, rename = "otherParticipants")]
    other_participants: Vec<OtherParticipant>,
    #[serde(default, rename = "projectId")]
    project_id: Option<String>,
    #[serde(default)]
    participants: Vec<Participant>,
}

#[derive(Deserialize)]
struct OtherParticipant {
    #[serde(rename = "characterId")]
    character_id: String,
    name: String,
    description: Option<String>,
    #[serde(rename = "controlledBy")]
    controlled_by: String,
}

#[derive(Deserialize)]
struct Participant {
    #[serde(rename = "type")]
    participant_type: String,
    #[serde(rename = "characterId")]
    character_id: Option<String>,
    #[serde(default, rename = "controlledBy")]
    controlled_by: Option<String>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/first-message-context.json")
}

fn oracle_line(oracle_text: &str, label: &str) -> Value {
    for line in oracle_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).expect("parse oracle ndjson line");
        if v.get("label").and_then(Value::as_str) == Some(label) {
            return v;
        }
    }
    panic!("oracle ndjson missing label {label}");
}

/// Render an f64 the way `JSON.stringify` does (integer-valued → JSON integer).
fn js_number(f: f64) -> Value {
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 9e15 {
        Value::from(f as i64)
    } else {
        Value::from(f)
    }
}

fn participant_memories_json(
    mems: &[quilltap_core::services::first_message_context::ParticipantMemory],
) -> Value {
    Value::Array(
        mems.iter()
            .map(|m| {
                json!({
                    "aboutCharacterId": m.about_character_id,
                    "aboutCharacterName": m.about_character_name,
                    "summary": m.summary,
                    "importance": js_number(m.importance),
                })
            })
            .collect(),
    )
}

fn project_context_json(
    ctx: &Option<quilltap_core::services::first_message_context::ProjectContext>,
) -> Value {
    match ctx {
        None => Value::Null,
        Some(c) => json!({
            "name": c.name,
            "description": c.description,
            "instructions": c.instructions,
        }),
    }
}

#[tokio::test]
async fn first_message_context_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_FMC") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_FMC to the oracle NDJSON (see header).");
            return;
        }
    };
    let (fx_main, fx_mount) = match (
        std::env::var("QT_FIXTURE_FMC_MAIN"),
        std::env::var("QT_FIXTURE_FMC_MOUNT"),
    ) {
        (Ok(m), Ok(x)) => (m, x),
        _ => {
            eprintln!("SKIP: set QT_FIXTURE_FMC_MAIN + QT_FIXTURE_FMC_MOUNT (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    // Fresh copies so the shared fixtures stay pristine.
    let work_main =
        std::env::temp_dir().join(format!("qt-fmc-rust-main-{}.db", std::process::id()));
    let work_mount =
        std::env::temp_dir().join(format!("qt-fmc-rust-mount-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(&fx_main, &work_main).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&fx_mount, &work_mount).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let mut provider = CannedEmbeddingProvider::new();
    for (text, vec) in &spec.canned_embeddings {
        provider = provider.with_vector(text.clone(), vec.clone());
    }
    for text in &spec.canned_failures {
        provider = provider.with_failure(text.clone());
    }

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copies: {e}"));

    for c in &spec.cases {
        let got: Value = match c.kind.as_str() {
            "loadParticipantMemories" => {
                let others: Vec<ParticipantInfo> = c
                    .other_participants
                    .iter()
                    .map(|p| ParticipantInfo {
                        character_id: p.character_id.clone(),
                        name: p.name.clone(),
                        description: p.description.clone(),
                        controlled_by: p.controlled_by.clone(),
                    })
                    .collect();
                let mems = load_participant_memories(
                    &db,
                    &provider,
                    c.speaking_character_id.as_deref().unwrap(),
                    &others,
                    &spec.user_id,
                    None,
                    5,
                    spec.now_ms,
                )
                .await
                .unwrap_or_else(|e| panic!("case {}: {e:?}", c.label));
                json!({ "participantMemories": participant_memories_json(&mems) })
            }
            "loadProjectContext" => {
                let ctx = load_project_context(&db, c.project_id.as_deref().unwrap())
                    .unwrap_or_else(|e| panic!("case {}: {e:?}", c.label));
                json!({ "projectContext": project_context_json(&ctx) })
            }
            "buildFirstMessageContext" => {
                let participants: Vec<ChatParticipantInput> = c
                    .participants
                    .iter()
                    .map(|p| ChatParticipantInput {
                        participant_type: p.participant_type.clone(),
                        character_id: p.character_id.clone(),
                        controlled_by: p.controlled_by.clone(),
                    })
                    .collect();
                let res = build_first_message_context(
                    &db,
                    &provider,
                    c.speaking_character_id.as_deref().unwrap(),
                    &participants,
                    &FirstMessageContextOptions {
                        user_id: spec.user_id.clone(),
                        project_id: c.project_id.clone(),
                        embedding_profile_id: None,
                        now_ms: spec.now_ms,
                    },
                )
                .await
                .unwrap_or_else(|e| panic!("case {}: {e:?}", c.label));
                json!({
                    "projectContext": project_context_json(&res.project_context),
                    "participantMemories": participant_memories_json(&res.participant_memories),
                })
            }
            other => panic!("unknown case kind {other}"),
        };

        let want = oracle_line(&oracle_text, &c.label);
        // Compare only the payload fields (strip the label).
        let mut want_payload = want.clone();
        want_payload.as_object_mut().unwrap().remove("label");
        assert_eq!(got, want_payload, "case {} diverged", c.label);
    }

    drop(db);
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    eprintln!("OK: first-message-context read-differential matched oracle.");
}
