//! P4.134 (dogfood #134(b)) — boot HARDNESS: a damaged table that v4 only ever
//! logs about no longer kills the v5 boot, and everything v4 lets kill its boot
//! still kills v5's.
//!
//! v4's evidence is its source (no v4 test drives `instrumentation.ts` —
//! `register()` calls `process.exit`), read at `ca363178d`:
//!
//!   - the mount-index case repairs and the link-group column run in each
//!     repository's LAZY `ensureTable`, which logs ERROR `Failed to ensure
//!     <table> table in mount index database` `{error}` and rethrows into the
//!     caller's fallback read, per access (`dedicated-db.repository.ts:134-152`);
//!   - the orphaned store-children reap (phase 3.3b) is a fallback read:
//!     ERROR `Error sweeping orphaned store children` `{collection, error}`
//!     (`doc-mount-file-links.repository.ts:1419-1435`);
//!   - the templates seed (phase 1.25) is a fallback `safeQuery`: ERROR `Error
//!     seeding built-in roleplay templates` `{collection, error}`
//!     (`roleplay-templates.repository.ts:141-191`);
//!   - `help_docs` is lazy: the backend's ERROR `Failed to ensure collection`
//!     `{table, error}`, then the repository's `Failed to ensure collection
//!     exists` `{collection, error}` (`backend.ts:763-791`,
//!     `base.repository.ts:113-124`);
//!   - every MIGRATION is fatal (`migrations/index.ts:162-205` →
//!     `instrumentation.ts:417-431`, `process.exit(1)`).
//!
//! Each arm plants damage on a COPY, boots the real `Host`, and reads the line
//! (level + target + message + fields, the bare driver message — never
//! `DbError`'s `sqlite error: ` prefix), then proves the boot CONTINUED past the
//! step, not merely that it did not crash: the three store provisions run
//! AFTER the five lazy sub-steps inside `ensure_builtin_mounts`, so a deleted
//! Lantern `tool` subfolder coming back proves the core pass went on; and the
//! general `state.json` ensure is the closure's step after the mounts, so its
//! line (re-seeded after the plant deleted it) proves the host closure went on.
//!
//! The substrate is a FRESH provisioned instance (v4's full `generateDDL`
//! shape), not the committed `post-office-*` pair the order named: that pair
//! has no `instance_settings` table (measured), so `ensure_builtin_mounts`
//! skips itself whole there and every mount arm would pass vacuously. One arm
//! runs the #134 plant on the post-office pair with `instance_settings` added
//! (a populated mount index — six vaults and a letter).
//!
//! Every boot logs from the writer and seed threads, which a thread-scoped
//! capture cannot see, so the shared `CaptureLayer` is the process-GLOBAL
//! default and the arms run one at a time behind `SERIAL`.
//!
//! Red-first (P4.134's lane record): every GUARDED arm fails at `Host::start`
//! on `main` as it stood; the FATAL arms stay red-on-softening by design.
//!
//! Run:
//!   cargo test -p quilltap-host --test host_boot_hardness

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use quilltap_core::api::{QuilltapCore as _, Request, Response};
use quilltap_core::db::Writer;
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_core::test_support::CaptureLayer;
use quilltap_host::{Host, HostConfig};
use tracing_subscriber::layer::SubscriberExt;

/// The committed fixtures' synthetic test pepper.
const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const MAIN: &str = "quilltap.db";
const MOUNT: &str = "quilltap-mount-index.db";

/// Arms boot one at a time: the capture is process-global.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn capture() -> &'static Arc<Mutex<Vec<String>>> {
    static LOGS: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();
    LOGS.get_or_init(|| {
        let logs = Arc::new(Mutex::new(Vec::new()));
        tracing::subscriber::set_global_default(
            tracing_subscriber::registry().with(CaptureLayer(logs.clone())),
        )
        .expect("this binary owns the global subscriber");
        logs
    })
}

fn web_fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

enum Substrate {
    /// `provision_fresh_instance` — v4's full `generateDDL` shape, the three
    /// built-in stores provisioned.
    Fresh,
    /// The committed `post-office-*` pair + an `instance_settings` table (it
    /// has none), so the built-in mount pass runs over a populated index.
    PostOffice,
}

