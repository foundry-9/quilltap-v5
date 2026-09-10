//! Tier-2 differential for Continue Elsewhere — `services::chat_continuation::
//! apply_chat_continuation` vs v4's REAL `applyChatContinuation`
//! (`lib/chat/apply-chat-continuation.ts`).
//!
//! P4.D172's OPEN item 2. Until this family existed, everything
//! `replicate_turn_state` copies — the turn queue, the spoken set, the drawn
//! rotation, the impersonation overlay, the pause state, the two participant
//! anchors — was inspection-only: no differential drove the service at all, and
//! the only behavioural pin in the tree was the route trail's ABSENCE
//! (`route_trail_continuation_guard`, which stays: its source census is the
//! anti-rot half and this family cannot replace it).
//!
//! # What is compared
//!
//! Per case, over a FRESH copy of the same two-DB fixture on each side: the
//! returned `{replayedMessageCount, hadLibrarianSummary, postedSourceTailBubble}`
//! triple, the whole `chats` and `chat_messages` tables, and a `rowid`-ordered
//! message projection. The last is the point: continuation's contract is
//! POSITIONAL — the link bubble goes into the new chat FIRST, then the replayed
//! tail, and the source's tail bubble goes in LAST — and a key-sorted table dump
//! cannot see any of that.
//!
//! # Ids
//!
//! Every id the fixture bakes stays LITERAL; only the ids each side mints are
//! remapped to `<minted-N>` in first-appearance order. That is deliberate:
//! `turnQueue`, `spokenThisCycleParticipantIds`, `cycleOrderParticipantIds`,
//! `impersonatingParticipantIds`, `lastTurnParticipantId` and
//! `activeTypingParticipantId` all carry PINNED participant ids, and whether the
//! remap put the RIGHT one there is exactly the thing a first-appearance
//! relabelling can hide (the P4.D44 `chat_template_ids` trap). The keep-list is
//! read out of the corpus itself, so a new pinned id needs no code change.
//!
//! # What it does NOT cover
//!
//! The route that ships the feature (`POST /api/v1/chats` with
//! `continuationFromChatId`): its `notFound('Source chat')` pre-check and its
//! try/catch belong to `chat_create_capstone_equivalence`, which already carries
//! `cs_continuation_bubble_before_replay`.
//!
//! Build the fixture + oracle (Node 24, from the v4 checkout; jest ignores
//! `.claude/` paths, so the case is staged in a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-continuation-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-continuation-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-chat-continuation-fixture.ts
//!   TMPO=/tmp/qt-continuation-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/chat-continuation-tier2.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/chat-continuation-tier2.json"  "$TMPO/fixtures/"
//!   TZ=UTC QT_FIXTURE_CONT_MAIN=/tmp/qt-continuation-main.db \
//!   QT_FIXTURE_CONT_MOUNT=/tmp/qt-continuation-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-chat-continuation.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "chat-continuation-tier2\.test\.ts$"
//! Run:
//!   QT_ORACLE_CHAT_CONTINUATION=/tmp/oracle-chat-continuation.ndjson \
//!   QT_FIXTURE_CONT_MAIN=/tmp/qt-continuation-main.db \
//!   QT_FIXTURE_CONT_MOUNT=/tmp/qt-continuation-mount.db \
//!     cargo test -p quilltap-harness --test chat_continuation_tier2_equivalence

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::chat_continuation::apply_chat_continuation;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

/// The corpus, embedded so the keep-list of pinned ids is derived from the same
/// bytes both sides seed from.
const SPEC: &str = include_str!("../../../harness/oracle/fixtures/chat-continuation-tier2.json");

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

