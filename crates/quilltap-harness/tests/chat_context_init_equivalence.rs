//! Read-differential (P4.4 unit 2, sub-unit 2): buildChatContext
//! (`services::chat_initialize::build_chat_context`) vs v4's REAL
//! `buildChatContext`. Both sides READ the SAME baked fixture (three characters
//! with vaults — Aria/llm, Sam/user, Bob/llm-with-defaultPartner=Sam) and run
//! the SAME matrix, comparing the computed `systemPrompt` + `firstMessage` + the
//! resolved character id/name + the resolved user-character id/name EXACTLY (no
//! normalization — this path mints no clock/id).
//!
//! Matrix: a bare responding character (no user char); an explicit user
//! character + a scenario override (aliases/pronouns/description rendered into
//! the prompt); a selected non-default system prompt; the `defaultPartnerId`
//! user-character resolution.
//!
//! Build the fixtures + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_CCTX_MAIN=/tmp/qt-cctx-main.db QT_FIXTURE_CCTX_MOUNT=/tmp/qt-cctx-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-chat-context-init-fixture.ts
//!   QT_FIXTURE_CCTX_MAIN=/tmp/qt-cctx-main.db QT_FIXTURE_CCTX_MOUNT=/tmp/qt-cctx-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/cases/chat-context-init.ts > /tmp/oracle-cctx.ndjson
//! Run:
//!   QT_ORACLE_CCTX=/tmp/oracle-cctx.ndjson \
//!   QT_FIXTURE_CCTX_MAIN=/tmp/qt-cctx-main.db QT_FIXTURE_CCTX_MOUNT=/tmp/qt-cctx-mount.db \
//!     cargo test -p quilltap-harness --test chat_context_init_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::Writer;
use quilltap_core::services::chat_initialize::build_chat_context;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "ariaId")]
    aria_id: String,
    #[serde(rename = "samId")]
    sam_id: String,
    #[serde(rename = "bobId")]
    bob_id: String,
    #[serde(rename = "ariaSp2")]
    aria_sp2: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-context-init.json")
}

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

#[test]
fn chat_context_init_matches_oracle() {
    let (Some(oracle_path), Some(main_fixture), Some(mount_fixture)) = (
        env_or_skip("QT_ORACLE_CCTX"),
        env_or_skip("QT_FIXTURE_CCTX_MAIN"),
        env_or_skip("QT_FIXTURE_CCTX_MOUNT"),
    ) else {
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let row: Value = serde_json::from_str(line).expect("oracle line parses");
        oracle.insert(row["id"].as_str().unwrap().to_string(), row);
    }

    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-cctx-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-cctx-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let main_w = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount_w = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));
    let main = main_w.connection();
    let mount = mount_w.connection();

    type Case = (
        &'static str,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let cases: Vec<Case> = vec![
        ("basic", spec.aria_id.clone(), None, None, None),
        (
            "with_user_scenario",
            spec.aria_id.clone(),
            Some(spec.sam_id.clone()),
            Some("A misty harbor at dawn.".to_string()),
            None,
        ),
        (
            "selected_prompt",
            spec.aria_id.clone(),
            None,
            None,
            Some(spec.aria_sp2.clone()),
        ),
        ("default_partner", spec.bob_id.clone(), None, None, None),
    ];

    for (id, character_id, ucid, scenario, sel) in cases {
        let ctx = build_chat_context(
            main,
            mount,
            &character_id,
            ucid.as_deref(),
            scenario.as_deref(),
            sel.as_deref(),
        )
        .unwrap_or_else(|e| panic!("case {id}: build_chat_context: {e}"));

        let got = json!({
            "id": id,
            "systemPrompt": ctx.system_prompt,
            "firstMessage": ctx.first_message,
            "characterId": ctx.character.get("id").and_then(Value::as_str).unwrap_or_default(),
            "characterName": ctx.character.get("name").and_then(Value::as_str).unwrap_or_default(),
            "userCharacterId": ctx.user_character.as_ref().map(|u| u.id.clone()),
            "userCharacterName": ctx.user_character.as_ref().map(|u| u.name.clone()),
        });

        let want = oracle
            .get(id)
            .unwrap_or_else(|| panic!("oracle missing case {id}"));
        assert_eq!(&got, want, "case {id}: rust != oracle");
    }
    assert_eq!(oracle.len(), 4, "oracle case count drifted");
}
