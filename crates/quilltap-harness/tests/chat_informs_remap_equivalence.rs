//! Tier-1 differential: `remapChatInform` (P4.D205, v4 `e7d77bb60`).
//!
//! Drives v4's REAL `lib/import/quilltap-import/reconcile.ts` over the corpus and
//! diffs the whole result — `ok`, the drop `reason` with its ids interpolated,
//! the remapped `data`, and `consumedByMessageIdCleared`.
//!
//! The three rules, which are the whole point of the function:
//!   - a `participantId` that is not a seat of the DESTINATION chat → DROPPED
//!     (an inform aimed at nobody would sit pending forever);
//!   - a `recordMessageId` the destination does not have → DROPPED (a record
//!     pointer into empty space is a transcript lie);
//!   - a `consumedByMessageId` the destination does not have → **KEPT**, field
//!     nulled, flag set. The row is still a real historical inform; it only
//!     loses its swipe anchor.
//!
//! `id` / `createdAt` / `updatedAt` are NOT compared: v4's import calls
//! `chatInforms.create(result.data)` with no `CreateOptions`, so the destination
//! row mints its own — v4's `data` does not even carry them (the type is
//! `Omit<ChatInform, 'id' | 'createdAt' | 'updatedAt'>`), and the port's minted
//! values are asserted to be present and fresh instead.
//!
//! Generate the oracle (Node 24, from the TARGET-pinned v4 worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chat-informs-remap.ts \
//!     > /tmp/oracle-chat-informs-remap.ndjson
//! Run:
//!   QT_ORACLE_CHAT_INFORMS_REMAP=/tmp/oracle-chat-informs-remap.ndjson \
//!     cargo test -p quilltap-harness --test chat_informs_remap_equivalence -- --nocapture

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use quilltap_core::services::quilltap_import::reconcile::{remap_chat_inform, ChatInformRemap};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    label: String,
    inform: Value,
    #[serde(rename = "chatMap")]
    chat_map: std::collections::HashMap<String, String>,
    #[serde(rename = "knownParticipantIds")]
    known_participant_ids: Vec<String>,
    #[serde(rename = "knownMessageIds")]
    known_message_ids: Vec<String>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-informs-remap.json")
}

#[test]
fn chat_informs_remap_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHAT_INFORMS_REMAP") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHAT_INFORMS_REMAP to the oracle NDJSON (see header).");
            return;
        }
    };

    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("read spec"))
            .expect("parse spec");
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .expect("read oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle row"))
        .collect();
    assert!(!oracle.is_empty(), "oracle is EMPTY — the regen failed");
    assert_eq!(
        oracle.len(),
        spec.cases.len(),
        "oracle row count does not match the corpus"
    );

    let mut dropped = 0usize;
    let mut cleared = 0usize;
    for (case, want) in spec.cases.iter().zip(oracle.iter()) {
        assert_eq!(
            want["label"].as_str().unwrap_or(""),
            case.label,
            "corpus and oracle fell out of step"
        );

        // The caller resolves the destination chat id before calling; reproduce
        // that here exactly as the importer does.
        let source_chat_id = case.inform["chatId"].as_str().unwrap_or_default();
        let remapped_chat_id = case
            .chat_map
            .get(source_chat_id)
            .cloned()
            .unwrap_or_else(|| source_chat_id.to_string());

        let participants: HashSet<String> = case.known_participant_ids.iter().cloned().collect();
        let messages: HashSet<String> = case.known_message_ids.iter().cloned().collect();
        let got = remap_chat_inform(&case.inform, &remapped_chat_id, &participants, &messages);

        let w = &want["result"];
        match got {
            ChatInformRemap::Dropped { reason } => {
                dropped += 1;
                assert_eq!(
                    w["ok"],
                    json!(false),
                    "[{}] v5 dropped the row and v4 kept it",
                    case.label
                );
                assert_eq!(
                    reason,
                    w["reason"].as_str().unwrap_or_default(),
                    "[{}] the drop sentence diverged",
                    case.label
                );
            }
            ChatInformRemap::Ok {
                data,
                consumed_by_message_id_cleared,
            } => {
                assert_eq!(
                    w["ok"],
                    json!(true),
                    "[{}] v5 kept the row and v4 dropped it ({})",
                    case.label,
                    w["reason"]
                );
                if consumed_by_message_id_cleared {
                    cleared += 1;
                }
                assert_eq!(
                    json!(consumed_by_message_id_cleared),
                    w["consumedByMessageIdCleared"],
                    "[{}] the cleared flag diverged",
                    case.label
                );

                // v4's `data` is `Omit<ChatInform, 'id'|'createdAt'|'updatedAt'>`,
                // so compare exactly the keys v4 carries.
                let d = &w["data"];
                let want_opt = |k: &str| match &d[k] {
                    Value::Null => None,
                    Value::String(s) => Some(s.clone()),
                    other => panic!("[{}] unexpected {k}: {other}", case.label),
                };
                assert_eq!(
                    data.chat_id,
                    d["chatId"].as_str().unwrap_or_default(),
                    "[{}] chatId",
                    case.label
                );
                assert_eq!(
                    data.batch_id,
                    d["batchId"].as_str().unwrap_or_default(),
                    "[{}] batchId",
                    case.label
                );
                assert_eq!(
                    data.participant_id,
                    d["participantId"].as_str().unwrap_or_default(),
                    "[{}] participantId",
                    case.label
                );
                assert_eq!(
                    data.content_markdown,
                    d["contentMarkdown"].as_str().unwrap_or_default(),
                    "[{}] contentMarkdown",
                    case.label
                );
                assert_eq!(
                    data.record_message_id,
                    want_opt("recordMessageId"),
                    "[{}] recordMessageId",
                    case.label
                );
                assert_eq!(
                    data.consumed_at,
                    want_opt("consumedAt"),
                    "[{}] consumedAt",
                    case.label
                );
                assert_eq!(
                    data.consumed_by_message_id,
                    want_opt("consumedByMessageId"),
                    "[{}] consumedByMessageId",
                    case.label
                );

                // v4's import mints the destination row's id and timestamps, so
                // the port must too — and must NOT carry the source id through.
                assert_ne!(
                    Some(data.id.as_str()),
                    case.inform["id"].as_str(),
                    "[{}] the SOURCE id survived the remap — v4's import calls \
                     `create(data)` with no CreateOptions, so the destination row \
                     mints its own",
                    case.label
                );
                assert!(
                    !data.created_at.is_empty() && data.created_at == data.updated_at,
                    "[{}] a created row's timestamps are one minted `now`",
                    case.label
                );
            }
        }
    }

    // Corpus guards: all three rules must actually be exercised, or a rule could
    // be deleted without a red.
    assert!(dropped >= 2, "the corpus must exercise BOTH drop rules");
    assert!(
        cleared >= 1,
        "the corpus must exercise the keep-with-null rule"
    );

    eprintln!(
        "OK: chat_informs remap matched oracle ({} cases, {dropped} dropped, {cleared} cleared).",
        spec.cases.len()
    );
}
