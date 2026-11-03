//! Tier-2 route differential (P4.D347, v4 `9a88f862f`): `GET /api/v1/chats/[id]
//! ?action=absent-mentions` — v4's REAL `handleGetAbsentMentions`
//! (`harness/oracle/cases/chat-absent-mentions.test.ts`) against
//! `quilltap_core::api::chat_absent_mentions::chat_absent_mentions`, each case
//! over a FRESH copy of the widened build-context pair with the case's plant
//! SQL (emitted by the oracle, applied here verbatim).
//!
//! Compared per case: the status; the body as BYTES (`serde_json::to_string`
//! against v4's `JSON.stringify` — key order `messageId, characterId, text,
//! card` and `name, aliases, pronouns, body`, `pronouns: null` never absent);
//! and v4's `[Chats v1] Read absent-character mentions` DEBUG (`chatId`,
//! `mentions`, `context`), with the 404 logging none.
//!
//! Cases: two absent characters first named in one message (one by alias, the
//! pending list's reverse order), a repeat and a Staff row skipped; only seated
//! characters named (`{"mentions":[]}`); the persona named in a Salon (in the
//! room) and in an autonomous room (a mention — bug 172); a missing chat (404
//! `{"error":"Chat not found"}`, v4's `notFound('Chat')`).
//!
//! Build the pair + the oracle (Node 24, from the v4 checkout; jest ignores
//! `.claude/`, so stage the case and the corpus in a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-cam-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/chat-absent-mentions.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/build-context-tier3.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_FIXTURE_OUT=/tmp/qt-build-context-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-build-context-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-context-tier3-fixture.ts
//!   TZ=UTC QT_FIXTURE_BC_MAIN=/tmp/qt-build-context-main.db \
//!   QT_FIXTURE_BC_MOUNT=/tmp/qt-build-context-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-chat-absent-mentions.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "chat-absent-mentions\.test\.ts$"
//! Run:
//!   QT_ORACLE_CHAT_ABSENT_MENTIONS=/tmp/oracle-chat-absent-mentions.ndjson \
//!   QT_FIXTURE_BC_MAIN=/tmp/qt-build-context-main.db \
//!   QT_FIXTURE_BC_MOUNT=/tmp/qt-build-context-mount.db \
//!     cargo test -p quilltap-harness --test chat_absent_mentions_routes_equivalence -- --nocapture

use std::path::Path;

use quilltap_core::api::chat_absent_mentions::chat_absent_mentions;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::test_support::global_capture;
use serde_json::Value;

const LINE: &str = "[Chats v1] Read absent-character mentions";

fn spec() -> Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/build-context-tier3.json");
    serde_json::from_str(&std::fs::read_to_string(p).expect("read spec")).expect("parse spec")
}

/// `(status, body bytes)` for a `Response`, as the wire carries it.
fn status_body(r: &Response) -> (u16, String) {
    match r {
        Response::ChatAbsentMentions(v) => (200, serde_json::to_string(v).unwrap()),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::NotFound => 404,
                ErrorKind::BadRequest => 400,
                ErrorKind::Unavailable => 503,
                _ => 500,
            };
            (
                status,
                serde_json::to_string(&serde_json::json!({ "error": e.message })).unwrap(),
            )
        }
        other => (500, serde_json::to_string(other).unwrap()),
    }
}

#[test]
fn chat_absent_mentions_routes_match_v4() {
    let vars = [
        "QT_ORACLE_CHAT_ABSENT_MENTIONS",
        "QT_FIXTURE_BC_MAIN",
        "QT_FIXTURE_BC_MOUNT",
    ];
    let Some(env) = vars
        .iter()
        .map(|k| std::env::var(k).ok())
        .collect::<Option<Vec<_>>>()
    else {
        eprintln!("SKIP: set {vars:?} (see the test header).");
        return;
    };
    let (oracle, fixture_main, fixture_mount) = (&env[0], &env[1], &env[2]);
    let spec = spec();
    let pepper = spec["testPepperBase64"].as_str().unwrap().to_string();
    let user_id = spec["userId"].as_str().unwrap().to_string();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let text = std::fs::read_to_string(oracle).expect("read oracle");
    let mut cases = 0usize;
    let mut mentions_seen = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let name = row["name"].as_str().unwrap();
        let chat_id = row["chatId"].as_str().unwrap();

        let dir = tempfile::Builder::new()
            .prefix("qt-cam-")
            .tempdir()
            .unwrap();
        let main = dir.path().join("main.db");
        let mount = dir.path().join("mount.db");
        std::fs::copy(fixture_main, &main).unwrap();
        std::fs::copy(fixture_mount, &mount).unwrap();
        {
            let w = quilltap_core::db::Writer::open_writable(&main, &pepper).expect("open main");
            for sql in row["plantSql"].as_array().unwrap() {
                w.connection()
                    .execute_batch(sql.as_str().unwrap())
                    .unwrap_or_else(|e| panic!("{name}: plant failed: {e}"));
            }
        }
        let db = Db::open(
            DbPaths {
                main,
                mount_index: Some(mount),
                llm_logs: None,
            },
            &pepper,
        )
        .expect("open db");

        let (resp, lines) = rt.block_on(global_capture::capture_async(chat_absent_mentions(
            &db, chat_id, &user_id,
        )));
        let (status, body) = status_body(&resp);
        assert_eq!(
            status,
            row["status"].as_u64().unwrap() as u16,
            "{name}: status"
        );
        assert_eq!(body, row["raw"].as_str().unwrap(), "{name}: body bytes");
        if status == 200 {
            mentions_seen += serde_json::from_str::<Value>(&body).unwrap()["mentions"]
                .as_array()
                .unwrap()
                .len();
        }

        // The DEBUG: v4's per-case lines vs v5's, field for field.
        let got: Vec<&String> = lines.iter().filter(|l| l.contains(LINE)).collect();
        let want = row["logs"].as_array().unwrap();
        assert_eq!(got.len(), want.len(), "{name}: `{LINE}` lines {got:#?}");
        for (g, w) in got.iter().zip(want) {
            assert!(g.starts_with("DEBUG "), "{name}: {g}");
            for (k, v) in w["fields"].as_object().unwrap() {
                let needle = match v {
                    Value::String(s) => format!(" {k}={s}"),
                    other => format!(" {k}={other}"),
                };
                assert!(g.contains(&needle), "{name}: {needle:?} in {g}");
            }
        }
        cases += 1;
    }
    assert_eq!(cases, 5, "the oracle lost a case");
    assert!(
        mentions_seen >= 3,
        "the mention arms went vacuous: {mentions_seen}"
    );
    println!("chat_absent_mentions_routes: {cases} cases equal ({mentions_seen} mentions)");
}
