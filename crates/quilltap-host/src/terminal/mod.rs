//! The PTY host driver (P4.1c, D5) — v4 `lib/terminal/pty-manager.ts` over
//! `portable-pty` (replacing node-pty).
//!
//! A session manager owning the live PTY sessions of one host process: spawn
//! (shell/cwd/size/env defaults verbatim from v4), the in-memory ring buffer
//! (the LAST 256 KB of output, measured in UTF-16 code units like v4's JS
//! `.slice`), the raw transcript append stream under `<logs>/terminals/<id>.log`
//! (deliberately under `logs/`, not `files/`, so nothing indexes it — the DB
//! row stores only `transcriptPath`, transcript text never enters the DB), the
//! `terminal_sessions` DB row at spawn + the exit-stamp update, per-subscriber
//! broadcast of the [`protocol`] server messages, kill/write/resize/
//! kick-for-chat, the attach replay (the ring buffer as one `output` message,
//! then `meta`), and the exit sequence in v4's order.
//!
//! ## The Ariel flush drivers
//!
//! Raw output accumulates in a per-session buffer; a per-session tokio task
//! flushes it on **idle** (30 s of quiet since the last chunk) or **max-age**
//! (120 s since the buffer started filling, guarding a steady drip — e.g. a
//! long build — that would never satisfy the idle condition). A flush cleans
//! the buffer (`quilltap_core::terminal_clean::clean_terminal_output`) and
//! posts one Ariel message through the core writer
//! (`services::ariel_notifications`); when the writer actually posted
//! something, subscribers get a `chat-update` nudge. The timers live HERE
//! (D20: the core is scheduler-free); the durations are config knobs whose
//! defaults are v4's constants (tests shorten them).
//!
//! ## What is deliberately not here
//!
//! - The WebSocket route + upgrade handling → P4.2 (`quilltap-web`); this
//!   module delivers everything below it (the route only marshals
//!   [`protocol`] messages onto a subscription).
//! - The session-opened announcement — v4 posts it from the spawn REST route,
//!   not the PTY manager; the writer is ported
//!   (`ariel_notifications::post_ariel_session_opened_announcement`) and P4.2
//!   calls it.
//! - **Shell-init (deferred, tracked):** v4's per-session bootstrap
//!   (`lib/terminal/shell-init.ts` — a bash `--rcfile` / zsh `ZDOTDIR`
//!   carrying a version-matched `quilltap` alias, bash completions, and a
//!   cwd-aware prompt) targets the **Node launcher**
//!   (`packages/quilltap/bin/quilltap.js`, invoked as `node "$__qt_script"`),
//!   which does not exist in v5. A v5 alias needs the P4.3 `quilltap` binary
//!   path (alias `quilltap='<path-to-v5-binary>'` + `quilltap completion
//!   bash`), so the machinery lands with/after P4.3. The degradation matches
//!   v4's own fallback (a bootstrap failure spawns a plain shell): every
//!   session still carries `QUILLTAP_DATA_DIR` (set authoritatively in the
//!   spawn env, below), only the alias/completions/prompt are absent — and
//!   without the rcfile, a user dotfile exporting its own `QUILLTAP_DATA_DIR`
//!   can shadow the spawn env's (v4's rcfile re-exports it after `.bashrc`;
//!   accepted as part of the deferral).

pub mod protocol;
pub mod scrollback;

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use tokio::sync::mpsc;
use tokio::sync::Notify;
use tokio::time::Instant;

use quilltap_core::clock::now_iso;
use quilltap_core::db::runtime::Db;
use quilltap_core::db::terminal_sessions;
use quilltap_core::jsstr::{utf16_len, utf16_slice_from};
use quilltap_core::services::ariel_notifications::{
    post_ariel_session_closed_announcement, post_ariel_terminal_output_announcement,
    reconcile_terminal_sessions_for_chat, ArielFlushReason, ArielSessionClosedAnnouncement,
    ArielTerminalOutputAnnouncement, TerminalLivenessProbe,
};
use quilltap_core::terminal_clean::clean_terminal_output;

use protocol::{ChatUpdateReason, PtySessionMeta, WsServerMessage};

/// v4 `MAX_RING_BUFFER_SIZE` — 256 KB, measured in UTF-16 code units (v4 caps a
/// JS string with `.slice`).
const MAX_RING_BUFFER_SIZE: usize = 256 * 1024;

