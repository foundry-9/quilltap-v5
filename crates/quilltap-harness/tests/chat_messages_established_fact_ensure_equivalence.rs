//! P4.D362 — the `chat_messages.isEstablishedFact` boot ensure
//! (`db::chat_messages_established_fact_repair`, the TWENTY-SECOND D23 move)
//! against v4's REAL migration `add-established-fact-field-v1` through v4's
//! REAL `MigrationRunner` narrowed to it
//! (`cases/chat-messages-established-fact-ensure.ts`).
//!
//! Four shapes over the SAME base files the oracle builds at the pin from v4's
//! generateDDL (`QT_FIXTURE_ESTABLISHED_FACT_ENSURE_DIR/established-fact-<mode>.db`):
//! (a) BASELINE → both ADD `INTEGER DEFAULT NULL` (appended); (b) NO TABLE,
//! (c) MIGRATED, (d) GENERATEDDL-CURRENT → `not needed`. Comparands: v4's
//! report vs v5's answer, `PRAGMA table_info`, `sqlite_master.sql`, every row's
//! `quote()` / `typeof` flag, the ledger (v4 stamps on a run, v5 never does).
//! The STORAGE-CLASS leg on (c) / (d): a `true` and a `false` flag written
//! through v5's `ChatMessagesRepository::add_message` beside v4's REAL
//! `addMessage` — each cell's `quote()` / `typeof()` (measured: INTEGER `1` /
//! `0` on (c); TEXT `'1.0'` / `'0.0'` on (d)), then read back through v5's
//! `get_messages` against v4's `getMessages` (on (d) v4 hydrates `'0.0'` as
//! `true` — `Boolean('0.0')`; v5's `put_zero_one_flag` follows).
//!
//! Generate (Node 24, from the pinned v4 worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   rm -rf /tmp/qt-established-fact-ensure /tmp/oracle-established-fact-ensure.ndjson
//!   QT_FIXTURE_OUT_DIR=/tmp/qt-established-fact-ensure \
//!     $N/npx tsx $V5W/harness/oracle/cases/chat-messages-established-fact-ensure.ts \
//!     > /tmp/oracle-established-fact-ensure.ndjson
//! Run:
//!   cd $V5W
//!   QT_ORACLE_ESTABLISHED_FACT_ENSURE=/tmp/oracle-established-fact-ensure.ndjson \
//!   QT_FIXTURE_ESTABLISHED_FACT_ENSURE_DIR=/tmp/qt-established-fact-ensure \
//!     cargo test -p quilltap-harness --test chat_messages_established_fact_ensure_equivalence -- --nocapture

mod keystone_ensure_support;

use std::path::Path;

use keystone_ensure_support as kit;
use quilltap_core::db::chat_messages_established_fact_repair::{
    ensure_chat_messages_established_fact, MIGRATION_ID,
};
use quilltap_core::db::chats_messages::{ChatEventInput, ChatMessagesRepository};
use quilltap_core::db::Writer;
use serde_json::{json, Value};

const CHAT_ID: &str = "6f1c3a52-0000-4000-8000-0000000000c1";
const FACT_TRUE_ID: &str = "6f1c3a52-0000-4000-8000-0000000000f1";
const FACT_FALSE_ID: &str = "6f1c3a52-0000-4000-8000-0000000000f0";

