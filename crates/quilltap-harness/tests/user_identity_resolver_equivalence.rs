//! Read-differential test: the user-identity resolver (v4
//! `lib/services/chat-message/user-identity-resolver.service.ts`
//! `resolveUserIdentity`), ported as
//! `quilltap_core::services::user_identity_resolver`.
//!
//! Both sides run the SAME op sequence (`user-identity.json`) on a fresh copy of
//! the two-database seed fixture (a main DB with users + chats + slim/null-mount
//! characters, and an empty-but-valid mount-index DB), loading each chat and
//! resolving the user's identity. This proves the four-step fallback chain hits
//! each source:
//!   1. chat-participant — a user-controlled character participant (incl.
//!      preferring the active-typing "Speaking As" one over the first in order),
//!   2. single-user-character — exactly one user-controlled character system-wide,
//!   3. user-profile — the user's profile name,
//!   4. default — the `"User"` fallback (NULL profile name).
//!
//! NORMALIZATION: none. The resolver writes nothing and mints nothing, so each
//! resolved identity object is compared exactly.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-useridentity-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-useridentity-mount.db \
//!     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-user-identity-fixture.ts
//!   QT_FIXTURE_USERIDENTITY=/tmp/qt-useridentity-main.db \
//!   QT_FIXTURE_USERIDENTITY_MOUNT=/tmp/qt-useridentity-mount.db \
//!     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/cases/user-identity.ts > /tmp/oracle-useridentity.ndjson
//! Run:
//!   QT_ORACLE_USERIDENTITY=/tmp/oracle-useridentity.ndjson \
//!   QT_FIXTURE_USERIDENTITY=/tmp/qt-useridentity-main.db \
//!   QT_FIXTURE_USERIDENTITY_MOUNT=/tmp/qt-useridentity-mount.db \
//!     cargo test -p quilltap-harness --test user_identity_resolver_equivalence

use quilltap_core::db::chats_read;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::user_identity_resolver::resolve_user_identity;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct Op {
    #[serde(rename = "chatId")]
    chat_id: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "activeTypingParticipantId")]
    active_typing_participant_id: Option<String>,
}

#[tokio::test]
async fn user_identity_resolver_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_USERIDENTITY") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_USERIDENTITY to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_USERIDENTITY") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_USERIDENTITY to the main seed fixture .db (see header)."
            );
            return;
        }
    };
    let fixture_mount = match std::env::var("QT_FIXTURE_USERIDENTITY_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_USERIDENTITY_MOUNT to the mount seed fixture .db.");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/user-identity.json"),
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
    let work_main = std::env::temp_dir().join(format!("qt-useridentity-rust-main-{pid}.db"));
    let work_mount = std::env::temp_dir().join(format!("qt-useridentity-rust-mount-{pid}.db"));
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
        let chat = {
            let chat_id = op.chat_id.clone();
            db.read_main(move |conn| chats_read::find_by_id(conn, &chat_id))
        }
        .unwrap_or_else(|e| panic!("find chat {}: {e:?}", op.chat_id))
        .unwrap_or_else(|| panic!("chat not found: {}", op.chat_id));

        let identity = resolve_user_identity(
            &db,
            &op.user_id,
            &chat,
            op.active_typing_participant_id.as_deref(),
        )
        .await
        .unwrap_or_else(|e| panic!("resolveUserIdentity {}: {e:?}", op.chat_id));

        got_results.push(json!({
            "chatId": op.chat_id,
            "name": identity.name,
            "description": identity.description,
            "characterId": identity.character_id,
            "source": identity.source.as_str(),
        }));
    }

    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);

    assert_eq!(
        got_results.len(),
        oracle_results.len(),
        "op-count mismatch — regenerate the oracle NDJSON"
    );
    for (i, (got, want)) in got_results.iter().zip(oracle_results.iter()).enumerate() {
        assert_eq!(
            got, want,
            "op {i} identity diverged\n  rust:   {got}\n  oracle: {want}"
        );
    }

    eprintln!(
        "OK: user-identity resolver matched oracle ({} ops).",
        got_results.len()
    );
}
