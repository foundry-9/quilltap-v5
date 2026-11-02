//! P4.168 (the human's ruling H-2, 2026-10-27 — "refuse as v4 does"): the
//! memories repository `create` validates its row against `MemorySchema`, and
//! a refused row (an offset `occurredAt`) logs v4's THREE ERRORs in order —
//! `Data validation failed` / `Error creating entity` / `Error creating memory`
//! (`base.repository.ts:130-141, 350-379`, `memories.repository.ts:535-549`),
//! each carrying the `ZodError.message`.
//!
//! The lines are emitted INSIDE `Db::write` — on the WRITER thread — so the
//! thread-local `global_capture` the `memories_routes_equivalence` family uses
//! cannot see them (memory note `capture-rig-writer-thread`, third trap). This
//! binary owns a PROCESS-GLOBAL capture (the `avatar_job_tier3_equivalence`
//! template) and compares v5's `quilltap::db` memories ERROR lines with the
//! oracle's `repoLogs` for the rows that name them — and for a passing create,
//! that there are none on either side.
//!
//! The oracle is `memories_routes_equivalence`'s route NDJSON (its recipe
//! header). Run:
//!   QT_ORACLE_MEMORIES_ROUTES=/tmp/oracle-memories-routes.ndjson \
//!     cargo test -p quilltap-harness --test memory_row_schema_refusal_lines

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use quilltap_core::api::memories;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::embedding::ErasedEmbeddingProvider;
use quilltap_core::model::wire::CannedWireTransport;
use quilltap_core::services::embedding_provider::ApiEmbeddingProvider;
use serde::Deserialize;
use serde_json::{json, Value};

const MNEMO: &str = "b1000000-0000-4000-8000-000000000001";
/// The oracle's fixed `Date.now()` (mirrors `memories_routes_equivalence`).
const FIXED_NOW_MS: f64 = 1_783_000_000_000.0;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/memories-web.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

/// A PROCESS-GLOBAL capture: the repository create runs on the WRITER thread.
fn writer_thread_lines() -> Arc<Mutex<Vec<String>>> {
    static BUF: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();
    BUF.get_or_init(|| {
        use tracing_subscriber::layer::SubscriberExt;
        let buf = Arc::new(Mutex::new(Vec::<String>::new()));
        let subscriber = tracing_subscriber::registry()
            .with(quilltap_core::test_support::CaptureLayer(buf.clone()));
        tracing::subscriber::set_global_default(subscriber).expect("one global capture per binary");
        buf
    })
    .clone()
}

/// One of v4's memories-repository ERROR lines rendered as v5's capture
/// renders a `quilltap::db` event (`LEVEL target message k=v…`, v4's key order).
fn render_v4_repo_line(line: &Value) -> String {
    let level = line["level"].as_str().unwrap().to_uppercase();
    let message = line["message"].as_str().unwrap();
    let mut out = format!("{level} quilltap::db {message}");
    if let Some(fields) = line["fields"].as_object() {
        for (k, v) in fields {
            match v {
                Value::String(s) => out.push_str(&format!(" {k}={s}")),
                other => out.push_str(&format!(" {k}={other}")),
            }
        }
    }
    out
}

/// v5's memories-repository ERROR lines (the oracle's `repoLogs` filter:
/// level `error`, `collection: 'memories'`).
fn repo_v5_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|l| l.starts_with("ERROR quilltap::db ") && l.contains(" collection=memories"))
        .cloned()
        .collect()
}

struct ScratchDb {
    db: Db,
    _dir: tempfile::TempDir,
}

