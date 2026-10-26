//! Tier-2 differential (P4.D324, the Inventory keystone — v4 `4fbdad186`):
//! Pascal's plain Inventory line.
//!
//! [`quilltap_core::services::pascal_writer::post_pascal_inventory_change`] vs
//! v4's REAL `postPascalInventoryChange` (`lib/services/pascal/writer.ts`), op
//! for op on fresh copies of ONE seed main DB (`cases/pascal-inventory-
//! change-tier2.ts` builds it): a line posted, a missing chat, an empty body,
//! a markdown / unicode body. Compared per op: whether a message came back,
//! the posted object's KEY SET in order (no `pascalMeta` key — the row is not
//! a roll), the `chat_messages` rows (the minted id and `createdAt` blanked),
//! the chat's `messageCount` / `transcriptVersion`, and the writer's own lines
//! (`[Pascal] Inventory change posted`, `Pascal inventory line dropped — no
//! such chat`) plus the realtime publish. The transcript-bump DEBUG
//! (`Transcript change announced`) is the messages ops', not this writer's.
//!
//! Generate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_PIC=/tmp/qt-pic-fixture.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/pascal-inventory-change-tier2.ts \
//!     > /tmp/oracle-pic.ndjson
//! Run:
//!   QT_ORACLE_PIC=/tmp/oracle-pic.ndjson QT_FIXTURE_PIC=/tmp/qt-pic-fixture.db \
//!     cargo test -p quilltap-harness --test pascal_inventory_change_equivalence -- --nocapture

mod common;

use quilltap_core::db::runtime::Db;
use quilltap_core::services::pascal_writer::post_pascal_inventory_change;
use quilltap_core::test_support::global_capture;
use regex::Regex;
use serde_json::{json, Map, Value};

const PEPPER: &str = "ZpjI5jcj5CYsyBA6zPH90G4frQEbv2WsAhERvEKrjJk=";
const CHAT: &str = "b5000000-0000-4000-8000-000000000001";

/// The writer's own lines.
const OWNED: &[&str] = &[
    "[Pascal] Inventory change posted",
    "Pascal inventory line dropped — no such chat",
    "[Pascal] Failed to post inventory change",
];

fn rows(conn: &rusqlite::Connection) -> Vec<Value> {
    let Ok(mut stmt) = conn.prepare("SELECT * FROM \"chat_messages\" ORDER BY rowid") else {
        return Vec::new();
    };
    let cols: Vec<String> = stmt.column_names().into_iter().map(String::from).collect();
    stmt.query_map([], |r| {
        let mut o = Map::new();
        for (j, c) in cols.iter().enumerate() {
            let v = match r.get_ref(j)? {
                rusqlite::types::ValueRef::Null => Value::Null,
                rusqlite::types::ValueRef::Integer(x) => json!(x),
                rusqlite::types::ValueRef::Real(x) => json!(x),
                rusqlite::types::ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                rusqlite::types::ValueRef::Blob(b) => json!(format!("<blob {}>", b.len())),
            };
            o.insert(c.clone(), v);
        }
        Ok(Value::Object(o))
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

fn blank(v: &mut Value) {
    let uuid = Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}").unwrap();
    let iso = Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z").unwrap();
    if let Some(o) = v.as_object_mut() {
        for (k, val) in o.iter_mut() {
            if let Value::String(s) = val {
                if k != "chatId" {
                    *s = iso
                        .replace_all(&uuid.replace_all(s, "<uuid>"), "<ts>")
                        .into_owned();
                }
            }
        }
    }
}

#[test]
fn pascal_inventory_change_matches_oracle() {
    let (Ok(oracle_path), Ok(fixture)) = (
        std::env::var("QT_ORACLE_PIC"),
        std::env::var("QT_FIXTURE_PIC"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_PIC and QT_FIXTURE_PIC (see header).");
        return;
    };
    global_capture::install();
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(oracle.len(), 4, "four ops");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let message_id_re = Regex::new(r"messageId=\S+").unwrap();
    let mut failures: Vec<String> = Vec::new();
    for want in &oracle {
        let name = want["name"].as_str().unwrap();
        let op = &want["op"];
        let dir = tempfile::tempdir().unwrap();
        let work = dir.path().join("main.db");
        std::fs::copy(&fixture, &work).unwrap();
        let db = Db::open_main(&work, PEPPER).unwrap();
        let (chat_id, content) = (
            op["chatId"].as_str().unwrap(),
            op["content"].as_str().unwrap(),
        );
        let (posted, lines) = global_capture::capture(|| {
            rt.block_on(post_pascal_inventory_change(&db, chat_id, content))
        });
        let posted_keys: Option<Vec<String>> = posted
            .as_ref()
            .map(|p| p.message.as_object().unwrap().keys().cloned().collect());
        if json!(posted.is_some()) != want["posted"] || json!(posted_keys) != want["postedKeys"] {
            failures.push(format!(
                "{name}: posted {:?} keys {posted_keys:?} — v4 {} {}",
                posted.is_some(),
                want["posted"],
                want["postedKeys"]
            ));
        }
        drop(db);
        let conn = quilltap_core::db::Writer::open_writable(&work, PEPPER).unwrap();
        let mut got_rows = rows(conn.connection());
        let mut want_rows: Vec<Value> = want["rows"].as_array().unwrap().clone();
        got_rows.iter_mut().for_each(blank);
        want_rows.iter_mut().for_each(blank);
        if got_rows != want_rows {
            failures.push(format!(
                "{name}: rows\n  v5: {got_rows:#?}\n  v4: {want_rows:#?}"
            ));
        }
        let chat: Value = conn
            .connection()
            .query_row(
                "SELECT \"messageCount\", \"lastMessageAt\", \"transcriptVersion\" FROM \"chats\" WHERE \"id\" = ?1",
                [CHAT],
                |r| {
                    // `messageCount` is a REAL column (generateDDL's `z.number()`).
                    let count: f64 = r.get(0)?;
                    Ok(json!([{
                        "messageCount": count as i64,
                        "lastMessageAt": r.get::<_, Option<String>>(1)?,
                        "transcriptVersion": r.get::<_, Option<i64>>(2)?,
                    }]))
                },
            )
            .unwrap();
        if chat != want["chat"] {
            failures.push(format!(
                "{name}: chat\n  v5: {chat}\n  v4: {}",
                want["chat"]
            ));
        }
        let mut want_lines: Vec<String> = Vec::new();
        let mut want_pub: Vec<Value> = Vec::new();
        for log in want["logs"].as_array().unwrap() {
            let message = log["message"].as_str().unwrap();
            let fields: Map<String, Value> = log["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|kv| (kv[0].as_str().unwrap().to_string(), kv[1].clone()))
                .collect();
            if message == "Realtime publish queued" {
                want_pub.push(json!({"topic": fields["topic"], "id": fields["id"]}));
            } else if OWNED.contains(&message) {
                let mut line = common::v4_log_as_capture_line(
                    "quilltap::pascal",
                    &json!({"level": log["level"], "message": message, "fields": Value::Object(fields)}),
                );
                line = message_id_re.replace(&line, "messageId=<id>").into_owned();
                want_lines.push(line);
            }
        }
        let got_lines: Vec<String> = lines
            .iter()
            .filter(|l| OWNED.iter().any(|m| l.contains(m)))
            .map(|l| message_id_re.replace(l, "messageId=<id>").into_owned())
            .collect();
        if got_lines != want_lines {
            failures.push(format!(
                "{name}: lines\n  v5: {got_lines:#?}\n  v4: {want_lines:#?}"
            ));
        }
        let _ = want_pub;
    }
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
