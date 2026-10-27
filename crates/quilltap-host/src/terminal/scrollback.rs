//! The production [`TerminalReadSource`] — v4's resolution in
//! `lib/tools/handlers/terminal-handler.ts` (`executeTerminalReadTool`, bug 255
//! — v4 `a6292f169`, unmoved to the `322a5488e` pin):
//!
//! ```text
//! const liveSession = ptyManager.get(input.sessionId);
//! if (liveSession && session.exitedAt == null) {
//!   rendering = await ptyManager.renderScreen(input.sessionId);
//!   raw = ptyManager.getRingBuffer(input.sessionId) ?? '';
//! } else if (session.transcriptPath) {
//!   try {
//!     raw = await tailFile(session.transcriptPath, TRANSCRIPT_REPLAY_BYTES);
//!     rendering = await renderTranscript(raw, session.cols ?? DEFAULT_COLS,
//!       session.rows ?? DEFAULT_ROWS);
//!   } catch (err) {
//!     moduleLogger.warn('Failed to read transcript file', { sessionId,
//!       transcriptPath: session.transcriptPath, error });
//!   }
//! }
//! ```
//!
//! `session` there is the DB row (the ported `terminal_read` handler reads it
//! too, for its own checks — v4's double consultation of manager + DB). The
//! transcript tail is the last 4 MiB, decoded lossily (Node
//! `Buffer.toString('utf8')`), replayed at the row's stored geometry by core's
//! [`snapshot_from_transcript`] — the one transcript arm the harness's test
//! source calls too. A read failure logs v4's WARN (`module =
//! "terminal-tool-handler"`, the child-logger binding as a field) on the
//! caller's thread and answers no rendering. A session id with no DB row
//! answers the empty snapshot — unreachable through the tool dispatch (the
//! handler errors on the row lookup first and consults this source after), but
//! the seam contract is total.
//!
//! The struct keeps its pre-bug-255 name (`PtyScrollbackSource`): the host's
//! spine names it, and a rename is P4.D335's file, not this lane's (P4.D337
//! R-6).

use std::path::Path;
use std::sync::Arc;

use quilltap_core::db::runtime::Db;
use quilltap_core::db::terminal_sessions;
use quilltap_core::tools::executor::{TerminalReadSnapshot, TerminalReadSource};
use quilltap_core::tools::terminal::{snapshot_from_transcript, TRANSCRIPT_REPLAY_BYTES};

use super::{tail_file, TerminalManager};

/// The live-screen / transcript-replay resolver, over a [`TerminalManager`]
/// and the engine [`Db`].
pub struct PtyScrollbackSource {
    manager: Arc<TerminalManager>,
    db: Db,
}

impl PtyScrollbackSource {
    pub fn new(manager: Arc<TerminalManager>, db: Db) -> Self {
        Self { manager, db }
    }
}

impl TerminalReadSource for PtyScrollbackSource {
    fn read(&self, session_id: &str) -> TerminalReadSnapshot {
        let sid = session_id.to_string();
        let row = match self
            .db
            .read_main(move |conn| terminal_sessions::find_by_id(conn, &sid))
        {
            Ok(Some(row)) => row,
            // No row / read failure → the handler never reaches this branch
            // (it errors on the row lookup first); the default keeps the seam
            // total.
            _ => return TerminalReadSnapshot::default(),
        };

        // v4: a session present in the manager map ("liveSession") AND a DB row
        // whose exitedAt is still null renders the live emulator.
        if self.manager.contains(session_id) && row.exited_at.is_none() {
            return TerminalReadSnapshot {
                rendering: self.manager.render_screen(session_id),
                raw: self.manager.get_ring_buffer(session_id).unwrap_or_default(),
            };
        }

        // Otherwise the transcript's last 4 MiB, replayed at the stored geometry.
        let Some(path) = row.transcript_path.as_deref() else {
            return TerminalReadSnapshot::default();
        };
        match tail_file(Path::new(path), TRANSCRIPT_REPLAY_BYTES) {
            Ok(raw) => snapshot_from_transcript(raw, row.cols, row.rows),
            Err(e) => {
                tracing::warn!(
                    target: "quilltap::terminal",
                    module = "terminal-tool-handler",
                    sessionId = session_id,
                    transcriptPath = path,
                    error = %node_open_error(&e, path),
                    "Failed to read transcript file"
                );
                TerminalReadSnapshot::default()
            }
        }
    }
}

/// Node's `fs.open` error message — `ENOENT: no such file or directory, open
/// '<path>'` for a missing file (what v4's WARN carries; measured at the pin);
/// any other failure renders the OS text.
fn node_open_error(e: &std::io::Error, path: &str) -> String {
    match e.kind() {
        std::io::ErrorKind::NotFound => {
            format!("ENOENT: no such file or directory, open '{path}'")
        }
        _ => e.to_string(),
    }
}
