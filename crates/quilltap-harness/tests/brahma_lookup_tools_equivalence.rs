//! Tier-2 differential (P4.D374): the four Brahma lookup tools —
//! `quilltap_core::tools::brahma_lookup` vs v4's REAL handlers
//! (`lib/tools/handlers/brahma-lookup-handler.ts`, `c539c82e0`), plus the
//! dispatcher's operator gate and envelope (`BuiltInToolRunner` vs v4's
//! `executeToolCallWithContext`).
//!
//! Every oracle row carries its tool, args, `executor` flag and the plant SQL
//! v4 ran on its copy; this test repeats the row on a FRESH copy of the same
//! three fixture files (no id remap — both sides read copies of one build) and
//! compares:
//!   - `resultJson` BYTE-for-byte (`serde_json::to_string` on both sides;
//!     `preserve_order` + JS-number rendering in the handler);
//!   - the `BrahmaLookup` lines, line for line in order (the child bindings
//!     `service` / `module` first, as v4's `createServiceLogger` emits them),
//!     the silence leg included;
//!   - `unchanged`: `memories`, `chats` and `chat_messages` byte-identical
//!     after the call on BOTH sides (the read-only proof — `lookup_memories`
//!     writes no access stamp).
//!
//! `BRAHMA_LOOKUP_LONE_SURROGATE_CLIP` (ruling L-3, a NAMED both-ways table):
//! v4's `clip` can split a surrogate pair and keep the lone half (JSON
//! `\ud83d`); a Rust `String` cannot, so v5 decodes U+FFFD. Those rows compare
//! with v4's lone escape mapped to U+FFFD AND assert the raw bytes differ.
//!
//! `UNION_RED_*` (round §S.5, HANDOFFs H1 / H2): rows that need a sibling
//! lane's half — P4.D363's `participantId` on the global search hit (H1), and
//! P4.D375's window pass for a windowed `search_memories` (H2). They are
//! compared and REPORTED, and must be red ON THIS LANE (a row that turns green
//! here belongs in the green set — move it). The unifier deletes both tables.
//!
//! Generate the fixture + oracle (Node 24, from the pinned v4 checkout; stage
//! the case outside `.claude/`):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   STAGE=/tmp/qt-bl-stage
//!   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
//!   cp $V5W/harness/oracle/cases/brahma-lookup-tools.test.ts $STAGE/harness/oracle/cases/
//!   cp $V5W/harness/oracle/fixtures/brahma-lookup-tools.json $STAGE/harness/oracle/fixtures/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_BL_MAIN=/tmp/qt-bl-main.db QT_FIXTURE_BL_MOUNT=/tmp/qt-bl-mount.db \
//!   QT_FIXTURE_BL_LLM=/tmp/qt-bl-llm.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-brahma-lookup-fixture.ts
//!   QT_FIXTURE_BL_MAIN=/tmp/qt-bl-main.db QT_FIXTURE_BL_MOUNT=/tmp/qt-bl-mount.db \
//!   QT_FIXTURE_BL_LLM=/tmp/qt-bl-llm.db QT_ORACLE_OUT=/tmp/oracle-brahma-lookup.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 --roots "$PWD" \
//!       --roots "$STAGE/harness/oracle/cases" --runTestsByPath "$STAGE/harness/oracle/cases/brahma-lookup-tools.test.ts"
//! Run:
//!   QT_ORACLE_BRAHMA_LOOKUP=/tmp/oracle-brahma-lookup.ndjson \
//!   QT_FIXTURE_BL_MAIN=/tmp/qt-bl-main.db QT_FIXTURE_BL_MOUNT=/tmp/qt-bl-mount.db \
//!   QT_FIXTURE_BL_LLM=/tmp/qt-bl-llm.db \
//!     cargo test -p quilltap-harness --test brahma_lookup_tools_equivalence -- --nocapture

mod common;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::embedding::{CannedEmbeddingProvider, ErasedEmbeddingProvider};
use quilltap_core::services::tool_execution::{ToolCall, ToolExecutionContext, ToolRunner};
use quilltap_core::tools::brahma_lookup::{
    execute_lookup_character_tool, execute_lookup_memories_tool, execute_lookup_messages_tool,
    execute_read_messages_tool, BrahmaLookupContext,
};
use serde::Deserialize;
use serde_json::Value;

/// L-3: the rows whose clip splits an astral pair.
const BRAHMA_LOOKUP_LONE_SURROGATE_CLIP: &[&str] = &["lm_clip_astral", "rm_clip_astral"];

/// H1 (P4.D363): rows whose answer reads a hit's `participantId` (the seat
/// filter or the speaker label) — red on the lane until P4.D363's hit carried
/// the seat. EMPTIED at the union (`624d0d25c` round): `db/chats_search.rs`
/// carries `participantId`, so every `lmsg_*` row compares whole.
const UNION_RED_H1_PARTICIPANT_ID: &[&str] = &[];

