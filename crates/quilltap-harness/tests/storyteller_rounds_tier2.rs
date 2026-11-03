//! Tier-2 differential: the `storyteller_rounds` repository + the chat's turn
//! mode (P4.D343, work order items 16 + 11 + 28's C-2).
//!
//! Drives the SAME op sequence through v4's REAL `StorytellerRoundsRepository`
//! and `ChatsRepository` (`setTurnMode`, `delete`) — the oracle case
//! `harness/oracle/cases/storyteller-rounds.ts` — and through v5's
//! [`StorytellerRoundsRepository`], `ChatsRepository::set_turn_mode` and
//! `ChatsRepository::delete`, over copies of ONE fixture pair built by v4's
//! real code (`build-storyteller-rounds-fixture.ts`; the table made by v4's
//! REAL `add-storyteller-mode-v1` migration — the migrated-instance shape, FK
//! and all). The ops and their inputs live in the ORACLE only: every NDJSON
//! row carries its `input`, which this side replays.
//!
//! Each op compares two things, byte for byte (`serde_json::to_string` — a
//! `Map ==` is key-order blind):
//!
//!   - **`result`** — the method's answer as v4 returns it (a round object, in
//!     schema key order; `null`; a bool), or the raw row / table dump (cells as
//!     stored, JSON columns as their TEXT).
//!   - **`logs`** — the lines of the family's pinned set this op logged, each
//!     rendered as the capture rig renders a v5 event
//!     (`common::v4_log_as_capture_line`). All run on THIS thread (the
//!     repository is driven over a plain `Writer`, not the writer task), so
//!     `captured_with` sees every line.
//!
//! ## Normalisation (both sides)
//!
//! v4 stamps `updatedAt` / `closedAt` with the live clock inside `_update` /
//! `markClosed` / `setTurnMode`; v5 takes a fixed `NOW` (and `set_turn_mode`
//! reads the clock). Any `updatedAt` / `closedAt` string NOT among the
//! oracle header's pinned timestamps becomes `<ts>` — presence and null-ness
//! still compare.
//!
//! ## Named carves
//!
//! - `BACKEND_CORRUPT_JSON_LINE` — v4's backend WARN `Corrupted JSON in
//!   column, using default` (`backends/sqlite/backend.ts`), unported by
//!   standing convention (the backend lines). The DECODE it reports (an
//!   unparseable JSON cell reads as absent) is compared through the results.
//! - `ZOD_ERROR_TEXT` — `Error finding entity by ID`'s `error` when v4's is a
//!   Zod issue list (`[ { "code": … } ]`): compared by presence (both sides
//!   log the line with an `error` field), never by text.
//!
//! ## What the oracle measured about v4 (each row pins it)
//!
//! - A read OMITS the keys of NULL cells (`selection` / `closedReason` /
//!   `closedAt`): v4's backend turns NULL into `undefined` before Zod.
//! - A NULL `triggerMessageId` makes the row UNREADABLE: `.nullable()` refuses
//!   the `undefined` the backend hands it, so `findById` logs `Error finding
//!   entity by ID` and `findByFilter` drops the row (the create itself
//!   succeeds and answers `triggerMessageId: null`).
//! - An unparseable / `'null'` JSON cell reads as ABSENT (`fromJsonSafe`):
//!   `bids` falls to its `[]` default and the row survives; a parseable `bids`
//!   holding a card the schema refuses drops the row.
//! - `_update` rewrites EVERY column (an unparseable `selection` cell becomes
//!   NULL on the next bid).
//! - `Error finding active storyteller round` / `… latest …` are UNREACHABLE:
//!   `findByFilter` is itself a fallback (`Error finding entities by filter` →
//!   `[]`), so the outer wraps never see a throw (the throwing-plant rows).
//!
//! A second test, [`chat_delete_sweeps_rounds_without_fk`], is v5-only: it
//! proves contract C-2 (the chat delete's EXPLICIT round sweep) on the
//! FK-less shape a v5-provisioned instance has — the differential cannot, as
//! the fixture's FK cascade does the work on both sides.
//!
//! Generate the fixture + oracle (Node 24, from the PINNED v4 worktree —
//! §R.3 PIN REQUIRED):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_FIXTURE_SR_MAIN=/tmp/qt-sr-main.db QT_FIXTURE_SR_MOUNT=/tmp/qt-sr-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-storyteller-rounds-fixture.ts
//!   TZ=UTC QT_FIXTURE_SR_MAIN=/tmp/qt-sr-main.db QT_FIXTURE_SR_MOUNT=/tmp/qt-sr-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/cases/storyteller-rounds.ts \
//!     > /tmp/oracle-storyteller-rounds.ndjson
//! Run:
//!   QT_ORACLE_STORYTELLER_ROUNDS=/tmp/oracle-storyteller-rounds.ndjson \
//!   QT_FIXTURE_SR_MAIN=/tmp/qt-sr-main.db QT_FIXTURE_SR_MOUNT=/tmp/qt-sr-mount.db \
//!     cargo test -p quilltap-harness --test storyteller_rounds_tier2 -- --nocapture

