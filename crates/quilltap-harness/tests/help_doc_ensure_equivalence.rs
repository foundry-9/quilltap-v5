//! P4.d6 differential — `ensureHelpDocsSynced` (v4 `6c59b1ca` bug 1 +
//! `551f090b`'s divergence trigger / prune / embedding enqueue) vs
//! `quilltap_core::services::help_doc_sync::ensure_help_docs_synced`.
//!
//! Both sides copy the SAME per-scenario fixture DB (v4-built, byte-identical
//! seeds), walk the SAME committed help tree (`harness/oracle/fixtures/
//! help-ensure/help/`, the Rust side through the production host walker), run
//! the real `ensureHelpDocsSynced`, and diff the resulting `help_docs` rows and
//! `background_jobs`.
//!
//! Five scenarios share ONE tree and vary only the seed, so each isolates a
//! single trigger path:
//!   - `empty-table`  — the lazy gate that always worked (the control);
//!   - `in-sync`      — disk and DB agree → NO sync, no prune, no enqueue;
//!   - `added-doc`    — ⚠ **v4 BUG 1**: the table is POPULATED and the existing
//!     doc is unchanged, but a shipped doc has no row. The old gate
//!     (`existing.length > 0 → return`) left it invisible forever;
//!   - `deleted-doc`  — ⚠ the **deleted direction is load-bearing**: every file
//!     on disk already has a matching row, so the unsynced direction CANNOT
//!     fire. Only the row whose file is gone can trigger this sync — without it
//!     the prune would be dead code;
//!   - `no-profile`   — the sync completes with no embedding profile; only the
//!     enqueue backs off.
//!
//! Minted ids never enter the comparison: doc rows compare by `path` (a
//! non-seeded id collapses to `<minted>`), and jobs resolve their payload
//! `entityId` to the doc's PATH.
//!
//! Generate (Node 24, from the v4 checkout) — the fixture, then the JEST oracle
//! (`[[jest-real-db-oracle]]`; the mirror needs a `fixtures/` sibling, and the
//! case MUST be mirrored to /tmp because v4's jest config ignores `.claude/`
//! paths):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<this tree>
//!   cd ~/source/quilltap-server
//!   for s in empty-table in-sync added-doc deleted-doc no-profile; do
//!     QT_FIXTURE_ENSURE_DIR=/tmp/qt-ensure QT_ENSURE_SCENARIO=$s \
//!       $N/node --import tsx $V5/harness/oracle/fixtures/build-help-ensure-fixture.ts
//!   done
//!   TMPO=/tmp/qt-ensure-oracle; rm -rf $TMPO; mkdir -p $TMPO/cases $TMPO/fixtures
//!   cp $V5/harness/oracle/cases/help-sync-ensure.test.ts $TMPO/cases/
//!   cp $V5/harness/oracle/fixtures/help-ensure.json $TMPO/fixtures/
//!   cp -R $V5/harness/oracle/fixtures/help-ensure $TMPO/fixtures/
//!   PATH=$N:$PATH QT_FIXTURE_ENSURE_DIR=/tmp/qt-ensure \
//!   QT_ORACLE_OUT=/tmp/oracle-help-ensure.ndjson \
//!     npx jest --silent --testTimeout=120000 --roots $TMPO/cases -- help-sync-ensure
//! Run:
//!   QT_ORACLE_HELP_ENSURE=/tmp/oracle-help-ensure.ndjson \
//!   QT_FIXTURE_ENSURE_DIR=/tmp/qt-ensure \
//!     cargo test -p quilltap-harness --test help_doc_ensure_equivalence

use serde::Deserialize;
use serde_json::{json, Value};

use quilltap_core::db::runtime::Db;
use quilltap_core::services::help_doc_sync::ensure_help_docs_synced;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "seedSentinel")]
    seed_sentinel: String,
    scenarios: Vec<SpecScenario>,
}

#[derive(Deserialize)]
struct SpecScenario {
    name: String,
    #[serde(rename = "helpDocs")]
    help_docs: Vec<SpecSeedDoc>,
}

#[derive(Deserialize)]
struct SpecSeedDoc {
    id: String,
}