/// H2 (P4.D375): windowed `lookup_memories` rows (the window pass + the
/// ≤14-day floor relaxation on a plain windowed search). MEASURED EMPTY on this
/// corpus: `lm_window_dateonly` / `lm_window_iso` / `lm_window_open_since` are
/// GREEN on this lane alone (no candidate here sits between the relaxed and the
/// default floor), so the union has nothing of this family to flip.
const UNION_RED_H2_WINDOW: &[&str] = &[];

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "cannedEmbeddings")]
    canned_embeddings: HashMap<String, Vec<f32>>,
}

#[derive(Deserialize)]
struct Row {
    label: String,
    tool: String,
    args: Value,
    executor: Option<bool>,
    plants: Vec<String>,
    #[serde(rename = "resultJson")]
    result_json: String,
    logs: Vec<Value>,
    unchanged: bool,
}

const CH1: &str = "d7400000-0000-4000-8000-000000000001";

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see the header).");
            None
        }
    }
}

fn clear(p: &Path) {
    for suffix in ["", "-journal", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", p.display()));
    }
}

fn fresh_copy(fx: [&str; 3], tag: &str) -> [PathBuf; 3] {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let out = ["main", "mount", "llm"].map(|k| dir.join(format!("qt-bl-{k}-rust-{pid}-{tag}.db")));
    for (src, dst) in fx.iter().zip(out.iter()) {
        clear(dst);
        std::fs::copy(src, dst).unwrap_or_else(|e| panic!("copy {src}: {e}"));
    }
    out
}

fn build_provider(spec: &Spec) -> CannedEmbeddingProvider {
    let mut p = CannedEmbeddingProvider::new();
    for (text, vec) in &spec.canned_embeddings {
        p = p.with_vector(text.clone(), vec.clone());
    }
    p
}

/// Map each `\udXXX` escape of a lone surrogate half to U+FFFD.
fn map_lone_surrogates(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && i + 5 < b.len()
            && b[i + 1] == b'u'
            && (b[i + 2] == b'd' || b[i + 2] == b'D')
            && matches!(b[i + 3], b'8'..=b'9' | b'a'..=b'f' | b'A'..=b'F')
        {
            out.push('\u{FFFD}');
            i += 6;
            continue;
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn dumps(db: &Db) -> Vec<String> {
    ["memories", "chats", "chat_messages"]
        .iter()
        .map(
            |t| match db.read_main(|c| dump_table_json_conn(c, t, "id")) {
                Ok(v) => v.to_string(),
                Err(e) => format!("throws: {e}"),
            },
        )
        .collect()
}

/// Capture every event a future emits, the guard held across the await (the
/// test's current-thread runtime keeps the whole call on this thread).
async fn captured_async<T, F: std::future::Future<Output = T>>(f: F) -> (T, Vec<String>) {
    use tracing_subscriber::layer::SubscriberExt;
    let logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let subscriber = tracing_subscriber::registry()
        .with(quilltap_core::test_support::CaptureLayer(logs.clone()));
    let out = {
        let _guard = tracing::subscriber::set_default(subscriber);
        f.await
    };
    let lines = logs.lock().unwrap().clone();
    (out, lines)
}

/// The captured `BrahmaLookup` lines (target `quilltap::tools`, the child's
/// `service=BrahmaLookup` binding), in capture order.
fn brahma_lookup_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|l| {
            l.split_once(' ')
                .is_some_and(|(_, rest)| rest.starts_with("quilltap::tools "))
                && l.contains(" service=BrahmaLookup")
        })
        .cloned()
        .collect()
}