fn config(base: &Path) -> HostConfig {
    let mut config = HostConfig::new(base);
    config.instances_path = Some(base.join("instances.json"));
    config.env_pepper = Some(PEPPER.to_string());
    config.autonomous_tick_ms = 3_600_000;
    config.stuck_check_ms = 3_600_000;
    config.terminal = false;
    config.seed_sample_content = false;
    config
}

/// What one planted boot left behind.
struct Booted {
    _dir: tempfile::TempDir,
    data: PathBuf,
    result: Result<Host, String>,
    lines: Vec<String>,
}

/// Build the substrate in a tempdir, run each `(partition, sql)` plant on the
/// COPY in order, then boot the real `Host` with the capture cleared.
async fn boot_planted(substrate: Substrate, plants: &[(&str, &str)]) -> Booted {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    match substrate {
        Substrate::Fresh => provision_fresh_instance(&data, PEPPER).expect("provision"),
        Substrate::PostOffice => {
            std::fs::copy(web_fixtures().join("post-office-main.db"), data.join(MAIN)).unwrap();
            std::fs::copy(
                web_fixtures().join("post-office-mount.db"),
                data.join(MOUNT),
            )
            .unwrap();
            let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
            w.connection()
                .execute_batch(
                    "CREATE TABLE instance_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
                )
                .unwrap();
        }
    }
    for (partition, sql) in plants {
        let w = Writer::open_writable(&data.join(partition), PEPPER).unwrap();
        w.connection()
            .execute_batch(sql)
            .unwrap_or_else(|e| panic!("plant {sql:?}: {e}"));
    }
    capture().lock().unwrap().clear();
    let result = Host::start(config(dir.path())).map_err(|e| e.to_string());
    let lines = capture().lock().unwrap().clone();
    if let Ok(host) = &result {
        match host.core().dispatch(Request::Health).await {
            Response::Health(h) => assert!(h.ready, "the boot finished but is not ready"),
            other => panic!("unexpected: {other:?}"),
        }
    }
    Booted {
        _dir: dir,
        data,
        result,
        lines,
    }
}

impl Booted {
    fn host(&self) -> &Host {
        match &self.result {
            Ok(host) => host,
            Err(e) => panic!(
                "the boot FAILED — this step must not kill it: {e}\n{}",
                self.lines.join("\n")
            ),
        }
    }

    fn boot_error(&self) -> &str {
        match &self.result {
            Ok(_) => panic!(
                "the boot SUCCEEDED — this step is v4-fatal and must stay fatal\n{}",
                self.lines.join("\n")
            ),
            Err(e) => e,
        }
    }

    /// Exactly one captured line equal to `line`.
    fn assert_line(&self, line: &str) {
        let hits = self.lines.iter().filter(|l| l.as_str() == line).count();
        assert_eq!(
            hits,
            1,
            "expected exactly one line {line:?}; captured:\n{}",
            self.lines.join("\n")
        );
    }

    /// No captured line contains `needle`.
    fn assert_silent(&self, needle: &str) {
        let hits: Vec<&String> = self.lines.iter().filter(|l| l.contains(needle)).collect();
        assert!(hits.is_empty(), "unexpected {needle:?} line(s): {hits:?}");
    }

    /// The Lantern store's `tool` subfolder exists — the store provisions
    /// (`ensure_one_mount`) ran AFTER the guarded sub-steps.
    fn assert_tool_folder_restored(&self) {
        let _ = self.host();
        let w = Writer::open_writable(&self.data.join(MOUNT), PEPPER).unwrap();
        let n: i64 = w
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM doc_mount_folders WHERE path = 'tool'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            n, 1,
            "the store provisions after the guarded sub-step did not run"
        );
    }
}

/// Delete the Lantern `tool` subfolder (the store provisions re-create it) and
/// the general `state.json` link (the closure's next step re-seeds it) — the two
/// "the boot went on" markers. Run BEFORE the damaging plant.
const DELETE_MARKERS: &str = "DELETE FROM doc_mount_folders WHERE path = 'tool';\
     DELETE FROM doc_mount_file_links WHERE relativePath = 'state.json';";

/// v4 `instrumentation.ts:787-790` — the INFO the re-seeded `state.json`
/// logs, with v4's `context` field.
const STATE_SEEDED: &str = "INFO quilltap::boot Seeded general state.json in the Quilltap General mount context=instrumentation.register";

