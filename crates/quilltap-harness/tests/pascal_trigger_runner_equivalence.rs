//! Real-DB differential (P4.D305, v4 `362426c8f`): the trigger-phrase RUNNER —
//! `pascal::trigger_runner::run_triggered_custom_tools` against v4's REAL
//! `runTriggeredCustomTools` (the real roster, run service, Pascal / Prospero
//! writers and repositories on both sides; only v4's logger is spliced), over
//! the committed `pascal-run-custom-{main,mount}.db` fixture's trigger room
//! (`c1…07`, whose project links the Trigger Shelf store).
//!
//! Per case, on a fresh copy: the room's `chat_messages` system rows in
//! INSERTION order (`rowid` — two posts in one message may share a
//! `createdAt` millisecond), with the minted id + `createdAt` positionally
//! normalised and `pascalMeta` compared as its stored TEXT (bytes — so
//! `invokedBy`, `triggerPhrase`, `callerParticipantId` key order is checked);
//! the returned posts and the `on_posted` calls as positions into those rows;
//! the stores after the run; and the pinned log lines — the runner NEVER
//! fails, so its failures are log-only and the lines are the evidence. The
//! gate rows prove nothing was read by the absence of every line and row.
//! `nowMs` is the ONE clock reading for the message: the oracle froze
//! `Date.now()` to it and recorded it; this side passes the recorded value.
//!
//! The consult seam: this side passes a real `ProviderConsultRunner` (the
//! route family's shape) — the fixture holds no connection profiles, so both
//! sides stop at v4's `no connection profiles are configured` fail-soft arm.
//! v5's OTHER arm (no runner in hand → `execute_custom_tool`'s no-invoker
//! fail-soft) has no v4 twin; it is pinned by `consult_none_takes_the_no_invoker_arm`
//! below, v5-only.
//!
//! Generate the oracle (Node 24; jest ignores `.claude/`, so mirror the case
//! file; the fixture is copied first so the committed pair is never written):
//!   cd ~/source/quilltap-server
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   M=/tmp/qt-pascal-trigger-mirror; rm -rf $M; mkdir -p $M/cases $M/fixtures $M/db
//!   cp $V5W/harness/oracle/cases/pascal-trigger-runner.test.ts $M/cases/
//!   cp $V5W/harness/oracle/fixtures/pascal-run-custom.json $M/fixtures/
//!   cp $V5W/crates/quilltap-web/tests/fixtures/pascal-run-custom-main.db $M/db/
//!   cp $V5W/crates/quilltap-web/tests/fixtures/pascal-run-custom-mount.db $M/db/
//!   QT_FIXTURE_PASCAL_MAIN=$M/db/pascal-run-custom-main.db \
//!   QT_FIXTURE_PASCAL_MOUNT=$M/db/pascal-run-custom-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-pascal-trigger-runner.ndjson \
//!     npx jest --silent --roots "$PWD" --roots "$M/cases" --testTimeout=300000 --runTestsByPath "$M/cases/pascal-trigger-runner.test.ts"
//! Run:
//!   QT_ORACLE_PASCAL_TRIGGER_RUNNER=/tmp/oracle-pascal-trigger-runner.ndjson \
//!     cargo test -p quilltap-harness --test pascal_trigger_runner_equivalence

mod common;

use std::path::{Path, PathBuf};

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::CannedCompletionProvider;
use quilltap_core::pascal::llm_consult::ProviderConsultRunner;
use quilltap_core::pascal::trigger_runner::{
    run_triggered_custom_tools, RunTriggeredCustomToolsParams, TriggerSettings, TriggeredSpeaker,
};
use quilltap_core::test_support::global_capture;
use serde_json::{json, Value};

const CHAT: &str = "c1000000-0000-4000-8000-000000000007";
const GROUP: &str = "a2000000-0000-4000-8000-0000000000aa";
const CHAR_A: &str = "a1000000-0000-4000-8000-00000000000a";
const CHAR_B: &str = "a1000000-0000-4000-8000-00000000000b";
const CHAR_C: &str = "a1000000-0000-4000-8000-00000000000c";

/// The oracle's `PINNED_MESSAGES`: the runner's lines and the run service's.
const PINNED_MESSAGES: [&str; 10] = [
    "Trigger-phrase scan failed; no custom tools fired",
    "Trigger-phrase scan skipped — speaking character not found",
    "Trigger-phrase scan skipped — speaking character unreadable",
    "Trigger phrases over the per-message cap; later tools not fired",
    "Triggered custom tool refused",
    "Triggered custom tool ran but its announcement did not land",
    "Custom tool fired by trigger phrase",
    "Custom tool run: state cascade unavailable, defaulting to {}",
    "Custom tool run refused",
    "Custom tool dealt",
];

#[derive(serde::Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn spec() -> Spec {
    serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/pascal-run-custom.json"),
        )
        .unwrap(),
    )
    .unwrap()
}

