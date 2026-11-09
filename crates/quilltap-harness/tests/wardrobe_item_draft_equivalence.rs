//! Tier-1 differential (P4.D377 item 7 — v4 `600c2906f`, the wardrobe
//! Duplicate): the server-side item draft.
//!
//! Diffs [`quilltap_core::wardrobe::item_draft::draft_from_item`] and
//! [`draft_to_payload`](quilltap_core::wardrobe::item_draft::draft_to_payload)
//! against v4's REAL `lib/wardrobe/item-draft.ts` (`draftFromItem`,
//! `draftToPayload`). The draft is compared field by field; the payload BYTE
//! for byte (`serde_json::to_string` against v4's `JSON.stringify` — key order
//! is part of the claim, and `serde_json::Map ==` ignores it). Each corpus item
//! enters v5 through `WardrobeItem::from_read_value`, the read-item shape the
//! duplicate route's `find_item` returns.
//!
//! Generate the oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/node --import tsx $V5W/harness/oracle/cases/wardrobe-item-draft.ts \
//!     > /tmp/oracle-wardrobe-item-draft.ndjson
//! Run:
//!   QT_ORACLE_WARDROBE_ITEM_DRAFT=/tmp/oracle-wardrobe-item-draft.ndjson \
//!     cargo test -p quilltap-harness --test wardrobe_item_draft_equivalence -- --nocapture

use quilltap_core::vault_overlay::WardrobeItem;
use quilltap_core::wardrobe::item_draft::{draft_from_item, draft_to_payload, WardrobeItemDraft};
use serde::Deserialize;
use serde_json::{json, Value};

/// The corpus's own marker — a stale NDJSON cannot pass quietly.
const BASELINE: &str = "p4.d377-wardrobe-item-draft";

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "meta")]
    Meta { baseline: String },
    #[serde(rename = "case")]
    Case {
        id: String,
        item: Value,
        draft: Value,
        #[serde(rename = "payloadJson")]
        payload_json: String,
    },
}

/// The draft in v4's `WardrobeItemDraft` declaration order.
fn draft_json(d: &WardrobeItemDraft) -> String {
    serde_json::to_string(&json!({
        "title": d.title,
        "description": d.description,
        "imagePrompt": d.image_prompt,
        "appropriateness": d.appropriateness,
        "isDefault": d.is_default,
        "types": d.types,
        "componentItemIds": d.component_item_ids,
        "replace": d.replace,
        "pronounGuidance": d.pronoun_guidance,
        "folder": d.folder,
    }))
    .unwrap()
}

#[test]
fn wardrobe_item_draft_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_WARDROBE_ITEM_DRAFT") else {
        eprintln!(
            "SKIP: set QT_ORACLE_WARDROBE_ITEM_DRAFT to the oracle NDJSON (see test header)."
        );
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle NDJSON");
    assert!(
        text.contains(r#""imagePrompt":null"#) && text.contains(r#""replace":false"#),
        "oracle NDJSON lacks its markers — truncated or stale?"
    );
    let mut saw_meta = false;
    let mut cases = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).expect("parse oracle row") {
            Row::Meta { baseline } => {
                assert_eq!(baseline, BASELINE, "oracle NDJSON is from another corpus");
                saw_meta = true;
            }
            Row::Case {
                id,
                item,
                draft,
                payload_json,
            } => {
                cases += 1;
                let d = draft_from_item(&WardrobeItem::from_read_value(&item));
                let got_draft = draft_json(&d);
                let want_draft = serde_json::to_string(&draft).unwrap();
                if got_draft != want_draft {
                    failures.push(format!("'{id}' draft: v5 {got_draft} != v4 {want_draft}"));
                }
                let got_payload = serde_json::to_string(&draft_to_payload(&d)).unwrap();
                if got_payload != payload_json {
                    failures.push(format!(
                        "'{id}' payload: v5 {got_payload} != v4 {payload_json}"
                    ));
                }
            }
        }
    }
    assert!(saw_meta, "no meta row — not this corpus");
    assert!(cases >= 25, "only {cases} cases — a truncated NDJSON?");
    assert!(
        failures.is_empty(),
        "{} divergences over {cases} rows:\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!("wardrobe_item_draft: {cases} rows equal (draft + payload bytes)");
}