mod common;

use std::collections::HashSet;

use quilltap_core::db::chats::ChatsRepository;
use quilltap_core::db::storyteller_rounds::StorytellerRoundsRepository;
use quilltap_core::db::Writer;
use quilltap_core::storyteller::types::{
    StorytellerBid, StorytellerClosedReason, StorytellerRound, StorytellerRoundStatus,
    StorytellerSelection,
};
use quilltap_core::test_support::captured_with;
use serde_json::{Map, Value};

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
/// v5's clock for every update — deliberately NOT a pinned timestamp, so it
/// masks to `<ts>` exactly as v4's live clock does.
const NOW: &str = "2026-06-01T00:00:00.000Z";
/// The minimum op rows a current oracle carries (a stale / truncated NDJSON
/// fails loudly instead of passing on nothing).
const MIN_OPS: usize = 55;

/// The family's pinned v4 lines (the oracle's `PINNED`, less the carve).
const PINNED: &[&str] = &[
    "Storyteller bid dropped — round not open",
    "Error finding active storyteller round",
    "Error finding latest storyteller round",
    "Error finding entity by ID",
    "Error finding entities by filter",
    "Entity not found for update",
    "Error updating entity",
    "Failed to write turn mode",
    "Turn mode write",
];
/// Carve `BACKEND_CORRUPT_JSON_LINE` (see the header).
const BACKEND_CORRUPT_JSON_LINE: &str = "Corrupted JSON in column, using default";

/// The v5 tracing target each pinned line is emitted under.
fn target_for(message: &str) -> &'static str {
    match message {
        "Storyteller bid dropped — round not open" => "quilltap::storyteller",
        _ => "quilltap::db",
    }
}

fn mask_ts(v: &mut Value, pinned: &HashSet<String>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m.iter_mut() {
                if (k == "updatedAt" || k == "closedAt")
                    && x.as_str().is_some_and(|s| !pinned.contains(s))
                {
                    *x = Value::String("<ts>".into());
                } else {
                    mask_ts(x, pinned);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| mask_ts(x, pinned)),
        _ => {}
    }
}

fn s<'a>(input: &'a Value, key: &str) -> &'a str {
    input[key]
        .as_str()
        .unwrap_or_else(|| panic!("input key {key} not a string: {input}"))
}

fn round_value(r: Option<StorytellerRound>) -> Value {
    r.map_or(Value::Null, |r| {
        serde_json::to_value(r).expect("round serialises")
    })
}

/// Raw rows, cells as stored (TEXT as strings, NULL as null), columns in the
/// statement's order — v4's `better-sqlite3` `.all()` shape.
fn raw_rows(conn: &rusqlite::Connection, sql: &str, args: &[&str]) -> Vec<Value> {
    let mut stmt = conn.prepare(sql).expect("prepare raw select");
    let names: Vec<String> = stmt.column_names().iter().map(|n| n.to_string()).collect();
    let rows = stmt
        .query_map(rusqlite::params_from_iter(args.iter()), |r| {
            let mut m = Map::new();
            for (i, name) in names.iter().enumerate() {
                let v = match r.get_ref(i)? {
                    rusqlite::types::ValueRef::Null => Value::Null,
                    rusqlite::types::ValueRef::Integer(n) => Value::from(n),
                    rusqlite::types::ValueRef::Real(f) => Value::from(f),
                    rusqlite::types::ValueRef::Text(t) => {
                        Value::String(String::from_utf8_lossy(t).into_owned())
                    }
                    rusqlite::types::ValueRef::Blob(b) => Value::from(b.to_vec()),
                };
                m.insert(name.clone(), v);
            }
            Ok(Value::Object(m))
        })
        .expect("query raw rows");
    rows.map(|r| r.expect("raw row")).collect()
}

