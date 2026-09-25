//! P4.d6 → P4.D222 differential — v4 `ensureHelpDocsSynced` (since `492771aff`
//! the once-per-process `reconcileHelpDocs`: a full content-hash sync, the
//! section backfill for docs with no chunk rows, the incomplete rule, ONE
//! enqueue of the incomplete docs) vs `quilltap_core::services::help_doc_sync::
//! HelpDocReconcileGate::ensure`.
//!
//! Both sides copy the SAME per-scenario fixture DB (v4-built, byte-identical
//! seeds), walk the SAME committed help tree (`harness/oracle/fixtures/
//! help-ensure/help/`, the Rust side through the production host walker), run
//! the real reconcile, and diff the resulting `help_docs` rows,
//! `help_doc_chunks` counts, `background_jobs`, and the reconcile's two log
//! lines (`reconciled` / `reconcileFailed` counts).
//!
//! Twelve scenarios share ONE tree and vary only the seed (the first six are
//! P4.d6/P4.D77's, re-recorded at `b0b6656b5` — four of them MOVED under
//! `492771aff`; the last six are P4.D222's planted mirrors of v4's
//! `help-doc-sync.reconcile.test.ts` vectors):
//!   - `empty-table`      — the control: every file created, sliced, enqueued;
//!   - `in-sync`          — disk and DB agree, NO chunk rows → the backfill
//!     slices both docs and enqueues both (each is incomplete: zero embedded
//!     sections);
//!   - `in-sync-chunked`  — as `in-sync` with one chunk row seeded: the seeded
//!     doc is NOT re-sliced (its count is > 0) but IS enqueued (its one section
//!     has no vector), the other doc is sliced and enqueued;
//!   - `added-doc`        — a shipped doc with no row is created (v4 bug 1's
//!     shape, now unconditional);
//!   - `deleted-doc`      — the row whose file is gone is pruned; the rest
//!     backfilled and enqueued;
//!   - `no-profile`       — the sync + backfill complete; the enqueue backs off
//!     (no embedding profile);
//!   - `edited-page`      — a changed hash updates the row, clears its vector
//!     and status, re-slices, enqueues;
//!   - `complete`         — every doc and section vector present → nothing
//!     enqueued;
//!   - `null-doc-vector`  — sections embedded, the doc vector NULL → enqueued;
//!   - `partial-sections` — `{total 4, embedded 3}` → enqueued, NOT re-sliced;
//!   - `concurrent-then-later` — two racing callers + one later: ONE reconcile;
//!   - `fail-once-then-retry` — a planted `RAISE(ABORT)` trigger fails the
//!     first reconcile (WARN, no throw), the next caller retries and succeeds.
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
//!   for s in empty-table in-sync in-sync-chunked added-doc deleted-doc no-profile \
//!            edited-page complete null-doc-vector partial-sections \
//!            concurrent-then-later fail-once-then-retry; do
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
use quilltap_core::services::help_doc_sync::HelpDocReconcileGate;

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
    /// P4.D77 — pre-existing section chunks (the backfill's short-circuit arm).
    #[serde(rename = "seedChunks", default)]
    seed_chunks: Vec<SpecSeedDoc>,
    /// P4.D222 — the multi-call gate arms; `None` = one call.
    #[serde(default)]
    calls: Option<String>,
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
    /// P4.D77 — the section chunks after the run: the BACKFILL's output on the
    /// early-return path, the sync's on the diverged one.
    chunks: Vec<Value>,
    /// P4.D222 — `{reconciled, reconcileFailed}`: the reconcile's INFO and the
    /// gate's WARN, counted.
    logs: Value,
}

/// P4.D222 — the fail-once-then-retry plant, byte-identical to the oracle's.
const FAIL_TRIGGER_CREATE: &str = "CREATE TRIGGER \"p4d222_fail_section_insert\" BEFORE INSERT ON \"help_doc_chunks\" BEGIN SELECT RAISE(ABORT, 'planted reconcile failure'); END";
const FAIL_TRIGGER_DROP: &str = "DROP TRIGGER \"p4d222_fail_section_insert\"";

