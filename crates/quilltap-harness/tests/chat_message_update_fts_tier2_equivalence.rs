//! Tier-2 differential test: `updateMessage` UNDER the FTS triggers (P4.105).
//!
//! v4's `updateMessage` is an `UPDATE … SET <every key of the parsed event>
//! WHERE id = ?` (`chats-messages.ops.ts:517`, `query-translator.ts:683`).
//! Under `f45a517a9`'s triggers only `chat_messages_fts_au` can fire on it, and
//! only when the DECODED `content` changed and the row already has a map row.
//! v5's `update_message` used to be a DELETE + re-INSERT — byte-identical to
//! v4 before `f45a517a9`, and NOT state-equivalent since: `_ad` + `_ai` fire
//! instead, so the message's `ftsId` is re-minted, unchanged content is
//! re-indexed past `_au`'s guard, a row that became eligible is indexed where
//! v4 leaves it out, and the base rowid moves. Search RESULTS are unchanged by
//! all four (the same tokens under a new id), which is why no family saw it.
//!
//! This family puts the STORAGE FORM in the comparand (the
//! `a-total-decoder-hides-a-lost-write` rule): after every op the whole
//! `chat_messages_fts_map` and every base rowid; at the end the FTS index's
//! rowids, its TOKENS per rowid (a temp `fts5vocab(…, instance)` table — the
//! index is contentless, so `SELECT content` is NULL on both engines), and the
//! `chat_messages` + `chats` dumps.
//!
//! The ops (`chat-message-update-fts.json`): (a) a `content` edit on an
//! indexed row → same `ftsId`, new tokens; (b) a non-content edit
//! (`reasoningContent` + `attachments`) → index untouched; (c) the SAME text
//! over a COMPRESSED cell (≥ 512 bytes, so `qt_text` is what compares, not the
//! BLOB) → untouched; (d) a `TOOL` row (never indexed) becomes an eligible
//! `USER` row with new text → still NOT indexed, because `_au` needs a map
//! row; (e) a `system` row edit → nothing;
//! (f) two edits in sequence → one `ftsId` throughout; (g) every base rowid
//! stable. The last seeded row is the untouched max of both id spaces, so a
//! re-INSERT anywhere above it must mint a new id rather than reuse the max.
//!
//! NORMALIZATION: NONE.
//!
//! Generate (Node 24, from the PINNED v4 worktree — drift-ledger §5.1):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-chatmsgupdatefts-fixture.db \
//!     $N/npx tsx "$V5W/harness/oracle/fixtures/build-chat-message-update-fts-fixture.ts"
//!   QT_FIXTURE_CHATMSGUPDATEFTS=/tmp/qt-chatmsgupdatefts-fixture.db \
//!     $N/npx tsx "$V5W/harness/oracle/cases/chat-message-update-fts-tier2.ts" \
//!     > /tmp/oracle-chatmsgupdatefts.ndjson
//! Run:
//!   QT_ORACLE_CHATMSGUPDATEFTS=/tmp/oracle-chatmsgupdatefts.ndjson \
//!   QT_FIXTURE_CHATMSGUPDATEFTS=/tmp/qt-chatmsgupdatefts-fixture.db \
//!     cargo test -p quilltap-harness --test chat_message_update_fts_tier2_equivalence -- --nocapture

use quilltap_core::db::Writer;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct Op {
    label: String,
    #[serde(rename = "chatId")]
    chat_id: String,
    #[serde(rename = "messageId")]
    message_id: String,
    updates: Value,
}

// The readback queries — byte-identical to the oracle case's.
const MAP_SQL: &str =
    r#"SELECT "ftsId", "messageId" FROM "chat_messages_fts_map" ORDER BY "ftsId""#;
const ROWIDS_SQL: &str = r#"SELECT "id", rowid AS "rid" FROM "chat_messages" ORDER BY "id""#;
const FTS_ROWIDS_SQL: &str = r#"SELECT rowid AS "rid" FROM "chat_messages_fts" ORDER BY rowid"#;
const VOCAB_DDL: &str = r#"CREATE VIRTUAL TABLE temp."qt_p4105_vocab" USING fts5vocab(main, "chat_messages_fts", instance)"#;
const VOCAB_SQL: &str =
    r#"SELECT "doc", "offset", "term" FROM temp."qt_p4105_vocab" ORDER BY "doc", "offset""#;

