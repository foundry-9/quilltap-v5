//! P4.D294 — tier-1 EXACT differential for bug 228's remint:
//! `quilltap_core::services::quilltap_import::message_id_remap::{
//! remint_chat_message_ids, remap_imported_message_id}` against v4's REAL
//! `remintChatMessageIds` / `remapImportedMessageId`
//! (`lib/import/quilltap-import/message-id-remap.ts`, `8050958be`, widened by
//! `c2795733f` / `ac6348c5f` / `64743ee13`) at the `83334b9a5` pin.
//!
//! Every oracle line names its corpus row (`harness/oracle/fixtures/
//! message-id-remap.json`), which this test reads too, so both sides replay the
//! same input; both mint with the same per-row counter (`m-1`, `m-2`, …), so
//! nothing is normalized. Compared: the reminted messages (serialized — key
//! order included), the chat patch's BYTES, the id map in insertion order,
//! `droppedRefs`, `summaryCleared`; and each remap row's answer.
//!
//! Regenerate (Node 24, from the v4 checkout at the pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx "$V5W/harness/oracle/cases/message-id-remap.ts" \
//!     > /tmp/oracle-message-id-remap.ndjson
//! then:
//!   QT_ORACLE_MESSAGE_ID_REMAP=/tmp/oracle-message-id-remap.ndjson \
//!     cargo test -p quilltap-harness --test message_id_remap_equivalence -- --nocapture

use std::collections::HashMap;

use quilltap_core::services::quilltap_import::message_id_remap::{
    remap_imported_message_id, remint_chat_message_ids, MessageIdMaps,
};
use serde_json::Value;

const CORPUS: &str = include_str!("../../../harness/oracle/fixtures/message-id-remap.json");

fn rows<'a>(spec: &'a Value, kind: &str) -> HashMap<&'a str, &'a Value> {
    spec[kind]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["label"].as_str().unwrap(), r))
        .collect()
}

#[test]
fn message_id_remap_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_MESSAGE_ID_REMAP") else {
        eprintln!("SKIP: QT_ORACLE_MESSAGE_ID_REMAP unset");
        return;
    };
    let spec: Value = serde_json::from_str(CORPUS).expect("the corpus parses");
    let remint_rows = rows(&spec, "remint");
    let remap_rows = rows(&spec, "remap");
    let lines: Vec<Value> = std::fs::read_to_string(&path)
        .expect("read oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line"))
        .collect();
    assert!(!lines.is_empty(), "the oracle wrote no lines");

    let mut failures = Vec::new();
    let (mut remints, mut remaps) = (0usize, 0usize);
    for line in &lines {
        let label = line["label"].as_str().unwrap();
        match line["kind"].as_str().unwrap() {
            "remint" => {
                remints += 1;
                let row = remint_rows[label];
                let chat = row["chat"].as_object().unwrap();
                let messages = row["messages"].as_array().unwrap();
                let mut n = 0u32;
                let got = remint_chat_message_ids(chat, messages, || {
                    n += 1;
                    format!("m-{n}")
                });
                let got_messages = serde_json::to_string(&got.messages).unwrap();
                let want_messages = serde_json::to_string(&line["messages"]).unwrap();
                if got_messages != want_messages {
                    failures.push(format!(
                        "[{label}] messages\n  rust:   {got_messages}\n  oracle: {want_messages}"
                    ));
                }
                let got_patch = serde_json::to_string(&got.chat_patch).unwrap();
                if got_patch != line["chatPatch"].as_str().unwrap() {
                    failures.push(format!(
                        "[{label}] chatPatch\n  rust:   {got_patch}\n  oracle: {}",
                        line["chatPatch"]
                    ));
                }
                let got_map: Vec<Value> = got
                    .id_map
                    .iter()
                    .map(|(k, v)| serde_json::json!([k, v]))
                    .collect();
                if Value::Array(got_map.clone()) != line["idMap"] {
                    failures.push(format!(
                        "[{label}] idMap\n  rust:   {got_map:?}\n  oracle: {}",
                        line["idMap"]
                    ));
                }
                if Some(got.dropped_refs as u64) != line["droppedRefs"].as_u64() {
                    failures.push(format!(
                        "[{label}] droppedRefs rust {} vs oracle {}",
                        got.dropped_refs, line["droppedRefs"]
                    ));
                }
                if Some(got.summary_cleared) != line["summaryCleared"].as_bool() {
                    failures.push(format!(
                        "[{label}] summaryCleared rust {} vs oracle {}",
                        got.summary_cleared, line["summaryCleared"]
                    ));
                }
            }
            "remap" => {
                remaps += 1;
                let row = remap_rows[label];
                let maps: MessageIdMaps = row["maps"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(chat, m)| {
                        let inner = m
                            .as_object()
                            .unwrap()
                            .iter()
                            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
                            .collect();
                        (chat.clone(), inner)
                    })
                    .collect();
                let got = remap_imported_message_id(
                    &maps,
                    row["sourceChatId"].as_str(),
                    row["messageId"].as_str().unwrap(),
                );
                let got = got.map_or(Value::Null, Value::String);
                if got != line["got"] {
                    failures.push(format!(
                        "[{label}] remap rust {got} vs oracle {}",
                        line["got"]
                    ));
                }
            }
            other => failures.push(format!("[{label}] unknown kind {other}")),
        }
    }
    assert_eq!(
        (remints, remaps),
        (remint_rows.len(), remap_rows.len()),
        "every corpus row has its oracle line"
    );
    assert!(
        failures.is_empty(),
        "{} mismatch(es):\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!("message_id_remap: {remints} remint + {remaps} remap rows equal");
}