#[derive(Deserialize)]
struct OracleLine {
    scenario: String,
    #[serde(rename = "helpDocs")]
    help_docs: Vec<Value>,
    jobs: Vec<Value>,
}

fn fixtures_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
}

/// Render a numeric cell the way the oracle's `JSON.stringify` does: an
/// integer-valued double collapses to an integer (`0`, not `0.0`).
/// `background_jobs.priority` / `maxAttempts` have REAL affinity, so reading
/// them as `f64` and serializing naively is a spurious diff — this mirrors
/// quilltap-core's `js_number_to_json`, which is `pub(crate)` and out of the
/// harness's reach (the `[[jest-real-db-oracle]]` "inline the small helper"
/// idiom).
fn js_number(f: f64) -> Value {
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 9e15 {
        Value::from(f as i64)
    } else {
        Value::from(f)
    }
}

#[test]
fn help_doc_ensure_matches_oracle() {
    let (oracle_path, fixture_dir) = match (
        std::env::var("QT_ORACLE_HELP_ENSURE"),
        std::env::var("QT_FIXTURE_ENSURE_DIR"),
    ) {
        (Ok(o), Ok(f)) => (o, f),
        _ => {
            eprintln!(
                "skipping help_doc_ensure_matches_oracle: set QT_ORACLE_HELP_ENSURE + \
                 QT_FIXTURE_ENSURE_DIR to run the differential"
            );
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("help-ensure.json")).expect("spec"),
    )
    .expect("parse spec");

    let oracle_text = std::fs::read_to_string(&oracle_path).expect("oracle ndjson");
    let oracle: Vec<OracleLine> = oracle_text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line"))
        .collect();
    assert_eq!(
        oracle.len(),
        spec.scenarios.len(),
        "oracle is missing scenarios ({} of {}) — regenerate {oracle_path}",
        oracle.len(),
        spec.scenarios.len()
    );

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let tree_root = fixtures_dir().join("help-ensure");

    for expected in &oracle {
        let def = spec
            .scenarios
            .iter()
            .find(|s| s.name == expected.scenario)
            .unwrap_or_else(|| panic!("oracle names an unknown scenario {}", expected.scenario));
        let seeded_ids: Vec<&str> = def.help_docs.iter().map(|d| d.id.as_str()).collect();

        let tmp = tempfile::tempdir().expect("tempdir");
        let work_main = tmp.path().join("ensure-main.db");
        let fixture_main =
            std::path::Path::new(&fixture_dir).join(format!("qt-ensure-{}-main.db", def.name));
        std::fs::copy(&fixture_main, &work_main)
            .unwrap_or_else(|e| panic!("copy fixture {}: {e}", fixture_main.display()));

        let db = Db::open_main(&work_main, &spec.test_pepper_base64).expect("open db");
        let files = quilltap_host::files_store::load_help_source_files(&tree_root);

        runtime
            .block_on(ensure_help_docs_synced(&db, &files))
            .expect("ensure_help_docs_synced");

        // ---- dump help_docs (same SELECT/order/normalization as the oracle) ----
        let doc_rows: Vec<(String, String, String, String, String)> = db
            .read_main(|c| {
                let mut stmt = c
                    .prepare(
                        "SELECT id, path, title, hex(embedding) AS embeddingHex, updatedAt \
                         FROM help_docs ORDER BY path ASC",
                    )
                    .map_err(quilltap_core::db::DbError::from)?;
                let mapped = stmt
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                            r.get::<_, String>(4)?,
                        ))
                    })
                    .map_err(quilltap_core::db::DbError::from)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(quilltap_core::db::DbError::from)?;
                Ok(mapped)
            })
            .expect("dump help_docs");

        let path_by_id: std::collections::HashMap<&str, &str> = doc_rows
            .iter()
            .map(|(id, path, ..)| (id.as_str(), path.as_str()))
            .collect();

        let help_docs: Vec<Value> = doc_rows
            .iter()
            .map(|(id, path, title, emb_hex, updated_at)| {
                json!({
                    "id": if seeded_ids.contains(&id.as_str()) { id.as_str() } else { "<minted>" },
                    "path": path,
                    "title": title,
                    "hasEmbedding": !emb_hex.is_empty(),
                    "updatedAt": if updated_at == &spec.seed_sentinel { "<sentinel>" } else { "<ts>" },
                })
            })
            .collect();

        // ---- dump background_jobs (entityId -> the doc's PATH) ----
        let job_rows: Vec<(String, String, f64, f64, String, String)> = db
            .read_main(|c| {
                let mut stmt = c
                    .prepare(
                        "SELECT type, status, priority, maxAttempts, payload, userId \
                         FROM background_jobs",
                    )
                    .map_err(quilltap_core::db::DbError::from)?;
                let mapped = stmt
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, f64>(2)?,
                            r.get::<_, f64>(3)?,
                            r.get::<_, String>(4)?,
                            r.get::<_, String>(5)?,
                        ))
                    })
                    .map_err(quilltap_core::db::DbError::from)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(quilltap_core::db::DbError::from)?;
                Ok(mapped)
            })
            .expect("dump background_jobs");

        let mut jobs: Vec<Value> = job_rows
            .iter()
            .map(|(ty, status, priority, max_attempts, payload, user_id)| {
                let p: Value = serde_json::from_str(payload).unwrap_or(Value::Null);
                let entity_id = p
                    .get("entityId")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                json!({
                    "type": ty,
                    "status": status,
                    "priority": js_number(*priority),
                    "maxAttempts": js_number(*max_attempts),
                    "entityType": p.get("entityType").and_then(Value::as_str).unwrap_or_default(),
                    "entityPath": path_by_id
                        .get(entity_id)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("<unknown:{entity_id}>")),
                    "profileId": p.get("profileId").and_then(Value::as_str).unwrap_or_default(),
                    "userId": user_id,
                })
            })
            .collect();
        jobs.sort_by(|a, b| {
            a["entityPath"]
                .as_str()
                .unwrap_or_default()
                .cmp(b["entityPath"].as_str().unwrap_or_default())
        });

        assert_eq!(
            Value::Array(help_docs.clone()),
            Value::Array(expected.help_docs.clone()),
            "[{}] help_docs diverged\nrust:   {:?}\noracle: {:?}",
            def.name,
            help_docs,
            expected.help_docs
        );
        assert_eq!(
            Value::Array(jobs.clone()),
            Value::Array(expected.jobs.clone()),
            "[{}] background_jobs diverged\nrust:   {:?}\noracle: {:?}",
            def.name,
            jobs,
            expected.jobs
        );

        eprintln!(
            "ensure {}: {} help_docs, {} job(s)",
            def.name,
            help_docs.len(),
            jobs.len()
        );
    }

    // Belt-and-braces pins on the DRIFT itself, so a regeneration that quietly
    // lost a scenario — or two identically-broken sides — cannot pass as
    // agreement. These assert against the ORACLE (v4's recorded behavior).
    let by_name = |n: &str| {
        oracle
            .iter()
            .find(|c| c.scenario == n)
            .unwrap_or_else(|| panic!("oracle lost the {n} scenario"))
    };

    // Bug 1: a populated table + a doc with no row MUST sync it (and enqueue it).
    let added = by_name("added-doc");
    assert!(
        added
            .help_docs
            .iter()
            .any(|d| d["path"] == "help/brahma-console.md" && d["id"] == "<minted>"),
        "added-doc must CREATE the doc that ships with no row — this is v4 bug 1"
    );
    assert_eq!(
        added.jobs.len(),
        1,
        "the newly synced doc must be enqueued for embedding, or it stays invisible to help_search"
    );

    // The deleted direction is load-bearing: only it can trigger this sync.
    let deleted = by_name("deleted-doc");
    assert!(
        !deleted
            .help_docs
            .iter()
            .any(|d| d["path"] == "help/retired.md"),
        "deleted-doc must PRUNE the row whose file is gone — reachable only because the \
         divergence check looks in the deleted direction too"
    );

    // in-sync must be a genuine no-op: the seeded titles/timestamps survive.
    let in_sync = by_name("in-sync");
    assert!(
        in_sync.jobs.is_empty()
            && in_sync
                .help_docs
                .iter()
                .all(|d| d["updatedAt"] == "<sentinel>"),
        "in-sync must not sync, prune, or enqueue anything"
    );
}
