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

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use rusqlite::{Connection, OpenFlags};
use tokio::sync::{mpsc, oneshot};

use crate::dbkey;
use crate::write_partition::WriteDbTarget;

use super::{DbError, Writer};

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
}

impl Db {
    /// Open an instance: RW writers for each present partition (owned by a new
    /// writer thread) plus a matching read pool. `pepper_b64` is the base64 pepper
    /// (as [`dbkey::load_pepper`] yields it).
    pub fn open(paths: DbPaths, pepper_b64: &str) -> Result<Db, DbError> {
        let key_hex =
            dbkey::pepper_b64_to_key_hex(pepper_b64).map_err(|e| DbError::Key(e.to_string()))?;

        // The writer thread's owned set — one RW connection per present partition.
        let writers = WriterSet {
            main: Writer::open_writable(&paths.main, pepper_b64)?,
            mount_index: match &paths.mount_index {
                Some(p) => Some(Writer::open_writable(p, pepper_b64)?),
                None => None,
            },
            llm_logs: match &paths.llm_logs {
                Some(p) => Some(Writer::open_writable(p, pepper_b64)?),
                None => None,
            },
        };

        // The read pool — direct, pooled read-only connections per partition.
        let reads = ReadPool {
            main: PartitionPool::new(paths.main.clone(), key_hex.clone()),
            mount_index: paths
                .mount_index
                .as_ref()
                .map(|p| PartitionPool::new(p.clone(), key_hex.clone())),
            llm_logs: paths
                .llm_logs
                .as_ref()
                .map(|p| PartitionPool::new(p.clone(), key_hex.clone())),
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
            inner: Arc::new(DbInner { reads, writes: tx }),
        })
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
    /// [`DbError::PartitionUnavailable`] if this instance has no mount-index DB.
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
    /// [`DbError::PartitionUnavailable`] if this instance has no llm-logs DB.
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
    Ok(conn)
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