/// Run one op through v5. `Err` is a thrown op (rendered `{threw}` as the
/// oracle does).
fn run_op(writer: &Writer, op: &str, input: &Value) -> Result<Value, String> {
    let conn = writer.connection();
    let repo = StorytellerRoundsRepository::new(conn);
    let e = |e: quilltap_core::db::DbError| e.to_string();
    Ok(match op {
        "create" => {
            let status: StorytellerRoundStatus =
                serde_json::from_value(input["status"].clone()).expect("status");
            let bids: Vec<StorytellerBid> =
                serde_json::from_value(input["bids"].clone()).expect("bids");
            let r = repo
                .create(
                    s(input, "chatId"),
                    input["triggerMessageId"].as_str(),
                    status,
                    bids,
                    s(input, "id"),
                    s(input, "createdAt"),
                )
                .map_err(e)?;
            round_value(Some(r))
        }
        "findById" => round_value(repo.find_by_id(s(input, "id"))),
        "findActiveForChat" => round_value(repo.find_active_for_chat(s(input, "chatId"))),
        "findLatestForChat" => round_value(repo.find_latest_for_chat(s(input, "chatId"))),
        "recordBid" => {
            let bid: StorytellerBid = serde_json::from_value(input["bid"].clone()).expect("bid");
            round_value(repo.record_bid(s(input, "roundId"), bid, NOW).map_err(e)?)
        }
        "markRunning" => {
            let sel: StorytellerSelection =
                serde_json::from_value(input["selection"].clone()).expect("selection");
            round_value(
                repo.mark_running(s(input, "roundId"), sel, NOW)
                    .map_err(e)?,
            )
        }
        "markClosed" => {
            let reason: StorytellerClosedReason =
                serde_json::from_value(input["reason"].clone()).expect("reason");
            // `selection` absent → leave the stored one; present `null` →
            // write a null; present object → write it.
            let selection = input.get("selection").map(|v| {
                (!v.is_null()).then(|| {
                    serde_json::from_value::<StorytellerSelection>(v.clone()).expect("sel")
                })
            });
            round_value(
                repo.mark_closed(s(input, "roundId"), reason, selection, NOW)
                    .map_err(e)?,
            )
        }
        "setTurnMode" => Value::Bool(
            ChatsRepository::new(conn).set_turn_mode(s(input, "chatId"), s(input, "mode")),
        ),
        "deleteChat" => Value::Bool(writer.chats().delete(s(input, "chatId")).map_err(e)?),
        "execSql" => {
            conn.execute_batch(s(input, "sql"))
                .map_err(|x| x.to_string())?;
            Value::Null
        }
        "rawRow" => raw_rows(
            conn,
            r#"SELECT * FROM "storyteller_rounds" WHERE "id" = ?1"#,
            &[s(input, "id")],
        )
        .into_iter()
        .next()
        .unwrap_or(Value::Null),
        "chatCells" => Value::Array(raw_rows(
            conn,
            r#"SELECT "id", "turnMode", "updatedAt" FROM "chats" ORDER BY "id""#,
            &[],
        )),
        "dump" => {
            let mut m = Map::new();
            m.insert(
                "rounds".into(),
                Value::Array(raw_rows(
                    conn,
                    r#"SELECT * FROM "storyteller_rounds" ORDER BY "id""#,
                    &[],
                )),
            );
            m.insert(
                "chats".into(),
                Value::Array(raw_rows(
                    conn,
                    r#"SELECT "id", "turnMode", "updatedAt" FROM "chats" ORDER BY "id""#,
                    &[],
                )),
            );
            Value::Object(m)
        }
        other => panic!("unknown op {other}"),
    })
}

