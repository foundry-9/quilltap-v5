//! The writer-task runtime (Phase-3 Unit 0) — the runtime shell that turns the
//! single-writer *ownership* rule into a live, compiler-enforced invariant.
//!
//! Phase 2 built [`Writer`] (the type that owns a read-write connection) and
//! [`crate::write_apply`] (the partitioned apply logic, trace-verified). What was
//! missing is the shell from `docs/developer/porting/api-boundary.md` Part 2 that
//! makes "the channel is the only mutator" real at runtime:
//!
//! ```text
//! ┌── Db (Clone, Send, Sync) — what every service holds ──────────────┐
//! │   reads:  ReadPool         → direct, pooled read-only connections │
//! │   writes: mpsc::Sender<Job> → the ONLY way to mutate              │
//! └───────────────────────────────────────────────────────────────────┘
//!                                  │  send(job)
//!                                  ▼
//!        one dedicated OS thread owns the WriterSet (main / mount-index
//!        / llm-logs RW connections) and drains the channel serially.
//! ```
//!
//! - A **write** is a type-erased closure `FnOnce(&mut WriterSet)`; the writer
//!   thread runs each closure to completion before the next, so batch-apply is
//!   naturally serial — exactly the property v4's folder-conflict remap and
//!   main-primary ordering assume. Services call the same typed repository methods
//!   they'd call directly, but only ever *on the writer thread*, reached through
//!   the channel. There is no cross-process `{method, args}` reflection (that
//!   dissolves into the type system per the design doc); [`crate::write_apply`]
//!   remains available for the multi-DB background-job path and is simply invoked
//!   *inside* a write closure when a job needs it.
//! - A **read** goes direct to a per-partition [`ReadPool`] of read-only
//!   connections, so reads never contend with the writer.
//! - [`Db`] is `Clone` and is what every service holds.
//!
//! The read-only opens follow the CLAUDE.md rule: `PRAGMA key` is the first and
//! only pragma before the first read (no `journal_mode`/`foreign_keys` on a read
//! path — those would force header writes that race the cipher context).
//!
//! ## The sibling open (P4.159, dogfood #150)
//!
//! The MAIN database is fatal: v4's `getSQLiteClient` rethrows and its
//! migrations `process.exit(1)`. A SIBLING — the mount index or the LLM logs —
//! is not: v4 isolates them precisely so "corruption in the mount index DB can
//! never threaten characters, chats, messages, or memories"
//! (`mount-index-client.ts:5-7`). [`Db::open`] opens each present sibling the
//! way v4's `SQLiteBackend.connect()` does (`backend.ts:571-617` at
//! `94fbb1ae3`), LLM logs FIRST:
//!
//! - BOTH through ONE four-attempt cold-open ladder `[200, 600, 1500]` ms
//!   apart ([`open_sibling_with_ladder`] — v4 `cold-open-retry.ts:1-60` at
//!   `f5e953a3f`; a cold open over an iCloud/VirtioFS bind mount can read
//!   incomplete page-1 bytes once and succeed a moment later). Until v4
//!   `039f7017c` (bug 180, this port's filing) the LLM logs had ONE attempt
//!   and v5 pinned that asymmetry faithfully (P4.159); v4 now opens both
//!   siblings through `openWithColdOpenRetry` "so the two cannot drift
//!   again", and so does v5 (P4.D258). Each leg keeps its own lines: the
//!   mount index's bytes are unchanged (`mount-index-client.ts:47-126`), the
//!   LLM logs log their key DEBUG INSIDE each attempt and carry `attempts` on
//!   both terminal lines (`llm-logs-client.ts:46-126`). The sleeps block the
//!   opening thread as v4's `sleepSync` blocks its event loop (R-C — v4's
//!   constants, no injection seam);
//! - an opened sibling then runs v4's `quick_check` (`*-protection.ts:44-66`);
//! - a sibling that failed either step is DEGRADED: [`PartitionState::Degraded`],
//!   no writer, no read pool. v4 KEEPS the connection after a failed integrity
//!   check and lets its two guards throw on every use; v5 has no guard layer —
//!   its `Option<Writer>` / `Option<PartitionPool>` IS the guard — so dropping
//!   both reaches the same boundary behaviour (every read and write of that
//!   partition refuses with [`DbError::PartitionUnavailable`]). Ruled R-A.
//!
//! DEGRADED is distinct from ABSENT (no path — a v5-only state: v4's `new
//! Database` on a missing path CREATES the file; recorded, not ported). The
//! boot's structural pass COUNTS a degraded partition (v4's `<label> database
//! unavailable: …` per repository, so `/health` answers `degraded`) and skips
//! an absent one (R4). Collapsing the two would answer a healthy 200 over a
//! dead mount index (R-B).
//!
//! The writable open needs no separate verify probe (R-D): v4's sibling
//! clients — the mount index's always, the LLM logs' since `039f7017c`
//! (`llm-logs-client.ts:58`) — probe `SELECT count(*) FROM sqlite_master`
//! before their pragmas so a bad page 1 fails there; v5's
//! [`Writer::open_writable`] fails at its `journal_mode` pragma on the same
//! bytes, and SQLite3MC answers the same `file is not a database` at both
//! steps (measured through v4's own binding and pinned by
//! `degraded_sibling_open_equivalence`, for both partitions).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};
use tokio::sync::{mpsc, oneshot};

