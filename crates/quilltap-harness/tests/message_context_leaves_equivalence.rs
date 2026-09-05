//! Tier-1 differential test: the `buildMessageContext` pure leaves
//! (`quilltap_core::services::message_context` — v4
//! `context-builder.service.ts`'s `buildConversationMessages` /
//! `normalizeWhisperRoles` / `collectLanternImageFileIdsForCharacter` /
//! `collectUnseenUserAttachmentsForCharacter`).
//!
//! The oracle drives v4's REAL exported leaves over a fixed corpus; this test
//! re-runs the ported leaves on the same inputs and compares each output
//! field-for-field (outputs projected to the WhisperMessage field set with
//! null/undefined dropped, so `JSON.stringify`'s `undefined`-dropping + v4's
//! explicit `systemSender: null` both collapse to the Rust `Option::None`).
//!
//! Generate the oracle (Node 24, from the v4 checkout):
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/message-context-leaves.ts \
//!     > /tmp/oracle-message-context-leaves.ndjson
//! Run:
//!   QT_ORACLE_MESSAGE_CONTEXT_LEAVES=/tmp/oracle-message-context-leaves.ndjson \
//!     cargo test -p quilltap-harness --test message_context_leaves_equivalence

use quilltap_core::services::build_context::MessageWithParticipant;
use quilltap_core::services::message_context::{
    build_conversation_messages, collect_lantern_image_file_ids_for_character,
    collect_unseen_user_attachments_for_character, normalize_whisper_roles, ConversationMessage,
    WhisperMessage,
};
use serde_json::{json, Map, Value};

/// Insert `k → v` only when `v` is `Some` (drops `None` → mirrors v4's
/// null/undefined-dropping projection).
fn put_opt(m: &mut Map<String, Value>, k: &str, v: Option<Value>) {
    if let Some(v) = v {
        m.insert(k.to_string(), v);
    }
}

fn parse_messages(v: &Value) -> Vec<WhisperMessage> {
    v.as_array()
        .map(|a| a.iter().map(WhisperMessage::from_value).collect())
        .unwrap_or_default()
}

fn conv_to_value(c: &ConversationMessage) -> Value {
    let mut m = Map::new();
    m.insert("role".into(), json!(c.role));
    m.insert("content".into(), json!(c.content));
    put_opt(&mut m, "id", c.id.clone().map(Value::from));
    put_opt(
        &mut m,
        "thoughtSignature",
        c.thought_signature.clone().map(Value::from),
    );
    Value::Object(m)
}

fn mwp_to_value(m0: &MessageWithParticipant) -> Value {
    let mut m = Map::new();
    put_opt(&mut m, "id", m0.id.clone().map(Value::from));
    m.insert("role".into(), json!(m0.role));
    m.insert("content".into(), json!(m0.content));
    put_opt(
        &mut m,
        "participantId",
        m0.participant_id.clone().map(Value::from),
    );
    put_opt(
        &mut m,
        "thoughtSignature",
        m0.thought_signature.clone().map(Value::from),
    );
    put_opt(&mut m, "createdAt", m0.created_at.clone().map(Value::from));
    put_opt(
        &mut m,
        "targetParticipantIds",
        m0.target_participant_ids.clone().map(|a| json!(a)),
    );
    Value::Object(m)
}

/// Project a normalized [`WhisperMessage`] to the WhisperMessage field set
/// (null/undefined dropped), matching the oracle's `project(m, WHISPER_FIELDS)`.
fn whisper_to_value(w: &WhisperMessage) -> Value {
    let mut m = Map::new();
    put_opt(&mut m, "type", w.message_type.clone().map(Value::from));
    put_opt(&mut m, "role", w.role.clone().map(Value::from));
    put_opt(&mut m, "content", w.content.clone().map(Value::from));
    put_opt(&mut m, "id", w.id.clone().map(Value::from));
    put_opt(
        &mut m,
        "thoughtSignature",
        w.thought_signature.clone().map(Value::from),
    );
    put_opt(
        &mut m,
        "participantId",
        w.participant_id.clone().map(Value::from),
    );
    put_opt(
        &mut m,
        "targetParticipantIds",
        w.target_participant_ids.clone().map(|a| json!(a)),
    );
    put_opt(&mut m, "createdAt", w.created_at.clone().map(Value::from));
    put_opt(
        &mut m,
        "attachments",
        w.attachments.clone().map(|a| json!(a)),
    );
    put_opt(
        &mut m,
        "systemSender",
        w.system_sender.clone().map(Value::from),
    );
    put_opt(&mut m, "systemKind", w.system_kind.clone().map(Value::from));
    put_opt(
        &mut m,
        "opaqueContent",
        w.opaque_content.clone().map(Value::from),
    );
    Value::Object(m)
}