async fn plant(db: &Db, sql: &'static str) {
    db.write(move |ws| {
        ws.main()
            .connection()
            .execute_batch(sql)
            .map_err(quilltap_core::db::DbError::from)
    })
    .await
    .expect("plant");
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
                "SKIP: help_doc_ensure_matches_oracle: set QT_ORACLE_HELP_ENSURE + \
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
        let seeded_chunk_ids: Vec<&str> = def.seed_chunks.iter().map(|c| c.id.as_str()).collect();

        let tmp = tempfile::tempdir().expect("tempdir");
        let work_main = tmp.path().join("ensure-main.db");
        let fixture_main =
            std::path::Path::new(&fixture_dir).join(format!("qt-ensure-{}-main.db", def.name));
        std::fs::copy(&fixture_main, &work_main)
            .unwrap_or_else(|e| panic!("copy fixture {}: {e}", fixture_main.display()));

        let db = Db::open_main(&work_main, &spec.test_pepper_base64).expect("open db");
        let files = quilltap_host::files_store::load_help_source_files(&tree_root);

        // One gate per scenario — v4's once-per-process memo, owned.
        let gate = HelpDocReconcileGate::new();
        let ((), lines) = quilltap_core::test_support::captured_with(|| {
            runtime.block_on(async {
                match def.calls.as_deref() {
                    Some("concurrent-then-later") => {
                        tokio::join!(gate.ensure(&db, &files), gate.ensure(&db, &files));
                        gate.ensure(&db, &files).await;
                    }
                    Some("fail-once-then-retry") => {
                        plant(&db, FAIL_TRIGGER_CREATE).await;
                        gate.ensure(&db, &files).await;
                        plant(&db, FAIL_TRIGGER_DROP).await;
                        gate.ensure(&db, &files).await;
                    }
                    None => {
                        gate.ensure(&db, &files).await;
                    }
                    Some(other) => panic!("unknown calls arm {other}"),
                }
            })
        });
        let count_lines = |needle: &str| lines.iter().filter(|l| l.contains(needle)).count();
        let logs = json!({
            "reconciled": count_lines("[HelpDocSync] Help docs reconciled"),
            "reconcileFailed": count_lines("[HelpDocSync] Help doc reconcile failed"),
        });

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

        // ---- dump help_doc_chunks (P4.D77; same normalization as the oracle) ----
        #[allow(clippy::type_complexity)]
        let chunk_rows: Vec<(String, String, f64, Option<String>, String, String, String)> = db
            .read_main(|c| {
                let mut stmt = c
                    .prepare(
                        "SELECT id, docId, chunkIndex, heading, content, \
                         hex(embedding) AS embeddingHex, updatedAt FROM help_doc_chunks",
                    )
                    .map_err(quilltap_core::db::DbError::from)?;
                let mapped = stmt
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, f64>(2)?,
                            r.get::<_, Option<String>>(3)?,
                            r.get::<_, String>(4)?,
                            r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                            r.get::<_, String>(6)?,
                        ))
                    })
                    .map_err(quilltap_core::db::DbError::from)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(quilltap_core::db::DbError::from)?;
                Ok(mapped)
            })
            .expect("dump help_doc_chunks");

        let mut chunks: Vec<Value> = chunk_rows
            .iter()
            .map(|(id, doc_id, idx, heading, content, emb_hex, updated_at)| {
                json!({
                    "id": if seeded_chunk_ids.contains(&id.as_str()) { id.as_str() } else { "<minted>" },
                    "docPath": path_by_id
                        .get(doc_id.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("<unknown:{doc_id}>")),
                    "chunkIndex": js_number(*idx),
                    "heading": heading,
                    "content": content,
                    "hasEmbedding": !emb_hex.is_empty(),
                    "updatedAt": if updated_at == &spec.seed_sentinel { "<sentinel>" } else { "<ts>" },
                })
            })
            .collect();
        chunks.sort_by(|a, b| {
            a["docPath"]
                .as_str()
                .unwrap_or_default()
                .cmp(b["docPath"].as_str().unwrap_or_default())
                .then(
                    a["chunkIndex"]
                        .as_i64()
                        .unwrap_or_default()
                        .cmp(&b["chunkIndex"].as_i64().unwrap_or_default()),
                )
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

        assert_eq!(
            Value::Array(chunks.clone()),
            Value::Array(expected.chunks.clone()),
            "[{}] help_doc_chunks diverged\nrust:   {:?}\noracle: {:?}",
            def.name,
            chunks,
            expected.chunks
        );

        assert_eq!(
            logs, expected.logs,
            "[{}] the reconcile/gate log counts diverged",
            def.name
        );

        eprintln!(
            "ensure {}: {} help_docs, {} job(s), {} chunk(s)",
            def.name,
            help_docs.len(),
            jobs.len(),
            chunks.len()
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
    // P4.D222: the unchanged aurora is section-less, so it is backfilled and
    // queued too (it was the created doc alone before the reconcile).
    assert_eq!(
        added.jobs.len(),
        2,
        "the created doc AND the section-less unchanged one must both be queued"
    );

    // The prune still runs (now on every reconcile, not only on a divergence).
    let deleted = by_name("deleted-doc");
    assert!(
        !deleted
            .help_docs
            .iter()
            .any(|d| d["path"] == "help/retired.md"),
        "deleted-doc must PRUNE the row whose file is gone"
    );

    // in-sync must not rewrite a doc: the seeded titles/timestamps survive.
    let in_sync = by_name("in-sync");
    assert!(
        in_sync
            .help_docs
            .iter()
            .all(|d| d["updatedAt"] == "<sentinel>"),
        "in-sync must not rewrite or prune anything"
    );
    // The section-less docs are sliced and queued EVEN THOUGH both carry their
    // own vector — the job is what fills the SECTION vectors.
    assert_eq!(in_sync.chunks.len(), 2, "in-sync must backfill both docs");
    assert!(in_sync.chunks.iter().all(|c| c["hasEmbedding"] == false));
    assert_eq!(in_sync.jobs.len(), 2);
    assert!(in_sync.help_docs.iter().all(|d| d["hasEmbedding"] == true));

    // ==== P4.D222 — the reconcile (v4 `492771aff`) ====
    // in-sync-chunked MOVED: the old `count() > 0` gate skipped the whole
    // table; the reconcile looks per doc, so brahma (no rows) is backfilled and
    // queued while aurora (complete) is left alone.
    let chunked = by_name("in-sync-chunked");
    assert!(
        chunked
            .chunks
            .iter()
            .any(|c| c["docPath"] == "help/aurora.md"
                && c["id"] != "<minted>"
                && c["updatedAt"] == "<sentinel>"),
        "aurora's seeded section must survive untouched"
    );
    assert!(chunked
        .chunks
        .iter()
        .any(|c| c["docPath"] == "help/brahma-console.md" && c["id"] == "<minted>"));
    assert_eq!(chunked.jobs.len(), 1);
    assert_eq!(chunked.jobs[0]["entityPath"], "help/brahma-console.md");

    let edited = by_name("edited-page");
    assert!(
        edited
            .help_docs
            .iter()
            .any(|d| d["path"] == "help/aurora.md"
                && d["id"] != "<minted>"
                && d["hasEmbedding"] == false
                && d["updatedAt"] == "<ts>"),
        "the edited page is rewritten in place (its own id) and its vector cleared"
    );
    assert_eq!(edited.jobs.len(), 1);
    assert_eq!(edited.jobs[0]["entityPath"], "help/aurora.md");

    let complete = by_name("complete");
    assert!(
        complete.jobs.is_empty(),
        "a complete index enqueues nothing"
    );
    assert!(complete.chunks.iter().all(|c| c["id"] != "<minted>"));

    let null_vec = by_name("null-doc-vector");
    assert_eq!(null_vec.jobs.len(), 1);
    assert_eq!(null_vec.jobs[0]["entityPath"], "help/brahma-console.md");

    let partial = by_name("partial-sections");
    assert_eq!(partial.jobs.len(), 1);
    assert_eq!(partial.jobs[0]["entityPath"], "help/aurora.md");
    assert!(
        partial.chunks.iter().all(|c| c["id"] != "<minted>"),
        "a partial doc is queued, NOT re-sliced"
    );

    let no_profile = by_name("no-profile");
    assert!(no_profile.jobs.is_empty());
    assert!(
        !no_profile.chunks.is_empty(),
        "the reconcile still slices with no profile"
    );

    let concurrent = by_name("concurrent-then-later");
    assert_eq!(
        concurrent.logs["reconciled"], 1,
        "the reconcile runs ONCE across three calls"
    );
    assert_eq!(concurrent.help_docs.len(), 2);

    let retry = by_name("fail-once-then-retry");
    assert_eq!(
        retry.logs["reconcileFailed"], 1,
        "the planted failure is logged once"
    );
    assert_eq!(
        retry.logs["reconciled"], 1,
        "the next call ran a FRESH reconcile"
    );
    assert_eq!(retry.chunks.len(), 2, "…and the retry backfilled both docs");
}