#[tokio::test]
async fn brahma_lookup_tools_match_oracle() {
    let (Some(oracle_path), Some(main_fx), Some(mount_fx), Some(llm_fx)) = (
        env_or_skip("QT_ORACLE_BRAHMA_LOOKUP"),
        env_or_skip("QT_FIXTURE_BL_MAIN"),
        env_or_skip("QT_FIXTURE_BL_MOUNT"),
        env_or_skip("QT_FIXTURE_BL_LLM"),
    ) else {
        return;
    };
    let spec_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/brahma-lookup-tools.json");
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(&spec_path).expect("spec")).expect("spec");
    let rows: Vec<Row> = std::fs::read_to_string(&oracle_path)
        .expect("oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle row"))
        .collect();
    assert!(rows.len() >= 90, "only {} oracle rows", rows.len());

    let mut red: Vec<String> = Vec::new();
    let mut union_red: Vec<String> = Vec::new();
    let mut union_green: Vec<String> = Vec::new();
    let mut logged = 0usize;
    for row in &rows {
        let paths = fresh_copy([&main_fx, &mount_fx, &llm_fx], &row.label);
        if !row.plants.is_empty() {
            let w = quilltap_core::db::Writer::open_writable(&paths[0], &spec.test_pepper_base64)
                .expect("open copy");
            for sql in &row.plants {
                w.connection()
                    .execute_batch(sql)
                    .unwrap_or_else(|e| panic!("{}: plant `{sql}`: {e}", row.label));
            }
        }
        let db = Db::open(
            DbPaths {
                main: paths[0].clone(),
                mount_index: Some(paths[1].clone()),
                llm_logs: Some(paths[2].clone()),
            },
            &spec.test_pepper_base64,
        )
        .unwrap_or_else(|e| panic!("open: {e}"));
        let provider = build_provider(&spec);
        let before = dumps(&db);

        let ctx = BrahmaLookupContext {
            user_id: &spec.user_id,
            memory_zone_fallback: "UTC",
            now_ms: quilltap_core::clock::now_unix_ms() as f64,
        };
        let (got, lines) = match row.executor {
            Some(operator) => {
                let runner = quilltap_core::tools::executor::BuiltInToolRunner::new(
                    db.clone(),
                    quilltap_core::tools::self_inventory::SelfInventoryEnv {
                        version: String::new(),
                        runtime_mode: "local-dev".to_string(),
                        client_shell: quilltap_core::tools::self_inventory::ClientShell::Browser,
                        mount_index_degraded: false,
                        release_notes: None,
                        changelog: None,
                        model_info: Vec::new(),
                        fallback_pricing: Vec::new(),
                        registry_default_context: 8192,
                    },
                )
                .with_embedding_provider(ErasedEmbeddingProvider::new(build_provider(&spec)));
                let tctx = ToolExecutionContext {
                    chat_id: CH1.to_string(),
                    user_id: spec.user_id.clone(),
                    operator_surface: operator,
                    ..Default::default()
                };
                let call = ToolCall {
                    name: row.tool.clone(),
                    arguments: row.args.clone(),
                    call_id: None,
                };
                let (r, lines) = captured_async(runner.run(&call, &tctx)).await;
                let json = serde_json::json!({
                    "toolName": r.tool_name,
                    "success": r.success,
                    "result": r.result,
                    "error": r.error,
                });
                (serde_json::to_string(&json).unwrap(), lines)
            }
            None => {
                let (out, lines) = captured_async(async {
                    match row.tool.as_str() {
                        "lookup_character" => execute_lookup_character_tool(&db, &row.args, &ctx),
                        "lookup_memories" => {
                            execute_lookup_memories_tool(&db, &provider, &row.args, &ctx).await
                        }
                        "lookup_messages" => execute_lookup_messages_tool(&db, &row.args, &ctx),
                        _ => execute_read_messages_tool(&db, &row.args, &ctx),
                    }
                })
                .await;
                (serde_json::to_string(&out).unwrap(), lines)
            }
        };
        let after = dumps(&db);
        drop(db);
        for p in &paths {
            clear(p);
        }

        let label = row.label.as_str();
        let result_ok = if BRAHMA_LOOKUP_LONE_SURROGATE_CLIP.contains(&label) {
            got != row.result_json && got == map_lone_surrogates(&row.result_json)
        } else {
            got == row.result_json
        };
        let want_lines: Vec<String> = row
            .logs
            .iter()
            .map(|l| common::v4_log_as_capture_line("quilltap::tools", l))
            .collect();
        let got_lines = brahma_lookup_lines(&lines);
        let lines_ok = got_lines == want_lines;
        let unchanged_ok = row.unchanged && before == after;
        if !want_lines.is_empty() {
            logged += 1;
        }
        let ok = result_ok && lines_ok && unchanged_ok;
        let detail = || {
            let at = got
                .chars()
                .zip(row.result_json.chars())
                .take_while(|(a, b)| a == b)
                .count();
            let ctx = |s: &str| {
                s.chars()
                    .skip(at.saturating_sub(60))
                    .take(220)
                    .collect::<String>()
            };
            format!(
                "{label}: result {} lines {} unchanged {}\n   v5: {}\n   v4: {}\n   v5 lines: {got_lines:?}\n   v4 lines: {want_lines:?}",
                if result_ok { "ok" } else { "DIFF" },
                if lines_ok { "ok" } else { "DIFF" },
                if unchanged_ok { "ok" } else { "DIFF" },
                ctx(&got),
                ctx(&row.result_json),
            )
        };
        let union =
            UNION_RED_H1_PARTICIPANT_ID.contains(&label) || UNION_RED_H2_WINDOW.contains(&label);
        match (union, ok) {
            (false, true) => {}
            (false, false) => red.push(detail()),
            (true, false) => union_red.push(detail()),
            (true, true) => union_green.push(label.to_string()),
        }
    }
    for u in &union_red {
        println!("UNION-RED (expected on this lane): {u}");
    }
    assert!(
        logged >= 50,
        "only {logged} rows logged a BrahmaLookup line"
    );
    assert!(
        union_green.is_empty(),
        "rows listed as union-red are GREEN here — move them to the green set: {union_green:?}"
    );
    if !red.is_empty() {
        panic!(
            "{} of {} rows RED:\n{}",
            red.len(),
            rows.len(),
            red.join("\n")
        );
    }
    println!(
        "OK: brahma lookup tools byte-exact for {} rows ({} union-red by name).",
        rows.len() - union_red.len(),
        union_red.len()
    );
}