/// A fresh copy of the committed pair, the oracle row's `plant` SQL run on
/// the main copy first, then opened.
fn open_copy(scratch: &Path, pepper: &str, plant: Option<&Vec<Value>>) -> Db {
    let _ = std::fs::remove_dir_all(scratch);
    std::fs::create_dir_all(scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    let llm_logs = scratch.join("llm-logs.db");
    std::fs::copy(fixtures_dir().join("pascal-run-custom-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("pascal-run-custom-mount.db"), &mount).unwrap();
    common::materialize_llm_logs(&llm_logs, pepper);
    if let Some(plant) = plant {
        let writer = quilltap_core::db::Writer::open_writable(&main, pepper)
            .expect("open main for the plant");
        for sql in plant {
            writer
                .connection()
                .execute_batch(sql.as_str().expect("plant SQL is text"))
                .expect("the plant applies");
        }
    }
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: Some(llm_logs),
        },
        pepper,
    )
    .expect("open db")
}

fn opt_str(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
}

fn params_of(row: &Value) -> RunTriggeredCustomToolsParams {
    let p = &row["params"];
    let settings = p["chatSettings"].as_object().map(|s| TriggerSettings {
        custom_tools: s.get("customTools").and_then(Value::as_bool),
        custom_tool_triggers: s.get("customToolTriggers").and_then(Value::as_bool),
    });
    RunTriggeredCustomToolsParams {
        user_id: p["userId"].as_str().unwrap().to_string(),
        chat_id: p["chatId"].as_str().unwrap().to_string(),
        content: p["content"].as_str().unwrap().to_string(),
        speaker: TriggeredSpeaker {
            participant_id: opt_str(&p["speaker"]["participantId"]),
            character_id: opt_str(&p["speaker"]["characterId"]),
        },
        project_id: opt_str(&p["projectId"]),
        character_ids: p["characterIds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect(),
        chat_settings: settings,
    }
}

/// The room's rows in insertion order (v5): the `rowid` order, joined to the
/// whole-row dump.
fn v5_rows(db: &Db) -> Vec<Value> {
    db.read_main(|conn| {
        let mut stmt =
            conn.prepare("SELECT id FROM chat_messages WHERE chatId = ?1 ORDER BY rowid")?;
        let ids: Vec<String> = stmt
            .query_map([CHAT], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        let dump = dump_table_json_conn(conn, "chat_messages", "id")?;
        let all = dump["rows"].as_array().cloned().unwrap_or_default();
        Ok(ids
            .iter()
            .filter_map(|id| all.iter().find(|r| r["id"] == json!(id)).cloned())
            .collect())
    })
    .unwrap()
}

/// The pinned projection of one row, its minted id + `createdAt` replaced by
/// placeholders. `pascalMeta` / `targetParticipantIds` stay the stored TEXT.
fn project(rows: &[Value]) -> Vec<Value> {
    rows.iter()
        .map(|r| {
            let get = |k: &str| r.get(k).cloned().unwrap_or(Value::Null);
            json!({
                "role": get("role"),
                "content": get("content"),
                "opaqueContent": get("opaqueContent"),
                "participantId": get("participantId"),
                "systemSender": get("systemSender"),
                "systemKind": get("systemKind"),
                "targetParticipantIds": get("targetParticipantIds"),
                "pascalMeta": get("pascalMeta"),
            })
        })
        .collect()
}

fn canon(v: &Value) -> Value {
    match v {
        Value::Number(n) => n
            .as_f64()
            .filter(|f| f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15)
            .map(|f| Value::from(f as i64))
            .unwrap_or_else(|| v.clone()),
        Value::Array(a) => Value::Array(a.iter().map(canon).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), canon(x))).collect()),
        _ => v.clone(),
    }
}

