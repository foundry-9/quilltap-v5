//! Tier-2 differential — P4.D292: the boot embedding-dimension reconcile
//! (`services::embedding_dimension_reconcile`) against v4's REAL
//! `reconcileEmbeddingDimensions` at the `83334b9a5` pin — the result NARROWED
//! by `af48f6cc2` (the two vector-mirror arms and their two keys gone; the
//! KEY SET byte-compared), the INFO fired only on `totalMismatched > 0` (the
//! DEBUG on a conforming corpus), the dedupe INFO, the no-fixed-dim WARN, the
//! silent BUILTIN / no-profile skips, absent tables counting zero, and the
//! `mismatched-dim` reindex row. Every line carries v4's service-logger
//! binding `module=service` first.
//!
//! Each scenario's SQL runs on copies of the case's `base-main.db` /
//! `base-mount.db`; the pass runs on the caller thread over writable
//! connections, so its lines are captured whole.
//!
//! Generate the oracle (Node 24, from the round's pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   rm -rf /tmp/qt-embedding-dimension-reconcile /tmp/oracle-embedding-dimension-reconcile.ndjson
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_EDR_DIR=/tmp/qt-embedding-dimension-reconcile \
//!     $N/npx tsx $V5W/harness/oracle/cases/embedding-dimension-reconcile.ts \
//!     > /tmp/oracle-embedding-dimension-reconcile.ndjson
//! Run:
//!   cd $V5W
//!   QT_ORACLE_EMBEDDING_DIMENSION_RECONCILE=/tmp/oracle-embedding-dimension-reconcile.ndjson \
//!   QT_FIXTURE_EDR_DIR=/tmp/qt-embedding-dimension-reconcile \
//!     cargo test -p quilltap-harness --test embedding_dimension_reconcile_equivalence

use quilltap_core::db::Writer;
use quilltap_core::services::embedding_dimension_reconcile::{
    reconcile_embedding_dimensions, DimensionReconcileResult,
};
use quilltap_core::test_support::captured_with;
use serde_json::{json, Map, Value};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

/// v4's `EmbeddingDimensionReconcileResult`, in its key order.
fn result_json(r: &DimensionReconcileResult) -> Value {
    json!({
        "targetDimensions": r.target_dimensions,
        "skippedReason": r.skipped_reason.map(|s| s.as_str()),
        "mismatched": {
            "memories": r.mismatched.memories,
            "conversationChunks": r.mismatched.conversation_chunks,
            "helpDocs": r.mismatched.help_docs,
            "mountChunks": r.mismatched.mount_chunks,
        },
        "reindexEnqueued": r.reindex_enqueued,
    })
}

fn want_lines(lines: &Value) -> Vec<String> {
    lines
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            let mut out = format!(
                "{} {}",
                l["level"].as_str().unwrap().to_uppercase(),
                l["message"].as_str().unwrap()
            );
            for (k, v) in l["fields"].as_object().unwrap() {
                match v {
                    Value::String(s) => out.push_str(&format!(" {k}={s}")),
                    other => out.push_str(&format!(" {k}={other}")),
                }
            }
            out
        })
        .collect()
}

fn got_lines(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .map(|l| {
            let mut parts = l.splitn(3, ' ');
            let level = parts.next().unwrap().to_string();
            let _target = parts.next();
            format!("{level} {}", parts.next().unwrap_or_default())
        })
        .collect()
}

fn jobs(conn: &rusqlite::Connection) -> Value {
    let mut stmt = conn
        .prepare(
            r#"SELECT "id","userId","type","status","payload","priority","attempts","maxAttempts","lastError" FROM "background_jobs" ORDER BY "createdAt", "id""#,
        )
        .unwrap();
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let rows = stmt
        .query_map([], |r| {
            let mut row = Map::new();
            for (i, name) in names.iter().enumerate() {
                let v = match r.get_ref(i)? {
                    rusqlite::types::ValueRef::Null => Value::Null,
                    rusqlite::types::ValueRef::Integer(n) => n.into(),
                    rusqlite::types::ValueRef::Real(f) if f.fract() == 0.0 => (f as i64).into(),
                    rusqlite::types::ValueRef::Real(f) => f.into(),
                    rusqlite::types::ValueRef::Text(t) => {
                        String::from_utf8_lossy(t).into_owned().into()
                    }
                    rusqlite::types::ValueRef::Blob(_) => "<blob>".into(),
                };
                row.insert(name.clone(), v);
            }
            Ok(Value::Object(row))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    mask(rows)
}

/// Minted ids (not the plants' `e2920000-…`) masked.
fn mask(rows: Vec<Value>) -> Value {
    Value::Array(
        rows.into_iter()
            .map(|mut r| {
                if r["id"]
                    .as_str()
                    .is_some_and(|id| !id.starts_with("e2920000"))
                {
                    r["id"] = "<minted>".into();
                }
                r
            })
            .collect(),
    )
}

#[test]
fn the_reconcile_matches_v4() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_EMBEDDING_DIMENSION_RECONCILE") else {
        eprintln!(
            "SKIP: set QT_ORACLE_EMBEDDING_DIMENSION_RECONCILE to the oracle NDJSON (see header)."
        );
        return;
    };
    let Ok(dir) = std::env::var("QT_FIXTURE_EDR_DIR") else {
        eprintln!("SKIP: set QT_FIXTURE_EDR_DIR to the case's dir (see header).");
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line"))
        .collect();
    assert!(
        oracle.len() >= 8,
        "expected the oracle's 8 scenarios, saw {}",
        oracle.len()
    );
    let scratch = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();
    for (i, sc) in oracle.iter().enumerate() {
        let name = sc["name"].as_str().unwrap();
        let (m, n) = (
            scratch.path().join(format!("main-{i}.db")),
            scratch.path().join(format!("mount-{i}.db")),
        );
        std::fs::copy(dir.join("base-main.db"), &m).unwrap();
        std::fs::copy(dir.join("base-mount.db"), &n).unwrap();
        let main = Writer::open_writable(&m, PEPPER).unwrap();
        let mount = Writer::open_writable(&n, PEPPER).unwrap();
        for (w, key) in [(&main, "main"), (&mount, "mount")] {
            for s in sc[key].as_array().unwrap() {
                w.connection()
                    .execute_batch(s.as_str().unwrap())
                    .unwrap_or_else(|e| panic!("{name}: plant {s}: {e}"));
            }
        }
        let (result, lines) = captured_with(|| {
            reconcile_embedding_dimensions(main.connection(), Some(mount.connection()))
        });
        let (got, want) = (
            serde_json::to_string(&result_json(&result)).unwrap(),
            serde_json::to_string(&sc["result"]).unwrap(),
        );
        if got != want {
            failures.push(format!("{name}: result\n  v5 {got}\n  v4 {want}"));
        }
        let (gl, wl) = (got_lines(lines), want_lines(&sc["lines"]));
        if gl != wl {
            failures.push(format!("{name}: lines\n  v5 {gl:#?}\n  v4 {wl:#?}"));
        }
        let (gj, wj) = (
            jobs(main.connection()),
            mask(sc["jobs"].as_array().unwrap().clone()),
        );
        if gj != wj {
            failures.push(format!("{name}: background_jobs\n  v5 {gj}\n  v4 {wj}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} divergences:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    eprintln!(
        "OK: {} dimension-reconcile scenarios matched v4.",
        oracle.len()
    );
}
