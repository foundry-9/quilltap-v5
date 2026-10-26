//! Tier-2 differential (P4.D331, v4 `e4a60ad9e` + `a495faa26`, at the
//! `322a5488e` pin): the three log tools — `log_create_entry`, `log_grep`,
//! `log_read` — through v5's REAL `BuiltInToolRunner` against v4's REAL
//! `executeToolCallWithContext` (`harness/oracle/cases/log-tools.test.ts`).
//!
//! Each oracle row carries its own call (tool, arguments, chat, calling
//! character, and whether the llm-logs file is the seeded fixture or a
//! garbage file — v4's degraded mode). Both sides run it over fresh copies of
//! the committed chat-dialogs pair (the characters `toViews` names and the
//! resolver reads) plus that llm-logs file, and compare the executor's
//! envelope (`success`, `result`, `error`), every log line the call emitted
//! (`LEVEL message key=value …`, the handler logger's `service` / `module`
//! bindings first), and after a create the whole `log_entries` table.
//!
//! The handlers run INSIDE `Db::write` — on the writer thread — so this is a
//! DEDICATED binary with ONE test and a process-wide subscriber (the
//! thread-scoped rig cannot see the writer thread; memory note
//! `capture-rig-writer-thread`).
//!
//! Recorded divergences:
//!   - [`CHILD_BUFFER_VISIBILITY`] (R-11) — v4's autonomous turn runs in the
//!     job child, whose `logEntries.create` is buffered until the turn's
//!     writes apply, so a later read in the SAME turn misses the line; v5's
//!     in-process runner writes at once. The oracle cannot drive v4's child
//!     path, so the row is pinned from v5's side with v4's answer quoted from
//!     `log-handler.ts:10-11` and `help/the-log.md` (oracle-unreachable).
//!   - v5's ABSENT llm-logs file answers v4's `not initialized` sentence (a
//!     v4 uninitialized client, which a v4 boot never leaves) — a unit test
//!     here, no oracle row.
//!
//! Generate (Node 24, from the pinned v4 checkout; jest ignores `.claude/`):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-log-tools-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp "$V5W/harness/oracle/cases/log-tools.test.ts" "$TMPO/cases/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_SEED=tools QT_FIXTURE_OUT=/tmp/qt-log-tools-llmlogs.db PATH=$N:$PATH \
//!     npx tsx $V5W/harness/oracle/fixtures/build-log-entries-fixture.ts
//!   QT_FIXTURE_CD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/chat-dialogs-main.db \
//!   QT_FIXTURE_CD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/chat-dialogs-mount.db \
//!   QT_FIXTURE_LOG_ENTRIES=/tmp/qt-log-tools-llmlogs.db \
//!   QT_ORACLE_OUT=/tmp/oracle-log-tools.ndjson TZ=UTC PATH=$N:$PATH \
//!     npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- log-tools
//! Run:
//!   QT_ORACLE_LOG_TOOLS=/tmp/oracle-log-tools.ndjson \
//!   QT_FIXTURE_CD_MAIN=crates/quilltap-web/tests/fixtures/chat-dialogs-main.db \
//!   QT_FIXTURE_CD_MOUNT=crates/quilltap-web/tests/fixtures/chat-dialogs-mount.db \
//!   QT_FIXTURE_LOG_ENTRIES=/tmp/qt-log-tools-llmlogs.db \
//!     cargo test -p quilltap-harness --test log_tools_equivalence -- --nocapture
//!
//! Skips (does not fail) when an env var is unset — the standing gated
//! discipline.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::tool_execution::{ToolCall, ToolExecutionContext, ToolRunner};
use quilltap_core::test_support::CaptureLayer;
use quilltap_core::tools::executor::BuiltInToolRunner;
use quilltap_core::tools::self_inventory::{ClientShell, SelfInventoryEnv};
use serde_json::{json, Value};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const USER: &str = "f0000000-0000-4000-8000-00000000000f";
const NORA: &str = "a2000000-0000-4000-8000-000000000001";