use crate::dbkey;
use crate::write_partition::WriteDbTarget;

use super::fallback::error_text;
use super::table_shape::Partition;
use super::text_compression;
use super::{DbError, Writer};

/// The siblings' shared cold-open ladder (v4 `cold-open-retry.ts:18`,
/// `COLD_OPEN_RETRY_BACKOFF_MS`): the sleep after each failed attempt but the
/// last; the attempt budget is one more than its length.
const COLD_OPEN_RETRY_BACKOFF_MS: [u64; 3] = [200, 600, 1500];

/// v4's child-logger `module` on each client's and protection module's lines.
const MOUNT_INDEX_CLIENT: &str = "database:mount-index-client";
const MOUNT_INDEX_PROTECTION: &str = "database:mount-index-protection";
const LLM_LOGS_CLIENT: &str = "database:llm-logs-client";
const LLM_LOGS_PROTECTION: &str = "database:llm-logs-protection";

/// How a sibling partition came out of [`Db::open`] (R-B). The main database
/// is always `Open` — its failure fails the open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartitionState {
    /// Opened and integrity-checked: a writer and a read pool.
    Open,
    /// No file at all (no path) — a v5-only state (v4 creates the file);
    /// skipped, not counted, by the structural pass.
    Absent,
    /// A file that failed v4's open or its `quick_check` — v4's degraded mode.
    /// No writer, no read pool; COUNTED by the structural pass.
    Degraded,
}

/// Max read-only connections kept warm per partition. Beyond this the pool drops
/// returned connections rather than growing without bound; the next checkout
/// re-opens. Small because reads are short and the working set is a handful of
/// concurrent services.
const MAX_IDLE_CONNS: usize = 4;

/// Bound on the write channel. `Db::write` awaits `send`, so the bound provides
/// backpressure if a burst of writers outruns the single writer thread; it is
/// generous enough that steady-state traffic never blocks.
const WRITE_CHANNEL_CAPACITY: usize = 512;

/// A unit of work handed to the writer thread. Type-erased: the closure captures
/// its own reply channel and sends the (typed) result back itself, so the channel
/// can carry heterogeneous jobs.
type WriteJob = Box<dyn FnOnce(&mut WriterSet) + Send>;

/// The (up to three) read-write connections a batch may touch — one per
/// partition database (main / mount-index / llm-logs). Owned exclusively by the
/// writer thread; never `Clone`, never shared. A write closure receives `&mut`
/// to this set and drives the ordinary repositories through it.
pub struct WriterSet {
    main: Writer,
    mount_index: Option<Writer>,
    llm_logs: Option<Writer>,
}

impl WriterSet {
    /// The main-database writer (always present).
    pub fn main(&self) -> &Writer {
        &self.main
    }

    /// The mount-index sibling-database writer, if this instance was opened with
    /// one.
    pub fn mount_index(&self) -> Option<&Writer> {
        self.mount_index.as_ref()
    }

    /// The llm-logs sibling-database writer, if this instance was opened with one.
    pub fn llm_logs(&self) -> Option<&Writer> {
        self.llm_logs.as_ref()
    }
}

/// Paths to the (up to three) partition database files that back one instance.
/// Only `main` is required; the sibling databases are opened when present.
pub struct DbPaths {
    pub main: PathBuf,
    pub mount_index: Option<PathBuf>,
    pub llm_logs: Option<PathBuf>,
}

impl DbPaths {
    /// A main-only instance (no sibling databases) — the shape the memory gate
    /// and other main-DB-only services use.
    pub fn main_only(main: impl Into<PathBuf>) -> Self {
        Self {
            main: main.into(),
            mount_index: None,
            llm_logs: None,
        }
    }
}

/// The cloneable handle every service holds. Reads go direct to the read pool;
/// writes are sent to the writer thread over the channel — the only mutator.
#[derive(Clone)]
pub struct Db {
    inner: Arc<DbInner>,
}