/// Every line a guarded step logs — none may appear on a healthy boot.
const GUARDED_MESSAGES: &[&str] = &[
    "Failed to ensure doc_mount_",
    "Error sweeping orphaned store children",
    "Error seeding built-in roleplay templates",
    "Failed to ensure collection",
    "[GeneralScenarios] Failed to ensure Scenarios folder",
    "Error ensuring general scenarios folder",
    "Error ensuring general state.json",
    "Embedding dimension reconciliation failed",
];

/// The silence leg: a healthy fresh instance logs none of the guarded lines,
/// and both markers come back (the probes themselves work).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_unplanted_boot_logs_none_of_the_guarded_lines() {
    let _serial = SERIAL.lock().await;
    let booted = boot_planted(Substrate::Fresh, &[(MOUNT, DELETE_MARKERS)]).await;
    booted.host();
    for message in GUARDED_MESSAGES {
        booted.assert_silent(message);
    }
    booted.assert_line(STATE_SEEDED);
    booted.assert_tool_folder_restored();
}

/// 25c — the #134 failure itself: the file-links case repair's SELECT names
/// `relativePath`. v4's links-repository `ensureTable` line; the boot goes on,
/// the provisions after it run, and the `state.json` ensure (which reads links
/// too) logs v4's own guarded WARN — with the bare message and v4's `context`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_renamed_link_column_logs_v4s_ensure_table_line_and_boots() {
    let _serial = SERIAL.lock().await;
    let booted = boot_planted(
        Substrate::Fresh,
        &[
            (MOUNT, DELETE_MARKERS),
            (
                MOUNT,
                "ALTER TABLE doc_mount_file_links RENAME COLUMN relativePath TO relativePath_x",
            ),
        ],
    )
    .await;
    booted.host();
    booted.assert_line("ERROR quilltap::db Failed to ensure doc_mount_file_links table in mount index database error=no such column: relativePath");
    booted.assert_tool_folder_restored();
    booted.assert_line("WARN quilltap::boot Error ensuring general state.json, continuing startup context=instrumentation.register error=no such column: l.relativePath");
}

/// 25c on the POST-OFFICE pair — a populated mount index (six vaults, a
/// letter), the substrate P4.131's mail plants run on and dogfood #134's
/// live row shares.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_134_plant_on_the_post_office_pair_boots() {
    let _serial = SERIAL.lock().await;
    let booted = boot_planted(
        Substrate::PostOffice,
        &[(
            MOUNT,
            "ALTER TABLE doc_mount_file_links RENAME COLUMN relativePath TO relativePath_x",
        )],
    )
    .await;
    booted.host();
    booted.assert_line("ERROR quilltap::db Failed to ensure doc_mount_file_links table in mount index database error=no such column: relativePath");
    booted.assert_line("WARN quilltap::boot Error ensuring general state.json, continuing startup context=instrumentation.register error=no such column: l.relativePath");
}

/// 25b — the folder case repair. `name`, not `path`: `path` also breaks the
/// store provisions' `ensure_folder_path`, which stays fatal (the arm below).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_renamed_folder_name_logs_v4s_ensure_table_line_and_boots() {
    let _serial = SERIAL.lock().await;
    // `tool` is NOT deleted here: re-creating it would INSERT a `name`.
    let booted = boot_planted(
        Substrate::Fresh,
        &[
            (
                MOUNT,
                "DELETE FROM doc_mount_file_links WHERE relativePath = 'state.json';\
                 INSERT INTO doc_mount_folders \
                   (id, mountPointId, parentId, name, path, createdAt, updatedAt) \
                 VALUES ('orphan', 'vanished', NULL, 'o', 'o', \
                         '2024-01-01T00:00:00.000Z', '2024-01-01T00:00:00.000Z');",
            ),
            (
                MOUNT,
                "ALTER TABLE doc_mount_folders RENAME COLUMN name TO name_x",
            ),
        ],
    )
    .await;
    booted.host();
    booted.assert_line("ERROR quilltap::db Failed to ensure doc_mount_folders table in mount index database error=no such column: name");
    // The FIRST lazy sub-step failed; the reap, the LAST, still ran — the
    // orphaned folder planted under a vanished store is gone.
    let w = Writer::open_writable(&booted.data.join(MOUNT), PEPPER).unwrap();
    let orphans: i64 = w
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM doc_mount_folders WHERE mountPointId = 'vanished'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(orphans, 0, "the sub-steps after the failed one did not run");
    booted.assert_line(STATE_SEEDED);
}

