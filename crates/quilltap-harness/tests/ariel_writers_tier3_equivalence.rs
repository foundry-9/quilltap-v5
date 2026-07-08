//! Tier-3 differential: the Ariel terminal-announcement writers + the
//! terminal-session reconcile pass (P4.1c).
//!
//! Both sides run the SAME case sequence — session-opened (label present /
//! absent / empty-string / missing chat), terminal-output (all three reasons,
//! the fence-length growth, the 16 K elide with UTF-16 unit math + the en-US
//! grouped elided count, the exact-cap boundary, the empty / whitespace-only
//! skips), session-closed (exit 0 / 137 / null / missing chat), and two
//! reconcile passes (one orphan reconciled + explicit-NULL exitCode; the
//! second a no-op) — against a COPY of the same v4-baked fixture, then diff:
//!   1. the per-case posted booleans / reconcile counts,
//!   2. `chat_messages` — every Ariel row byte-for-byte (content incl. the
//!      `<!-- terminalSessionId:… -->` embed, opaqueContent, systemSender /
//!      systemKind, role, participantId),
//!   3. `chats` — the `add_message` side-effect (messageCount etc.),
//!   4. `terminal_sessions` — the reconcile exit stamps (sentinel-aware).
//!
//! v4's REAL `postAriel*` + `reconcileTerminalSessionsForChat` are driven by
//! the oracle (`harness/oracle/cases/ariel-writers-tier3.ts` — the PTY manager
//! map is empty there, so the Rust side passes the matching `is_live = false`
//! probe); the ported `services::ariel_notifications` functions are driven
//! here.
//!
//! Normalization: `chat_messages` — the minted `id` tokenized to `<id-N>` in
//! row order (dump ordered by `content`; every Ariel body embeds a distinct
//! session id), `createdAt` → `<ts>`. `chats` — the minted `updatedAt` /
//! `lastMessageAt` → `<ts>`. `terminal_sessions` — `exitedAt` / `updatedAt`
//! values that differ from the seed sentinel → `<ts>` (a stray mint on an
//! untouched row would surface as an unexpected placeholder).
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<this checkout>
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_ARIEL_MAIN=/tmp/qt-ariel-main.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-ariel-writers-fixture.ts
//!   QT_FIXTURE_ARIEL_MAIN=/tmp/qt-ariel-main.db \
//!     $N/npx tsx $V5/harness/oracle/cases/ariel-writers-tier3.ts > /tmp/oracle-ariel.ndjson
//! Run:
//!   QT_ORACLE_ARIEL=/tmp/oracle-ariel.ndjson \
//!   QT_FIXTURE_ARIEL_MAIN=/tmp/qt-ariel-main.db \
//!     cargo test -p quilltap-harness --test ariel_writers_tier3_equivalence

use std::collections::HashMap;

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::ariel_notifications::{
    post_ariel_session_closed_announcement, post_ariel_session_opened_announcement,
    post_ariel_terminal_output_announcement, reconcile_terminal_sessions_for_chat,
    ArielFlushReason, ArielSessionClosedAnnouncement, ArielSessionOpenedAnnouncement,
    ArielTerminalOutputAnnouncement,
};
use serde::Deserialize;
use serde_json::Value;

// ---------------------------------------------------------------------------
// Spec structures (harness/oracle/fixtures/ariel-writers-tier3.json).
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    kind: String,
    #[serde(default)]
    chat_ref: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    // Distinguish "label": null (present-null) from an absent key — the writer
    // treats both as no-label, so a plain Option is fine here.
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    shell: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    cleaned_output: Option<String>,
    #[serde(default)]
    repeat_segment: Option<String>,
    #[serde(default)]
    repeat_times: Option<usize>,
    #[serde(default)]
    exit_code: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatSpec {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    seed_timestamp: String,
    chat: ChatSpec,
    missing_chat_id: String,
    cases: Vec<Case>,
}