struct DbInner {
    reads: ReadPool,
    writes: mpsc::Sender<WriteJob>,
    mount_index_state: PartitionState,
    llm_logs_state: PartitionState,
}

/// One sibling's open: its writer (when `Open`) and its state.
struct SiblingOpen {
    writer: Option<Writer>,
    state: PartitionState,
}

/// Open one sibling partition the way v4's `connect()` does (see the module
/// doc): the client's open through the shared ladder, then the integrity
/// check; either failing leaves it
/// [`PartitionState::Degraded`] with no writer. `None` is `Absent`, silently.
fn open_sibling(partition: Partition, path: Option<&Path>, pepper_b64: &str) -> SiblingOpen {
    let Some(path) = path else {
        return SiblingOpen {
            writer: None,
            state: PartitionState::Absent,
        };
    };
    let writer = match partition {
        Partition::MountIndex => open_mount_index(path, pepper_b64),
        Partition::LlmLogs => open_llm_logs(path, pepper_b64),
        Partition::Main => unreachable!("the main database is not a sibling"),
    };
    match writer {
        Some(writer) if integrity_check_passes(partition, writer.connection()) => SiblingOpen {
            writer: Some(writer),
            state: PartitionState::Open,
        },
        // R-A: a failed integrity check drops the connection (v4 keeps it and
        // lets its guards refuse every use — the same boundary behaviour).
        _ => SiblingOpen {
            writer: None,
            state: PartitionState::Degraded,
        },
    }
}

/// v4 `openWithColdOpenRetry` (`cold-open-retry.ts:32-60`): run `attempt`
/// until it opens or the budget is spent, a WARN `<label> cold-open failed —
/// retrying` (keys `path, attempt, maxAttempts, backoffMs, error`, v4's
/// order) and a blocking sleep after each failure but the last. Never logs a
/// terminal line — each caller renders its own INFO / ERROR from the outcome
/// and `attempts` (the attempts it took, or the whole budget when spent).
/// `Err` carries the LAST attempt's error text (v4's `lastError`).
fn open_sibling_with_ladder(
    label: &str,
    module: &'static str,
    path_text: &str,
    mut attempt: impl FnMut() -> Result<Writer, DbError>,
) -> (Result<Writer, String>, usize) {
    let max_attempts = COLD_OPEN_RETRY_BACKOFF_MS.len() + 1;
    let mut last_error = String::new();
    for i in 0..max_attempts {
        match attempt() {
            Ok(writer) => return (Ok(writer), i + 1),
            Err(error) => {
                last_error = error_text(&error);
                if let Some(&backoff) = COLD_OPEN_RETRY_BACKOFF_MS.get(i) {
                    tracing::warn!(
                        target: "quilltap::db",
                        module = module,
                        path = path_text,
                        attempt = i + 1,
                        maxAttempts = max_attempts,
                        backoffMs = backoff,
                        error = last_error.as_str(),
                        "{label} cold-open failed — retrying"
                    );
                    thread::sleep(Duration::from_millis(backoff));
                }
            }
        }
    }
    (Err(last_error), max_attempts)
}

/// v4 `getMountIndexSQLiteClient` (`mount-index-client.ts:99-126` at
/// `f5e953a3f`) over the shared ladder (`'Mount index'`); `attemptOpenMountIndex`
/// (`:47-60`) logs nothing of its own. `walMode` is v4's config value: v5
/// never runs WAL (TRUNCATE journaling, cloud-sync safety), so it is always
/// `false` — v4's value with `SQLITE_WAL_MODE` unset.
fn open_mount_index(path: &Path, pepper_b64: &str) -> Option<Writer> {
    let path_text = path.display().to_string();
    tracing::info!(
        target: "quilltap::db",
        module = MOUNT_INDEX_CLIENT,
        path = path_text.as_str(),
        walMode = false,
        "Initializing mount index database connection"
    );
    let (outcome, attempts) =
        open_sibling_with_ladder("Mount index", MOUNT_INDEX_CLIENT, &path_text, || {
            Writer::open_writable(path, pepper_b64)
        });
    match outcome {
        Ok(writer) => {
            tracing::info!(
                target: "quilltap::db",
                module = MOUNT_INDEX_CLIENT,
                path = path_text.as_str(),
                attempts = attempts,
                "Mount index database connection established"
            );
            Some(writer)
        }
        Err(error) => {
            tracing::error!(
                target: "quilltap::db",
                module = MOUNT_INDEX_CLIENT,
                path = path_text.as_str(),
                attempts = attempts,
                error = error.as_str(),
                "Failed to initialize mount index database — entering degraded mode"
            );
            None
        }
    }
}