/// 25d — the mount-point name repair.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_renamed_mount_point_name_logs_v4s_ensure_table_line_and_boots() {
    let _serial = SERIAL.lock().await;
    let booted = boot_planted(
        Substrate::Fresh,
        &[
            (MOUNT, DELETE_MARKERS),
            (
                MOUNT,
                "ALTER TABLE doc_mount_points RENAME COLUMN name TO name_x",
            ),
        ],
    )
    .await;
    booted.host();
    booted.assert_line("ERROR quilltap::db Failed to ensure doc_mount_points table in mount index database error=no such column: name");
    booted.assert_tool_folder_restored();
    booted.assert_line(STATE_SEEDED);
}

/// 25e — the link-group column. A renamed column self-heals (the ensure ADDs
/// it back), so the plant blocks its partial index instead: a TABLE holding
/// the index's name fails `CREATE INDEX IF NOT EXISTS` outright.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_blocked_link_group_index_logs_v4s_ensure_table_line_and_boots() {
    let _serial = SERIAL.lock().await;
    let booted = boot_planted(
        Substrate::Fresh,
        &[
            (MOUNT, DELETE_MARKERS),
            (
                MOUNT,
                "DROP INDEX idx_doc_mount_file_links_linkGroupId;\
                 CREATE TABLE idx_doc_mount_file_links_linkGroupId (x TEXT);",
            ),
        ],
    )
    .await;
    booted.host();
    booted.assert_line("ERROR quilltap::db Failed to ensure doc_mount_file_links table in mount index database error=there is already a table named idx_doc_mount_file_links_linkGroupId");
    booted.assert_tool_folder_restored();
    booted.assert_line(STATE_SEEDED);
}

/// 25g — the orphaned store-children reap (v4 phase 3.3b, a fallback read).
/// `doc_mount_chunks.mountPointId` is read by the reap and nothing else at boot.
/// v4's reaper deletes links / folders / DOCUMENTS and never reads chunks, so
/// under this plant v4 is SILENT: the arm proves v5's line SHAPE for a reaper
/// failure (transcribed from `doc-mount-file-links.repository.ts:1419-1435` +
/// `withRawDb`'s fallback mode), not a plant v4 reproduces.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_renamed_chunk_column_logs_v4s_reap_fallback_line_and_boots() {
    let _serial = SERIAL.lock().await;
    let booted = boot_planted(
        Substrate::Fresh,
        &[
            (MOUNT, DELETE_MARKERS),
            (
                MOUNT,
                "ALTER TABLE doc_mount_chunks RENAME COLUMN mountPointId TO mountPointId_x",
            ),
        ],
    )
    .await;
    booted.host();
    booted.assert_line("ERROR quilltap::db Error sweeping orphaned store children collection=doc_mount_file_links error=no such column: c.mountPointId");
    booted.assert_silent("Error reaping orphaned doc-store children");
    booted.assert_tool_folder_restored();
    booted.assert_line(STATE_SEEDED);
}

/// #1 — the built-in roleplay templates seed (v4 phase 1.25, a fallback
/// `safeQuery`; `seed-initial-data.ts`'s own ERROR is unreachable behind it).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_renamed_template_column_logs_v4s_seed_line_and_boots() {
    let _serial = SERIAL.lock().await;
    let booted = boot_planted(
        Substrate::Fresh,
        &[
            (MOUNT, DELETE_MARKERS),
            (
                MAIN,
                "ALTER TABLE roleplay_templates RENAME COLUMN name TO name_x",
            ),
        ],
    )
    .await;
    booted.host();
    booted.assert_line("ERROR quilltap::db Error seeding built-in roleplay templates collection=roleplay_templates error=no such column: name");
    booted.assert_silent("Failed to seed built-in roleplay templates");
    booted.assert_tool_folder_restored();
    booted.assert_line(STATE_SEEDED);
}

