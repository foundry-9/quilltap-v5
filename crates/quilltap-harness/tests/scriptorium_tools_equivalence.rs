//! Differential test — the Project Scriptorium tool trio (`read_conversation` /
//! `upsert_annotation` / `delete_annotation`, W4.1d batch 1).
//!
//! Both sides open a COPY of one baked fixture (chats + seed annotations, ids +
//! timestamps pinned), run the SAME op sequence (state accumulates), and compare
//! each op's Output JSON + formatted string **byte-exact**. After the sequence,
//! the `conversation_annotations` table is diffed in the fully-placeholdered
//! natural-key-sorted form — the write ops (upsert/delete) mint fresh
//! ids/timestamps on each side, so `id`/`createdAt`/`updatedAt` are replaced with
//! placeholders and rows sorted by `(chatId, messageIndex, characterName)`; the
//! `created`/`updated` action is proven byte-exact by the per-op Output instead.
//!
//! The Rust port drives [`read_conversation`]/[`annotations`]'s `execute_*` +
//! `format_*`; the oracle drives v4's REAL handlers + `format*Results`.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_SCRIPTORIUM=/tmp/qt-scriptorium.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-scriptorium-tools-fixture.ts
//!   QT_FIXTURE_SCRIPTORIUM=/tmp/qt-scriptorium.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/scriptorium-tools.ts \
//!     > /tmp/oracle-scriptorium.ndjson
//! Run:
//!   QT_ORACLE_SCRIPTORIUM=/tmp/oracle-scriptorium.ndjson \
//!   QT_FIXTURE_SCRIPTORIUM=/tmp/qt-scriptorium.db \
//!     cargo test -p quilltap-harness --test scriptorium_tools_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::Db;
use quilltap_core::tools::annotations::{execute_delete_annotation, execute_upsert_annotation};
use quilltap_core::tools::annotations::{format_delete_annotation, format_upsert_annotation};
use quilltap_core::tools::read_conversation::{
    execute_read_conversation, format_read_conversation,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct Op {
    tool: String,
    #[serde(rename = "chatId")]
    chat_id: String,
    #[serde(default, rename = "characterId")]
    character_id: Option<String>,
    #[serde(default, rename = "characterName")]
    character_name: Option<String>,
    args: Value,
}

#[derive(Deserialize)]
struct OracleOp {
    tool: String,
    output: Value,
    formatted: String,
}

#[derive(Deserialize)]
struct Oracle {
    ops: Vec<OracleOp>,
    dump: Value,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/scriptorium-tools.json")
}

/// Normalize a `conversation_annotations` dump for cross-side comparison: sort
/// rows by the natural key `(chatId, messageIndex, characterName)`, then replace
/// the minted `id`/`createdAt`/`updatedAt` cells with placeholders (the write ops
/// mint different values on each side; the payload columns stay pinned).
fn normalize_dump(dump: &Value) -> Value {
    let mut rows: Vec<Value> = dump["rows"].as_array().cloned().unwrap_or_default();
    rows.sort_by(|a, b| {
        let key = |v: &Value| {
            (
                v["chatId"].as_str().unwrap_or("").to_string(),
                v["messageIndex"].as_f64().unwrap_or(0.0).to_bits(),
                v["characterName"].as_str().unwrap_or("").to_string(),
            )
        };
        key(a).cmp(&key(b))
    });
    let rows: Vec<Value> = rows
        .into_iter()
        .map(|mut r| {
            if let Some(obj) = r.as_object_mut() {
                if obj.contains_key("id") {
                    obj.insert("id".into(), json!("<id>"));
                }
                if obj.contains_key("createdAt") {
                    obj.insert("createdAt".into(), json!("<ts>"));
                }
                if obj.contains_key("updatedAt") {
                    obj.insert("updatedAt".into(), json!("<ts>"));
                }
            }
            r
        })
        .collect();
    json!({
        "table": dump["table"],
        "columns": dump["columns"],
        "rows": rows,
    })
}

#[tokio::test]
async fn scriptorium_tools_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_SCRIPTORIUM") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_SCRIPTORIUM to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_SCRIPTORIUM") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_SCRIPTORIUM to the fixture .db (header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle: Oracle = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle dump");

    assert_eq!(spec.ops.len(), oracle.ops.len(), "op count: spec vs oracle");

    // Work on a fresh copy of the seed fixture.
    let pid = std::process::id();
    let work = std::env::temp_dir().join(format!("qt-scriptorium-rust-{pid}.db"));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let db = Db::open_main(&work, &spec.test_pepper_base64).unwrap_or_else(|e| panic!("open: {e}"));

    for (i, op) in spec.ops.iter().enumerate() {
        let (output, formatted): (Value, String) = match op.tool.as_str() {
            "read_conversation" => {
                let out = execute_read_conversation(
                    &db,
                    &spec.user_id,
                    &op.chat_id,
                    op.character_id.as_deref(),
                    &op.args,
                )
                .await;
                let f = format_read_conversation(&out);
                (serde_json::to_value(&out).unwrap(), f)
            }
            "upsert_annotation" => {
                let out = execute_upsert_annotation(
                    &db,
                    &spec.user_id,
                    &op.chat_id,
                    op.character_name.as_deref().expect("characterName"),
                    &op.args,
                )
                .await;
                let f = format_upsert_annotation(&out);
                (serde_json::to_value(&out).unwrap(), f)
            }
            "delete_annotation" => {
                let out = execute_delete_annotation(
                    &db,
                    &spec.user_id,
                    &op.chat_id,
                    op.character_name.as_deref().expect("characterName"),
                    &op.args,
                )
                .await;
                let f = format_delete_annotation(&out);
                (serde_json::to_value(&out).unwrap(), f)
            }
            other => panic!("op {i}: unknown tool {other}"),
        };

        let oo = &oracle.ops[i];
        assert_eq!(oo.tool, op.tool, "op {i}: tool mismatch");
        assert_eq!(
            output,
            oo.output,
            "op {i} ({}): output diverged\n  rust:   {}\n  oracle: {}",
            op.tool,
            serde_json::to_string(&output).unwrap(),
            serde_json::to_string(&oo.output).unwrap()
        );
        assert_eq!(
            formatted, oo.formatted,
            "op {i} ({}): formatted string diverged\n  rust:   {:?}\n  oracle: {:?}",
            op.tool, formatted, oo.formatted
        );
    }

    // Diff the final table state in the normalized form.
    let got_dump = db
        .read_main(|conn| dump_table_json_conn(conn, "conversation_annotations", "id"))
        .expect("dump conversation_annotations");

    let got_norm = normalize_dump(&got_dump);
    let oracle_norm = normalize_dump(&oracle.dump);

    assert_eq!(got_norm["table"], oracle_norm["table"], "table name");
    assert_eq!(
        got_norm["columns"], oracle_norm["columns"],
        "column set / order"
    );
    assert_eq!(
        got_norm["rows"], oracle_norm["rows"],
        "row state diverged\n  rust:   {}\n  oracle: {}",
        got_norm["rows"], oracle_norm["rows"]
    );

    let n = got_norm["rows"].as_array().map(|a| a.len()).unwrap_or(0);
    assert!(n > 0, "dump looks empty");

    // Drop the Db (closes the writer thread) before removing the file.
    drop(db);
    let _ = std::fs::remove_file(&work);
    eprintln!(
        "OK: scriptorium tools matched oracle ({} ops, {n} annotation rows).",
        spec.ops.len()
    );
}