/// v4 `getLLMLogsSQLiteClient` (`llm-logs-client.ts:94-126` at `f5e953a3f`)
/// over the shared ladder (`'LLM logs'`; bug 180). v4's DEBUG `SQLCipher key
/// set on LLM logs database` fires INSIDE each attempt, once the key pragma
/// has run and BEFORE the probe that fails on a bad file
/// (`attemptOpenLLMLogs`, `:51-53`) — so four times on a garbage file. v5's
/// key is the first step of [`Writer::open_writable`] and cannot fail on an
/// existing file, so each attempt logs the line just ahead of its open.
fn open_llm_logs(path: &Path, pepper_b64: &str) -> Option<Writer> {
    let path_text = path.display().to_string();
    tracing::info!(
        target: "quilltap::db",
        module = LLM_LOGS_CLIENT,
        path = path_text.as_str(),
        walMode = false,
        "Initializing LLM logs database connection"
    );
    let (outcome, attempts) =
        open_sibling_with_ladder("LLM logs", LLM_LOGS_CLIENT, &path_text, || {
            tracing::debug!(
                target: "quilltap::db",
                module = LLM_LOGS_CLIENT,
                "SQLCipher key set on LLM logs database"
            );
            Writer::open_writable(path, pepper_b64)
        });
    match outcome {
        Ok(writer) => {
            tracing::info!(
                target: "quilltap::db",
                module = LLM_LOGS_CLIENT,
                path = path_text.as_str(),
                attempts = attempts,
                "LLM logs database connection established"
            );
            Some(writer)
        }
        Err(error) => {
            tracing::error!(
                target: "quilltap::db",
                module = LLM_LOGS_CLIENT,
                path = path_text.as_str(),
                attempts = attempts,
                error = error.as_str(),
                "Failed to initialize LLM logs database — entering degraded mode"
            );
            None
        }
    }
}

/// v4 `runMountIndexIntegrityCheck` / `runLLMLogsIntegrityCheck`
/// (`*-protection.ts:44-66`): `pragma('quick_check', { simple: true })` — the
/// first column of the first row.
fn integrity_check_passes(partition: Partition, conn: &Connection) -> bool {
    let outcome = conn
        .query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
        .map_err(DbError::from);
    integrity_verdict(partition, outcome)
}

/// The integrity check's three arms and their lines: `ok` passes (INFO); any
/// other result is the FAILED arm (ERROR `{result}`); a throw is the `threw`
/// arm (ERROR `{error}`). A page-overwrite plant reaches FAILED (SQLite3MC's
/// page authentication fails the read and `quick_check` REPORTS it — measured);
/// no plant reaches `threw`, so it is unit-pinned over a stubbed outcome.
fn integrity_verdict(partition: Partition, outcome: Result<String, DbError>) -> bool {
    match (partition, outcome) {
        (Partition::MountIndex, Ok(result)) if result == "ok" => {
            tracing::info!(
                target: "quilltap::db",
                module = MOUNT_INDEX_PROTECTION,
                "Mount index database integrity check passed"
            );
            true
        }
        (Partition::MountIndex, Ok(result)) => {
            tracing::error!(
                target: "quilltap::db",
                module = MOUNT_INDEX_PROTECTION,
                result = result.as_str(),
                "Mount index database integrity check FAILED — entering degraded mode"
            );
            false
        }
        (Partition::MountIndex, Err(error)) => {
            tracing::error!(
                target: "quilltap::db",
                module = MOUNT_INDEX_PROTECTION,
                error = error_text(&error).as_str(),
                "Mount index database integrity check threw an error — entering degraded mode"
            );
            false
        }
        (_, Ok(result)) if result == "ok" => {
            tracing::info!(
                target: "quilltap::db",
                module = LLM_LOGS_PROTECTION,
                "LLM logs database integrity check passed"
            );
            true
        }
        (_, Ok(result)) => {
            tracing::error!(
                target: "quilltap::db",
                module = LLM_LOGS_PROTECTION,
                result = result.as_str(),
                "LLM logs database integrity check FAILED — entering degraded mode"
            );
            false
        }
        (_, Err(error)) => {
            tracing::error!(
                target: "quilltap::db",
                module = LLM_LOGS_PROTECTION,
                error = error_text(&error).as_str(),
                "LLM logs database integrity check threw an error — entering degraded mode"
            );
            false
        }
    }
}

