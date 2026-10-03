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
//!   - every MIGRATION is fatal (`migrations/index.ts:189-244` →
//!     `instrumentation.ts:419-429`, `process.exit(1)`) — EXCEPT a
//!     `resumable` one, which since `e5c6bd0c0` (v4 bug 175, this port's own
//!     filing) DEFERS to the next boot with no ledger row and lets the boot
//!     continue (`deferResumable`, `:73-81`); the avatar-roll collapse is the
//!     only one marked, and P4.135's FATAL ruling for it is overtaken — but
//!     only on an instance whose ledger lacks the migration: v4 skips a
//!     ledgered one before its `shouldRun`, where v5 re-runs its structural
//!     ensures every boot (the ledger-gate divergence, ruled KEPT; v4 bug 176,
//!     whose `e5c6bd0c0` fix adds a read-only boot pass over v4's eleven
//!     structural tables and a `structure` service in `/api/health` — so a
//!     damaged table now boots LOUD and reachable on v4, ported by P4.D248).
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
//! P4.135's two collapse arms panicked "the boot SUCCEEDED" on `main` as it
//! stood (the soft guard); its cadence arm pins behaviour `main` already had.
//! P4.D248 rewrote those two collapse arms to v4's `e5c6bd0c0` defer-and-boot
//! (and added the ledger-write arm): all three panicked "the boot FAILED" on
//! `main`'s `host.rs` as it stood.
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
    boot_dir(dir, data).await
}

