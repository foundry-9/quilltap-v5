//! Tier-1 differential (P4.D140, v4 `735d9408c` — bug 112): the chat-activity
//! chokepoint — `crate::chat_activity` vs v4's REAL `lib/chat/chat-activity.ts`
//! exports.
//!
//! Four row kinds:
//!   - `predicate` — `isCharacterAuthoredMessage` over v4's own test table
//!     (every Staff sender by name, whispers and silent messages counting,
//!     announcement bubbles and TOOL/SYSTEM roles not) PLUS the edges v4's
//!     table leaves unstated: `systemSender: ''` (the truthiness-vs-`IS NULL`
//!     seam between the in-memory predicate and the SQL mirror — v4 measures
//!     TRUE, so the two spellings are deliberately not equivalent),
//!     `customAnnouncer: {}` (truthy in JS), a lowercase role, and missing
//!     `role`/`type`.
//!   - `activity` — `chatActivityAt` + `chatActivityTime`, including the
//!     nullish-`??` empty-string win, the NaN → 0 clamp that keeps comparators
//!     total, and a row proving `updatedAt` is never consulted.
//!   - `sort` — `byChatActivityDesc` over the shapes the Salon list, home,
//!     projects and the Brahma console all now share, incl. tie stability.
//!   - `filter` — v4's `CHARACTER_AUTHORED_MESSAGE_FILTER` object. v4 spells
//!     the mirror as a `QueryFilter`, v5 as a SQL WHERE fragment, so this arm
//!     TRANSLATES v4's object into the fragment mechanically and compares —
//!     nothing is transcribed by hand.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   ~/.nvm/versions/node/v24.13.1/bin/npx tsx \
//!     ~/source/quilltap-v5/harness/oracle/cases/chat-activity.ts \
//!     > /tmp/oracle-chat-activity.ndjson
//! Run:
//!   QT_ORACLE_CHAT_ACTIVITY=/tmp/oracle-chat-activity.ndjson \
//!     cargo test -p quilltap-harness --test chat_activity_equivalence

use quilltap_core::chat_activity::{
    by_chat_activity_desc, chat_activity_at, chat_activity_time, is_character_authored_message,
    CHARACTER_AUTHORED_MESSAGE_FILTER,
};
use serde_json::Value;

fn rows() -> Vec<Value> {
    let Ok(path) = std::env::var("QT_ORACLE_CHAT_ACTIVITY") else {
        eprintln!("QT_ORACLE_CHAT_ACTIVITY unset — skipping");
        return Vec::new();
    };
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle row"))
        .collect()
}

/// Collapse the Rust const's line-continuation whitespace so the comparison is
/// about clauses, not formatting.
fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// v4's `QueryFilter` object → the SQL WHERE fragment it means. Mechanical, so
/// the mirror is derived from v4's own value rather than hand-transcribed.
fn filter_to_sql(filter: &Value) -> String {
    let obj = filter.as_object().expect("filter object");
    let mut clauses = Vec::new();
    for (key, value) in obj {
        match value {
            Value::Null => clauses.push(format!("{key} IS NULL")),
            Value::String(s) => clauses.push(format!("{key} = '{s}'")),
            Value::Object(o) if o.contains_key("$in") => {
                let list = o["$in"]
                    .as_array()
                    .expect("$in array")
                    .iter()
                    .map(|v| format!("'{}'", v.as_str().expect("$in string")))
                    .collect::<Vec<_>>()
                    .join(", ");
                clauses.push(format!("{key} IN ({list})"));
            }
            other => panic!("unhandled filter clause {key}: {other}"),
        }
    }
    clauses.join(" AND ")
}

#[test]
fn chat_activity_matches_v4() {
    let rows = rows();
    if rows.is_empty() {
        return;
    }
    let (mut predicates, mut activities, mut sorts, mut filters) = (0, 0, 0, 0);

    for row in &rows {
        let id = row["id"].as_str().expect("id");
        match row["kind"].as_str().expect("kind") {
            "filter" => {
                assert_eq!(
                    squash(CHARACTER_AUTHORED_MESSAGE_FILTER),
                    filter_to_sql(&row["filter"]),
                    "[{id}] the SQL mirror does not encode v4's filter object"
                );
                filters += 1;
            }
            "predicate" => {
                assert_eq!(
                    is_character_authored_message(&row["event"]),
                    row["out"].as_bool().expect("out"),
                    "[{id}] isCharacterAuthoredMessage"
                );
                predicates += 1;
            }
            "activity" => {
                assert_eq!(
                    chat_activity_at(&row["chat"]),
                    row["at"].as_str().expect("at"),
                    "[{id}] chatActivityAt"
                );
                assert_eq!(
                    chat_activity_time(&row["chat"]),
                    row["time"].as_i64().expect("time"),
                    "[{id}] chatActivityTime"
                );
                activities += 1;
            }
            "sort" => {
                let mut chats: Vec<Value> = row["chats"]
                    .as_array()
                    .expect("chats")
                    .iter()
                    .cloned()
                    .collect();
                // `sort_by` is stable, as `Array.prototype.sort` is.
                chats.sort_by(by_chat_activity_desc);
                let got: Vec<&str> = chats
                    .iter()
                    .map(|c| c["id"].as_str().expect("chat id"))
                    .collect();
                let want: Vec<&str> = row["order"]
                    .as_array()
                    .expect("order")
                    .iter()
                    .map(|v| v.as_str().expect("order id"))
                    .collect();
                assert_eq!(got, want, "[{id}] byChatActivityDesc");
                sorts += 1;
            }
            other => panic!("unknown row kind {other}"),
        }
    }

    // Shape guard: a corpus that silently loses an arm must not read as green.
    assert_eq!(filters, 1, "the SQL-mirror arm went missing");
    assert!(predicates >= 26, "predicate corpus shrank: {predicates}");
    assert!(activities >= 7, "activity corpus shrank: {activities}");
    assert!(sorts >= 5, "sort corpus shrank: {sorts}");
}
