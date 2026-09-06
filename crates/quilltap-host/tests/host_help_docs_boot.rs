//! P4.9I2A — the boot-time help-docs sync + the embedded-table reindex, wired
//! into the host assembly.
//!
//! Three pins over a REAL fresh-provisioned instance:
//!
//!   1. **The boot ensure runs.** `Host::start` alone leaves `help_docs` at the
//!      shipped tree's 120 rows with their section chunks — the
//!      `ensure_help_docs_synced` call in `assemble` (v4's LAZY
//!      `HelpSearch.loadFromDatabase()` path, run EAGERLY here). Removing that
//!      call reads 0 (the lane record's mutation).
//!   2. **A second boot writes nothing.** The `contentHash` short-circuit: every
//!      row keeps its `updatedAt`, and the count stays 120.
//!   3. **`EMBEDDING_REINDEX_ALL` re-syncs from the EMBEDDED table.** With the
//!      table emptied by hand, one reindex-all job pumped through the host's
//!      registry restores all 120 rows. Tests run with cwd = the crate dir,
//!      which has NO `help/` — so restoring the retired `current_dir()` walk in
//!      the registration reads an empty tree and leaves 0 rows (the lane
//!      record's second mutation).
//!
//! Run:
//!   cargo test -p quilltap-host --test host_help_docs_boot

use std::path::Path;
use std::time::Duration;

use quilltap_core::api::QuilltapCore as _;
use quilltap_core::api::{PepperState, Request, Response};
use quilltap_core::db::runtime::Db;
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_core::services::queue_service;
use quilltap_host::help_content::embedded_help_count;
use quilltap_host::{Host, HostConfig};

const PEPPER: &str = "3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=";

fn hermetic_config(base: &Path) -> HostConfig {
    let mut config = HostConfig::new(base);
    config.instances_path = Some(base.join("instances.json"));
    config.env_pepper = Some(PEPPER.to_string());
    config.autonomous_tick_ms = 3_600_000;
    config.stuck_check_ms = 3_600_000;
    config.terminal = false;
    config.seed_sample_content = false;
    config
}

fn count(db: &Db, sql: &'static str) -> i64 {
    db.read_main(|c| Ok(c.query_row(sql, [], |r| r.get::<_, i64>(0))?))
        .unwrap()
}

fn updated_at_by_path(db: &Db) -> Vec<(String, String)> {
    db.read_main(|c| {
        let mut stmt = c.prepare("SELECT path, updatedAt FROM help_docs ORDER BY path")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .unwrap()
}

async fn wait_for_status(db: &Db, job_id: &str, want: &str) {
    for _ in 0..600 {
        let status = queue_service::get_job_status(db, job_id).await.unwrap();
        if status.as_ref().map(|j| j.status.as_str()) == Some(want) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("job {job_id} never reached {want}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn boot_syncs_the_embedded_help_tree_and_reindex_reads_it() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");

    let expected = embedded_help_count() as i64;
    assert_eq!(expected, 120, "the vendored tree at v4 d883a5ee1");

    // ── 1. The boot ensure. ──
    let host = Host::start(hermetic_config(dir.path())).unwrap();
    let core = host.core();
    match core.dispatch(Request::Health).await {
        Response::Health(h) => {
            assert!(h.ready);
            assert_eq!(h.pepper_state, PepperState::NeedsVaultStorage);
        }
        other => panic!("unexpected: {other:?}"),
    }
    let db = core.db().expect("engine ready");
    assert_eq!(
        count(&db, "SELECT count(*) FROM help_docs"),
        expected,
        "the boot ensure must sync the whole embedded tree"
    );
    assert!(
        count(&db, "SELECT count(*) FROM help_doc_chunks") > 0,
        "the sync slices section chunks"
    );
    // The sync enqueued one HELP_DOC embedding job per doc (the provisioned
    // builtin profile is the default). Let the pump drain them so the second
    // boot's `background_jobs` is quiet.
    let before = updated_at_by_path(&db);
    drop(db);
    drop(host);

    // ── 2. A second boot writes nothing (the contentHash short-circuit). ──
    let host = Host::start(hermetic_config(dir.path())).unwrap();
    let db = host.core().db().expect("engine ready");
    assert_eq!(count(&db, "SELECT count(*) FROM help_docs"), expected);
    assert_eq!(
        updated_at_by_path(&db),
        before,
        "an unchanged tree must not touch a single row"
    );

    // ── 3. EMBEDDING_REINDEX_ALL re-syncs from the EMBEDDED table. ──
    // Empty the table by hand (chunks first — the FK-less schema needs no
    // order, but be explicit), then run one full-scope reindex through the
    // host's registered handler.
    db.write(|ws| {
        let c = ws.main().connection();
        c.execute_batch("DELETE FROM help_doc_chunks; DELETE FROM help_docs;")?;
        Ok(())
    })
    .await
    .unwrap();
    assert_eq!(count(&db, "SELECT count(*) FROM help_docs"), 0);
    let (user_id, profile_id) = db
        .read_main(|c| {
            let u: String = c.query_row("SELECT id FROM users LIMIT 1", [], |r| r.get(0))?;
            let p: String = c.query_row(
                "SELECT id FROM embedding_profiles WHERE isDefault = 1 LIMIT 1",
                [],
                |r| r.get(0),
            )?;
            Ok((u, p))
        })
        .unwrap();
    let job_id = queue_service::enqueue_job(
        &db,
        &user_id,
        "EMBEDDING_REINDEX_ALL",
        serde_json::json!({ "profileId": profile_id, "scope": "all" }),
        3.0,
    )
    .await
    .unwrap();
    wait_for_status(&db, &job_id, "COMPLETED").await;
    assert_eq!(
        count(&db, "SELECT count(*) FROM help_docs"),
        expected,
        "reindex-all must re-sync every embedded doc (a cwd walk from the crate dir finds none)"
    );
    assert!(count(&db, "SELECT count(*) FROM help_doc_chunks") > 0);
}

/// The `p4.9i2` unification's catch: an instance WITHOUT a `help_docs` table
/// (v4 creates the collection lazily on the first help read; the e2e `salon-*`
/// fixture is such an instance — 18 tables, none of them help) must get the
/// table at boot and then the full sync — not a `no such table` warn, an empty
/// Guide and a dead Ask tab. Mutation: remove the `ensure_help_docs_table` call
/// in `host.rs`'s boot repairs → the second boot reads 0 (or fails the read).
#[tokio::test]
async fn boot_creates_a_missing_help_docs_table_before_syncing() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    let expected = embedded_help_count() as i64;

    // Boot once, then DROP both help tables — the pre-help_docs vintage.
    let host = Host::start(hermetic_config(dir.path())).unwrap();
    let db = host.core().db().expect("engine ready");
    assert_eq!(count(&db, "SELECT count(*) FROM help_docs"), expected);
    db.write(|ws| {
        let c = ws.main().connection();
        c.execute_batch("DROP TABLE help_doc_chunks; DROP TABLE help_docs;")?;
        Ok(())
    })
    .await
    .unwrap();
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name LIKE 'help_%'"
        ),
        0
    );
    drop(db);
    drop(host);

    // The next boot must recreate the table AND sync the whole tree into it.
    let host = Host::start(hermetic_config(dir.path())).unwrap();
    let db = host.core().db().expect("engine ready");
    assert_eq!(
        count(&db, "SELECT count(*) FROM help_docs"),
        expected,
        "a missing help_docs table must be created at boot, then synced"
    );
    assert!(count(&db, "SELECT count(*) FROM help_doc_chunks") > 0);
}