impl Case {
    fn output(&self) -> String {
        if let Some(o) = &self.cleaned_output {
            return o.clone();
        }
        match (&self.repeat_segment, self.repeat_times) {
            (Some(seg), Some(times)) => seg.repeat(times),
            _ => panic!("case {}: no cleanedOutput", self.name),
        }
    }
}

fn flush_reason(s: &str) -> ArielFlushReason {
    match s {
        "idle" => ArielFlushReason::Idle,
        "max-age" => ArielFlushReason::MaxAge,
        "session-closed" => ArielFlushReason::SessionClosed,
        other => panic!("unknown reason {other}"),
    }
}

// ---------------------------------------------------------------------------
// Normalization.
// ---------------------------------------------------------------------------

/// Tokenize each row's `id` to `<id-N>` in row order (dump already sorted by
/// `content`), placeholder `createdAt` → `<ts>`.
fn normalize_messages(dump: &mut Value) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .expect("rows array");
    let mut id_map: HashMap<String, String> = HashMap::new();
    for row in rows.iter_mut() {
        let obj = row.as_object_mut().expect("row object");
        if let Some(Value::String(id)) = obj.get("id") {
            let next = format!("<id-{}>", id_map.len());
            let token = id_map.entry(id.clone()).or_insert(next).clone();
            obj.insert("id".to_string(), Value::String(token));
        }
        if obj.get("createdAt").map(|v| !v.is_null()).unwrap_or(false) {
            obj.insert("createdAt".to_string(), Value::String("<ts>".to_string()));
        }
    }
}

/// Placeholder the minted `updatedAt` / `lastMessageAt` on the chats rows.
fn normalize_chats(dump: &mut Value) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .expect("chats rows array");
    for row in rows.iter_mut() {
        let obj = row.as_object_mut().expect("chats row object");
        for col in ["updatedAt", "lastMessageAt"] {
            if obj.get(col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert(col.to_string(), Value::String("<ts>".to_string()));
            }
        }
    }
}

/// Sentinel-aware placeholder for the reconcile-minted `exitedAt`/`updatedAt`
/// on terminal_sessions rows: a value equal to the seed sentinel stays pinned
/// (proving untouched rows were not bumped); anything else → `<ts>`.
fn normalize_terminal_sessions(dump: &mut Value, sentinel: &str) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .expect("terminal_sessions rows array");
    for row in rows.iter_mut() {
        let obj = row.as_object_mut().expect("ts row object");
        for col in ["exitedAt", "updatedAt"] {
            let minted = matches!(obj.get(col), Some(Value::String(s)) if s != sentinel);
            if minted {
                obj.insert(col.to_string(), Value::String("<ts>".to_string()));
            }
        }
    }
}

fn assert_dump_eq(got: &Value, oracle: &Value, label: &str) {
    assert_eq!(
        got.get("columns"),
        oracle.get("columns"),
        "{label}: column set differs"
    );
    let g = got.get("rows").and_then(Value::as_array).expect("got rows");
    let o = oracle
        .get("rows")
        .and_then(Value::as_array)
        .expect("oracle rows");
    assert_eq!(g.len(), o.len(), "{label}: row count differs");
    for (i, (gr, or)) in g.iter().zip(o.iter()).enumerate() {
        assert_eq!(
            gr, or,
            "{label}: row {i} differs\n  rust:   {gr}\n  oracle: {or}"
        );
    }
}