/// `(row, v4's answer, v5's answer)` — R-11's ruled divergence, pinned from
/// v5's side (oracle-unreachable: the oracle runs v4 in process, never in the
/// job child).
const CHILD_BUFFER_VISIBILITY: &[(&str, &str, &str)] = &[(
    "autonomous-turn-read-after-write",
    "the line is not seen (buffered in the job child until the turn commits)",
    "the line is seen (written through the writer task at once)",
)];

fn lines() -> &'static Arc<Mutex<Vec<String>>> {
    static LINES: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();
    LINES.get_or_init(|| {
        use tracing_subscriber::layer::SubscriberExt;
        let sink = Arc::new(Mutex::new(Vec::new()));
        tracing::subscriber::set_global_default(
            tracing_subscriber::registry().with(CaptureLayer(sink.clone())),
        )
        .expect("this binary owns its global subscriber");
        sink
    })
}

fn env() -> SelfInventoryEnv {
    SelfInventoryEnv {
        version: String::new(),
        runtime_mode: "local-dev".to_string(),
        client_shell: ClientShell::Browser,
        mount_index_degraded: false,
        release_notes: None,
        changelog: None,
        model_info: Vec::new(),
        fallback_pricing: Vec::new(),
        registry_default_context: 8192,
    }
}

/// v5's capture line (`LEVEL target message k=v …`) as the oracle renders it.
fn render(captured: &str, minted: &[String]) -> String {
    let mut parts = captured.splitn(3, ' ');
    let level = parts.next().unwrap();
    let _target = parts.next().unwrap();
    let mut out = format!("{level} {}", parts.next().unwrap_or(""));
    for id in minted {
        out = out.replace(id.as_str(), "<minted>");
    }
    out
}

/// The lines a call is compared on — the ones its own code emits. v5's
/// partition plumbing (the writer channel, the pool) logs nothing of v4's.
fn relevant(line: &str) -> bool {
    !line.starts_with("TRACE ")
}

fn open(work: &Path, main: &str, mount: &str, logs: Option<&str>) -> Db {
    std::fs::copy(main, work.join("main.db")).unwrap();
    std::fs::copy(mount, work.join("mount.db")).unwrap();
    let llm = work.join("llm-logs.db");
    match logs {
        Some(path) => {
            std::fs::copy(path, &llm).unwrap();
        }
        None => std::fs::write(&llm, vec![0x5a_u8; 4096]).unwrap(),
    }
    Db::open(
        DbPaths {
            main: work.join("main.db"),
            mount_index: Some(work.join("mount.db")),
            llm_logs: Some(llm),
        },
        PEPPER,
    )
    .expect("open the case's copies")
}

