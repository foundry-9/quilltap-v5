//! Tier-3 differential — the embedding-family remainder (P4.6BM): v4's
//! `handleConversationRender` (`lib/background-jobs/handlers/conversation-render.ts`)
//! against `quilltap_core::services::conversation_render_job`.
//!
//! Both sides copy the same committed two-DB fixture
//! (`crates/quilltap-web/tests/fixtures/embedding-remainder-{main,mount}.db`),
//! run the same corpus phases in the same order with state accumulating, and
//! then diff EIGHT tables: `chats`, `conversation_chunks`, `background_jobs`,
//! `embedding_status`, `memories`, `help_docs`, `vector_entries` and
//! `vector_indices`.
//!
//! Nothing about this family is model-dependent — no LLM, no embedding provider —
//! so the oracle drives v4's real handler with **only** `ensureProcessorRunning`
//! stubbed (in-process it forks v4's job child, which would claim the very jobs
//! under measurement). The one non-determinism, the wall clock, is frozen on the
//! oracle side (`globalThis.Date`) and injected on this side, which is what lets
//! `chats.renderedMarkdown` — a string carrying a `Current time:` line — compare
//! **byte-exact**.
//!
//! ## Minted values
//!
//! The render mints a UUID per NEW chunk and per enqueued job, and a minted chunk
//! id appears again inside its job's `payload`. So rows are not comparable by id:
//! both sides are re-keyed here by a natural key ((chatId, interchangeIndex) for
//! chunks, the payload for jobs, …), every minted chunk id is REWRITTEN to
//! `<chunk:{chatId}#{index}>` wherever it occurs (including inside payload JSON),
//! and a minted row's own id becomes `<minted>`. Rows whose ids ARE pinned by the
//! corpus keep them, so "the upsert reused the existing chunk row" stays a
//! visible, asserted fact rather than something normalization hides.
//!
//! Timestamps on minted rows are placeholdered — EXCEPT on `conversation_chunks`,
//! where both sides stamp the injected clock and so must agree exactly.
//!
//! Regenerate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<this worktree>
//!   cd ~/source/quilltap-server
//!   # (only when the corpus changes — the .db pair is COMMITTED)
//!   QT_FIXTURE_OUT=/tmp/qt-er-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-er-mount.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-embedding-remainder-fixture.ts
//!   cp /tmp/qt-er-main.db  $V5/crates/quilltap-web/tests/fixtures/embedding-remainder-main.db
//!   cp /tmp/qt-er-mount.db $V5/crates/quilltap-web/tests/fixtures/embedding-remainder-mount.db
//!   mkdir -p /tmp/qt-er-oracle/cases /tmp/qt-er-oracle/fixtures
//!   cp $V5/harness/oracle/cases/embedding-remainder.test.ts /tmp/qt-er-oracle/cases/
//!   cp $V5/harness/oracle/fixtures/embedding-remainder.json  /tmp/qt-er-oracle/fixtures/
//!   cp -R $V5/harness/oracle/fixtures/help-sync              /tmp/qt-er-oracle/fixtures/
//!   TZ=UTC \
//!   QT_FIXTURE_ER_MAIN=$V5/crates/quilltap-web/tests/fixtures/embedding-remainder-main.db \
//!   QT_FIXTURE_ER_MOUNT=$V5/crates/quilltap-web/tests/fixtures/embedding-remainder-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-embedding-remainder.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=300000 \
//!       --roots "$PWD" --roots /tmp/qt-er-oracle/cases -- embedding-remainder
//! Run:
//!   TZ=UTC QT_ORACLE_EMBEDDING_REMAINDER=/tmp/oracle-embedding-remainder.ndjson \
//!     cargo test -p quilltap-harness --test embedding_remainder_equivalence

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::conversation_render_job::{
    handle_conversation_render, ConversationRenderPayload,
};
use quilltap_core::services::conversation_render_reconcile::{
    reconcile_conversation_rendering, ReconcileResult,
};
use quilltap_core::services::embedding_reindex_job::{
    handle_embedding_reindex_all, EmbeddingReindexAllPayload,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenderCase {
    name: String,
    chat_id: String,
    full_reembed: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReindexCase {
    name: String,
    profile_id: String,
    scope: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueueMemoriesCase {
    name: String,
    chat_id: String,
    user_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    render_now_iso: String,
    render_cases: Vec<RenderCase>,
    reindex_cases: Vec<ReindexCase>,
    queue_memories_cases: Vec<QueueMemoriesCase>,
}

/// v4 `lib/api/responses.ts` kind→status (the routes-family mapping).
fn status_of(kind: quilltap_core::api::ErrorKind) -> u16 {
    use quilltap_core::api::ErrorKind;
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Unprocessable => 422,
        ErrorKind::Locked => 423,
        ErrorKind::Internal => 500,
    }
}

/// A `Response` as the (status, body) pair the oracle's rows compare against.
fn status_body(r: &quilltap_core::api::Response) -> (u16, Value) {
    if let quilltap_core::api::Response::Error(e) = r {
        return (status_of(e.kind), serde_json::json!({ "error": e.message }));
    }
    let v = serde_json::to_value(r).unwrap();
    (200, v.get("data").cloned().unwrap_or(Value::Null))
}

/// (table, main?, natural sort columns, timestamps stay EXACT on minted rows).
const TABLES: &[(&str, bool, &[&str], bool)] = &[
    ("chats", true, &["id"], true),
    (
        "conversation_chunks",
        true,
        &["chatId", "interchangeIndex"],
        // Both sides stamp the injected clock here, so a minted chunk's
        // timestamps must agree exactly — normalizing them would hide it.
        true,
    ),
    (
        "background_jobs",
        true,
        &["type", "status", "priority", "payload"],
        false,
    ),
    (
        "embedding_status",
        true,
        &["entityType", "entityId", "profileId"],
        false,
    ),
    ("memories", true, &["characterId", "summary"], false),
    ("help_docs", true, &["path"], false),
    ("vector_entries", true, &["characterId", "id"], false),
    ("vector_indices", true, &["characterId"], false),
];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/embedding-remainder.json")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

/// Every UUID-shaped string anywhere in the corpus — the set of ids BOTH sides
/// pinned, so anything else in a dump is minted.
fn pinned_ids(spec: &Value) -> BTreeSet<String> {
    fn walk(v: &Value, out: &mut BTreeSet<String>) {
        match v {
            Value::String(s) => {
                if s.len() == 36 && s.as_bytes()[14] == b'4' && s.matches('-').count() == 4 {
                    out.insert(s.clone());
                }
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            Value::Object(o) => o.values().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    walk(spec, &mut out);
    out
}

/// `interchangeIndex` renders as a JS number (`0`, not `0.0`); the dumps already
/// carry it that way, so plain `to_string` on the JSON value matches both sides.
fn cell(row: &Value, col: &str) -> String {
    match row.get(col) {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// Label every minted id by its row's natural key, so it can be rewritten
/// wherever it appears — its own row AND inside a job payload's `entityId`.
///
/// Two tables mint ids in this family: `conversation_chunks` (the render's new
/// interchanges) and `help_docs` (the reindex's disk sync, which re-creates every
/// doc from the committed help tree). Both reappear in `background_jobs.payload`,
/// so blanking the row id alone would leave the payloads incomparable.
fn minted_labels(
    tables: &BTreeMap<String, Vec<Value>>,
    pinned: &BTreeSet<String>,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for row in tables.get("conversation_chunks").into_iter().flatten() {
        let id = cell(row, "id");
        if pinned.contains(&id) {
            continue;
        }
        out.push((
            id,
            format!(
                "<chunk:{}#{}>",
                cell(row, "chatId"),
                cell(row, "interchangeIndex")
            ),
        ));
    }
    for row in tables.get("help_docs").into_iter().flatten() {
        let id = cell(row, "id");
        if pinned.contains(&id) {
            continue;
        }
        out.push((id, format!("<helpdoc:{}>", cell(row, "path"))));
    }
    out
}

/// Rewrite minted ids, placeholder minted rows' own ids (and, where the table
/// says so, their timestamps), then sort by the natural key.
fn normalize(tables: &mut BTreeMap<String, Vec<Value>>, pinned: &BTreeSet<String>) {
    let labels = minted_labels(tables, pinned);
    for (table, _, sort_cols, exact_ts) in TABLES {
        let Some(rows) = tables.get_mut(*table) else {
            continue;
        };
        for row in rows.iter_mut() {
            let minted = !pinned.contains(&cell(row, "id"));
            let obj = row.as_object_mut().expect("row object");
            let cols: Vec<String> = obj.keys().cloned().collect();
            for col in cols {
                // Rewrite minted chunk ids wherever they occur (job payloads
                // carry them as `entityId`).
                if let Some(Value::String(s)) = obj.get(&col) {
                    let mut s = s.clone();
                    for (id, label) in &labels {
                        if s.contains(id.as_str()) {
                            s = s.replace(id.as_str(), label);
                        }
                    }
                    obj.insert(col.clone(), Value::String(s));
                }
                if !minted {
                    continue;
                }
                if col == "id" {
                    // The chunk rewrite above already relabelled its own id.
                    let relabelled = obj
                        .get("id")
                        .and_then(Value::as_str)
                        .is_some_and(|s| s.starts_with("<chunk:") || s.starts_with("<helpdoc:"));
                    if !relabelled {
                        obj.insert(col.clone(), Value::String("<minted>".to_string()));
                    }
                } else if col.ends_with("At")
                    && !*exact_ts
                    && obj.get(&col).map(|v| !v.is_null()).unwrap_or(false)
                {
                    obj.insert(col.clone(), Value::String("<ts>".to_string()));
                }
            }
        }
        // The natural key first, then the whole normalized row as a
        // tie-breaker. The key alone is NOT total here: a seeded DEAD job and a
        // minted one cancelled by the reindex share type/status/priority/payload
        // exactly, and an arbitrary order between them made the diff report a
        // divergence that was only a permutation.
        rows.sort_by_key(|r| {
            let key = sort_cols
                .iter()
                .map(|c| cell(r, c))
                .collect::<Vec<_>>()
                .join(" ");
            (key, r.to_string())
        });
    }
}

#[tokio::test]
async fn embedding_remainder_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_EMBEDDING_REMAINDER") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_EMBEDDING_REMAINDER to the oracle NDJSON (see header).");
            return;
        }
    };

    let spec_json: Value = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec json");
    let spec: Spec = serde_json::from_value(spec_json.clone()).expect("parse spec");
    let pinned = pinned_ids(&spec_json);

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let mut oracle_reconcile: Option<ReconcileResult> = None;
    let mut oracle_renders: Vec<(String, String, Option<String>)> = Vec::new();
    let mut oracle_reindexes: Vec<(String, String, Option<String>)> = Vec::new();
    let mut oracle_queue: Vec<(String, u16, Value)> = Vec::new();
    let mut want_tables: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut want_columns: BTreeMap<String, Value> = BTreeMap::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("reconcile") => {
                oracle_reconcile = Some(ReconcileResult {
                    incomplete_chats: v["incompleteChats"].as_u64().unwrap() as usize,
                    enqueued: v["enqueued"].as_u64().unwrap() as usize,
                    reused: v["reused"].as_u64().unwrap() as usize,
                    failed: v["failed"].as_u64().unwrap() as usize,
                })
            }
            Some("render") => oracle_renders.push((
                v["name"].as_str().unwrap().to_string(),
                v["outcome"].as_str().unwrap().to_string(),
                v.get("error").and_then(Value::as_str).map(str::to_string),
            )),
            Some("reindex") => oracle_reindexes.push((
                v["name"].as_str().unwrap().to_string(),
                v["outcome"].as_str().unwrap().to_string(),
                v.get("error").and_then(Value::as_str).map(str::to_string),
            )),
            Some("queueMemories") => oracle_queue.push((
                v["name"].as_str().unwrap().to_string(),
                v["status"].as_u64().unwrap() as u16,
                v["body"].clone(),
            )),
            Some("table") => {
                let table = v["table"].as_str().unwrap().to_string();
                want_columns.insert(table.clone(), v["columns"].clone());
                want_tables.insert(table, v["rows"].as_array().unwrap().clone());
            }
            other => panic!("unknown oracle row kind {other:?}"),
        }
    }

    // Corpus-shape assertions (not hand counts): the oracle ran EXACTLY this
    // corpus's render cases in order, and dumped exactly the diffed table set.
    assert_eq!(
        oracle_renders
            .iter()
            .map(|(n, _, _)| n.as_str())
            .collect::<Vec<_>>(),
        spec.render_cases
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        "oracle render-case set/order diverges from the corpus — regenerate"
    );
    assert_eq!(
        oracle_reindexes
            .iter()
            .map(|(n, _, _)| n.as_str())
            .collect::<Vec<_>>(),
        spec.reindex_cases
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        "oracle reindex-case set/order diverges from the corpus — regenerate"
    );
    // Both throw arms must actually have thrown, or the corpus stopped covering
    // them and a port that never throws would agree.
    assert_eq!(
        oracle_reindexes
            .iter()
            .filter(|(_, o, _)| o == "error")
            .count(),
        2,
        "the corpus stopped exercising both reindex throw arms"
    );
    assert_eq!(
        oracle_queue
            .iter()
            .map(|(n, _, _)| n.as_str())
            .collect::<Vec<_>>(),
        spec.queue_memories_cases
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        "oracle queue-memories case set/order diverges from the corpus — regenerate"
    );
    // All three 400 arms must actually have fired (both empty-batch messages and
    // the no-cheap-LLM one), or a port that never rejects would agree.
    assert_eq!(
        oracle_queue.iter().filter(|(_, s, _)| *s == 400).count(),
        3,
        "the corpus stopped exercising all three queue-memories 400 arms"
    );
    assert_eq!(
        want_tables.keys().cloned().collect::<BTreeSet<_>>(),
        TABLES
            .iter()
            .map(|(t, ..)| t.to_string())
            .collect::<BTreeSet<_>>(),
        "oracle dumped a different table set"
    );

    // Fresh copies so the committed fixtures stay pristine.
    let pid = std::process::id();
    let work_main = std::env::temp_dir().join(format!("qt-er-main-rust-{pid}.db"));
    let work_mount = std::env::temp_dir().join(format!("qt-er-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(
        fixtures_dir().join("embedding-remainder-main.db"),
        &work_main,
    )
    .unwrap_or_else(|e| panic!("copy main fixture: {e}"));
    std::fs::copy(
        fixtures_dir().join("embedding-remainder-mount.db"),
        &work_mount,
    )
    .unwrap_or_else(|e| panic!("copy mount fixture: {e}"));

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copies: {e}"));

    // ── Phase 1: the startup reconcile, over the pristine fixture copy.
    let want_reconcile = oracle_reconcile.expect("oracle emitted no reconcile row — regenerate");
    // A reconcile that found nothing would agree with a port that did nothing,
    // so pin the corpus's own shape: BOTH arms fire, and the dedupe reuses.
    assert!(
        want_reconcile.incomplete_chats >= 2
            && want_reconcile.enqueued >= 1
            && want_reconcile.reused >= 1,
        "the corpus stopped exercising both reconcile arms + the dedupe: {want_reconcile:?}"
    );
    // The reconcile is a BOOT pass: synchronous, on the writer connection. Under
    // the async test runtime that needs a blocking thread (`write_blocking`
    // panics if called on a runtime thread).
    let got_reconcile = {
        let db = db.clone();
        tokio::task::spawn_blocking(move || {
            db.write_blocking(|ws| Ok(reconcile_conversation_rendering(ws.main().connection())))
        })
        .await
        .expect("join reconcile")
        .expect("reconcile")
    };
    assert_eq!(got_reconcile, want_reconcile, "reconcile counters diverge");

    // ── Phase 2: the render handler, corpus order, state accumulating.
    let mut got_renders: Vec<(String, String, Option<String>)> = Vec::new();
    for rc in &spec.render_cases {
        let payload = ConversationRenderPayload {
            chat_id: rc.chat_id.clone(),
            full_reembed: rc.full_reembed,
        };
        let outcome =
            handle_conversation_render(&db, &spec.user_id, &payload, &spec.render_now_iso).await;
        got_renders.push(match outcome {
            Ok(()) => (rc.name.clone(), "ok".to_string(), None),
            Err(e) => (rc.name.clone(), "error".to_string(), Some(e)),
        });
    }
    assert_eq!(
        got_renders, oracle_renders,
        "render outcome sequence diverges"
    );

    // ── Phase 3: the reindex handler, corpus order. Both sides walk the SAME
    // committed help tree — the oracle by chdir (v4 captures
    // `HELP_DIR = join(process.cwd(), 'help')` at module load), this side
    // through the production host walker.
    let help_files = quilltap_host::files_store::load_help_source_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/help-sync"),
    );
    assert!(
        !help_files.is_empty(),
        "the committed help tree walked empty — both sides would sync nothing"
    );
    let mut got_reindexes: Vec<(String, String, Option<String>)> = Vec::new();
    for rc in &spec.reindex_cases {
        let payload = EmbeddingReindexAllPayload {
            profile_id: rc.profile_id.clone(),
            scope: rc.scope.clone(),
        };
        let outcome = handle_embedding_reindex_all(
            &db,
            &spec.user_id,
            &payload,
            &help_files,
            &spec.render_now_iso,
        )
        .await;
        got_reindexes.push(match outcome {
            Ok(()) => (rc.name.clone(), "ok".to_string(), None),
            Err(e) => (rc.name.clone(), "error".to_string(), Some(e)),
        });
    }
    assert_eq!(
        got_reindexes, oracle_reindexes,
        "reindex outcome sequence diverges"
    );

    // ── Phase 4: the queue-memories action, corpus order. The user id is the
    // CASE's, not the corpus default — one case names the second user, whose
    // missing chat-settings row is the resolver's `null` → 400 arm.
    for (case, (want_name, want_status, want_body)) in
        spec.queue_memories_cases.iter().zip(oracle_queue.iter())
    {
        let got =
            quilltap_core::api::memories::chat_queue_memories(&db, &case.user_id, &case.chat_id)
                .await;
        let (got_status, got_body) = status_body(&got);
        assert_eq!(
            (got_status, &got_body),
            (*want_status, want_body),
            "queue-memories case {want_name} diverges"
        );
    }

    // ── Dump + diff.
    let mut got_tables: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut got_columns: BTreeMap<String, Value> = BTreeMap::new();
    for (table, main, ..) in TABLES {
        let dump = if *main {
            db.read_main(|conn| dump_table_json_conn(conn, table, "id"))
        } else {
            db.read_mount_index(|conn| dump_table_json_conn(conn, table, "id"))
        }
        .unwrap_or_else(|e| panic!("dump {table}: {e:?}"));
        got_columns.insert(table.to_string(), dump["columns"].clone());
        got_tables.insert(
            table.to_string(),
            dump["rows"].as_array().cloned().unwrap_or_default(),
        );
    }
    drop(db);
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);

    normalize(&mut got_tables, &pinned);
    normalize(&mut want_tables, &pinned);

    for (table, ..) in TABLES {
        assert_eq!(
            got_columns[*table], want_columns[*table],
            "{table} column set / order"
        );
        let g = &got_tables[*table];
        let w = &want_tables[*table];
        for (i, (gr, wr)) in g.iter().zip(w.iter()).enumerate() {
            if gr != wr {
                let gp = std::env::temp_dir().join(format!("qt-er-got-{table}.json"));
                let wp = std::env::temp_dir().join(format!("qt-er-want-{table}.json"));
                let _ = std::fs::write(&gp, serde_json::to_string_pretty(g).unwrap());
                let _ = std::fs::write(&wp, serde_json::to_string_pretty(w).unwrap());
                panic!(
                    "{table} rows diverge at index {i} (full dumps: {} / {})\n got: {gr:#}\nwant: {wr:#}",
                    gp.display(),
                    wp.display()
                );
            }
        }
        assert_eq!(
            g.len(),
            w.len(),
            "{table} row-count diverges: got {} vs want {}",
            g.len(),
            w.len()
        );
    }

    println!(
        "embedding_remainder: reconcile + {} render / {} reindex / {} queue-memories cases, {} tables OK",
        got_renders.len(),
        got_reindexes.len(),
        spec.queue_memories_cases.len(),
        TABLES.len()
    );
}