/// The message of a capture-rig line (`<LEVEL> <target> <message> k=v …`),
/// when it is one of the pinned set.
fn pinned_message(line: &str) -> Option<&'static str> {
    let rest = line.split_once(' ')?.1.split_once(' ')?.1;
    PINNED
        .iter()
        .copied()
        .find(|p| rest == *p || rest.starts_with(&format!("{p} ")))
}

/// Carve `ZOD_ERROR_TEXT`: the `error=` tail of a line (the last field on
/// `Error finding entity by ID`) replaced by a presence token.
fn mask_error_tail(line: &str) -> String {
    match line.find(" error=") {
        Some(i) => format!("{} error=<schema-refusal>", &line[..i]),
        None => line.to_string(),
    }
}

fn copy_fixture(dir: &std::path::Path) -> std::path::PathBuf {
    let main_src = std::env::var("QT_FIXTURE_SR_MAIN").expect("QT_FIXTURE_SR_MAIN");
    let mount_src = std::env::var("QT_FIXTURE_SR_MOUNT").expect("QT_FIXTURE_SR_MOUNT");
    let main = dir.join("main.db");
    std::fs::copy(&main_src, &main).unwrap_or_else(|e| panic!("copy main fixture: {e}"));
    std::fs::copy(&mount_src, dir.join("mount.db"))
        .unwrap_or_else(|e| panic!("copy mount fixture: {e}"));
    main
}