impl Db {
    /// Open an instance: RW writers for each present partition (owned by a new
    /// writer thread) plus a matching read pool. `pepper_b64` is the base64 pepper
    /// (as [`dbkey::load_pepper`] yields it).
    ///
    /// The main database's failure is this call's error; a sibling's is not —
    /// it opens DEGRADED (see the module doc and [`Db::partition_state`]).
    pub fn open(paths: DbPaths, pepper_b64: &str) -> Result<Db, DbError> {
        let key_hex =
            dbkey::pepper_b64_to_key_hex(pepper_b64).map_err(|e| DbError::Key(e.to_string()))?;

        let main = Writer::open_writable(&paths.main, pepper_b64)?;
        // v4's `connect()` order: the LLM logs, then the mount index.
        let llm_logs = open_sibling(Partition::LlmLogs, paths.llm_logs.as_deref(), pepper_b64);
        let mount_index = open_sibling(
            Partition::MountIndex,
            paths.mount_index.as_deref(),
            pepper_b64,
        );

        // The read pool — direct, pooled read-only connections per OPEN
        // partition (a degraded one has none, like an absent one).
        let pool_for = |path: &Option<PathBuf>, sibling: &SiblingOpen| match path {
            Some(p) if sibling.state == PartitionState::Open => {
                Some(PartitionPool::new(p.clone(), key_hex.clone()))
            }
            _ => None,
        };
        let reads = ReadPool {
            main: PartitionPool::new(paths.main.clone(), key_hex.clone()),
            mount_index: pool_for(&paths.mount_index, &mount_index),
            llm_logs: pool_for(&paths.llm_logs, &llm_logs),
        };
        let (mount_index_state, llm_logs_state) = (mount_index.state, llm_logs.state);

        // The writer thread's owned set — one RW connection per open partition.
        let writers = WriterSet {
            main,
            mount_index: mount_index.writer,
            llm_logs: llm_logs.writer,
        };

        let (tx, mut rx) = mpsc::channel::<WriteJob>(WRITE_CHANNEL_CAPACITY);

        // The dedicated writer thread owns the WriterSet and drains the channel
        // serially. It is a plain OS thread (not a tokio worker) so `blocking_recv`
        // is legal; it exits when the last `Db` clone drops the sender.
        thread::Builder::new()
            .name("quilltap-writer".to_string())
            .spawn(move || {
                let mut writers = writers;
                while let Some(job) = rx.blocking_recv() {
                    job(&mut writers);
                }
            })
            .map_err(|e| DbError::WriterSpawn(e.to_string()))?;

        Ok(Db {
            inner: Arc::new(DbInner {
                reads,
                writes: tx,
                mount_index_state,
                llm_logs_state,
            }),
        })
    }

    /// How `partition` came out of [`Db::open`]: `Open`, `Absent` (no file) or
    /// `Degraded` (v4's degraded mode — the file failed its open or its
    /// integrity check). The main database is always `Open`.
    pub fn partition_state(&self, partition: Partition) -> PartitionState {
        match partition {
            Partition::Main => PartitionState::Open,
            Partition::MountIndex => self.inner.mount_index_state,
            Partition::LlmLogs => self.inner.llm_logs_state,
        }
    }

    /// Open a main-only instance (no sibling databases).
    pub fn open_main(main: impl Into<PathBuf>, pepper_b64: &str) -> Result<Db, DbError> {
        Db::open(DbPaths::main_only(main), pepper_b64)
    }

    /// Run a write on the writer thread and await its result. `f` receives the
    /// owned [`WriterSet`] and returns any `Send` value; because every write
    /// funnels through one thread, writes never interleave.
    pub async fn write<T, F>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&mut WriterSet) -> Result<T, DbError> + Send + 'static,
        T: Send + 'static,
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        let job: WriteJob = Box::new(move |writers| {
            // The receiver may have gone away (caller dropped the future); ignore.
            let _ = reply_tx.send(f(writers));
        });
        self.inner
            .writes
            .send(job)
            .await
            .map_err(|_| DbError::WriterGone)?;
        reply_rx.await.map_err(|_| DbError::WriterGone)?
    }

    /// Synchronous counterpart to [`Self::write`] for non-async callers (e.g. the
    /// tier-2 differential harness, whose tests are plain `#[test]`). Must NOT be
    /// called from within a tokio runtime worker (it blocks the thread).
    pub fn write_blocking<T, F>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&mut WriterSet) -> Result<T, DbError> + Send + 'static,
        T: Send + 'static,
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        let job: WriteJob = Box::new(move |writers| {
            let _ = reply_tx.send(f(writers));
        });
        self.inner
            .writes
            .blocking_send(job)
            .map_err(|_| DbError::WriterGone)?;
        reply_rx.blocking_recv().map_err(|_| DbError::WriterGone)?
    }

    /// Run a read against a pooled read-only connection to the **main** database.
    /// Direct (never contends with the writer); the connection is returned to the
    /// pool afterward.
    pub fn read_main<T, F>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&Connection) -> Result<T, DbError>,
    {
        self.inner.reads.main.with_conn(f)
    }

    /// Run a read against the **mount-index** sibling database. Errors with
    /// [`DbError::PartitionUnavailable`] if this instance has no mount-index DB
    /// or it opened degraded.
    pub fn read_mount_index<T, F>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&Connection) -> Result<T, DbError>,
    {
        match &self.inner.reads.mount_index {
            Some(p) => p.with_conn(f),
            None => Err(DbError::PartitionUnavailable(WriteDbTarget::MountIndex)),
        }
    }

    /// Run a read against the **llm-logs** sibling database. Errors with
    /// [`DbError::PartitionUnavailable`] if this instance has no llm-logs DB or
    /// it opened degraded.
    pub fn read_llm_logs<T, F>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&Connection) -> Result<T, DbError>,
    {
        match &self.inner.reads.llm_logs {
            Some(p) => p.with_conn(f),
            None => Err(DbError::PartitionUnavailable(WriteDbTarget::LlmLogs)),
        }
    }
}

