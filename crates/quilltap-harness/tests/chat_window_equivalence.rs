//! P4.D375 differential: `services::chat_window` (C8 — P4.D375's alone) and
//! the `chat_ids` scope of `db::conversation_search` vs v4 4.12.1's REAL
//! `lib/scriptorium/chat-window.ts` + `searchConversationChunks`
//! (`harness/oracle/cases/chat-window.ts`, the `624d0d25c` pin).
//!
//! Tier 1 (`span` / `overlap` rows): `chat_span_of` / `chat_overlaps_window`
//! exact over a pure corpus. Tier 2 (`db` rows): over a COPY of the
//! search-tools fixture pair with the oracle's chat plants replayed byte-for-
//! byte; per op the result (ids in order; chunk scores at 1e-12) and the lines
//! (`Chats in time window`, `Could not read chats for a time window`, `Error
//! finding entities by filter`, `Conversation search restricted to window
//! chats` — message and fields in v4's order).
//!
//! **`CHATS_BY_CHARACTER_RETHROW_LINE_ABSENT` (a ruled both-ways carve,
//! P4.D375 item 23 / L-2):** inside the strict scope v4's
//! `chats.findByCharacterId` is a RETHROW-mode `safeQuery` that logs ERROR
//! `Failed to find chats by character ID` between the filter line and the
//! chat window's WARN. v5 has no home for that line for ANY caller of
//! `chats_read::find_by_character_id`, so the carve asserts it PRESENT in v4's
//! lines and ABSENT from v5's, then drops it before the compare. Retires when
//! the rethrow line is ported (a follow-ups item — it moves every caller).
//!
//! Regenerate + run:
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-cw-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-cw-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-search-tools-fixture.ts
//!   QT_FIXTURE_CW_MAIN=/tmp/qt-cw-main.db QT_FIXTURE_CW_MOUNT=/tmp/qt-cw-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/cases/chat-window.ts > /tmp/oracle-chat-window.ndjson
//!   cd $V5W
//!   QT_ORACLE_CHAT_WINDOW=/tmp/oracle-chat-window.ndjson \
//!   QT_FIXTURE_CW_MAIN=/tmp/qt-cw-main.db QT_FIXTURE_CW_MOUNT=/tmp/qt-cw-mount.db \
//!     cargo test -p quilltap-harness --test chat_window_equivalence -- --nocapture

use std::collections::HashSet;

use quilltap_core::db::conversation_search::{
    search_conversation_chunks, ConversationSearchOptions,
};
use quilltap_core::db::fallback::with_strict_repository_failures;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::recall_tags::TimeWindow;
use quilltap_core::services::chat_window::{
    chat_overlaps_window, chat_span_of, find_chats_in_window,
};
use quilltap_core::test_support::global_capture;
use serde_json::{json, Value};

/// The messages compared, and v5's target for each.
const LINES: &[(&str, &str)] = &[
    ("Chats in time window", "quilltap::chat_window"),
    (
        "Could not read chats for a time window",
        "quilltap::chat_window",
    ),
    ("Error finding entities by filter", "quilltap::db"),
    (
        "Conversation search restricted to window chats",
        "quilltap_core::db::conversation_search",
    ),
];

/// `CHATS_BY_CHARACTER_RETHROW_LINE_ABSENT` — v4's rethrow-mode line v5 has no
/// home for (module doc).
const CHATS_BY_CHARACTER_RETHROW_LINE_ABSENT: &str = "Failed to find chats by character ID";

fn spec() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/search-tools.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn window_of(v: &Value) -> TimeWindow {
    TimeWindow {
        from: v["from"].as_str().unwrap().to_string(),
        to: v["to"].as_str().unwrap().to_string(),
    }
}

fn close(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() <= 1e-12
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| close(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y)
                    .all(|((k, p), (l, q))| k == l && close(p, q))
        }
        _ => a == b,
    }
}

fn want_lines(op: &Value) -> Vec<String> {
    op["lines"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["message"] != CHATS_BY_CHARACTER_RETHROW_LINE_ABSENT)
        .map(|l| {
            let message = l["message"].as_str().unwrap();
            let target = LINES.iter().find(|(m, _)| *m == message).unwrap().1;
            let mut s = format!(
                "{} {target} {message}",
                l["level"].as_str().unwrap().to_uppercase()
            );
            for (k, v) in l["fields"].as_object().unwrap() {
                match v {
                    Value::String(t) => s.push_str(&format!(" {k}={t}")),
                    other => s.push_str(&format!(" {k}={other}")),
                }
            }
            s
        })
        .collect()
}

fn got_lines(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .filter(|l| LINES.iter().any(|(m, t)| l.contains(&format!(" {t} {m}"))))
        .collect()
}