/// Every row as a JSON object keyed by column name — integers and text only,
/// which is all these queries project (better-sqlite3 hands v4 the same
/// shapes).
fn query_objects(conn: &Connection, sql: &str) -> Value {
    let mut stmt = conn
        .prepare(sql)
        .unwrap_or_else(|e| panic!("prepare {sql}: {e}"));
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let rows = stmt
        .query_map([], |row| {
            let mut o = Map::new();
            for (i, name) in names.iter().enumerate() {
                let v = match row.get_ref(i)? {
                    rusqlite::types::ValueRef::Integer(n) => json!(n),
                    rusqlite::types::ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                    rusqlite::types::ValueRef::Null => Value::Null,
                    other => panic!("{sql}: unexpected cell type {other:?}"),
                };
                o.insert(name.clone(), v);
            }
            Ok(Value::Object(o))
        })
        .unwrap_or_else(|e| panic!("query {sql}: {e}"))
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|e| panic!("rows {sql}: {e}"));
    Value::Array(rows)
}

fn snapshot(conn: &Connection) -> (Value, Value) {
    (
        query_objects(conn, MAP_SQL),
        query_objects(conn, ROWIDS_SQL),
    )
}

/// `messageId → ftsId` from a map snapshot, and `id → rowid` from a rowids
/// snapshot, as one keyed view.
fn keyed(map: &Value, rowids: &Value) -> BTreeMap<String, (Option<i64>, Option<i64>)> {
    let mut out: BTreeMap<String, (Option<i64>, Option<i64>)> = BTreeMap::new();
    for r in map.as_array().expect("map array") {
        let id = r["messageId"].as_str().expect("messageId").to_string();
        out.entry(id).or_default().0 = r["ftsId"].as_i64();
    }
    for r in rowids.as_array().expect("rowids array") {
        let id = r["id"].as_str().expect("id").to_string();
        out.entry(id).or_default().1 = r["rid"].as_i64();
    }
    out
}

/// What ONE op moved — every message whose `ftsId` (incl. gaining or losing a
/// map row) or base rowid changed, as sorted lines.
///
/// The per-op snapshots are cumulative, so comparing them absolutely lets ONE
/// broken op redden every op after it. Comparing each op's own DELTA on both
/// sides keeps the attribution exact — which is what lets each mutation proof
/// redden exactly the op it targets — and the absolute seed state plus every
/// delta agreeing implies the absolute states agree too.
fn delta(before: &(Value, Value), after: &(Value, Value)) -> Vec<String> {
    let b = keyed(&before.0, &before.1);
    let a = keyed(&after.0, &after.1);
    let mut ids: Vec<&String> = b.keys().chain(a.keys()).collect();
    ids.sort();
    ids.dedup();
    let mut out = Vec::new();
    for id in ids {
        let (bf, br) = b.get(id).copied().unwrap_or_default();
        let (af, ar) = a.get(id).copied().unwrap_or_default();
        if bf != af {
            out.push(format!("{id}: ftsId {bf:?} -> {af:?}"));
        }
        if br != ar {
            out.push(format!("{id}: rowid {br:?} -> {ar:?}"));
        }
    }
    out
}

fn assert_dump_eq(got: &Value, oracle: &Value, label: &str) {
    assert_eq!(got["table"], oracle["table"], "{label}: table name");
    assert_eq!(
        got["columns"], oracle["columns"],
        "{label}: column set / order"
    );
    assert_eq!(
        got["rows"], oracle["rows"],
        "{label}: row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["rows"]
    );
}