#[test]
fn storyteller_rounds_tier2_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_STORYTELLER_ROUNDS") else {
        eprintln!("SKIP: set QT_ORACLE_STORYTELLER_ROUNDS (see header).");
        return;
    };
    if std::env::var("QT_FIXTURE_SR_MAIN").is_err() || std::env::var("QT_FIXTURE_SR_MOUNT").is_err()
    {
        eprintln!("SKIP: set QT_FIXTURE_SR_MAIN / QT_FIXTURE_SR_MOUNT (see header).");
        return;
    }
    let text = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle {oracle_path}: {e}"));
    let mut rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle row"))
        .collect();
    let header = rows.remove(0);
    assert_eq!(
        header["op"], "header",
        "the oracle's first row is its header"
    );
    let pinned: HashSet<String> = header["pinnedTimestamps"]
        .as_array()
        .expect("pinnedTimestamps")
        .iter()
        .map(|v| v.as_str().expect("ts").to_string())
        .collect();
    assert!(
        rows.len() >= MIN_OPS,
        "oracle carries {} op rows, expected at least {MIN_OPS} — stale or truncated",
        rows.len()
    );

    let dir = tempfile::Builder::new()
        .prefix("qt-sr-rust-")
        .tempdir()
        .expect("tempdir");
    let main = copy_fixture(dir.path());
    let writer = Writer::open_writable(&main, TEST_PEPPER).expect("open fixture copy");

    let mut mismatches = Vec::new();
    for row in &rows {
        let i = row["i"].as_u64().expect("i");
        let op = row["op"].as_str().expect("op");
        let input = &row["input"];

        let (got, lines) = captured_with(|| run_op(&writer, op, input));
        let mut got = got.unwrap_or_else(|t| {
            let mut m = Map::new();
            m.insert("threw".into(), Value::String(t));
            Value::Object(m)
        });
        let mut want = row["result"].clone();
        mask_ts(&mut got, &pinned);
        mask_ts(&mut want, &pinned);
        let (got_s, want_s) = (
            serde_json::to_string(&got).unwrap(),
            serde_json::to_string(&want).unwrap(),
        );
        if got_s != want_s {
            mismatches.push(format!(
                "op {i} {op} RESULT\n   v5: {got_s}\n   v4: {want_s}"
            ));
        }

        // Logs: v4's pinned lines (less the carve) vs v5's pinned lines.
        let v4_logs: Vec<&Value> = row["logs"]
            .as_array()
            .expect("logs")
            .iter()
            .filter(|l| l["message"] != BACKEND_CORRUPT_JSON_LINE)
            .collect();
        let v5_lines: Vec<String> = lines
            .iter()
            .filter(|l| pinned_message(l).is_some())
            .cloned()
            .collect();
        let mut want_lines = Vec::new();
        let mut got_lines = Vec::new();
        for (k, log) in v4_logs.iter().enumerate() {
            let message = log["message"].as_str().expect("message");
            let mut line = common::v4_log_as_capture_line(target_for(message), log);
            let mut v5 = v5_lines.get(k).cloned().unwrap_or_default();
            let zod = log["fields"]["error"]
                .as_str()
                .is_some_and(|e| e.trim_start().starts_with('['));
            if zod {
                line = mask_error_tail(&line);
                v5 = mask_error_tail(&v5);
            }
            want_lines.push(line);
            got_lines.push(v5);
        }
        got_lines.extend(v5_lines.iter().skip(v4_logs.len()).cloned());
        if got_lines != want_lines {
            mismatches.push(format!(
                "op {i} {op} LOGS\n   v5: {got_lines:#?}\n   v4: {want_lines:#?}"
            ));
        }
    }

    eprintln!(
        "storyteller_rounds_tier2: {} ops compared, {} mismatches",
        rows.len(),
        mismatches.len()
    );
    assert!(
        mismatches.is_empty(),
        "{} mismatch(es):\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

/// Contract C-2 (v5-only): a chat delete removes the chat's rounds
/// EXPLICITLY. A v5-provisioned instance carries generateDDL's table with NO
/// foreign key, so the cascade the differential's fixture relies on is
/// absent there; with `foreign_keys = OFF` the FK cannot act, so only the
/// sweep can. The control proves the plant: a bare `DELETE FROM chats` under
/// the same pragma leaves the rows behind.
#[test]
fn chat_delete_sweeps_rounds_without_fk() {
    if std::env::var("QT_FIXTURE_SR_MAIN").is_err() || std::env::var("QT_FIXTURE_SR_MOUNT").is_err()
    {
        eprintln!("SKIP: set QT_FIXTURE_SR_MAIN / QT_FIXTURE_SR_MOUNT (see header).");
        return;
    }
    const CHAT_A: &str = "c5000000-0000-4000-8000-00000000000a";
    const CHAT_B: &str = "c5000000-0000-4000-8000-00000000000b";
    let count = |conn: &rusqlite::Connection, chat: &str| -> i64 {
        conn.query_row(
            r#"SELECT COUNT(*) FROM "storyteller_rounds" WHERE "chatId" = ?1"#,
            [chat],
            |r| r.get(0),
        )
        .expect("count")
    };
    let seed = |writer: &Writer| {
        let repo = StorytellerRoundsRepository::new(writer.connection());
        for (id, chat) in [
            ("55000000-0000-4000-8000-0000000000c1", CHAT_A),
            ("55000000-0000-4000-8000-0000000000c2", CHAT_A),
            ("55000000-0000-4000-8000-0000000000c3", CHAT_B),
        ] {
            repo.create(
                chat,
                None,
                StorytellerRoundStatus::Open,
                Vec::new(),
                id,
                NOW,
            )
            .expect("seed round");
        }
    };

    // Control: the plant holds (no FK action, no sweep → rows survive).
    let dir = tempfile::Builder::new()
        .prefix("qt-sr-c2-")
        .tempdir()
        .expect("tempdir");
    let writer = Writer::open_writable(&copy_fixture(dir.path()), TEST_PEPPER).expect("open");
    writer
        .connection()
        .execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    seed(&writer);
    writer
        .connection()
        .execute(r#"DELETE FROM "chats" WHERE "id" = ?1"#, [CHAT_A])
        .expect("bare delete");
    assert_eq!(
        count(writer.connection(), CHAT_A),
        2,
        "control: no FK, no sweep"
    );
    drop(writer);

    // The sweep.
    let dir = tempfile::Builder::new()
        .prefix("qt-sr-c2-")
        .tempdir()
        .expect("tempdir");
    let writer = Writer::open_writable(&copy_fixture(dir.path()), TEST_PEPPER).expect("open");
    writer
        .connection()
        .execute_batch("PRAGMA foreign_keys = OFF")
        .expect("fk off");
    seed(&writer);
    assert!(writer.chats().delete(CHAT_A).expect("delete chat A"));
    assert_eq!(
        count(writer.connection(), CHAT_A),
        0,
        "C-2: A's rounds swept"
    );
    assert_eq!(
        count(writer.connection(), CHAT_B),
        1,
        "C-2: B's round untouched"
    );
}