/// Boot the real `Host` over an instance already on disk, with the capture
/// cleared — the second half of [`boot_planted`], and the whole of
/// [`Booted::reboot`].
async fn boot_dir(dir: tempfile::TempDir, data: PathBuf) -> Booted {
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
    /// Drop this boot's host (or its error) and boot the SAME instance again —
    /// no re-provision, no new plant. The instance lock is re-entrant per PID,
    /// so the second `Host::start` claims it whether or not the first boot
    /// released it. Neither `Host` nor `CoreEngine` implements `Drop`, so the
    /// first host's drivers are NOT stopped and run alongside the second boot
    /// (the idiom `host_boot_avatar_rolls_collapse.rs` uses too). That is
    /// harmless for what these arms assert: boot-time lines and the rows the
    /// boot wrote, never a driver's later work.
    async fn reboot(self) -> Booted {
        let Booted {
            _dir, data, result, ..
        } = self;
        drop(result);
        boot_dir(_dir, data).await
    }

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
    // Not guarded steps: the avatar-roll collapse's two lines, the DEFERRED
    // pass (P4.135's FATAL arm until v4 `e5c6bd0c0`) and the SKIPPED
    // `shouldRun` (the unification review). They are
    // swept here for their silence legs. On a fresh instance the collapse
    // answers `NotApplicable`, so these two are weak legs; the resume arm's
    // second-boot `assert_silent` is the strong one for the pass's line.
    "Failed to collapse duplicate avatar rolls",
    "Error checking if migration should run",
    // v4's two runner lines and `deferResumable`'s WARN on the collapse's
    // deferring arms (P4.D248, `e5c6bd0c0`) — weak legs on a fresh instance
    // for the same reason; the resume and ledger-write arms' reboots are the
    // strong ones for the WARN.
    "Migration failed",
    "Migration threw an exception",
    "Resumable migration deferred",
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
/// ledger-gate divergence P4.134 named — ruled KEPT 2026-10-01, filed as v4
/// bug 176, its cadence pinned by `the_134_plant_is_re_ensured_and_re_logged_
/// on_every_boot`). The boot must still FAIL (nothing over-softened). Since
/// v4 `e5c6bd0c0` a ledger-complete v4 instance boots this LOUD and reachable —
/// its PHASE 3.1 pass reports the table and `/api/health` answers `degraded` —
/// where v5 still refuses to boot (the FATAL class as a reported problem is
/// R5, surfaced for the human at P4.D248, not this round).
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
/// (v5 HARDER — the ledger-gate divergence, recorded; v4 bug 176 — since
/// `e5c6bd0c0` v4 boots it LOUD: its structural pass reports the table and
/// `/api/health` answers `degraded`; R5).
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
/// v4 boots where v5 does not (the ledger-gate divergence, recorded; v4 bug
/// 176 — since `e5c6bd0c0` v4 boots it LOUD and reachable, its structural pass
/// reporting `doc_mount_folders is a view, not a table` behind a `degraded`
/// `/api/health`; R5).
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

/// Two unkeyed avatar rolls of ONE configuration (same prompt, same model —
/// one v0 key), the older a victim of the newer. A non-`mount-blob:`
/// `storageKey`, so the P4.D152 realign (which reads `storageKey LIKE
/// 'mount-blob:%'` and propagates) passes over them and the collapse is the
/// first boot step that touches them. Shape: `host_boot_avatar_rolls_collapse.rs`'s
/// seed, with every NOT NULL column of the fresh `files` DDL filled.
const TWO_AVATAR_ROLLS: &str = "INSERT INTO files \
       (id, userId, sha256, originalFilename, mimeType, size, source, category, \
        generationPrompt, generationModel, storageKey, createdAt, updatedAt) \
     VALUES \
       ('roll-old', 'user-1', 'ab', 'avatar_Friday_roll-old.webp', 'image/webp', 12, \
        'GENERATED', 'IMAGE', 'Friday in her study', 'flux-dev', 'files/roll-old.webp', \
        '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z'), \
       ('roll-new', 'user-1', 'cd', 'avatar_Friday_roll-new.webp', 'image/webp', 12, \
        'GENERATED', 'IMAGE', 'Friday in her study', 'flux-dev', 'files/roll-new.webp', \
        '2026-06-01T00:00:00.000Z', '2026-06-01T00:00:00.000Z');";

/// The two rolls' `(id, generationKey)` and whether the collapse's ledger row
/// exists, read off the main partition after the boot.
fn collapse_readback(data: &Path) -> (Vec<(String, Option<String>)>, bool) {
    let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
    let c = w.connection();
    let rows = c
        .prepare("SELECT id, generationKey FROM files WHERE id LIKE 'roll-%' ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let has_table: bool = c
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master \
                             WHERE type = 'table' AND name = 'migrations_state')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let ledger = has_table
        && c.query_row(
            "SELECT EXISTS (SELECT 1 FROM migrations_state \
                             WHERE id = 'collapse-duplicate-avatar-rolls-v1')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    (rows, ledger)
}

/// v4's three lines on a failed collapse PASS, in v4's order — the
/// migration's catch (`collapse-duplicate-avatar-rolls-v1.ts:646-663`), the
/// runner's result arm (`migrations/index.ts:203-208`, logged before the defer
/// check; v4's `message` META field carried as `resultMessage` — see
/// `host.rs`), and `deferResumable`'s WARN (`:73-81`), all at `e5c6bd0c0`.
fn deferred_pass_lines(error: &str) -> [String; 3] {
    [
        format!("ERROR quilltap::boot Failed to collapse duplicate avatar rolls context=migration.collapse-duplicate-avatar-rolls error={error}"),
        format!("ERROR quilltap::boot Migration failed context=migrations.runMigrations migrationId=collapse-duplicate-avatar-rolls-v1 error={error} resultMessage=Failed to collapse duplicate avatar rolls"),
        DEFERRED.to_string(),
    ]
}

/// v4 `deferResumable`'s WARN (`migrations/index.ts:73-81` at `e5c6bd0c0`).
const DEFERRED: &str = "WARN quilltap::boot Resumable migration deferred to the next boot; continuing startup context=migrations.runMigrations migrationId=collapse-duplicate-avatar-rolls-v1";

impl Booted {
    /// Each of `lines` exactly once, in that order.
    fn assert_lines_in_order(&self, lines: &[String]) {
        let mut last = None;
        for line in lines {
            self.assert_line(line);
            let at = self.lines.iter().position(|l| l == line).unwrap();
            if let Some(prev) = last {
                assert!(
                    prev < at,
                    "{line:?} logged out of v4's order; captured:\n{}",
                    self.lines.join("\n")
                );
            }
            last = Some(at);
        }
    }
}

/// DEFER class, host level — the avatar-roll collapse (v4's migration
/// `collapse-duplicate-avatar-rolls-v1`, P4.D184) at `e5c6bd0c0`: v4 bug 175
/// (this port's own filing) is FIXED upstream — the migration is `resumable`,
/// so a failed pass logs the migration's ERROR, the runner's `Migration failed`
/// and `deferResumable`'s WARN, writes no ledger row, and the boot CONTINUES
/// (`collapse-duplicate-avatar-rolls-v1.ts:328-331`, `:646-663`;
/// `migrations/index.ts:73-81`, `:189-217`). This arm was P4.135's
/// `a_failed_avatar_roll_collapse_fails_the_boot` (the 2026-10-01 FATAL ruling,
/// overtaken by v4's own fix), rewritten on the SAME plant: on `main`'s
/// `host.rs` as it stood it panicked "the boot FAILED … sqlite error: planted
/// collapse failure" (P4.D248's red-first record).
///
/// The plant is a trigger on the collapse's OWN first write (the keying
/// `UPDATE … SET generationKey`), which no earlier boot step issues — a column
/// rename would be caught by the P4.D152 realign first, or turn the gate to
/// `NotApplicable`. It fires before any delete, so nothing changed.
///
/// Mutation proofs (P4.D248's lane record): the `Pass` arm re-fattened to
/// `return Err` reds `host()`; the WARN dropped reds its `assert_line`; the
/// arm stamping anyway reds the readback; `error = %error` (the `DbError`, not
/// the bare text) reds the first two lines on the `sqlite error: ` prefix.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_avatar_roll_collapse_is_deferred_and_the_boot_continues() {
    let _serial = SERIAL.lock().await;
    let plant = format!(
        "{TWO_AVATAR_ROLLS}\
         CREATE TRIGGER qt_plant_collapse BEFORE UPDATE OF generationKey ON files \
         BEGIN SELECT RAISE(ABORT, 'planted collapse failure'); END;"
    );
    let booted = boot_planted(Substrate::Fresh, &[(MOUNT, DELETE_MARKERS), (MAIN, &plant)]).await;
    booted.host();
    booted.assert_lines_in_order(&deferred_pass_lines("planted collapse failure"));
    booted.assert_silent("Migration threw an exception");
    // Nothing changed: both rolls present, both unkeyed, no ledger row — the
    // next boot's gate (`generationKey IS NULL`) finds them again.
    let (rows, ledger) = collapse_readback(&booted.data);
    assert_eq!(
        rows,
        vec![
            ("roll-new".to_string(), None),
            ("roll-old".to_string(), None)
        ]
    );
    assert!(!ledger, "a failed pass must not stamp the ledger");
    // The boot went on past the collapse: the store provisions and the
    // general `state.json` ensure after it both ran.
    booted.assert_tool_folder_restored();
    booted.assert_line(STATE_SEEDED);
}

/// SKIP class, host level: the same migration's `shouldRun` FAILING, which is
/// not a failed pass. v4's runner catches a `shouldRun` throw, logs `Error
/// checking if migration should run` `{context: 'migrations.runMigrations',
/// migrationId, error}`, skips the migration and boots on
/// (`migrations/index.ts:131-148` at `f6426e196`). P4.135's flip had made this
/// arm fatal too, harder than v4. The `CollapseError::ShouldRun` split (the
/// unification review) restores the skip, with v4's one line on this path.
///
/// The plant renames `generationPrompt` after seeding two rolls. The table and
/// `generationKey` probes pass, so only the pending-row `SELECT` fails. No
/// earlier boot step reads the column. Nothing is keyed or stamped, and the
/// pass's own line never fires.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_collapse_should_run_read_is_v4s_logged_skip() {
    let _serial = SERIAL.lock().await;
    let plant = format!(
        "{TWO_AVATAR_ROLLS}\
         ALTER TABLE files RENAME COLUMN generationPrompt TO generationPrompt_x;"
    );
    let booted = boot_planted(Substrate::Fresh, &[(MAIN, &plant)]).await;
    booted.host();
    booted.assert_line("ERROR quilltap::boot Error checking if migration should run context=migrations.runMigrations migrationId=collapse-duplicate-avatar-rolls-v1 error=no such column: generationPrompt");
    booted.assert_silent("Failed to collapse duplicate avatar rolls");
    let (rows, ledger) = collapse_readback(&booted.data);
    assert_eq!(
        rows,
        vec![
            ("roll-new".to_string(), None),
            ("roll-old".to_string(), None)
        ],
        "a skipped migration touches nothing"
    );
    assert!(!ledger, "a skipped migration stamps nothing");
}