fn fresh_db(spec: &Spec, tag: &str) -> ScratchDb {
    let scratch = tempfile::Builder::new()
        .prefix(&format!("qt-mem-refusal-{tag}-"))
        .tempdir()
        .expect("tempdir");
    let main = scratch.path().join("main.db");
    let mount = scratch.path().join("mount.db");
    std::fs::copy(fixtures_dir().join("memories-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("memories-mount.db"), &mount).unwrap();
    // The same heals `memories_routes_equivalence::fresh_db` runs on its copy.
    {
        let w = quilltap_core::db::Writer::open_writable(&main, &spec.test_pepper_base64)
            .expect("open the fixture copy writable to heal it");
        quilltap_core::test_support::ensure_p4d171_columns(w.connection());
        quilltap_core::test_support::ensure_memory_tiers_columns(w.connection());
        quilltap_core::test_support::ensure_embedding_status_table(w.connection());
    }
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db");
    ScratchDb { db, _dir: scratch }
}

fn count(db: &Db) -> i64 {
    db.read_main(|c| {
        c.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get::<_, i64>(0))
            .map_err(Into::into)
    })
    .unwrap()
}

#[test]
fn memory_create_refusal_logs_v4_three_repository_lines() {
    let lines = writer_thread_lines();
    let Ok(path) = std::env::var("QT_ORACLE_MEMORIES_ROUTES") else {
        eprintln!("SKIP: set QT_ORACLE_MEMORIES_ROUTES (see test header).");
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let oracle: HashMap<String, Value> = std::fs::read_to_string(&path)
        .expect("read the route oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).unwrap();
            (v["name"].as_str().unwrap().to_string(), v)
        })
        .collect();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let base = json!({
        "characterId": MNEMO,
        "content": "The tide clock above the chandlery runs nine minutes fast.",
        "summary": "The chandlery tide clock runs fast.",
    });
    let with = |extra: Value| {
        let mut b = base.clone();
        b.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        b
    };
    // (row, body, expect a refusal, plant) — the refusal and passing controls.
    // P4.D341 (v4 `8cb584eff`): a TEXT `keystone` cell on the fold target
    // (`a1`) — the `findByFilter` reads hydrate it `Boolean('yes')`, so v4
    // logs NO repository line and drops nothing (a silence row on both
    // sides). The raw-path drop of a TEXT cell (`Memory failed validation in
    // most-important fetch`) is `memories_read_equivalence`'s K09 row; C-1's
    // `keystone fetch` can never select one (`keystone = 1` against a TEXT
    // cell in an INTEGER-affinity column is never true — measured).
    const A1_TEXT_KEYSTONE: &[&str] = &[
        "UPDATE memories SET keystone = 'yes' WHERE id = 'b2000000-0000-4000-8000-0000000000a1'",
    ];
    let rows: [(&str, Value, bool, Option<&[&str]>); 3] = [
        (
            "create_offset_occurred_at_row_refusal",
            with(json!({ "occurredAt": "2026-03-01T10:00:00+05:30" })),
            true,
            None,
        ),
        (
            "create_kind_occurred_entities",
            with(json!({
                "kind": "episodic",
                "occurredAt": "2026-03-01T10:00:00.000Z",
                "entities": ["Alden", "harbor"],
            })),
            false,
            None,
        ),
        (
            "keystone_create_reinforce_into_text_keystone",
            json!({
                "characterId": MNEMO,
                "content": "Alden the lighthouse keeper trusts only the harbor master.",
                "summary": "Alden trusts the harbor master.",
                "keystone": true,
            }),
            false,
            Some(A1_TEXT_KEYSTONE),
        ),
    ];
    let mut failed = Vec::new();
    for (name, body, refuses, plant) in rows {
        let rec = oracle
            .get(name)
            .unwrap_or_else(|| panic!("oracle row {name} missing — regenerate"));
        let want: Vec<String> = rec["repoLogs"]
            .as_array()
            .expect("repoLogs (regenerate the oracle — P4.168)")
            .iter()
            .map(render_v4_repo_line)
            .collect();
        let scratch = fresh_db(&spec, name);
        let db = &scratch.db;
        if let Some(statements) = plant {
            rt.block_on(db.write(move |w| {
                for sql in statements {
                    w.main()
                        .connection()
                        .execute_batch(sql)
                        .map_err(quilltap_core::db::DbError::from)?;
                }
                Ok(())
            }))
            .expect("plant");
        }
        let p = ErasedEmbeddingProvider::new(ApiEmbeddingProvider::new(
            db.clone(),
            CannedWireTransport::new(),
        ));
        let before = count(db);
        lines.lock().unwrap().clear();
        let resp = rt.block_on(memories::memory_create(
            db,
            Some(&p),
            &spec.user_id,
            FIXED_NOW_MS,
            "UTC",
            body,
        ));
        let got = repo_v5_lines(&lines.lock().unwrap());
        // P4.D341: no raw-path drop WARN on either side for these rows (v4's
        // `Memory failed validation in …` would ride the oracle's sibling
        // capture as a non-`[Memories API` line; it never fires here).
        let dropped = lines
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.contains("Memory failed validation in"));
        let after = count(db);
        let mut bad = Vec::new();
        if dropped {
            bad.push("v5 dropped a row with `Memory failed validation in …`".to_string());
        }
        if refuses == want.is_empty() {
            bad.push(format!(
                "the oracle row's premise moved: {} v4 lines",
                want.len()
            ));
        }
        if got != want {
            bad.push(format!(
                "repository lines:\n--- v4 ---\n{}\n--- v5 ---\n{}",
                want.join("\n"),
                got.join("\n")
            ));
        }
        match (&resp, refuses) {
            (Response::Error(e), true) => {
                if e.kind != ErrorKind::BadRequest || e.message != "Validation error" {
                    bad.push(format!(
                        "v5 {:?} {:?}, v4 400 Validation error",
                        e.kind, e.message
                    ));
                }
                // The 400's `details` — v4's `zodError.issues`, by bytes.
                let got_details = e
                    .details
                    .as_deref()
                    .map(|d| serde_json::to_string(d).unwrap());
                let want_details = serde_json::to_string(&rec["body"]["details"]).unwrap();
                if got_details.as_deref() != Some(want_details.as_str()) {
                    bad.push(format!(
                        "details:\n  v5 {got_details:?}\n  v4 {want_details}"
                    ));
                }
                if after != before {
                    bad.push(format!("a refused create wrote {} row(s)", after - before));
                }
            }
            (Response::Error(e), false) => bad.push(format!("v5 refused a passing row: {e:?}")),
            (_, true) => bad.push("v5 wrote the refused row".to_string()),
            (_, false) => {}
        }
        if bad.is_empty() {
            eprintln!("[{name}] OK (P4.168).");
        } else {
            eprintln!("[{name}] MISMATCH (P4.168):\n{}", bad.join("\n"));
            failed.push(name);
        }
    }
    assert!(failed.is_empty(), "mismatched rows: {failed:?}");
}