fn storage_leg(c: &rusqlite::Connection) -> Value {
    let repo = ChatMessagesRepository::new(c);
    for (id, flag) in [(FACT_TRUE_ID, true), (FACT_FALSE_ID, false)] {
        let event: ChatEventInput = serde_json::from_value(json!({
            "type": "message",
            "id": id,
            "role": "USER",
            "content": format!("fact {flag}"),
            // Distinct times, as the oracle's (a tie orders per read plan).
            "createdAt": if flag { "2026-01-02T00:00:00.000Z" } else { "2026-01-02T00:00:01.000Z" },
            "isEstablishedFact": flag,
        }))
        .unwrap();
        repo.add_message(CHAT_ID, &event).expect("v5 add_message");
    }
    let mut stmt = c
        .prepare(
            "SELECT id, quote(\"isEstablishedFact\") AS q, typeof(\"isEstablishedFact\") AS t \
             FROM chat_messages WHERE id IN (?1, ?2) ORDER BY id",
        )
        .unwrap();
    let stored: Vec<Value> = stmt
        .query_map([FACT_TRUE_ID, FACT_FALSE_ID], |r| {
            Ok(json!({"id": r.get::<_, String>(0)?, "q": r.get::<_, String>(1)?, "t": r.get::<_, String>(2)?}))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let hydrated: Vec<Value> = quilltap_core::db::chats_messages_read::get_messages(c, CHAT_ID)
        .unwrap()
        .into_iter()
        .filter(|m| m["id"] == FACT_TRUE_ID || m["id"] == FACT_FALSE_ID)
        .map(|m| {
            json!({"id": m["id"], "isEstablishedFact": m.get("isEstablishedFact").cloned().unwrap_or(json!("<absent>"))})
        })
        .collect();
    json!({"stored": stored, "hydrated": hydrated})
}

#[test]
fn the_established_fact_ensure_matches_v4s_migration_in_four_shapes() {
    let (Some(oracle), Some(dir)) = (
        std::env::var_os("QT_ORACLE_ESTABLISHED_FACT_ENSURE"),
        std::env::var_os("QT_FIXTURE_ESTABLISHED_FACT_ENSURE_DIR"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_ESTABLISHED_FACT_ENSURE and QT_FIXTURE_ESTABLISHED_FACT_ENSURE_DIR (see header)."
        );
        return;
    };
    let mut modes = Vec::new();
    for v4 in kit::read_ndjson(&oracle) {
        let mode = v4["mode"].as_str().unwrap().to_string();
        let report = kit::report(&v4);
        let runs = mode == "a";
        assert_eq!(report.len(), 1, "[{mode}] {report:?}");
        if runs {
            assert!(
                report[0].starts_with(&format!("+RAN {MIGRATION_ID} ")),
                "[{mode}] {report:?}"
            );
        } else {
            assert!(report[0].contains(" not needed"), "[{mode}] {report:?}");
        }
        assert_eq!(
            serde_json::from_value::<Vec<String>>(v4["ledger"].clone()).unwrap(),
            if runs {
                vec![MIGRATION_ID.to_string()]
            } else {
                vec![]
            },
            "[{mode}] v4's ledger"
        );

        let scratch = tempfile::tempdir().unwrap();
        let work = scratch.path().join("work.db");
        std::fs::copy(
            Path::new(&dir).join(format!("established-fact-{mode}.db")),
            &work,
        )
        .unwrap();
        let w = Writer::open_writable(&work, kit::PEPPER).unwrap();
        let c = w.connection();
        let before = kit::table_sql(c, "chat_messages");
        let added = ensure_chat_messages_established_fact(c).expect("ensure");
        assert_eq!(added, runs, "[{mode}] the ensure's answer");
        if !runs {
            assert_eq!(
                kit::table_sql(c, "chat_messages"),
                before,
                "[{mode}] table touched"
            );
        }
        kit::assert_bytes(
            &kit::table_info(c, "chat_messages"),
            &v4["tableInfo"],
            &format!("[{mode}] table_info"),
        );
        kit::assert_bytes(
            &json!(kit::table_sql(c, "chat_messages")),
            &v4["sql"],
            &format!("[{mode}] table sql"),
        );
        kit::assert_bytes(
            &kit::cells(c, "chat_messages", &["isEstablishedFact"]),
            &v4["rows"],
            &format!("[{mode}] cells"),
        );
        assert!(kit::ledger(c).is_empty(), "[{mode}] v5 stamped (UNSTAMPED)");
        if mode == "a" {
            let sql = v4["sql"].as_str().unwrap();
            assert!(
                sql.ends_with(", \"isEstablishedFact\" INTEGER DEFAULT NULL)"),
                "marker {sql}"
            );
        }
        if matches!(mode.as_str(), "c" | "d") {
            let got = storage_leg(c);
            kit::assert_bytes(&got, &v4["storage"], &format!("[{mode}] storage leg"));
            eprintln!("[{mode}] storage leg {got}");
        } else {
            assert!(v4["storage"].is_null());
        }
        modes.push(mode);
    }
    assert_eq!(modes, ["a", "b", "c", "d"], "every shape ran");
    eprintln!("OK: the established-fact ensure matched v4 in shapes {modes:?}.");
}