/// Bug 175's premise, MEASURED: a pass that fails mid-delete is resumable and
/// unstamped. The plant blocks the victim's `DELETE`, which runs AFTER the
/// survivor is keyed (`avatar_rolls_collapse_heal.rs` keys every survivor,
/// then repoints, then deletes; ordinary victims stay unkeyed until deleted —
/// v4's `:398-412` / `:569`, identically). The first boot DEFERS (v4's three
/// lines; it used to fail — P4.135's FATAL ruling, overtaken at `e5c6bd0c0`;
/// on `main`'s `host.rs` as it stood this arm panicked "the boot FAILED") with
/// the survivor keyed and the victim still present and unkeyed; with the
/// trigger gone, the next boot's gate finds the victim, the pass regroups both
/// rows under the same v0 key, picks the same survivor, deletes the victim and
/// stamps the ledger. That is what made v4's old `process.exit(1)` buy an
/// operator nothing — what bug 175 filed and v4 fixed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_collapse_failed_mid_delete_resumes_on_the_next_boot() {
    let _serial = SERIAL.lock().await;
    let plant = format!(
        "{TWO_AVATAR_ROLLS}\
         CREATE TRIGGER qt_plant_collapse_delete BEFORE DELETE ON files \
         BEGIN SELECT RAISE(ABORT, 'planted delete failure'); END;"
    );
    let booted = boot_planted(Substrate::Fresh, &[(MAIN, &plant)]).await;
    booted.host();
    booted.assert_lines_in_order(&deferred_pass_lines("planted delete failure"));
    let (rows, ledger) = collapse_readback(&booted.data);
    assert_eq!(rows.len(), 2, "the victim was deleted: {rows:?}");
    let survivor_key = rows[0]
        .1
        .clone()
        .expect("the survivor was keyed before the delete");
    assert_eq!(rows[0].0, "roll-new");
    assert_eq!(
        rows[1],
        ("roll-old".to_string(), None),
        "the victim stays unkeyed"
    );
    assert!(!ledger, "a failed pass must not stamp the ledger");

    Writer::open_writable(&booted.data.join(MAIN), PEPPER)
        .unwrap()
        .connection()
        .execute_batch("DROP TRIGGER qt_plant_collapse_delete;")
        .unwrap();
    let booted = booted.reboot().await;
    booted.host();
    booted.assert_silent("Failed to collapse duplicate avatar rolls");
    booted.assert_silent("Resumable migration deferred");
    let (rows, ledger) = collapse_readback(&booted.data);
    assert_eq!(
        rows,
        vec![("roll-new".to_string(), Some(survivor_key))],
        "the second boot finishes the pass on the same survivor and key"
    );
    assert!(ledger, "the completed pass stamps the ledger");
}