#[test]
fn chat_window_matches_v4() {
    let Ok(oracle) = std::env::var("QT_ORACLE_CHAT_WINDOW") else {
        eprintln!("SKIP: set QT_ORACLE_CHAT_WINDOW to the oracle NDJSON (see header).");
        return;
    };
    let (Ok(main_fixture), Ok(mount_fixture)) = (
        std::env::var("QT_FIXTURE_CW_MAIN"),
        std::env::var("QT_FIXTURE_CW_MOUNT"),
    ) else {
        eprintln!("SKIP: set QT_FIXTURE_CW_MAIN / QT_FIXTURE_CW_MOUNT (see header).");
        return;
    };
    global_capture::install();
    let spec = spec();
    let pepper = spec["testPepperBase64"].as_str().unwrap();
    let user = spec["userId"].as_str().unwrap();
    let rows: Vec<Value> = std::fs::read_to_string(&oracle)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let plants = rows
        .iter()
        .find(|r| r["kind"] == "plants")
        .expect("the plants row");

    let mut failures = Vec::new();
    let (mut tier1, mut db_ops, mut carve_met) = (0usize, 0usize, 0usize);
    // ---- tier 1 ----
    for r in rows
        .iter()
        .filter(|r| r["kind"] == "span" || r["kind"] == "overlap")
    {
        let label = r["label"].as_str().unwrap();
        let got = if r["kind"] == "span" {
            match chat_span_of(&r["chat"]) {
                Some(s) => json!({ "start": s.start, "end": s.end }),
                None => Value::Null,
            }
        } else {
            json!(chat_overlaps_window(&r["chat"], &window_of(&r["window"])))
        };
        if !close(&got, &r["result"]) {
            failures.push(format!(
                "{} {label}: v5 {got} v4 {}",
                r["kind"], r["result"]
            ));
        }
        tier1 += 1;
    }

    // ---- tier 2: the plants on a fresh copy, then the ops in order ----
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("main.db");
    let mount = dir.path().join("mount.db");
    std::fs::copy(&main_fixture, &main).unwrap();
    std::fs::copy(&mount_fixture, &mount).unwrap();
    {
        let w = quilltap_core::db::Writer::open_writable(&main, pepper).unwrap();
        for sql in plants["plants"].as_array().unwrap() {
            w.connection().execute_batch(sql.as_str().unwrap()).unwrap();
        }
    }
    let open = || {
        Db::open(
            DbPaths {
                main: main.clone(),
                mount_index: Some(mount.clone()),
                llm_logs: None,
            },
            pepper,
        )
        .unwrap()
    };
    let mut db = open();
    let embedding: Vec<f32> = spec["cannedEmbeddings"]["what did we say about the ledger"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap() as f32)
        .collect();
    for (i, op) in rows.iter().filter(|r| r["kind"] == "db").enumerate() {
        let input = &op["input"];
        let kind = op["op"].as_str().unwrap();
        if kind == "plant" {
            drop(db);
            let w = quilltap_core::db::Writer::open_writable(&main, pepper).unwrap();
            w.connection()
                .execute_batch(input["sql"].as_str().unwrap())
                .unwrap();
            drop(w);
            db = open();
            continue;
        }
        let (got, lines) = global_capture::capture(|| match kind {
            "find" => {
                let w = window_of(&input["window"]);
                let cid = input["characterId"].as_str();
                let exclude = input["excludeChatId"].as_str();
                let strict = input["strict"].as_bool().unwrap_or(false);
                let chats = db
                    .read_main(|c| {
                        Ok(if strict {
                            with_strict_repository_failures(|| {
                                find_chats_in_window(c, cid, user, &w, exclude)
                            })
                        } else {
                            find_chats_in_window(c, cid, user, &w, exclude)
                        })
                    })
                    .unwrap();
                json!(chats.iter().map(|c| c["id"].clone()).collect::<Vec<_>>())
            }
            "conversations" => {
                let opts = ConversationSearchOptions {
                    character_id: spec["charAId"].as_str().map(str::to_string),
                    user_id: Some(user.to_string()),
                    limit: Some(10),
                    min_score: input["minScore"].as_f64(),
                    query: Some("what did we say about the ledger".to_string()),
                    apply_literal_phrase_boost: true,
                    chat_ids: input["chatIds"].as_array().map(|a| {
                        a.iter()
                            .map(|v| v.as_str().unwrap().to_string())
                            .collect::<HashSet<_>>()
                    }),
                };
                let hits = db
                    .read_main(|c| search_conversation_chunks(c, &embedding, &opts))
                    .unwrap();
                json!(hits
                    .iter()
                    .map(
                        |h| json!({ "chunkId": h.chunk_id, "chatId": h.chat_id, "score": h.score })
                    )
                    .collect::<Vec<_>>())
            }
            other => panic!("no Rust script for op {other}"),
        });
        db_ops += 1;
        let v4_lines = op["lines"].as_array().unwrap();
        if v4_lines
            .iter()
            .any(|l| l["message"] == CHATS_BY_CHARACTER_RETHROW_LINE_ABSENT)
        {
            assert!(
                !lines.iter().any(|l| l.contains(CHATS_BY_CHARACTER_RETHROW_LINE_ABSENT)),
                "op {i}: v5 now logs the rethrow line — retire CHATS_BY_CHARACTER_RETHROW_LINE_ABSENT"
            );
            carve_met += 1;
        }
        let got_lines = got_lines(lines);
        let want_lines = want_lines(op);
        if !close(&got, &op["result"]) {
            failures.push(format!(
                "op {i} ({kind} {input}): result\n  v5 {got}\n  v4 {}",
                op["result"]
            ));
        }
        if got_lines != want_lines {
            failures.push(format!(
                "op {i} ({kind} {input}): lines\n  v5 {got_lines:#?}\n  v4 {want_lines:#?}"
            ));
        }
    }
    assert!(tier1 >= 15, "tier-1 rows: {tier1}");
    assert!(db_ops >= 12, "db ops: {db_ops}");
    assert_eq!(
        carve_met, 1,
        "CHATS_BY_CHARACTER_RETHROW_LINE_ABSENT rows met"
    );
    assert!(
        failures.is_empty(),
        "{} divergence(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!("OK: chat window matched v4 ({tier1} tier-1 rows, {db_ops} db ops).");
}
