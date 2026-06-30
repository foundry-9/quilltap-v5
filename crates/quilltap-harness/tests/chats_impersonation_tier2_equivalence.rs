//! Tier-2 differential test: v4's `ChatImpersonationOps` (Phase-2, the
//! conversation capstone, sub-unit 6 — `addImpersonation` /
//! `removeImpersonation` / `getImpersonatedParticipantIds` /
//! `setActiveTypingParticipant` / `updateAllLLMPauseTurnCount`).
//!
//! Both sides run the SAME op sequence (`chats-impersonation-tier2.json`) on a
//! fresh copy of the seed fixture (five chats with pinned participants, seeded
//! via v4's real `repos.chats.create`), then the `chats` table is dumped
//! canonically and the post-op state asserted identical.
//!
//! These ops are the EASIEST chats sub-unit: each only rewrites
//! `impersonatingParticipantIds` / `activeTypingParticipantId` /
//! `allLLMPauseTurnCount`, mints NO ids and NO timestamps, and the chat's own
//! `updatedAt` is never bumped (v4 `_update` preserves it). So the differential
//! is run with **ZERO normalization** — the participant ids + every timestamp
//! are pinned in the seed and the whole dump is diffed exactly.
//!
//! Exercises: `addImpersonation` (fresh → sets `activeTyping`; second add → keeps
//! the existing `activeTyping`; duplicate add → no double-push; add of a
//! non-participant → no-op), `removeImpersonation` (removing the active one →
//! reassigns to the first remaining; removing the last → `activeTyping` null),
//! `setActiveTypingParticipant` (a valid switch; reject a non-impersonated id →
//! no-op; clear to null), `updateAllLLMPauseTurnCount`, plus the not-found
//! no-ops on a missing chat.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   export PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-chimp-fixture.db \
//!     npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chats-impersonation-fixture.ts
//!   QT_FIXTURE_CHIMP=/tmp/qt-chimp-fixture.db \
//!     npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-impersonation-tier2.ts > /tmp/oracle-chimp.ndjson
//! Run:
//!   QT_ORACLE_CHIMP=/tmp/oracle-chimp.ndjson \
//!   QT_FIXTURE_CHIMP=/tmp/qt-chimp-fixture.db \
//!     cargo test -p quilltap-harness --test chats_impersonation_tier2_equivalence

use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "addImpersonation")]
    AddImpersonation {
        #[serde(rename = "chatId")]
        chat_id: String,
        #[serde(rename = "participantId")]
        participant_id: String,
    },
    #[serde(rename = "removeImpersonation")]
    RemoveImpersonation {
        #[serde(rename = "chatId")]
        chat_id: String,
        #[serde(rename = "participantId")]
        participant_id: String,
    },
    #[serde(rename = "getImpersonatedParticipantIds")]
    GetImpersonatedParticipantIds {
        #[serde(rename = "chatId")]
        chat_id: String,
    },
    #[serde(rename = "setActiveTypingParticipant")]
    SetActiveTypingParticipant {
        #[serde(rename = "chatId")]
        chat_id: String,
        #[serde(rename = "participantId")]
        participant_id: Option<String>,
    },
    #[serde(rename = "updateAllLLMPauseTurnCount")]
    UpdateAllLlmPauseTurnCount {
        #[serde(rename = "chatId")]
        chat_id: String,
        count: f64,
    },
}

#[test]
fn chats_impersonation_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHIMP") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHIMP to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CHIMP") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHIMP to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/chats-impersonation-tier2.json"),
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

    let work = std::env::temp_dir().join(format!("qt-chimp-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.chat_impersonation();
        for op in &spec.ops {
            match op {
                Op::AddImpersonation {
                    chat_id,
                    participant_id,
                } => {
                    repo.add_impersonation(chat_id, participant_id)
                        .expect("add_impersonation");
                }
                Op::RemoveImpersonation {
                    chat_id,
                    participant_id,
                } => {
                    repo.remove_impersonation(chat_id, participant_id)
                        .expect("remove_impersonation");
                }
                Op::GetImpersonatedParticipantIds { chat_id } => {
                    repo.get_impersonated_participant_ids(chat_id)
                        .expect("get_impersonated_participant_ids");
                }
                Op::SetActiveTypingParticipant {
                    chat_id,
                    participant_id,
                } => {
                    repo.set_active_typing_participant(chat_id, participant_id.as_deref())
                        .expect("set_active_typing_participant");
                }
                Op::UpdateAllLlmPauseTurnCount { chat_id, count } => {
                    repo.update_all_llm_pause_turn_count(chat_id, *count)
                        .expect("update_all_llm_pause_turn_count");
                }
            }
        }
    }

    let got = writer.dump_table_json("chats", "id").expect("dump chats");
    let _ = std::fs::remove_file(&work);

    assert_eq!(got["table"], oracle["chats"]["table"], "chats: table name");
    assert_eq!(
        got["columns"], oracle["chats"]["columns"],
        "chats: column set / order"
    );
    assert_eq!(
        got["rows"], oracle["chats"]["rows"],
        "chats: row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["chats"]["rows"]
    );

    // Guard against an empty-vs-empty false pass.
    assert_ne!(got["rows"], json!([]), "expected non-empty chats dump");

    eprintln!("OK: chats impersonation tier-2 matched oracle.");
}