// ---------------------------------------------------------------------------
// Test.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ariel_writers_tier3_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_ARIEL") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_ARIEL to the oracle NDJSON (see header).");
            return;
        }
    };
    let main_fixture = match std::env::var("QT_FIXTURE_ARIEL_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_ARIEL_MAIN to the main fixture .db (header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/ariel-writers-tier3.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle dump");

    // A fresh copy so the shared seed fixture stays pristine.
    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-ariel-main-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));

    let db = Db::open(
        DbPaths {
            main: main_work.clone(),
            mount_index: None,
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    // Run the case sequence; collect the per-case results exactly as the
    // oracle records them.
    let mut results: Vec<Value> = Vec::new();
    for c in &spec.cases {
        let chat_id = if c.chat_ref.as_deref() == Some("missing") {
            spec.missing_chat_id.clone()
        } else {
            spec.chat.id.clone()
        };
        match c.kind.as_str() {
            "opened" => {
                let posted = post_ariel_session_opened_announcement(
                    &db,
                    &ArielSessionOpenedAnnouncement {
                        chat_id,
                        session_id: c.session_id.clone().expect("sessionId"),
                        label: c.label.clone(),
                        shell: c.shell.clone().expect("shell"),
                        cwd: c.cwd.clone().expect("cwd"),
                    },
                )
                .await;
                results.push(serde_json::json!({ "name": c.name, "posted": posted.is_some() }));
            }
            "output" => {
                let posted = post_ariel_terminal_output_announcement(
                    &db,
                    &ArielTerminalOutputAnnouncement {
                        chat_id,
                        session_id: c.session_id.clone().expect("sessionId"),
                        cleaned_output: c.output(),
                        reason: flush_reason(c.reason.as_deref().expect("reason")),
                    },
                )
                .await;
                results.push(serde_json::json!({ "name": c.name, "posted": posted.is_some() }));
            }
            "closed" => {
                let posted = post_ariel_session_closed_announcement(
                    &db,
                    &ArielSessionClosedAnnouncement {
                        chat_id,
                        session_id: c.session_id.clone().expect("sessionId"),
                        exit_code: c.exit_code,
                    },
                )
                .await;
                results.push(serde_json::json!({ "name": c.name, "posted": posted.is_some() }));
            }
            "reconcile" => {
                // The oracle's PTY manager map is empty — every live orphan
                // reconciles; the matching probe here is `false` for all.
                let n = reconcile_terminal_sessions_for_chat(&db, &spec.chat.id, &|_| false).await;
                results.push(serde_json::json!({ "name": c.name, "reconciled": n }));
            }
            other => panic!("unknown case kind {other}"),
        }
    }

    let mut got_messages = db
        .read_main(|conn| dump_table_json_conn(conn, "chat_messages", "content"))
        .expect("dump chat_messages");
    let mut got_chats = db
        .read_main(|conn| dump_table_json_conn(conn, "chats", "id"))
        .expect("dump chats");
    let mut got_terminal = db
        .read_main(|conn| dump_table_json_conn(conn, "terminal_sessions", "id"))
        .expect("dump terminal_sessions");
    drop(db);
    let _ = std::fs::remove_file(&main_work);

    // Per-case results (posted booleans / reconcile counts) — exact.
    let oracle_results = oracle
        .get("results")
        .and_then(Value::as_array)
        .expect("oracle results");
    assert_eq!(
        &Value::Array(results.clone()),
        &Value::Array(oracle_results.clone()),
        "per-case results differ"
    );

    let mut oracle_messages = oracle
        .get("chatMessages")
        .cloned()
        .expect("oracle chatMessages");
    let mut oracle_chats = oracle.get("chats").cloned().expect("oracle chats");
    let mut oracle_terminal = oracle
        .get("terminalSessions")
        .cloned()
        .expect("oracle terminalSessions");

    normalize_messages(&mut got_messages);
    normalize_messages(&mut oracle_messages);
    normalize_chats(&mut got_chats);
    normalize_chats(&mut oracle_chats);
    normalize_terminal_sessions(&mut got_terminal, &spec.seed_timestamp);
    normalize_terminal_sessions(&mut oracle_terminal, &spec.seed_timestamp);

    assert_dump_eq(&got_messages, &oracle_messages, "chat_messages");
    assert_dump_eq(&got_chats, &oracle_chats, "chats");
    assert_dump_eq(&got_terminal, &oracle_terminal, "terminal_sessions");

    let nm = got_messages["rows"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    assert_eq!(
        nm, 13,
        "expected 12 posted announcement rows + 1 reconcile close row"
    );
    eprintln!(
        "OK: ariel-writers tier-3 matched oracle ({} cases, {nm} rows, reconcile stamps).",
        spec.cases.len()
    );
}
