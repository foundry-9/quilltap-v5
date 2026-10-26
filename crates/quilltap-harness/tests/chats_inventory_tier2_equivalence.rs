//! Tier-2 differential (P4.D324, the Inventory keystone — v4 `4fbdad186`): the
//! chats carried-Inventory ops.
//!
//! [`quilltap_core::db::chats_inventory::ChatsInventoryRepository`] vs v4's REAL
//! `ChatsRepository` (`getEquippedInventory` / `setEquippedInventory` /
//! `findChatIdsCarryingInventoryId` / `removeEquippedInventoryId`), op for op
//! over `chats-inventory-tier2.json` on ONE copy of the seed fixture: each op's
//! result (`JSON.stringify` bytes — the map's key order), the
//! `equippedInventory` BYTES of every chat after it, and the lines logged
//! (rendered as v5's capture rig renders a `quilltap::db` event, field order
//! included). The `dropColumn` op drops the column on the working copy (R-K):
//! the ops after it pin the pre-column fallbacks — `Failed to write equipped
//! inventory` with the BARE `no such column` message, `written: false`.
//!
//! Lines neither side's repository owns are dropped by NAME: v4's backend
//! lines (`SQLite … error` — the standing `UNPORTED_BACKEND_LINES`), the
//! backend's `Corrupted JSON in column, using default` WARN (unported across
//! v5's JSON columns — `chat_settings.rs`'s note), and the chat-row Zod lines
//! (`Data validation failed` / `Error finding entity by ID` for a `[]` cell —
//! the recorded P4.149 class: v5 validates no chat row; the result is `{}` on
//! both sides either way).
//!
//! Generate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-chinv-fixture.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-chats-inventory-fixture.ts
//!   QT_FIXTURE_CHINV=/tmp/qt-chinv-fixture.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/chats-inventory-tier2.ts \
//!     > /tmp/oracle-chinv.ndjson
//! Run:
//!   QT_ORACLE_CHINV=/tmp/oracle-chinv.ndjson QT_FIXTURE_CHINV=/tmp/qt-chinv-fixture.db \
//!     cargo test -p quilltap-harness --test chats_inventory_tier2_equivalence -- --nocapture

mod common;

use std::path::{Path, PathBuf};

use quilltap_core::db::chats_inventory::ChatsInventoryRepository;
use quilltap_core::db::Writer;
use quilltap_core::test_support::captured_with;
use serde_json::{json, Map, Value};

/// Lines neither repository owns (see the header).
const UNOWNED_LINES: &[&str] = &[
    "SQLite updateOne error",
    "SQLite find error",
    "SQLite findOne error",
    "Corrupted JSON in column, using default",
    "Data validation failed",
    "Error finding entity by ID",
    "Raw query failed",
];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chats-inventory-tier2.json")
}

fn v4_line(log: &Value) -> String {
    let fields: Map<String, Value> = log["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|kv| {
            let k = kv[0].as_str().unwrap().to_string();
            let v = match &kv[1] {
                Value::String(s) if k == "error" => Value::String(common::normalize_v4_sqlite(s)),
                other => other.clone(),
            };
            (k, v)
        })
        .collect();
    common::v4_log_as_capture_line(
        "quilltap::db",
        &json!({"level": log["level"], "message": log["message"], "fields": Value::Object(fields)}),
    )
}

