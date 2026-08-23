//! Tier-1 differential: the attachment anchor selector (P4.D106; v4 `a14a1811`
//! bug 95, `lib/services/chat-message/context-builder.service.ts`
//! `selectAttachmentAnchorIndex`), ported as
//! `quilltap_core::services::message_context::select_attachment_anchor_index`.
//!
//! The oracle drives v4's REAL export over v4's own six unit-test shapes (the
//! corpus floor) plus adversarial shapes computed on both sides: tier-1's
//! role-blindness, last-wins within each tier, tier-2's exact lowercase
//! `role === 'user'` + truthy-id + set membership, the tier-3 floor, and the
//! -1 arm. The Rust side rebuilds each message list and compares the index.
//!
//! Regenerate the oracle (from the v4 checkout):
//!   cd ~/source/quilltap-server
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   npx tsx $V5W/harness/oracle/cases/attachment-anchor.ts > /tmp/oracle-attachment-anchor.ndjson
//! Run:
//!   QT_ORACLE_ATTACHMENT_ANCHOR=/tmp/oracle-attachment-anchor.ndjson cargo test -p quilltap-harness --test attachment_anchor_equivalence -- --nocapture

use std::collections::HashSet;

use quilltap_core::services::message_context::{select_attachment_anchor_index, AnchorMessage};
use serde::Deserialize;

#[derive(Deserialize)]
struct RowMeta {
    #[serde(rename = "messageId")]
    message_id: Option<String>,
    #[serde(rename = "isUserTurn")]
    is_user_turn: Option<bool>,
}

#[derive(Deserialize)]
struct RowMsg {
    role: String,
    metadata: Option<RowMeta>,
}

#[derive(Deserialize)]
struct Row {
    label: String,
    messages: Vec<RowMsg>,
    #[serde(rename = "userTurnIds")]
    user_turn_ids: Vec<String>,
    index: i64,
}

#[test]
fn attachment_anchor_matches_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_ATTACHMENT_ANCHOR") else {
        eprintln!("QT_ORACLE_ATTACHMENT_ANCHOR not set; skipping");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle NDJSON");
    let mut rows = 0usize;
    let mut labels: HashSet<String> = HashSet::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).expect("parse oracle row");
        let msgs: Vec<AnchorMessage> = row
            .messages
            .iter()
            .map(|m| AnchorMessage {
                role: &m.role,
                message_id: m.metadata.as_ref().and_then(|md| md.message_id.as_deref()),
                is_user_turn: m
                    .metadata
                    .as_ref()
                    .and_then(|md| md.is_user_turn)
                    .unwrap_or(false),
            })
            .collect();
        let ids: HashSet<String> = row.user_turn_ids.iter().cloned().collect();
        let got = select_attachment_anchor_index(&msgs, &ids);
        assert_eq!(got, row.index, "anchor mismatch for {}", row.label);
        labels.insert(row.label);
        rows += 1;
    }
    // Corpus floor: v4's six test shapes must be present by name, plus the
    // adversarial growth.
    for required in [
        "v4_prefers_new_user_message",
        "v4_historical_turn_not_staff_whispers",
        "v4_tool_exchange_trails",
        "v4_floor_last_user_role",
        "v4_no_user_role_at_all",
        "v4_user_turn_id_on_assistant_row",
    ] {
        assert!(labels.contains(required), "missing v4 shape {required}");
    }
    assert!(rows >= 18, "expected the grown corpus, got {rows} rows");
    println!("attachment_anchor_equivalence: {rows} rows OK");
}