fn assert_eq_json(name: &str, kind: &str, got: &Value, want: &Value) {
    if got != want {
        panic!(
            "{kind} mismatch for {name}\n--- got ---\n{}\n--- want ---\n{}",
            serde_json::to_string_pretty(got).unwrap(),
            serde_json::to_string_pretty(want).unwrap(),
        );
    }
}

#[test]
fn message_context_leaves_match_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_MESSAGE_CONTEXT_LEAVES") else {
        eprintln!("SKIP: set QT_ORACLE_MESSAGE_CONTEXT_LEAVES to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("oracle readable");
    let mut count = 0usize;
    let mut by_kind: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("oracle line parses");
        let kind = row.get("kind").and_then(Value::as_str).unwrap();
        let name = row.get("name").and_then(Value::as_str).unwrap();
        let messages = parse_messages(row.get("messages").unwrap());
        count += 1;
        *by_kind.entry(kind.to_string()).or_default() += 1;

        match kind {
            "bcm" => {
                let is_multi = row
                    .get("isMultiCharacter")
                    .and_then(Value::as_bool)
                    .unwrap();
                let (conv, mwp) = build_conversation_messages(&messages, is_multi);
                let got_conv = Value::Array(conv.iter().map(conv_to_value).collect());
                assert_eq_json(
                    name,
                    "conversationMessages",
                    &got_conv,
                    row.get("conversationMessages").unwrap(),
                );
                let got_mwp = match &mwp {
                    Some(m) => Value::Array(m.iter().map(mwp_to_value).collect()),
                    None => Value::Null,
                };
                assert_eq_json(
                    name,
                    "messagesWithParticipants",
                    &got_mwp,
                    row.get("messagesWithParticipants").unwrap(),
                );
            }
            "nwr" => {
                let is_opaque = row
                    .get("isOpaqueAnywhere")
                    .and_then(Value::as_bool)
                    .unwrap();
                let out = normalize_whisper_roles(&messages, is_opaque);
                let got = Value::Array(out.iter().map(whisper_to_value).collect());
                assert_eq_json(
                    name,
                    "normalizeWhisperRoles",
                    &got,
                    row.get("output").unwrap(),
                );
            }
            "lantern" => {
                let cp = row
                    .get("characterParticipantId")
                    .and_then(Value::as_str)
                    .unwrap();
                let is_multi = row
                    .get("isMultiCharacter")
                    .and_then(Value::as_bool)
                    .unwrap();
                let cutoff = row.get("historyCutoff").and_then(Value::as_str);
                let lookback = row.get("lookback").and_then(Value::as_u64).unwrap() as usize;
                let out = collect_lantern_image_file_ids_for_character(
                    &messages, cp, is_multi, cutoff, lookback,
                );
                let got = json!(out);
                assert_eq_json(name, "collectLantern", &got, row.get("output").unwrap());
            }
            "unseen" => {
                let cp = row
                    .get("characterParticipantId")
                    .and_then(Value::as_str)
                    .unwrap();
                let is_multi = row
                    .get("isMultiCharacter")
                    .and_then(Value::as_bool)
                    .unwrap();
                let cutoff = row.get("historyCutoff").and_then(Value::as_str);
                let lookback = row.get("lookback").and_then(Value::as_u64).unwrap() as usize;
                let out = collect_unseen_user_attachments_for_character(
                    &messages, cp, is_multi, cutoff, lookback,
                );
                let got = Value::Array(
                    out.iter()
                        .map(|u| json!({ "messageId": u.message_id, "fileIds": u.file_ids }))
                        .collect(),
                );
                assert_eq_json(
                    name,
                    "collectUnseenUserAttachments",
                    &got,
                    row.get("output").unwrap(),
                );
            }
            other => panic!("unknown oracle kind: {other}"),
        }
    }
    assert!(count > 0, "no oracle rows exercised");
    // Stale-oracle floors. An oracle regenerated from a tree that predates
    // `e288ae2ec` carries no `unseen` rows at all and every assertion above
    // would pass vacuously; v4 ships TEN cases for the USER-side walk and this
    // corpus transcribes all ten.
    assert_eq!(
        by_kind.get("unseen").copied().unwrap_or(0),
        10,
        "expected v4's ten collectUnseenUserAttachmentsForCharacter cases; got {by_kind:?}"
    );
    assert!(
        by_kind.get("lantern").copied().unwrap_or(0) >= 4,
        "the Lantern leaf's rows went missing: {by_kind:?}"
    );
}