#[test]
fn trigger_runner_matches_oracle() {
    global_capture::install();
    let _canned_key = quilltap_core::test_support::CannedCheapLlmKey::install("test-key");
    let Ok(oracle_path) = std::env::var("QT_ORACLE_PASCAL_TRIGGER_RUNNER") else {
        eprintln!("SKIP: set QT_ORACLE_PASCAL_TRIGGER_RUNNER (see header).");
        return;
    };
    let spec = spec();
    let rows: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(!rows.is_empty(), "oracle file is empty");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut failures: Vec<String> = Vec::new();
    let (mut fired_rows, mut refused_rows, mut logged_rows, mut whisper_rows) = (0, 0, 0, 0);
    let mut unposted_rows = 0;
    for want in &rows {
        let name = want["name"].as_str().unwrap();
        let scratch =
            std::env::temp_dir().join(format!("qt-pascal-trig-{}-{name}", std::process::id()));
        let db = open_copy(&scratch, &spec.test_pepper_base64, want["plant"].as_array());
        let params = params_of(want);
        let now_ms = want["nowMs"].as_i64().expect("nowMs");
        let runner = ProviderConsultRunner {
            completion: CannedCompletionProvider::new(),
        };
        let mut on_posted: Vec<String> = Vec::new();
        let (posted, lines) = rt.block_on(global_capture::capture_async(async {
            let mut sink = |m: &Value| on_posted.push(m["id"].as_str().unwrap_or("").to_string());
            run_triggered_custom_tools(&db, params, Some(&runner), now_ms, &mut sink).await
        }));

        let got_rows = v5_rows(&db);
        let position = |id: &str| -> i64 {
            got_rows
                .iter()
                .position(|r| r["id"] == json!(id))
                .map_or(-1, |p| p as i64)
        };
        let got_posted: Vec<i64> = posted
            .iter()
            .map(|m| position(m["id"].as_str().unwrap_or("")))
            .collect();
        let got_on_posted: Vec<i64> = on_posted.iter().map(|id| position(id)).collect();
        let got_stores = common::dump_pascal_stores(&db, CHAT, GROUP, [CHAR_A, CHAR_B, CHAR_C]);
        drop(db);
        let _ = std::fs::remove_dir_all(&scratch);

        let mut check = |what: &str, got: Value, wanted: Value| {
            if got != wanted {
                eprintln!("MISMATCH case '{name}' {what}\n  left: {got}\n right: {wanted}");
                failures.push(format!("{name}: {what}"));
            }
        };
        check(
            "rows",
            Value::Array(project(&got_rows)),
            Value::Array(project(want["rows"].as_array().unwrap())),
        );
        check("posted", json!(got_posted), want["posted"].clone());
        check("onPosted", json!(got_on_posted), want["onPosted"].clone());
        check("stores", canon(&got_stores), canon(&want["stores"]));
        let want_lines: Vec<String> = want["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| common::v4_log_as_capture_line("quilltap::pascal", l))
            .collect();
        let got_lines = common::pinned_capture_lines(&lines, "quilltap::pascal", &PINNED_MESSAGES);
        check("log lines", json!(got_lines), json!(want_lines));

        let sys: Vec<&Value> = want["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| !r["systemSender"].is_null())
            .collect();
        if sys.iter().any(|r| {
            r["pascalMeta"]
                .as_str()
                .is_some_and(|m| m.contains("\"triggerPhrase\""))
        }) {
            fired_rows += 1;
        }
        if sys.iter().any(|r| r["systemSender"] == json!("prospero")) {
            refused_rows += 1;
        }
        if sys.iter().any(|r| !r["targetParticipantIds"].is_null()) {
            whisper_rows += 1;
        }
        if !want_lines.is_empty() {
            logged_rows += 1;
        }
        if want_lines
            .iter()
            .any(|l| l.contains("Triggered custom tool ran but its announcement did not land"))
        {
            unposted_rows += 1;
        }
    }

    assert!(
        failures.is_empty(),
        "{} comparand(s) differ from the oracle: {failures:#?}",
        failures.len()
    );
    // Coverage floors: a green run over a corpus that never fired, refused,
    // whispered or logged would prove nothing.
    assert!(fired_rows > 0, "no case fired a triggered run");
    assert!(refused_rows > 0, "no case posted Prospero's refusal");
    assert!(whisper_rows > 0, "no case whispered a triggered run");
    assert!(logged_rows > 0, "no case carried a pinned log line");
    assert!(unposted_rows > 0, "no case reached the unposted arm");
    eprintln!("OK: trigger runner matched oracle ({} cases).", rows.len());
}

/// v5-only (no v4 twin — v4 always builds an invoker): with NO consult runner in
/// hand, a consult-bearing trigger tool takes `execute_custom_tool`'s
/// no-invoker FAIL-SOFT arm — the run is dealt, `pascalMeta.llm` records the
/// reason, and the table branches on `ok: false`. Pinned here so the arm a
/// runner-less assembly reaches is measured, not assumed (R-L2).
#[test]
fn consult_none_takes_the_no_invoker_arm() {
    global_capture::install();
    let spec = spec();
    let scratch = std::env::temp_dir().join(format!("qt-pascal-trig-none-{}", std::process::id()));
    let db = open_copy(&scratch, &spec.test_pepper_base64, None);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let params = RunTriggeredCustomToolsParams {
        user_id: "e18e05bc-63e8-4539-8a85-719b7a508850".into(),
        chat_id: CHAT.into(),
        content: "read the omens".into(),
        speaker: TriggeredSpeaker {
            participant_id: Some("e7000000-0000-4000-8000-00000000000a".into()),
            character_id: Some(CHAR_A.into()),
        },
        project_id: Some("a3000000-0000-4000-8000-0000000000bb".into()),
        character_ids: vec![CHAR_A.into(), CHAR_B.into()],
        chat_settings: Some(TriggerSettings {
            custom_tools: Some(true),
            custom_tool_triggers: Some(true),
        }),
    };
    let mut sink = |_: &Value| {};
    let posted = rt.block_on(run_triggered_custom_tools(
        &db,
        params,
        None,
        1_788_004_800_000,
        &mut sink,
    ));
    drop(db);
    let _ = std::fs::remove_dir_all(&scratch);
    assert_eq!(posted.len(), 1, "the consult-bearing tool is still dealt");
    let meta = &posted[0]["pascalMeta"];
    assert_eq!(meta["tool"], json!("augur"));
    assert_eq!(
        meta["state"],
        json!("failure"),
        "the table branched on ok: false"
    );
    let llm = serde_json::to_string(&meta["llm"]).unwrap();
    assert!(
        llm.contains("no LLM invoker was available in this context"),
        "the no-invoker reason is on the record: {llm}"
    );
}