fn dump(db: &Db, minted: &[String]) -> Value {
    let minted = minted.to_vec();
    let rows = db
        .read_llm_logs(move |c| {
            let mut stmt = c.prepare(
                r#"SELECT "id", "chatId", "characterId", "createdAt", "updatedAt", hex("message") FROM log_entries"#,
            )?;
            let rows = stmt
                .query_map([], |r| {
                    let id: String = r.get(0)?;
                    let is_minted = minted.contains(&id);
                    Ok(json!({
                        "id": if is_minted { "<minted>".to_string() } else { id },
                        "chatId": r.get::<_, String>(1)?,
                        "characterId": r.get::<_, Option<String>>(2)?,
                        "createdAt": if is_minted { "<now>".to_string() } else { r.get::<_, String>(3)? },
                        "updatedAt": if is_minted { "<now>".to_string() } else { r.get::<_, String>(4)? },
                        "messageHex": r.get::<_, String>(5)?,
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .unwrap();
    let mut rows = rows;
    rows.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Value::Array(rows)
}

#[test]
fn every_log_tool_call_answers_logs_and_writes_as_v4() {
    let sink = lines();
    let (Ok(oracle), Ok(main), Ok(mount), Ok(logs)) = (
        std::env::var("QT_ORACLE_LOG_TOOLS"),
        std::env::var("QT_FIXTURE_CD_MAIN"),
        std::env::var("QT_FIXTURE_CD_MOUNT"),
        std::env::var("QT_FIXTURE_LOG_ENTRIES"),
    ) else {
        eprintln!("SKIP: QT_ORACLE_LOG_TOOLS / QT_FIXTURE_CD_MAIN / _MOUNT / QT_FIXTURE_LOG_ENTRIES unset");
        return;
    };
    let rows: Vec<Value> = std::fs::read_to_string(&oracle)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(rows.len() > 40, "{} rows", rows.len());
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    let mut failures = Vec::new();
    for row in &rows {
        let case = row["case"].as_str().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let fixture = row["llm"] == "fixture";
        let db = open(dir.path(), &main, &mount, fixture.then_some(logs.as_str()));
        let runner = BuiltInToolRunner::new(db.clone(), env());
        let ctx = ToolExecutionContext {
            chat_id: row["chatId"].as_str().unwrap().to_string(),
            user_id: USER.to_string(),
            character_id: row
                .get("characterId")
                .and_then(Value::as_str)
                .map(str::to_string),
            ..Default::default()
        };
        let call = ToolCall {
            name: row["tool"].as_str().unwrap().to_string(),
            arguments: row["args"].clone(),
            call_id: None,
        };
        sink.lock().unwrap().clear();
        let result = rt.block_on(runner.run(&call, &ctx));
        let captured: Vec<String> = std::mem::take(&mut *sink.lock().unwrap());

        let mut got_result = result.result.clone();
        let mut minted = Vec::new();
        if let Some(entry) = got_result.get_mut("entry").and_then(Value::as_object_mut) {
            minted.push(entry["id"].as_str().unwrap().to_string());
            entry.insert("id".into(), json!("<minted>"));
            entry.insert("createdAt".into(), json!("<now>"));
        }
        let got = json!({
            "success": result.success,
            "result": if result.success { got_result } else { Value::Null },
            "error": result.error,
        });
        if got != row["result"] {
            failures.push(format!(
                "{case} — result:\n  v4 {}\n  v5 {}",
                row["result"], got
            ));
        }
        let got_lines: Vec<String> = captured
            .iter()
            .filter(|l| relevant(l))
            .map(|l| render(l, &minted))
            .collect();
        let want_lines: Vec<String> = row["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l.as_str().unwrap().to_string())
            .collect();
        if got_lines != want_lines {
            failures.push(format!(
                "{case} — lines:\n  v4 {want_lines:#?}\n  v5 {got_lines:#?}"
            ));
        }
        if let Some(want_dump) = row.get("dump") {
            let got_dump = dump(&db, &minted);
            if &got_dump != want_dump {
                failures.push(format!("{case} — dump:\n  v4 {want_dump}\n  v5 {got_dump}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} disagreements:\n{}",
        failures.len(),
        failures.join("\n")
    );

    // R-11, pinned from v5's side: a read later in the same in-process turn
    // DOES see the line just written.
    let dir = tempfile::tempdir().unwrap();
    let db = open(dir.path(), &main, &mount, Some(&logs));
    let runner = BuiltInToolRunner::new(db, env());
    let ctx = ToolExecutionContext {
        chat_id: "c2000000-0000-4000-8000-000000000004".to_string(),
        user_id: USER.to_string(),
        character_id: Some(NORA.to_string()),
        ..Default::default()
    };
    let write = ToolCall {
        name: "log_create_entry".into(),
        arguments: json!({"message": "Seen at once"}),
        call_id: None,
    };
    let read = ToolCall {
        name: "log_grep".into(),
        arguments: json!({"pattern": "^Seen at once$"}),
        call_id: None,
    };
    assert!(rt.block_on(runner.run(&write, &ctx)).success);
    let found = rt.block_on(runner.run(&read, &ctx));
    assert_eq!(found.result["count"], 1, "{:?}", found);
    assert_eq!(
        CHILD_BUFFER_VISIBILITY[0].2,
        "the line is seen (written through the writer task at once)"
    );
    eprintln!("log_tools: {} rows agree", rows.len());
}