/// #17 — the `help_docs` ensure (lazy in v4). A VIEW holding the name passes
/// `CREATE TABLE IF NOT EXISTS` and fails the index: v4's backend + repository
/// pair of lines, in order; the help reconcile then logs its own guarded lines.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_help_docs_view_logs_v4s_ensure_collection_pair_and_boots() {
    let _serial = SERIAL.lock().await;
    let booted = boot_planted(
        Substrate::Fresh,
        &[
            (MOUNT, DELETE_MARKERS),
            (
                MAIN,
                "DROP INDEX IF EXISTS idx_help_docs_createdAt;\
                 ALTER TABLE help_docs RENAME TO help_docs_x;\
                 CREATE VIEW help_docs AS SELECT * FROM help_docs_x;",
            ),
        ],
    )
    .await;
    booted.host();
    let backend =
        "ERROR quilltap::db Failed to ensure collection table=help_docs error=views may not be indexed";
    let repository = "ERROR quilltap::db Failed to ensure collection exists collection=help_docs error=views may not be indexed";
    booted.assert_line(backend);
    booted.assert_line(repository);
    let at = |line: &str| booted.lines.iter().position(|l| l == line).unwrap();
    assert!(
        at(backend) < at(repository),
        "v4 logs the backend line first"
    );
    booted.assert_tool_folder_restored();
    booted.assert_line(STATE_SEEDED);
}

/// FATAL class, host level — `help_doc_chunks` is v4's migration
/// `create-help-doc-chunks-table-v1`; a failed migration exits v4 (on an
/// instance whose ledger lacks it — a ledger-complete instance skips it before
/// `shouldRun`, `migrations/index.ts:125-129`; v5's ensures run every boot, the
/// ledger-gate divergence P4.134 named). The boot must still FAIL (nothing
/// over-softened).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_migration_counterpart_still_fails_the_boot() {
    let _serial = SERIAL.lock().await;
    let booted = boot_planted(
        Substrate::Fresh,
        &[(
            MAIN,
            "ALTER TABLE help_doc_chunks RENAME TO help_doc_chunks_x;\
             CREATE VIEW help_doc_chunks AS SELECT * FROM help_doc_chunks_x;",
        )],
    )
    .await;
    assert_eq!(
        booted.boot_error(),
        "engine assembly failed: built-in seed failed: sqlite error: views may not be indexed"
    );
}

/// FATAL class, core level — the store provisions (v4's provisioning
/// migrations) stay fatal even though the folder case repair before them is
/// guarded: `path` breaks both, the repair logs and the provision kills the
/// boot. Wrapping `ensure_builtin_mounts` whole in one guard reds this arm.
/// v4-fatal only where its ledger lacks the provisioning migration; a
/// ledger-complete v4 instance reaches `doc_mount_folders` lazily and boots
/// (v5 HARDER — the ledger-gate divergence, recorded).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_store_provision_still_fails_the_boot() {
    let _serial = SERIAL.lock().await;
    let booted = boot_planted(
        Substrate::Fresh,
        &[
            (MOUNT, DELETE_MARKERS),
            (
                MOUNT,
                "ALTER TABLE doc_mount_folders RENAME COLUMN path TO path_x",
            ),
        ],
    )
    .await;
    let error = booted.boot_error().to_string();
    assert!(
        error.starts_with("engine assembly failed: built-in seed failed: sqlite error: "),
        "{error}"
    );
    booted.assert_line("ERROR quilltap::db Failed to ensure doc_mount_folders table in mount index database error=no such column: path");
}

/// FATAL class, core level — the mount-index DDL (v4's `ensureMountIndexTables`
/// inside the provisioning migrations) stays fatal. As above: on a
/// ledger-complete v4 instance the migration is skipped and a same-named VIEW
/// only fails the lazy index DDL per access (`views may not be indexed`), so
/// v4 boots where v5 does not (the ledger-gate divergence, recorded).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_mount_index_ddl_still_fails_the_boot() {
    let _serial = SERIAL.lock().await;
    let booted = boot_planted(
        Substrate::Fresh,
        &[(
            MOUNT,
            "ALTER TABLE doc_mount_folders RENAME TO doc_mount_folders_x;\
             CREATE VIEW doc_mount_folders AS SELECT * FROM doc_mount_folders_x;",
        )],
    )
    .await;
    assert_eq!(
        booted.boot_error(),
        "engine assembly failed: built-in seed failed: sqlite error: views may not be indexed"
    );
}
