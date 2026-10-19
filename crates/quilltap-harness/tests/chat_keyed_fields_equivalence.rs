//! P4.D317 — tier-1 EXACT differential for the pure bug-252 pass:
//! `quilltap_core::chat::character_keyed_fields::remap_character_keyed_chat_fields`
//! against v4's REAL `remapCharacterKeyedChatFields`
//! (`lib/chat/character-keyed-fields.ts`, `e8e17b2d9`).
//!
//! Every oracle line carries its own input — the chat and the remaps as DATA
//! (lookup tables, a `characterMiss` rule, ONE `mint-<n>` counter for unseen
//! files / messages / minted characters). Compared per row, with no
//! normalization: the patch as BYTES (`patchText` = v4's `JSON.stringify` —
//! key order included; `serde_json::Map ==` is order-blind), the `unreadable`
//! list, `droppedEntries`, and every mint in order (`minted`).
//!
//! `expectedDifferent` rows are P4.D317 R-2's three V8-only `JSON.parse`
//! classes (`1e400`, a lone surrogate, nesting past 128): v4 reads the column,
//! v5's `serde_json` refuses it. They are asserted the OTHER way — v5 reports
//! the column `unreadable` and patches nothing — so a v5 parser that closed the
//! gap would trip them (recorded, not closed).
//!
//! Regenerate (Node 24, from the v4 checkout at the pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx "$V5W/harness/oracle/cases/character-keyed-fields.ts" \
//!     > /tmp/oracle-character-keyed-fields.ndjson
//! then:
//!   QT_ORACLE_CHAT_KEYED_FIELDS=/tmp/oracle-character-keyed-fields.ndjson \
//!     cargo test -p quilltap-harness --test chat_keyed_fields_equivalence -- --nocapture

use std::cell::RefCell;
use std::collections::HashMap;

use quilltap_core::chat::character_keyed_fields::{
    remap_character_keyed_chat_fields, CharacterKeyedRemaps, CHARACTER_KEYED_CHAT_FIELDS,
};
use serde_json::{json, Value};

fn table(v: &Value) -> HashMap<String, Option<String>> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p[0].as_str().unwrap().to_string(),
                p[1].as_str().map(str::to_string),
            )
        })
        .collect()
}

/// The oracle's remaps, replayed: `(patchText, unreadable, droppedEntries, minted)`.
fn replay(input: &Value) -> (String, Value, usize, Value) {
    let remaps = &input["remaps"];
    let characters = table(&remaps["characters"]);
    let items = table(&remaps["items"]);
    let files = table(&remaps["files"]);
    let messages = table(&remaps["messages"]);
    let miss = remaps["characterMiss"].as_str().unwrap().to_string();
    let minted: RefCell<Vec<Value>> = RefCell::new(Vec::new());
    let mint = |kind: &str, from: &str| -> String {
        let to = format!("mint-{}", minted.borrow().len() + 1);
        minted
            .borrow_mut()
            .push(json!({ "kind": kind, "from": from, "to": to }));
        to
    };
    let character = |id: &str| -> Option<String> {
        if let Some(v) = characters.get(id) {
            return v.clone();
        }
        match miss.as_str() {
            "keep" => Some(id.to_string()),
            "mint" => Some(mint("character", id)),
            _ => None,
        }
    };
    let item = |id: &str| items.get(id).cloned().flatten().unwrap_or(id.to_string());
    let file = |id: &str| match files.get(id).cloned().flatten() {
        Some(v) => v,
        None => mint("file", id),
    };
    let message = |id: &str| match messages.get(id).cloned().flatten() {
        Some(v) => v,
        None => mint("message", id),
    };
    let out = remap_character_keyed_chat_fields(
        &input["chat"],
        &CharacterKeyedRemaps {
            character_id: &character,
            wardrobe_item_id: &item,
            file_id: &file,
            message_id: &message,
        },
    );
    (
        serde_json::to_string(&out.patch).unwrap(),
        json!(out.unreadable),
        out.dropped_entries,
        Value::Array(minted.into_inner()),
    )
}

#[test]
fn chat_keyed_fields_match_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_CHAT_KEYED_FIELDS") else {
        eprintln!("SKIP: QT_ORACLE_CHAT_KEYED_FIELDS unset");
        return;
    };
    let lines: Vec<Value> = std::fs::read_to_string(&path)
        .expect("read oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line"))
        .collect();
    assert!(!lines.is_empty(), "the oracle wrote no lines");

    let mut failures = Vec::new();
    let (mut rows, mut expected_different, mut fields_lines) = (0usize, 0usize, 0usize);
    for line in &lines {
        match line["kind"].as_str().unwrap() {
            // v4's first `it`: "lists the four keyed columns" — in v4's order.
            "fields" => {
                fields_lines += 1;
                assert_eq!(line["fields"], json!(CHARACTER_KEYED_CHAT_FIELDS));
            }
            "remap" => {
                rows += 1;
                let name = line["name"].as_str().unwrap();
                let (patch, unreadable, dropped, minted) = replay(&line["input"]);
                if let Some(reason) = line["expectedDifferent"].as_str() {
                    expected_different += 1;
                    // R-2: v4 READ the column (a non-empty patch); v5 cannot.
                    let field = "pendingOutfitNotifications";
                    let v4_read = line["patchText"].as_str().unwrap().contains(field);
                    if !(v4_read && patch == "{}" && unreadable == json!([field])) {
                        failures.push(format!(
                            "[{name}] EXPECTED-DIFFERENT row no longer differs ({reason}): \
                             rust patch {patch} unreadable {unreadable}"
                        ));
                    } else {
                        println!("EXPECTED-DIFFERENT {name}: {reason}");
                    }
                    continue;
                }
                let got = json!([patch, unreadable, dropped, minted]).to_string();
                let want = json!([
                    line["patchText"],
                    line["unreadable"],
                    line["droppedEntries"],
                    line["minted"]
                ])
                .to_string();
                if got != want {
                    failures.push(format!("[{name}]\n  rust:   {got}\n  oracle: {want}"));
                } else {
                    println!("OK {name}");
                }
            }
            other => panic!("unknown oracle kind {other}"),
        }
    }
    assert_eq!(
        (fields_lines, rows, expected_different),
        (1, 33, 3),
        "the corpus's line counts"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