/// The per-partition read pool. `Clone` (shared via `Arc` internally) so it rides
/// inside the cloneable [`Db`].
#[derive(Clone)]
struct ReadPool {
    main: PartitionPool,
    mount_index: Option<PartitionPool>,
    llm_logs: Option<PartitionPool>,
}

/// A pool of read-only connections to one partition database. Connections are
/// opened lazily and reused; the pool holds at most [`MAX_IDLE_CONNS`] idle.
#[derive(Clone)]
struct PartitionPool {
    inner: Arc<PartitionPoolInner>,
}

struct PartitionPoolInner {
    path: PathBuf,
    key_hex: String,
    idle: Mutex<Vec<Connection>>,
}

impl PartitionPool {
    fn new(path: PathBuf, key_hex: String) -> Self {
        Self {
            inner: Arc::new(PartitionPoolInner {
                path,
                key_hex,
                idle: Mutex::new(Vec::new()),
            }),
        }
    }

    /// Take an idle connection or open a fresh one.
    fn checkout(&self) -> Result<Connection, DbError> {
        if let Some(conn) = self.inner.idle.lock().unwrap().pop() {
            return Ok(conn);
        }
        open_readonly(&self.inner.path, &self.inner.key_hex)
    }

    /// Return a connection to the pool (dropped if the pool is already full).
    fn checkin(&self, conn: Connection) {
        let mut idle = self.inner.idle.lock().unwrap();
        if idle.len() < MAX_IDLE_CONNS {
            idle.push(conn);
        }
    }

    /// Check out a connection, run `f`, and return the connection — even if `f`
    /// errors (only a panic leaks it, in which case the pool simply re-opens).
    fn with_conn<T, F>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&Connection) -> Result<T, DbError>,
    {
        let conn = self.checkout()?;
        let result = f(&conn);
        self.checkin(conn);
        result
    }
}

/// Open a database **read-only** with the cipher key applied as the first and
/// only pragma (CLAUDE.md's read-path rule: no `journal_mode`/`foreign_keys`,
/// which would force header writes that race the cipher context). The raw-hex key
/// skips the KDF (the pepper was already derived when `dbkey` unwrapped `.dbkey`).
fn open_readonly(path: &Path, key_hex: &str) -> Result<Connection, DbError> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_NO_MUTEX
        | OpenFlags::SQLITE_OPEN_URI;
    let conn = Connection::open_with_flags(path, flags)?;
    conn.pragma_update(None, "key", format!("x'{key_hex}'"))?;
    // `qt_text()` immediately after the key, on EVERY connection — v4 registers
    // it at the same position in all six of its open sites. A read of a
    // compressed column through raw SQL, and every FTS trigger v4 4.10 puts
    // over `chat_messages`, needs it; a failure to register is an open ERROR,
    // not a warn, because a missing UDF fails loudly ("no such function:
    // qt_text") where a silent absence would let the index drift.
    text_compression::register_qt_text(&conn)?;
    // v4's page cache (`cache_size = -64000`, 64 MB) and in-memory temp store,
    // set on every connection v4 opens (`backends/sqlite/client.ts`
    // `applyPragmas`). Without them SQLite's ~2 MB default cache re-reads — and
    // re-DECRYPTS — the same pages on every repeated scan: the chat GET's
    // per-message `files.linkedTo` probe took 13 s over a 2,028-message chat
    // (dogfood #123). Both are connection-local and never touch the file, but
    // the read-path rule keeps `key` the only pragma before the first read, so
    // they follow one. (v4's `mmap_size` is not carried: SQLite3MC cannot
    // memory-map an encrypted database, so it is inert there too.)
    apply_cache_pragmas(&conn, true)?;
    Ok(conn)
}