fn is_uuid_at(s: &[u8], i: usize) -> bool {
    if i + 36 > s.len() {
        return false;
    }
    for (off, ch) in s[i..i + 36].iter().enumerate() {
        match off {
            8 | 13 | 18 | 23 => {
                if *ch != b'-' {
                    return false;
                }
            }
            _ => {
                if !ch.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

/// Every UUID the corpus pins — kept LITERAL through normalization.
fn pinned_ids() -> HashSet<String> {
    let b = SPEC.as_bytes();
    let mut out = HashSet::new();
    let mut i = 0usize;
    while i < b.len() {
        if is_uuid_at(b, i) {
            out.insert(SPEC[i..i + 36].to_lowercase());
            i += 36;
        } else {
            i += 1;
        }
    }
    out
}

/// Canonicalize a value for comparison: JS `1.0` → `1`, then serialize with keys
/// sorted, ISO stamps sentineled, and every NON-pinned UUID relabelled
/// `<minted-N>` in first-appearance order over the serialized form (so ids
/// nested inside JSON-TEXT columns are reached too).
fn normalize(v: &Value, pinned: &HashSet<String>) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    let serialized = serde_json::to_string(&sorted(&v)).unwrap();
    let ts = regex::Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z").unwrap();
    let serialized = ts.replace_all(&serialized, "<ts>").into_owned();

    let b = serialized.as_bytes();
    let mut out = String::with_capacity(serialized.len());
    let mut map: HashMap<String, String> = HashMap::new();
    let mut i = 0usize;
    while i < b.len() {
        if is_uuid_at(b, i) {
            let candidate = &serialized[i..i + 36];
            if pinned.contains(&candidate.to_lowercase()) {
                out.push_str(candidate);
            } else {
                let n = map.len() + 1;
                let label = map
                    .entry(candidate.to_lowercase())
                    .or_insert_with(|| format!("<minted-{n}>"));
                out.push_str(label);
            }
            i += 36;
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.fract() == 0.0 && f.abs() < 9e15 {
                    *v = json!(f as i64);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| canon_numbers(x)),
        _ => {}
    }
}

fn sorted(v: &Value) -> Value {
    match v {
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                out.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        _ => v.clone(),
    }
}

/// Sort a table's rows by an id-free key.
///
/// ⚠ Neither key may be a minted id: the replayed rows and the two Host bubbles
/// are minted afresh on each side, so an id sort orders the two dumps
/// differently the moment a table holds more than one. `chatId` IS safe — it is
/// a pinned fixture id — and it is also what breaks the tie between a replayed
/// message and the original it was copied from, which are otherwise identical in
/// role and content.
fn sort_rows(table: &str, rows: &mut [Value]) {
    let text = |r: &Value, k: &str| -> String {
        r.get(k).and_then(Value::as_str).unwrap_or("").to_string()
    };
    match table {
        "chat_messages" => rows.sort_by_key(|r| {
            (
                text(r, "chatId"),
                text(r, "systemSender"),
                text(r, "role"),
                text(r, "content"),
                text(r, "createdAt"),
            )
        }),
        "chats" => rows.sort_by_key(|r| text(r, "id")),
        _ => {}
    }
}

fn table_section(v: &Value, table: &str) -> Value {
    let mut v = v.clone();
    if let Some(rows) = v.get_mut("rows").and_then(Value::as_array_mut) {
        sort_rows(table, rows);
    }
    v
}

#[derive(serde::Deserialize)]
struct CaseW {
    name: String,
    source: String,
    destination: String,
}

#[derive(serde::Deserialize)]
struct SpecW {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    cases: Vec<CaseW>,
}

#[test]
fn chat_continuation_matches_oracle() {
    let (Some(oracle_path), Some(fixture_main), Some(fixture_mount)) = (
        env_or_skip("QT_ORACLE_CHAT_CONTINUATION"),
        env_or_skip("QT_FIXTURE_CONT_MAIN"),
        env_or_skip("QT_FIXTURE_CONT_MOUNT"),
    ) else {
        return;
    };

    let spec: SpecW = serde_json::from_str(SPEC).expect("corpus parses");
    let pinned = pinned_ids();

    let oracle_text = std::fs::read_to_string(&oracle_path).expect("read oracle");
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("oracle line parses");
        oracle.insert(
            v["name"]
                .as_str()
                .expect("oracle row has a name")
                .to_string(),
            v,
        );
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    for c in &spec.cases {
        let want = oracle
            .get(&c.name)
            .unwrap_or_else(|| panic!("oracle has no row for {}", c.name));

        let scratch = std::env::temp_dir().join(format!(
            "qt-continuation-harness-{}-{}",
            std::process::id(),
            c.name
        ));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).expect("scratch");
        let main_work = scratch.join("main.db");
        let mount_work = scratch.join("mount.db");
        std::fs::copy(&fixture_main, &main_work).expect("copy main");
        std::fs::copy(&fixture_mount, &mount_work).expect("copy mount");

        let db = Db::open(
            DbPaths {
                main: main_work.clone(),
                mount_index: Some(mount_work.clone()),
                llm_logs: None,
            },
            &spec.test_pepper_base64,
        )
        .expect("open");

        let result = rt
            .block_on(apply_chat_continuation(&db, &c.destination, &c.source))
            .expect("apply_chat_continuation");

        let dump = |t: &'static str| -> Value {
            table_section(
                &db.read_main(move |conn| dump_table_json_conn(conn, t, "id"))
                    .unwrap_or_else(|e| panic!("dump {t}: {e}")),
                t,
            )
        };

        // The insertion-ordered trace. `rowid` IS insertion order on both sides
        // (neither ever deletes a message row), so this is the position proof:
        // the ids come back in `rowid` order and the canonical row bodies come
        // from the same dump helper the table sections use, which keeps the two
        // sides' cell rendering identical by construction.
        const ORDER_COLUMNS: &[&str] = &[
            "chatId",
            "type",
            "role",
            "systemSender",
            "systemKind",
            "content",
            "participantId",
            "targetParticipantIds",
            "opaqueContent",
            "hostEvent",
            "provider",
            "modelName",
            "tokenCount",
            "routeTrail",
        ];
        let all_messages = db
            .read_main(|conn| dump_table_json_conn(conn, "chat_messages", "id"))
            .expect("dump chat_messages for the order trace");
        let by_id: HashMap<String, &Value> = all_messages["rows"]
            .as_array()
            .expect("rows")
            .iter()
            .map(|r| (r["id"].as_str().unwrap_or_default().to_string(), r))
            .collect();
        let ids_in_rowid_order: Vec<String> = db
            .read_main(|conn| {
                let mut stmt = conn.prepare("SELECT id FROM chat_messages ORDER BY rowid")?;
                let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(r?);
                }
                Ok(out)
            })
            .expect("rowid order");
        let message_order = Value::Array(
            ids_in_rowid_order
                .iter()
                .map(|id| {
                    let row = by_id.get(id).expect("every rowid id is in the dump");
                    let mut o = serde_json::Map::new();
                    for col in ORDER_COLUMNS {
                        o.insert(
                            (*col).to_string(),
                            row.get(*col).cloned().unwrap_or(Value::Null),
                        );
                    }
                    Value::Object(o)
                })
                .collect::<Vec<_>>(),
        );

        let got = json!({
            "result": {
                "replayedMessageCount": result.replayed_message_count,
                "hadLibrarianSummary": result.had_librarian_summary,
                "postedSourceTailBubble": result.posted_source_tail_bubble,
            },
            "tables": {
                "chats": dump("chats"),
                "chatMessages": dump("chat_messages"),
            },
            "messageOrder": message_order,
        });

        let want_cmp = json!({
            "result": want["result"],
            "tables": {
                "chats": table_section(&want["tables"]["chats"], "chats"),
                "chatMessages": table_section(&want["tables"]["chatMessages"], "chat_messages"),
            },
            "messageOrder": want["messageOrder"],
        });

        let g = normalize(&got, &pinned);
        let w = normalize(&want_cmp, &pinned);
        assert_eq!(g, w, "chat_continuation mismatch for {}", c.name);

        drop(db);
        let _ = std::fs::remove_dir_all(&scratch);
    }

    // Shape guard: a corpus that silently loses an arm must not read as green.
    assert!(
        spec.cases.len() >= 6,
        "corpus shrank: {} cases",
        spec.cases.len()
    );
    assert!(
        oracle.len() >= spec.cases.len(),
        "stale oracle: {} rows for {} cases",
        oracle.len(),
        spec.cases.len()
    );
}