/// v4 `ARIEL_FLUSH_IDLE_MS` — flush after this much quiet time since the last
/// chunk arrived.
pub const ARIEL_FLUSH_IDLE: Duration = Duration::from_millis(30_000);
/// v4 `ARIEL_FLUSH_MAX_AGE_MS` — flush regardless of activity once the buffer
/// is at least this old.
pub const ARIEL_FLUSH_MAX_AGE: Duration = Duration::from_millis(120_000);

/// Constructor parameters (directories passed in — the path resolution lives in
/// lane d's `paths` module; this module never reads it).
#[derive(Clone)]
pub struct TerminalManagerConfig {
    /// The engine handle: `terminal_sessions` rows + the Ariel announcements.
    pub db: Db,
    /// Default working directory for new sessions (v4 `getFilesDir()`).
    pub files_dir: PathBuf,
    /// The logs directory; transcripts live at `<logs_dir>/terminals/<id>.log`.
    pub logs_dir: PathBuf,
    /// The instance data dir every session's `QUILLTAP_DATA_DIR` pins (v4
    /// `getBaseDataDir()`).
    pub data_dir: PathBuf,
    /// The Ariel idle flush window (default [`ARIEL_FLUSH_IDLE`]).
    pub ariel_flush_idle: Duration,
    /// The Ariel max-age flush cap (default [`ARIEL_FLUSH_MAX_AGE`]).
    pub ariel_flush_max_age: Duration,
}

impl TerminalManagerConfig {
    pub fn new(db: Db, files_dir: PathBuf, logs_dir: PathBuf, data_dir: PathBuf) -> Self {
        Self {
            db,
            files_dir,
            logs_dir,
            data_dir,
            ariel_flush_idle: ARIEL_FLUSH_IDLE,
            ariel_flush_max_age: ARIEL_FLUSH_MAX_AGE,
        }
    }
}

/// v4 `spawn(opts)` options.
#[derive(Debug, Clone, Default)]
pub struct SpawnOptions {
    pub chat_id: String,
    pub label: Option<String>,
    /// Overrides the platform default shell (v4: `opts.shell || (win32 ?
    /// COMSPEC ?? 'powershell.exe' : SHELL ?? '/bin/bash')` — JS `||`, so an
    /// empty string falls through to the default).
    pub shell: Option<String>,
    /// Overrides the default cwd (`files_dir`); empty string falls through.
    pub cwd: Option<String>,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
    /// Extra environment entries merged over the inherited process env; the
    /// manager sets `QUILLTAP_DATA_DIR` authoritatively LAST (v4's spread
    /// order `{...process.env, ...opts.env, QUILLTAP_DATA_DIR}`).
    pub env: Vec<(String, String)>,
}

/// Spawn failure (v4 throws; the route maps to 500).
#[derive(Debug)]
pub enum SpawnError {
    /// PTY open / command spawn failed.
    Pty(String),
    /// The `terminal_sessions` row insert failed (v4 rethrows `dbErr` after
    /// killing the fresh PTY).
    Db(String),
}

impl std::fmt::Display for SpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpawnError::Pty(e) => write!(f, "failed to spawn PTY: {e}"),
            SpawnError::Db(e) => write!(f, "failed to persist session: {e}"),
        }
    }
}

impl std::error::Error for SpawnError {}

/// A live subscription: the receiver of server messages plus the token
/// [`TerminalManager::unsubscribe`] takes. Dropping the receiver alone does
/// not unsubscribe (the route calls `unsubscribe` on socket close, mirroring
/// v4's `ws.on('close')`).
pub struct Subscription {
    pub id: u64,
    pub rx: mpsc::UnboundedReceiver<WsServerMessage>,
}

// ---------------------------------------------------------------------------
// Internal session state
// ---------------------------------------------------------------------------

struct SessionIo {
    ring_buffer: String,
    ariel_buffer: String,
    /// Instant of the most recent output chunk (drives the idle deadline).
    last_data_at: Option<Instant>,
    /// Instant the Ariel buffer started filling after the last flush (drives
    /// the max-age deadline — v4 arms the max-age timer only when unset).
    ariel_started_at: Option<Instant>,
    subscribers: Vec<(u64, mpsc::UnboundedSender<WsServerMessage>)>,
    transcript: Option<File>,
}

