//! Tier-2 differential test: v4's chats equipped-outfit ops (Phase-2, the
//! conversation capstone, sub-unit 6 — `getEquippedOutfit` /
//! `getEquippedOutfitForCharacter` / `setEquippedOutfit` /
//! `removeEquippedItemFromAllChats`).
//!
//! Both sides run the SAME op sequence (`chats-outfits-tier2.json`) on a fresh
//! copy of the seed fixture (six chats with pinned ids/timestamps; some carry a
//! pre-seeded `equippedOutfit`, seeded via v4's real `repos.chats.create`), then
//! the `chats` table is dumped canonically and the post-op state asserted
//! identical. After the ops, both sides ALSO run the spec's `reads` (the two read
//! methods against the post-op state) and the marshaled results are diffed.
//!
//! The write ops are read-modify-writes of the `equippedOutfit` JSON column; the
//! CHAT's own `updatedAt` is NEVER bumped (v4 `_update` preserves it), and no
//! op mints an id/timestamp — so the dump is diffed with ZERO normalization. The
//! `equippedOutfit` JSON key order is reproduced byte-for-byte: the closed-schema
//! `EquippedSlots` (top/bottom/footwear/accessories) as a typed struct, the outer
//! characterId keys constrained to sorted order (see `chats_outfits.rs` header).
//!
//! Exercises: `setEquippedOutfit` on a chat with no outfit (creates the state),
//! adding a SECOND character (insertion-order append — the key-order test),
//! overwriting an existing character's slots; `removeEquippedItemFromAllChats`
//! nulling a referenced item across two chats (one with two characters, the item
//! in one slot of one character) and leaving an unreferenced chat untouched
//! (the "modified?" guard); plus a not-found `setEquippedOutfit` no-op. Reads bank
//! a created outfit, a two-character outfit, a not-found chat (null), and
//! getEquippedOutfitForCharacter for a present + an absent character.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-choutfit-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chats-outfits-fixture.ts
//!   QT_FIXTURE_CHOUTFIT=/tmp/qt-choutfit-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-outfits-tier2.ts > /tmp/oracle-choutfit.ndjson
//! Run:
//!   QT_ORACLE_CHOUTFIT=/tmp/oracle-choutfit.ndjson \
//!   QT_FIXTURE_CHOUTFIT=/tmp/qt-choutfit-fixture.db \
//!     cargo test -p quilltap-harness --test chats_outfits_tier2_equivalence

use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
    #[serde(default)]
    reads: Vec<Read>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "setEquippedOutfit")]
    SetEquippedOutfit {
        #[serde(rename = "chatId")]
        chat_id: String,
        #[serde(rename = "characterId")]
        character_id: String,
        slots: Value,
    },
    #[serde(rename = "removeEquippedItemFromAllChats")]
    RemoveEquippedItemFromAllChats {
        #[serde(rename = "itemId")]
        item_id: String,
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Read {
    #[serde(rename = "getEquippedOutfit")]
    GetEquippedOutfit {
        #[serde(rename = "chatId")]
        chat_id: String,
    },
    #[serde(rename = "getEquippedOutfitForCharacter")]
    GetEquippedOutfitForCharacter {
        #[serde(rename = "chatId")]
        chat_id: String,
        #[serde(rename = "characterId")]
        character_id: String,
    },
}

/// Map an `Option<Value>` read result to the oracle's JSON shape: `None` → JSON
/// `null` (v4 returns `null`, which `JSON.stringify` emits as `null`).
fn read_to_json(r: Option<Value>) -> Value {
    r.unwrap_or(Value::Null)
}

#[test]
fn chats_outfits_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHOUTFIT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHOUTFIT to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CHOUTFIT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHOUTFIT to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/chats-outfits-tier2.json"),
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

    let work = std::env::temp_dir().join(format!("qt-choutfit-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    let mut rust_reads: Vec<Value> = Vec::new();
    {
        let repo = writer.chat_outfits();
        for op in &spec.ops {
            match op {
                Op::SetEquippedOutfit {
                    chat_id,
                    character_id,
                    slots,
                } => {
                    repo.set_equipped_outfit(chat_id, character_id, slots)
                        .expect("set_equipped_outfit");
                }
                Op::RemoveEquippedItemFromAllChats { item_id } => {
                    repo.remove_equipped_item_from_all_chats(item_id)
                        .expect("remove_equipped_item_from_all_chats");
                }
            }
        }

        for r in &spec.reads {
            let got = match r {
                Read::GetEquippedOutfit { chat_id } => repo
                    .get_equipped_outfit(chat_id)
                    .expect("get_equipped_outfit"),
                Read::GetEquippedOutfitForCharacter {
                    chat_id,
                    character_id,
                } => repo
                    .get_equipped_outfit_for_character(chat_id, character_id)
                    .expect("get_equipped_outfit_for_character"),
            };
            rust_reads.push(read_to_json(got));
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

    // The read-method results (post-op state).
    let oracle_reads = oracle
        .get("reads")
        .cloned()
        .unwrap_or_else(|| Value::Array(vec![]));
    assert_eq!(
        Value::Array(rust_reads.clone()),
        oracle_reads,
        "read-method results diverged\n  rust:   {:?}\n  oracle: {}",
        rust_reads,
        oracle["reads"]
    );

    eprintln!("OK: chats outfits tier-2 matched oracle (dump + reads).");
}