#[test]
fn chat_message_update_fts_tier2_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_CHATMSGUPDATEFTS") else {
        eprintln!("SKIP: set QT_ORACLE_CHATMSGUPDATEFTS to the oracle NDJSON (see header).");
        return;
    };
    let Ok(fixture) = std::env::var("QT_FIXTURE_CHATMSGUPDATEFTS") else {
        eprintln!("SKIP: set QT_FIXTURE_CHATMSGUPDATEFTS to the seed fixture .db (see header).");
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/chat-message-update-fts.json"),
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

    let work = std::env::temp_dir().join(format!(
        "qt-chatmsgupdatefts-rust-{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    let conn = writer.connection();

    // The fixture must actually carry the triggers, or every row below
    // compares two trigger-less engines and proves nothing.
    let triggers: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' \
             AND name IN ('chat_messages_fts_ai','chat_messages_fts_ad','chat_messages_fts_au')",
            [],
            |r| r.get(0),
        )
        .expect("count triggers");
    assert_eq!(
        triggers, 3,
        "the fixture must carry v4's three FTS triggers"
    );

    // The seed state first — both sides read the same file, so a mismatch here
    // is an instrument fault, not a port defect.
    let (map0, rowids0) = snapshot(conn);
    assert_eq!(map0, oracle["initial"]["map"], "initial map (instrument)");
    assert_eq!(
        rowids0, oracle["initial"]["rowids"],
        "initial rowids (instrument)"
    );

    let after = oracle["afterEach"].as_array().expect("afterEach");
    assert_eq!(after.len(), spec.ops.len(), "op count: spec vs oracle");

    let mut diverged: Vec<String> = Vec::new();
    {
        let repo = writer.chat_messages();
        let mut rust_prev = (map0, rowids0);
        let mut oracle_prev = (
            oracle["initial"]["map"].clone(),
            oracle["initial"]["rowids"].clone(),
        );
        for (op, want) in spec.ops.iter().zip(after) {
            assert_eq!(want["label"], json!(op.label), "oracle out of step");
            let returned = repo
                .update_message(&op.chat_id, &op.message_id, &op.updates)
                .unwrap_or_else(|e| panic!("{}: update_message: {e}", op.label));
            assert_eq!(json!(returned), want["returned"], "{}: returned", op.label);
            let rust_next = snapshot(conn);
            let oracle_next = (want["map"].clone(), want["rowids"].clone());
            let got = delta(&rust_prev, &rust_next);
            let exp = delta(&oracle_prev, &oracle_next);
            if got != exp {
                diverged.push(format!(
                    "{}: what the op moved\n    rust:   {got:?}\n    oracle: {exp:?}",
                    op.label
                ));
            }
            rust_prev = rust_next;
            oracle_prev = oracle_next;
        }
    }

    conn.execute_batch(VOCAB_DDL)
        .expect("create the vocab view");
    let fts_rowids = query_objects(conn, FTS_ROWIDS_SQL);
    let vocab = query_objects(conn, VOCAB_SQL);
    if fts_rowids != oracle["ftsRowids"] {
        diverged.push(format!(
            "fts index rowids\n    rust:   {fts_rowids}\n    oracle: {}",
            oracle["ftsRowids"]
        ));
    }
    if vocab != oracle["vocab"] {
        diverged.push(format!(
            "fts index tokens\n    rust:   {vocab}\n    oracle: {}",
            oracle["vocab"]
        ));
    }
    assert!(
        diverged.is_empty(),
        "{} storage-form divergence(s) under the FTS triggers:\n  - {}",
        diverged.len(),
        diverged.join("\n  - ")
    );

    let got_messages = writer
        .dump_table_json("chat_messages", "id")
        .expect("dump chat_messages");
    let got_chats = writer.dump_table_json("chats", "id").expect("dump chats");
    let _ = std::fs::remove_file(&work);
    assert_dump_eq(&got_messages, &oracle["messages"], "chat_messages");
    assert_dump_eq(&got_chats, &oracle["chats"], "chats");

    eprintln!(
        "OK: update_message under the FTS triggers matched oracle ({} ops, {} indexed rows).",
        spec.ops.len(),
        fts_rowids.as_array().map(Vec::len).unwrap_or(0)
    );
}

/// The oracle itself must show the four v4 properties this family exists to
/// pin — or a green differential proves only that both sides agree on a
/// corpus that never exercised them. Reads the oracle alone.
#[test]
fn the_oracle_shows_v4s_four_update_properties() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_CHATMSGUPDATEFTS") else {
        eprintln!("SKIP: set QT_ORACLE_CHATMSGUPDATEFTS (see header).");
        return;
    };
    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .expect("read oracle")
            .trim(),
    )
    .expect("parse oracle");

    // (1) + (f) the map never moves across ANY op, and (4) nor do the rowids.
    for step in oracle["afterEach"].as_array().expect("afterEach") {
        assert_eq!(
            step["map"], oracle["initial"]["map"],
            "{}: map moved in v4",
            step["label"]
        );
        assert_eq!(
            step["rowids"], oracle["initial"]["rowids"],
            "{}: a base rowid moved in v4",
            step["label"]
        );
    }
    // (3) the TOOL row became an eligible USER row and is STILL not in the
    // map (the loop above already proved the map never moved; this proves the
    // row was absent to begin with, so "never moved" means "never indexed").
    let tool_row = "e0000070-0000-4000-8000-000000000004";
    assert!(
        !oracle["initial"]["map"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["messageId"] == tool_row),
        "the TOOL row must enter with no map row"
    );
    // (a) the edited row's tokens ARE the new text (`_au` fired): `slept` is
    // only in the edit.
    let terms: Vec<&str> = oracle["vocab"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["doc"] == 1)
        .map(|v| v["term"].as_str().unwrap())
        .collect();
    assert!(
        terms.contains(&"slept"),
        "op (a): the index must hold the edit: {terms:?}"
    );
    assert!(
        !terms.contains(&"waved"),
        "op (a): the old token must be gone: {terms:?}"
    );
    // (c) the compressed cell really is a BLOB, so `qt_text` is what compared.
    let row3 = oracle["messages"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "e0000070-0000-4000-8000-000000000003")
        .expect("row 3");
    let cell = serde_json::to_string(&row3["content"]).unwrap();
    assert!(
        !cell.contains("brass orrery"),
        "op (c) needs a COMPRESSED cell, got plaintext: {cell}"
    );
}