/// v4's per-connection cache pragmas (see [`open_readonly`]). `first_read`
/// performs a trivial read first, for a read-only open where `key` must stay
/// the only pragma issued before the first read.
pub(crate) fn apply_cache_pragmas(conn: &Connection, first_read: bool) -> Result<(), DbError> {
    if first_read {
        let _: i64 = conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0))?;
    }
    conn.pragma_update(None, "cache_size", -64000)?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use tempfile::tempdir;

    /// Any valid base64 keys a fresh encrypted DB; the same value opens it back.
    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// Create + seed a fresh encrypted main DB, then open a `Db` over it.
    fn make_db() -> (tempfile::TempDir, Db) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("main.db");
        {
            // A writable open creates the encrypted file with the sqleet cipher.
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            w.connection()
                .execute_batch(
                    "CREATE TABLE counter (id TEXT PRIMARY KEY, n INTEGER NOT NULL);
                     INSERT INTO counter (id, n) VALUES ('c', 0);",
                )
                .unwrap();
        }
        let db = Db::open_main(&path, PEPPER).unwrap();
        (dir, db)
    }

    /// Concurrent writers funnel through the one writer thread, so a
    /// read-modify-write increment cannot lose updates — the final count equals
    /// the number of writers. If the runtime allowed >1 writer (or interleaving),
    /// this would under-count.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_writes_serialize_without_lost_updates() {
        let (_dir, db) = make_db();
        let n: i64 = 100;

        let mut handles = Vec::new();
        for _ in 0..n {
            let db = db.clone();
            handles.push(tokio::spawn(async move {
                db.write(|ws| {
                    let conn = ws.main().connection();
                    let cur: i64 =
                        conn.query_row("SELECT n FROM counter WHERE id = 'c'", [], |r| r.get(0))?;
                    conn.execute("UPDATE counter SET n = ?1 WHERE id = 'c'", params![cur + 1])?;
                    Ok::<(), DbError>(())
                })
                .await
                .unwrap();
            }));
        }
        for h in handles {
            h.await.unwrap();
        }

        let total: i64 = db
            .read_main(|conn| {
                Ok(conn.query_row("SELECT n FROM counter WHERE id = 'c'", [], |r| r.get(0))?)
            })
            .unwrap();
        assert_eq!(total, n);
    }

    /// A read issued after an awaited write observes the committed state (the
    /// write's completion is signalled by the awaited reply, and the read opens a
    /// fresh committed view).
    #[tokio::test]
    async fn read_after_write_sees_committed_state() {
        let (_dir, db) = make_db();
        db.write(|ws| {
            ws.main()
                .connection()
                .execute("UPDATE counter SET n = 42 WHERE id = 'c'", [])?;
            Ok::<(), DbError>(())
        })
        .await
        .unwrap();

        let n: i64 = db
            .read_main(|conn| {
                Ok(conn.query_row("SELECT n FROM counter WHERE id = 'c'", [], |r| r.get(0))?)
            })
            .unwrap();
        assert_eq!(n, 42);
    }

    /// A sibling-partition read on a main-only instance is a clean typed error,
    /// not a panic.
    #[test]
    fn missing_partition_reads_error_cleanly() {
        let (_dir, db) = make_db();
        let err = db.read_mount_index(|_c| Ok(())).unwrap_err();
        assert!(matches!(
            err,
            DbError::PartitionUnavailable(WriteDbTarget::MountIndex)
        ));
    }

    /// Dogfood #123: every connection carries v4's page cache (`cache_size =
    /// -64000`) and in-memory temp store (`temp_store = MEMORY` → 2) — the
    /// pooled READ connection and the WRITER alike. Without them the chat GET's
    /// per-message `files` probe re-decrypted the same pages 2,028 times (13 s).
    #[test]
    fn every_connection_carries_v4s_cache_pragmas() {
        let (_dir, db) = make_db();
        let read = db
            .read_main(|conn| {
                let cache: i64 = conn.query_row("PRAGMA cache_size", [], |r| r.get(0))?;
                let temp: i64 = conn.query_row("PRAGMA temp_store", [], |r| r.get(0))?;
                Ok((cache, temp))
            })
            .unwrap();
        assert_eq!(read, (-64000, 2), "the pooled read connection");
        let write = db
            .write_blocking(|ws| {
                let conn = ws.main().connection();
                let cache: i64 = conn.query_row("PRAGMA cache_size", [], |r| r.get(0))?;
                let temp: i64 = conn.query_row("PRAGMA temp_store", [], |r| r.get(0))?;
                Ok::<_, DbError>((cache, temp))
            })
            .unwrap();
        assert_eq!(write, (-64000, 2), "the writer connection");
    }

    /// P4.159: v4's integrity check's three arms, over stubbed outcomes — the
    /// `threw` arm is reached by no plant (a page overwrite is REPORTED by
    /// `quick_check`, the FAILED arm), so this is its pin. Each arm's line and
    /// verdict, both partitions.
    #[test]
    fn the_integrity_verdict_has_v4s_three_arms() {
        let cases: [(Partition, Result<String, DbError>, bool, &str); 6] = [
            (
                Partition::MountIndex,
                Ok("ok".to_string()),
                true,
                "INFO quilltap::db Mount index database integrity check passed module=database:mount-index-protection",
            ),
            (
                Partition::MountIndex,
                Ok("*** in database main ***".to_string()),
                false,
                "ERROR quilltap::db Mount index database integrity check FAILED — entering degraded mode module=database:mount-index-protection result=*** in database main ***",
            ),
            (
                Partition::MountIndex,
                Err(DbError::Sqlite(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(26),
                    Some("file is not a database".to_string()),
                ))),
                false,
                "ERROR quilltap::db Mount index database integrity check threw an error — entering degraded mode module=database:mount-index-protection error=file is not a database",
            ),
            (
                Partition::LlmLogs,
                Ok("ok".to_string()),
                true,
                "INFO quilltap::db LLM logs database integrity check passed module=database:llm-logs-protection",
            ),
            (
                Partition::LlmLogs,
                Ok("row 3 missing from index".to_string()),
                false,
                "ERROR quilltap::db LLM logs database integrity check FAILED — entering degraded mode module=database:llm-logs-protection result=row 3 missing from index",
            ),
            (
                Partition::LlmLogs,
                Err(DbError::Sqlite(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(11),
                    Some("database disk image is malformed".to_string()),
                ))),
                false,
                "ERROR quilltap::db LLM logs database integrity check threw an error — entering degraded mode module=database:llm-logs-protection error=database disk image is malformed",
            ),
        ];
        for (partition, outcome, want, line) in cases {
            let (got, lines) =
                crate::test_support::captured_with(|| integrity_verdict(partition, outcome));
            assert_eq!(got, want, "{line}");
            assert_eq!(lines, vec![line.to_string()]);
        }
    }

    /// P4.159 (R-B): a garbage sibling opens DEGRADED — no read pool, no
    /// writer — while a sibling with no path stays ABSENT, and the main
    /// database is untouched. The LLM logs make ONE attempt (no ladder), so
    /// this test pays no backoff.
    #[test]
    fn a_garbage_sibling_is_degraded_and_a_missing_one_absent() {
        let (dir, _) = make_db();
        let llm = dir.path().join("llm.db");
        std::fs::write(&llm, vec![0x5Au8; 4608]).unwrap();
        let db = Db::open(
            DbPaths {
                main: dir.path().join("main.db"),
                mount_index: None,
                llm_logs: Some(llm),
            },
            PEPPER,
        )
        .unwrap();
        assert_eq!(db.partition_state(Partition::Main), PartitionState::Open);
        assert_eq!(
            db.partition_state(Partition::LlmLogs),
            PartitionState::Degraded
        );
        assert_eq!(
            db.partition_state(Partition::MountIndex),
            PartitionState::Absent
        );
        assert!(matches!(
            db.read_llm_logs(|_| Ok(())),
            Err(DbError::PartitionUnavailable(WriteDbTarget::LlmLogs))
        ));
        let has_writer = db
            .write_blocking(|ws| Ok::<_, DbError>(ws.llm_logs().is_some()))
            .unwrap();
        assert!(!has_writer, "a degraded partition holds no writer (R-A)");
        let n: i64 = db
            .read_main(|conn| {
                Ok(conn.query_row("SELECT n FROM counter WHERE id = 'c'", [], |r| r.get(0))?)
            })
            .unwrap();
        assert_eq!(n, 0, "the main database opened as usual");
    }

    /// The blocking write API works off the runtime (the harness's `#[test]`
    /// shape) and is observed by a subsequent read.
    #[test]
    fn write_blocking_commits() {
        let (_dir, db) = make_db();
        db.write_blocking(|ws| {
            ws.main()
                .connection()
                .execute("UPDATE counter SET n = 7 WHERE id = 'c'", [])?;
            Ok::<(), DbError>(())
        })
        .unwrap();
        let n: i64 = db
            .read_main(|conn| {
                Ok(conn.query_row("SELECT n FROM counter WHERE id = 'c'", [], |r| r.get(0))?)
            })
            .unwrap();
        assert_eq!(n, 7);
    }
}