fn cells(conn: &rusqlite::Connection) -> Value {
    // Probed, never selected blind: v5's SQLite reads a double-quoted unknown
    // identifier as a string (DQS), v4's refuses it.
    let has_column = conn
        .prepare("SELECT 1 FROM pragma_table_info('chats') WHERE name = 'equippedInventory'")
        .and_then(|mut s| s.exists([]))
        .unwrap();
    let sql = if has_column {
        "SELECT \"id\", \"equippedInventory\" FROM \"chats\" ORDER BY \"id\""
    } else {
        "SELECT \"id\", NULL AS \"equippedInventory\" FROM \"chats\" ORDER BY \"id\""
    };
    let mut stmt = conn.prepare(sql).unwrap();
    let rows: Vec<Value> = stmt
        .query_map([], |r| {
            Ok(json!({"id": r.get::<_, String>(0)?, "equippedInventory": r.get::<_, Option<String>>(1)?}))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    Value::Array(rows)
}

fn map_text(state: Vec<(String, Vec<String>)>) -> String {
    let m: Map<String, Value> = state.into_iter().map(|(k, ids)| (k, json!(ids))).collect();
    serde_json::to_string(&Value::Object(m)).unwrap()
}

#[test]
fn chats_inventory_tier2_matches_oracle() {
    let (Ok(oracle_path), Ok(fixture)) = (
        std::env::var("QT_ORACLE_CHINV"),
        std::env::var("QT_FIXTURE_CHINV"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_CHINV and QT_FIXTURE_CHINV (see header).");
        return;
    };
    let spec: Value = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let pepper = spec["testPepperBase64"].as_str().unwrap();
    let characters = spec["characters"].as_object().unwrap().clone();
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let ops = spec["ops"].as_array().unwrap();
    assert_eq!(oracle.len(), ops.len(), "one oracle row per op");

    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("chinv-work.db");
    std::fs::copy(&fixture, &work).unwrap();
    let writer = Writer::open_writable(&work, pepper).unwrap();
    let conn = writer.connection();
    let repo = ChatsInventoryRepository::new(conn);

    let mut failures: Vec<String> = Vec::new();
    let mut lines_seen = 0usize;
    for (op, want) in ops.iter().zip(&oracle) {
        let index = want["index"].as_u64().unwrap();
        let s = |k: &str| op[k].as_str().unwrap_or_default().to_string();
        let character = op["character"]
            .as_str()
            .map(|c| characters[c].as_str().unwrap().to_string())
            .unwrap_or_default();
        let (result, lines) = captured_with(|| match op["kind"].as_str().unwrap() {
            "get" => map_text(repo.get_equipped_inventory(&s("chatId"))),
            "set" => {
                let ids: Option<Vec<String>> = serde_json::from_value(op["ids"].clone()).unwrap();
                json!(repo.set_equipped_inventory(&s("chatId"), &character, ids.as_deref()))
                    .to_string()
            }
            "find" => json!(repo.find_chat_ids_carrying_inventory_id(&character, &s("itemId")))
                .to_string(),
            "remove" => {
                json!(repo.remove_equipped_inventory_id(&s("chatId"), &character, &s("itemId")))
                    .to_string()
            }
            "dropColumn" => {
                conn.execute_batch("ALTER TABLE \"chats\" DROP COLUMN \"equippedInventory\"")
                    .unwrap();
                "null".to_string()
            }
            other => panic!("unknown op {other}"),
        });
        let label = format!("#{index} {}", op["kind"]);
        if result != want["result"].as_str().unwrap() {
            failures.push(format!(
                "{label} result\n  v5: {result}\n  v4: {}",
                want["result"]
            ));
        }
        let got_cells = cells(conn);
        if got_cells != want["cells"] {
            failures.push(format!(
                "{label} cells\n  v5: {got_cells}\n  v4: {}",
                want["cells"]
            ));
        }
        let want_lines: Vec<String> = want["logs"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|l| !UNOWNED_LINES.contains(&l["message"].as_str().unwrap_or("")))
            .map(v4_line)
            .collect();
        let got_lines: Vec<String> = lines
            .into_iter()
            .filter(|l| l.contains(" quilltap::db "))
            .collect();
        lines_seen += want_lines.len();
        if got_lines != want_lines {
            failures.push(format!(
                "{label} lines\n  v5: {got_lines:#?}\n  v4: {want_lines:#?}"
            ));
        }
    }
    eprintln!("chats inventory: {} ops, {lines_seen} lines", ops.len());
    // Every write / find / remove logs its DEBUG (and the pre-column writes
    // and find their fallback ERROR first); a `get` logs nothing owned.
    assert_eq!(lines_seen, 28, "owned lines");
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