struct SessionShared {
    meta: Mutex<PtySessionMeta>,
    io: Mutex<SessionIo>,
    /// Wakes the Ariel timer task on new data / exit.
    notify: Notify,
    exited: AtomicBool,
    /// Child pid for signal delivery (unix `SIGTERM`).
    pid: Option<u32>,
    /// Fallback killer (windows / signal failure) — split from the `Child`
    /// the reader thread blocks in `wait()` on.
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    writer: Mutex<Option<Box<dyn std::io::Write + Send>>>,
}

impl SessionShared {
    fn snapshot_meta(&self) -> PtySessionMeta {
        self.meta.lock().expect("meta lock").clone()
    }
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// Cap `ring` to the LAST `max_units` UTF-16 code units — v4's
/// `(ringBuffer + data).slice(len - MAX)`.
fn cap_ring_utf16(ring: &mut String, max_units: usize) {
    let len = utf16_len(ring);
    if len > max_units {
        *ring = utf16_slice_from(ring, len - max_units);
    }
}

/// Incremental UTF-8 decode: append `chunk` to `pending`, decode the complete
/// prefix (invalid sequences → U+FFFD, matching node-pty's `StringDecoder`
/// behavior), and keep an incomplete trailing multi-byte sequence in `pending`
/// for the next read.
fn decode_utf8_incremental(pending: &mut Vec<u8>, chunk: &[u8]) -> String {
    let mut data = std::mem::take(pending);
    data.extend_from_slice(chunk);

    let mut out = String::new();
    let mut rest: &[u8] = &data;
    loop {
        match std::str::from_utf8(rest) {
            Ok(s) => {
                out.push_str(s);
                rest = &[];
                break;
            }
            Err(e) => {
                let valid = e.valid_up_to();
                out.push_str(std::str::from_utf8(&rest[..valid]).expect("valid prefix"));
                match e.error_len() {
                    Some(n) => {
                        out.push('\u{FFFD}');
                        rest = &rest[valid + n..];
                    }
                    None => {
                        // Incomplete trailing sequence — hold it for the next
                        // chunk (unless it's absurdly long, which can't happen
                        // for UTF-8: error_len() == None only within 3 bytes of
                        // the end).
                        rest = &rest[valid..];
                        *pending = rest.to_vec();
                        return out;
                    }
                }
            }
        }
    }
    pending.clear();
    let _ = rest;
    out
}

/// Read the last `max_bytes` of a file as UTF-8 (lossy) — v4 `tailFile`
/// (`Buffer.toString('utf8')` replaces invalid sequences the same way).
pub(crate) fn tail_file(path: &std::path::Path, max_bytes: u64) -> std::io::Result<String> {
    use std::io::{Read as _, Seek as _, SeekFrom};
    let mut f = File::open(path)?;
    let size = f.metadata()?.len();
    let start = size.saturating_sub(max_bytes);
    f.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::with_capacity((size - start) as usize);
    f.read_to_end(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

// ---------------------------------------------------------------------------
// The manager
// ---------------------------------------------------------------------------

/// The session manager (v4's `PtyManager` singleton — here an instance the
/// composition root owns; pin one per host process).
pub struct TerminalManager {
    db: Db,
    files_dir: PathBuf,
    logs_dir: PathBuf,
    data_dir: PathBuf,
    ariel_flush_idle: Duration,
    ariel_flush_max_age: Duration,
    sessions: Mutex<HashMap<String, Arc<SessionShared>>>,
    next_sub_id: AtomicU64,
    /// Runtime handle the blocking reader threads use to run the async exit
    /// sequence (DB update + announcements).
    rt: tokio::runtime::Handle,
}

impl TerminalManager {
    /// Build a manager. Must be called from within a tokio runtime (the host's)
    /// — the Ariel flush drivers and the exit sequences run on it.
    pub fn new(config: TerminalManagerConfig) -> Arc<Self> {
        Arc::new(Self {
            db: config.db,
            files_dir: config.files_dir,
            logs_dir: config.logs_dir,
            data_dir: config.data_dir,
            ariel_flush_idle: config.ariel_flush_idle,
            ariel_flush_max_age: config.ariel_flush_max_age,
            sessions: Mutex::new(HashMap::new()),
            next_sub_id: AtomicU64::new(1),
            rt: tokio::runtime::Handle::current(),
        })
    }

    fn get_session(&self, id: &str) -> Option<Arc<SessionShared>> {
        self.sessions
            .lock()
            .expect("sessions lock")
            .get(id)
            .cloned()
    }

    /// Whether the manager currently holds a session with this id (v4
    /// `ptyManager.get(id)` truthiness — includes exited sessions that still
    /// have subscribers attached).
    pub fn contains(&self, id: &str) -> bool {
        self.sessions
            .lock()
            .expect("sessions lock")
            .contains_key(id)
    }

    /// v4 `list()` — metas of every session in the map.
    pub fn list(&self) -> Vec<PtySessionMeta> {
        self.sessions
            .lock()
            .expect("sessions lock")
            .values()
            .map(|s| s.snapshot_meta())
            .collect()
    }

    /// The session meta, when the session is in the map.
    pub fn session_meta(&self, id: &str) -> Option<PtySessionMeta> {
        self.get_session(id).map(|s| s.snapshot_meta())
    }

    /// v4 `getRingBuffer(id)` — `None` for an unknown session.
    pub fn get_ring_buffer(&self, id: &str) -> Option<String> {
        self.get_session(id)
            .map(|s| s.io.lock().expect("io lock").ring_buffer.clone())
    }

    /// The pending (unflushed) Ariel buffer length, for instrumentation/tests.
    pub fn ariel_buffer_len(&self, id: &str) -> Option<usize> {
        self.get_session(id)
            .map(|s| s.io.lock().expect("io lock").ariel_buffer.len())
    }

    /// v4 `spawn(opts)` — spawn a shell into a fresh PTY, wire the transcript /
    /// ring buffer / Ariel buffer / subscriber broadcast, persist the
    /// `terminal_sessions` row (pinned to the pre-generated id so the in-memory
    /// map and the DB stay in sync), and return the session meta.
    pub async fn spawn(self: &Arc<Self>, opts: SpawnOptions) -> Result<PtySessionMeta, SpawnError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_iso();

        // v4: `opts.shell || (win32 ? COMSPEC ?? 'powershell.exe' : SHELL ?? '/bin/bash')`.
        let shell = match opts.shell.as_deref() {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => default_shell(),
        };
        // v4: `opts.cwd || getFilesDir()`.
        let cwd = match opts.cwd.as_deref() {
            Some(c) if !c.is_empty() => PathBuf::from(c),
            _ => self.files_dir.clone(),
        };
        let cols = opts.cols.unwrap_or(80);
        let rows = opts.rows.unwrap_or(24);

        // Transcripts live under logs/, not files/ (see the module header).
        let transcripts_dir = self.logs_dir.join("terminals");
        let transcript_path = transcripts_dir.join(format!("{id}.log"));
        // v4 warns + degrades on directory/stream failures (a transcript problem
        // must never block spawning a shell).
        let _ = std::fs::create_dir_all(&transcripts_dir);
        let transcript: Option<File> = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&transcript_path)
            .ok();

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| SpawnError::Pty(e.to_string()))?;

        // Env: the inherited process env (CommandBuilder's base env), the
        // caller's overrides, then QUILLTAP_DATA_DIR authoritatively LAST —
        // v4's `{...process.env, ...opts.env, QUILLTAP_DATA_DIR: dataDir}`.
        // node-pty's `name: 'xterm-256color'` becomes the TERM env var.
        let mut cmd = CommandBuilder::new(&shell);
        cmd.cwd(&cwd);
        cmd.env("TERM", "xterm-256color");
        for (k, v) in &opts.env {
            cmd.env(k, v);
        }
        cmd.env("QUILLTAP_DATA_DIR", &self.data_dir);
        // Shell-init (`--rcfile` / ZDOTDIR alias bootstrap) is a tracked
        // deferral — see the module header. No extra argv / env overrides.

        let mut child = match pair.slave.spawn_command(cmd) {
            Ok(c) => c,
            Err(e) => return Err(SpawnError::Pty(e.to_string())),
        };
        // Drop the parent's slave handle; the child holds its own. The reader
        // sees EOF once the child (and any holdouts) exit.
        drop(pair.slave);

        let pid = child.process_id();
        let killer = child.clone_killer();
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| SpawnError::Pty(e.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| SpawnError::Pty(e.to_string()))?;

        let meta = PtySessionMeta {
            id: id.clone(),
            chat_id: opts.chat_id.clone(),
            label: opts.label.clone(),
            shell: shell.clone(),
            cwd: cwd.to_string_lossy().into_owned(),
            started_at: now.clone(),
            exited_at: None,
            exit_code: None,
            transcript_path: transcript
                .is_some()
                .then(|| transcript_path.to_string_lossy().into_owned()),
        };

        let shared = Arc::new(SessionShared {
            meta: Mutex::new(meta.clone()),
            io: Mutex::new(SessionIo {
                ring_buffer: String::new(),
                ariel_buffer: String::new(),
                last_data_at: None,
                ariel_started_at: None,
                subscribers: Vec::new(),
                transcript,
            }),
            notify: Notify::new(),
            exited: AtomicBool::new(false),
            pid,
            killer: Mutex::new(killer),
            master: Mutex::new(Some(pair.master)),
            writer: Mutex::new(Some(writer)),
        });

        // The reader thread (blocking PTY reads; node-pty's onData analogue).
        {
            let shared = Arc::clone(&shared);
            let manager = Arc::downgrade(self);
            let rt = self.rt.clone();
            let mut reader = reader;
            std::thread::spawn(move || {
                let mut pending: Vec<u8> = Vec::new();
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        // EIO is the normal Linux end-of-session signal.
                        Err(_) => break,
                        Ok(n) => {
                            let text = decode_utf8_incremental(&mut pending, &buf[..n]);
                            if !text.is_empty() {
                                Self::on_data(&shared, &text);
                            }
                        }
                    }
                }
                if !pending.is_empty() {
                    let text = String::from_utf8_lossy(&pending).into_owned();
                    Self::on_data(&shared, &text);
                }
                // Reap the child for its exit status, then run the exit
                // sequence on the runtime.
                let status = child.wait().ok();
                let (code, signal) = match &status {
                    Some(s) => (Some(s.exit_code() as i64), s.signal().map(str::to_string)),
                    None => (None, None),
                };
                if let Some(manager) = manager.upgrade() {
                    rt.block_on(manager.exit_sequence(&shared, code, signal));
                }
            });
        }

        // Persist the DB row BEFORE registering the session (v4's order). The
        // base `_create` mints createdAt/updatedAt itself; the id is pinned so
        // the map and the row agree.
        {
            let create = terminal_sessions::TsCreate {
                chat_id: meta.chat_id.clone(),
                label: meta.label.clone(),
                shell: meta.shell.clone(),
                cwd: meta.cwd.clone(),
                started_at: meta.started_at.clone(),
                exited_at: None,
                exit_code: None,
                transcript_path: meta.transcript_path.clone(),
            };
            let row_id = id.clone();
            let db_result = self
                .db
                .write(move |writers| {
                    let created = now_iso();
                    writers.main().terminal_sessions().create(
                        &create,
                        &terminal_sessions::CreateOptions {
                            id: row_id,
                            created_at: created.clone(),
                            updated_at: created,
                        },
                    )
                })
                .await;
            if let Err(e) = db_result {
                // v4: kill the fresh PTY, end the transcript stream, rethrow.
                let _ = shared.killer.lock().expect("killer lock").kill();
                shared.io.lock().expect("io lock").transcript = None;
                return Err(SpawnError::Db(e.to_string()));
            }
        }

        self.sessions
            .lock()
            .expect("sessions lock")
            .insert(id.clone(), Arc::clone(&shared));

        // The Ariel flush driver (idle / max-age timers; D20 — host-owned).
        {
            let shared = Arc::clone(&shared);
            let db = self.db.clone();
            let idle = self.ariel_flush_idle;
            let max_age = self.ariel_flush_max_age;
            self.rt.spawn(async move {
                ariel_timer_loop(db, shared, idle, max_age).await;
            });
        }

        Ok(meta)
    }

    /// The per-chunk data path (v4's `onData` handler): cap-append the ring
    /// buffer, grow the Ariel buffer (+ schedule the flush), append the raw
    /// transcript, and broadcast an `output` frame.
    fn on_data(shared: &Arc<SessionShared>, data: &str) {
        {
            let mut io = shared.io.lock().expect("io lock");

            io.ring_buffer.push_str(data);
            cap_ring_utf16(&mut io.ring_buffer, MAX_RING_BUFFER_SIZE);

            io.ariel_buffer.push_str(data);
            let now = Instant::now();
            io.last_data_at = Some(now);
            if io.ariel_started_at.is_none() {
                io.ariel_started_at = Some(now);
            }

            if let Some(t) = io.transcript.as_mut() {
                // v4 warns + keeps going on a transcript write failure.
                let _ = t.write_all(data.as_bytes());
            }

            let msg = WsServerMessage::Output {
                data: data.to_string(),
            };
            for (_, tx) in &io.subscribers {
                let _ = tx.send(msg.clone());
            }
        }
        shared.notify.notify_one();
    }

    /// v4's `onExit` handler, in v4's order: stamp the meta, drain the Ariel
    /// buffer (`session-closed`), broadcast `exit` + close subscribers, end the
    /// transcript stream, persist the exit state, post the close announcement.
    async fn exit_sequence(
        self: &Arc<Self>,
        shared: &Arc<SessionShared>,
        code: Option<i64>,
        signal: Option<String>,
    ) {
        let now = now_iso();
        let (chat_id, session_id) = {
            let mut meta = shared.meta.lock().expect("meta lock");
            meta.exited_at = Some(now.clone());
            meta.exit_code = code;
            (meta.chat_id.clone(), meta.id.clone())
        };

        // Drain any remaining buffered output so trailing bytes (and the final
        // prompt right before exit) make it into the chat before the
        // session-closed announcement lands.
        flush_ariel(&self.db, shared, ArielFlushReason::SessionClosed).await;

        {
            let mut io = shared.io.lock().expect("io lock");
            let msg = WsServerMessage::Exit {
                code,
                signal: signal.clone(),
            };
            for (_, tx) in &io.subscribers {
                let _ = tx.send(msg.clone());
            }
            // Dropping the senders closes each subscription channel — the P4.2
            // route closes the socket (1000, 'Session exited') on channel end.
            io.subscribers.clear();
            // End the transcript stream.
            io.transcript = None;
        }

        shared.exited.store(true, Ordering::SeqCst);
        shared.notify.notify_one();

        // Release the PTY handles (writer EOF + master fd).
        *shared.writer.lock().expect("writer lock") = None;
        *shared.master.lock().expect("master lock") = None;

        // Persist the exit state (v4 fire-and-forget `.catch(log)`; awaited
        // here — same DB effect, deterministic order).
        {
            let sid = session_id.clone();
            let exited_at = now.clone();
            let _ = self
                .db
                .write(move |writers| {
                    let updated_at = now_iso();
                    terminal_sessions::mark_session_exited(
                        writers.main().connection(),
                        &sid,
                        &exited_at,
                        code.map(|c| c as f64),
                        &updated_at,
                    )
                    .map(|_| ())
                })
                .await;
        }

        post_ariel_session_closed_announcement(
            &self.db,
            &ArielSessionClosedAnnouncement {
                chat_id,
                session_id,
                exit_code: code,
            },
        )
        .await;
    }

    /// v4 `kill(id)` — send `SIGTERM` (the only signal v4 callers use). Unknown
    /// sessions and signal failures are logged-and-ignored in v4; here they are
    /// silent no-ops (host logging is P4.2+).
    pub fn kill(&self, id: &str) {
        let Some(shared) = self.get_session(id) else {
            return;
        };
        #[cfg(unix)]
        {
            if let Some(pid) = shared.pid {
                // SAFETY: plain kill(2) with a caught return value.
                let rc = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
                if rc == 0 {
                    return;
                }
            }
        }
        // Windows / signal-failure fallback: the portable-pty killer.
        let _ = shared.killer.lock().expect("killer lock").kill();
    }

    /// v4 `subscribe(id, ws)` — refuse unknown or exited sessions; otherwise
    /// register a subscriber and replay the ring buffer as one `output` frame
    /// followed by a `meta` frame.
    pub fn subscribe(&self, id: &str) -> Option<Subscription> {
        let shared = self.get_session(id)?;
        if shared.meta.lock().expect("meta lock").exited_at.is_some() {
            return None;
        }

        let sub_id = self.next_sub_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::unbounded_channel();

        let mut io = shared.io.lock().expect("io lock");
        let _ = tx.send(WsServerMessage::Output {
            data: io.ring_buffer.clone(),
        });
        let _ = tx.send(WsServerMessage::Meta {
            meta: shared.snapshot_meta(),
        });
        io.subscribers.push((sub_id, tx));

        Some(Subscription { id: sub_id, rx })
    }

    /// v4 `unsubscribe(id, ws)` — remove the subscriber; once the session has
    /// exited AND no subscribers remain, drop it from the map.
    pub fn unsubscribe(&self, id: &str, sub_id: u64) {
        let Some(shared) = self.get_session(id) else {
            return;
        };
        let remove_session = {
            let mut io = shared.io.lock().expect("io lock");
            io.subscribers.retain(|(sid, _)| *sid != sub_id);
            shared.meta.lock().expect("meta lock").exited_at.is_some() && io.subscribers.is_empty()
        };
        if remove_session {
            self.sessions.lock().expect("sessions lock").remove(id);
        }
    }

    /// v4 `write(id, data)` — returns `false` for an unknown session or a
    /// failed write.
    pub fn write(&self, id: &str, data: &str) -> bool {
        let Some(shared) = self.get_session(id) else {
            return false;
        };
        let mut writer = shared.writer.lock().expect("writer lock");
        match writer.as_mut() {
            Some(w) => w.write_all(data.as_bytes()).and_then(|_| w.flush()).is_ok(),
            None => false,
        }
    }

    /// v4 `resize(id, cols, rows)` — failures are swallowed.
    pub fn resize(&self, id: &str, cols: u16, rows: u16) {
        let Some(shared) = self.get_session(id) else {
            return;
        };
        let master = shared.master.lock().expect("master lock");
        if let Some(m) = master.as_ref() {
            let _ = m.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }

    /// v4 `kickAllForChat(chatId)` — drain each session's pending Ariel buffer
    /// synchronously with the kick (not whenever the OS reaps the process),
    /// send SIGTERM, and drop the map entries. The exit sequence still runs
    /// from the reader thread when the process dies (the DB exit stamp + close
    /// announcement land there, exactly as v4's `onExit` closure does).
    pub async fn kick_all_for_chat(&self, chat_id: &str) {
        let ids: Vec<String> = {
            let sessions = self.sessions.lock().expect("sessions lock");
            sessions
                .iter()
                .filter(|(_, s)| s.meta.lock().expect("meta lock").chat_id == chat_id)
                .map(|(id, _)| id.clone())
                .collect()
        };

        for id in ids {
            if let Some(shared) = self.get_session(&id) {
                flush_ariel(&self.db, &shared, ArielFlushReason::SessionClosed).await;
            }
            self.kill(&id);
            self.sessions.lock().expect("sessions lock").remove(&id);
        }
    }

    /// The reconcile pass for a chat (v4 `reconcileTerminalSessionsForChat`),
    /// with this manager's live-session map as the `is_live` probe.
    pub async fn reconcile_for_chat(&self, chat_id: &str) -> usize {
        reconcile_terminal_sessions_for_chat(&self.db, chat_id, &|id| self.contains(id)).await
    }
}