/// DEFER class, host level — a failed LEDGER WRITE after a committed pass
/// (ruled R2 at P4.D248's planning). v4's runner records the ledger row INSIDE
/// its `try` (`migrations/index.ts:194`); a throw there logs `Migration threw
/// an exception` `{context, migrationId, error}` and, the migration being
/// resumable, defers (`:218-244` at `e5c6bd0c0`). v4 reaches that arm only when
/// its FILE-ledger fallback fails too (`migrations/state.ts:181-205` — the
/// SQLite failure alone falls back and succeeds); v5 has no file ledger, so it
/// takes v4's both-writes-failed arm on the first failure (recorded). The
/// pass's success line still comes FIRST (v4 logs it inside `run()`).
///
/// The plant blocks exactly this migration's INSERT. On the reboot (trigger
/// gone) every surviving row is keyed, so the gate answers `NotApplicable`:
/// no pass, no success line, and still no ledger row — v4's both-writes-failed
/// outcome too (its `shouldRun` finds nothing unkeyed). Red-first: on `main`'s
/// `host.rs` as it stood the boot FAILED (the stamp's `?` was `Fatal`).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_collapse_ledger_write_is_deferred() {
    let _serial = SERIAL.lock().await;
    let plant = format!(
        "{TWO_AVATAR_ROLLS}\
         CREATE TABLE IF NOT EXISTS \"migrations_state\" (\
           \"id\" TEXT PRIMARY KEY, \"completedAt\" TEXT NOT NULL, \
           \"quilltapVersion\" TEXT NOT NULL, \
           \"itemsAffected\" INTEGER NOT NULL DEFAULT 0, \"message\" TEXT);\
         CREATE TRIGGER qt_plant_ledger BEFORE INSERT ON migrations_state \
         WHEN NEW.id = 'collapse-duplicate-avatar-rolls-v1' \
         BEGIN SELECT RAISE(ABORT, 'planted ledger failure'); END;"
    );
    let booted = boot_planted(Substrate::Fresh, &[(MOUNT, DELETE_MARKERS), (MAIN, &plant)]).await;
    booted.host();
    booted.assert_lines_in_order(&[
        COLLAPSED_TWO_ROLLS.to_string(),
        "ERROR quilltap::boot Migration threw an exception context=migrations.runMigrations migrationId=collapse-duplicate-avatar-rolls-v1 error=planted ledger failure".to_string(),
        DEFERRED.to_string(),
    ]);
    booted.assert_silent("Migration failed");
    booted.assert_silent("Failed to collapse duplicate avatar rolls");
    let (rows, ledger) = collapse_readback(&booted.data);
    assert_eq!(
        rows.len(),
        1,
        "the committed pass deleted the victim: {rows:?}"
    );
    assert_eq!(rows[0].0, "roll-new");
    let key = rows[0].1.clone().expect("the survivor is keyed");
    assert!(!ledger, "the planted INSERT must not have landed");
    booted.assert_tool_folder_restored();
    booted.assert_line(STATE_SEEDED);

    Writer::open_writable(&booted.data.join(MAIN), PEPPER)
        .unwrap()
        .connection()
        .execute_batch("DROP TRIGGER qt_plant_ledger;")
        .unwrap();
    let booted = booted.reboot().await;
    booted.host();
    booted.assert_silent("Collapsed duplicate avatar rolls");
    booted.assert_silent("Resumable migration deferred");
    let (rows, ledger) = collapse_readback(&booted.data);
    assert_eq!(rows, vec![("roll-new".to_string(), Some(key))]);
    assert!(!ledger, "a NotApplicable gate stamps nothing");
}