/// The manager IS the engine's live-PTY probe (v4 `ptyManager.get(id)`),
/// threaded through `EngineAssembly::terminal_probe` so `ChatGet`'s reconcile
/// pass spares live sessions.
impl TerminalLivenessProbe for TerminalManager {
    fn is_live(&self, session_id: &str) -> bool {
        self.contains(session_id)
    }
}

fn default_shell() -> String {
    #[cfg(windows)]
    {
        std::env::var("COMSPEC").unwrap_or_else(|_| "powershell.exe".to_string())
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
    }
}

/// v4 `flushArielBuffer`: clear the timers (here: the started/last markers the
/// deadlines derive from), drain the buffer (empty → no-op), clean it, post one
/// Ariel message, and — only when the writer actually wrote something —
/// broadcast a `chat-update` nudge to subscribers. Errors are swallowed:
/// terminal I/O must not fail because chat persistence had a hiccup.
async fn flush_ariel(db: &Db, shared: &Arc<SessionShared>, reason: ArielFlushReason) {
    let (raw, chat_id, session_id) = {
        let mut io = shared.io.lock().expect("io lock");
        io.last_data_at = None;
        io.ariel_started_at = None;
        let raw = std::mem::take(&mut io.ariel_buffer);
        let meta = shared.meta.lock().expect("meta lock");
        (raw, meta.chat_id.clone(), meta.id.clone())
    };
    if raw.is_empty() {
        return;
    }

    let cleaned = clean_terminal_output(&raw);
    let posted = post_ariel_terminal_output_announcement(
        db,
        &ArielTerminalOutputAnnouncement {
            chat_id: chat_id.clone(),
            session_id,
            cleaned_output: cleaned,
            reason,
        },
    )
    .await;

    // Only nudge the client if the writer actually wrote something —
    // empty/whitespace flushes return None and there's nothing new to refresh.
    if posted.is_some() {
        let io = shared.io.lock().expect("io lock");
        let msg = WsServerMessage::ChatUpdate {
            chat_id,
            reason: Some(ChatUpdateReason::ArielTerminalOutput),
        };
        for (_, tx) in &io.subscribers {
            let _ = tx.send(msg.clone());
        }
    }
}

/// The per-session Ariel flush driver: sleeps until the earlier of the idle
/// deadline (last chunk + `idle`) and the max-age deadline (buffer start +
/// `max_age`), recomputing whenever new data arrives; flushes with the reason
/// of whichever deadline fired. Equivalent to v4's two `setTimeout`s (the idle
/// timer reset per chunk; the max-age timer armed once per fill cycle).
async fn ariel_timer_loop(db: Db, shared: Arc<SessionShared>, idle: Duration, max_age: Duration) {
    loop {
        if shared.exited.load(Ordering::SeqCst) {
            break;
        }
        let deadline: Option<(Instant, ArielFlushReason)> = {
            let io = shared.io.lock().expect("io lock");
            if io.ariel_buffer.is_empty() {
                None
            } else {
                let idle_dl = io.last_data_at.map(|t| t + idle);
                let age_dl = io.ariel_started_at.map(|t| t + max_age);
                match (idle_dl, age_dl) {
                    (Some(i), Some(a)) if a <= i => Some((a, ArielFlushReason::MaxAge)),
                    (Some(i), _) => Some((i, ArielFlushReason::Idle)),
                    (None, Some(a)) => Some((a, ArielFlushReason::MaxAge)),
                    (None, None) => None,
                }
            }
        };
        match deadline {
            None => shared.notify.notified().await,
            Some((when, reason)) => {
                tokio::select! {
                    _ = shared.notify.notified() => {}
                    _ = tokio::time::sleep_until(when) => {
                        flush_ariel(&db, &shared, reason).await;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_cap_keeps_the_tail_in_utf16_units() {
        let mut ring = String::new();
        ring.push_str(&"a".repeat(10));
        cap_ring_utf16(&mut ring, 4);
        assert_eq!(ring, "aaaa");

        // 'é' = 1 UTF-16 unit, 2 UTF-8 bytes — the cap counts units.
        let mut ring = "\u{e9}".repeat(10);
        cap_ring_utf16(&mut ring, 3);
        assert_eq!(ring, "\u{e9}\u{e9}\u{e9}");
    }

    #[test]
    fn incremental_decode_holds_partial_sequences() {
        let mut pending = Vec::new();
        // '€' is E2 82 AC — split across reads.
        let out1 = decode_utf8_incremental(&mut pending, &[b'a', 0xE2, 0x82]);
        assert_eq!(out1, "a");
        assert_eq!(pending, vec![0xE2, 0x82]);
        let out2 = decode_utf8_incremental(&mut pending, &[0xAC, b'b']);
        assert_eq!(out2, "\u{20ac}b");
        assert!(pending.is_empty());
    }

    #[test]
    fn incremental_decode_replaces_invalid_bytes() {
        let mut pending = Vec::new();
        let out = decode_utf8_incremental(&mut pending, &[b'x', 0xFF, b'y']);
        assert_eq!(out, "x\u{FFFD}y");
        assert!(pending.is_empty());
    }

    #[test]
    fn tail_file_reads_last_bytes() {
        let dir = std::env::temp_dir().join(format!("qt-tailfile-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.log");
        std::fs::write(&p, "0123456789").unwrap();
        assert_eq!(tail_file(&p, 4).unwrap(), "6789");
        assert_eq!(tail_file(&p, 100).unwrap(), "0123456789");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