/// The collapse's success INFO for [`TWO_AVATAR_ROLLS`] (v5's own bytes —
/// v4's `Collapsed duplicate avatar rolls` + camelCase bag is a pre-existing
/// divergence, deferred by name to a smalls round).
const COLLAPSED_TWO_ROLLS: &str = "INFO quilltap::boot Collapsed duplicate avatar rolls into one image per configuration avatar_rows=2 configurations=1 rows_keyed=1 victims_deleted=1 protected_kept=0 album_copies_kept=0 blobs_deleted=0 chats_changed=0 characters_changed=0 messages_changed=0";

/// The ledger-gate divergence's CADENCE (ruled 2026-10-01: v5 KEEPS its
/// per-boot ensures; v4 bug 176). v5 re-runs every structural ensure on EVERY
/// boot (`host.rs`'s class (i), `builtin_mounts.rs` 25a/25f/25h) — so the #134
/// plant logs v4's lazy-ensure line on the first boot AND again on the second,
/// with nothing stamped in between that could stop it. v4 instead skips a
/// migration already in `migrations_state` BEFORE its `shouldRun`
/// (`migrations/index.ts:125-129` at `f6426e196`) and reaches the table only
/// through the repository's lazy `ensureTable` (`dedicated-db.repository.ts:
/// 134-152`), which logs `Failed to ensure doc_mount_file_links table in mount
/// index database` per ACCESS (`tableEnsured` flips only on success). Since
/// `e5c6bd0c0` (v4's bug-176 fix) v4 also checks each structural table ONCE
/// per boot in a read-only PHASE 3.1 pass and reports it through a `structure`
/// service in `/api/health` — it did NOT change the ledger gate, so the
/// cadence divergence stands (v5 re-ENSURES every boot, v4 re-CHECKS). v5
/// ports that pass after its own ensures, REUSING their failure text, so this
/// plant reports its ensure-form problem without a second `Failed to ensure`
/// line (the two exactly-one asserts here pin that). There is no v4 boot
/// oracle: "both ways" here is this arm + the three FATAL arms + the
/// recorded-divergence row.
///
/// Mutation proofs (P4.135's lane record): a once-per-PROCESS memo on
/// `ensure_builtin_mounts_with` in `host.rs` (v4's `tableEnsured` shape, a
/// static flag) reds this arm on its second boot and leaves the single-boot
/// arm above green — so the arm tells per-boot from per-process. Drop the
/// second boot and it proves only what
/// `a_renamed_link_column_logs_v4s_ensure_table_line_and_boots` already does —
/// the second boot is this arm's reason to exist.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_134_plant_is_re_ensured_and_re_logged_on_every_boot() {
    let _serial = SERIAL.lock().await;
    let line = "ERROR quilltap::db Failed to ensure doc_mount_file_links table in mount index database error=no such column: relativePath";
    let booted = boot_planted(
        Substrate::Fresh,
        &[(
            MOUNT,
            "ALTER TABLE doc_mount_file_links RENAME COLUMN relativePath TO relativePath_x",
        )],
    )
    .await;
    booted.host();
    booted.assert_line(line);
    let booted = booted.reboot().await;
    booted.host();
    booted.assert_line(line);
}
