//! The production [`TerminalScrollbackSource`] — v4's scrollback resolution in
//! `lib/tools/handlers/terminal-handler.ts` (`executeTerminalReadTool`):
//!
//! ```text
//! const liveSession = ptyManager.get(input.sessionId);
//! if (liveSession && session.exitedAt == null) {
//!   fullContent = ptyManager.getRingBuffer(input.sessionId) ?? '';
//! } else if (session.transcriptPath) {
//!   try { fullContent = await tailFile(session.transcriptPath); }  // last 1 MB
//!   catch { /* warn; fullContent stays '' */ }
//! }
//! ```
//!
//! `session` there is the DB row (the ported `terminal_read` handler reads it
//! again for its own fields, so the extra row read here is exactly v4's double
//! consultation of manager + DB). The transcript tail is capped at 1 MB
//! (`tailFile`'s `maxBytes = 1_000_000` default) and decoded as lossy UTF-8
//! (Node `Buffer.toString('utf8')`); read failures fall back to the empty
//! string. A session id with no DB row returns `''` — unreachable through the
//! tool dispatch (the handler errors on session-not-found before consulting the
//! scrollback source), but the seam contract is total.

use std::path::Path;
use std::sync::Arc;

use quilltap_core::db::runtime::Db;
use quilltap_core::db::terminal_sessions;
use quilltap_core::tools::executor::TerminalScrollbackSource;

use super::{tail_file, TerminalManager};

/// v4 `tailFile`'s default `maxBytes`.
const TRANSCRIPT_TAIL_MAX_BYTES: u64 = 1_000_000;

/// The live-PTY / transcript-tail scrollback resolver, over a
/// [`TerminalManager`] and the engine [`Db`].
pub struct PtyScrollbackSource {
    manager: Arc<TerminalManager>,
    db: Db,
}

impl PtyScrollbackSource {
    pub fn new(manager: Arc<TerminalManager>, db: Db) -> Self {
        Self { manager, db }
    }
}

impl TerminalScrollbackSource for PtyScrollbackSource {
    fn scrollback(&self, session_id: &str) -> String {
        let sid = session_id.to_string();
        let row = match self
            .db
            .read_main(move |conn| terminal_sessions::find_by_id(conn, &sid))
        {
            Ok(Some(row)) => row,
            // No row / read failure → the handler never reaches this branch
            // (it errors on the row lookup first); empty keeps the seam total.
            _ => return String::new(),
        };

        // v4: a session present in the manager map ("liveSession") AND a DB row
        // whose exitedAt is still null reads the in-memory ring buffer.
        if self.manager.contains(session_id) && row.exited_at.is_none() {
            return self.manager.get_ring_buffer(session_id).unwrap_or_default();
        }

        // Otherwise the transcript-file tail (last 1 MB), errors swallowed.
        if let Some(path) = row.transcript_path.as_deref() {
            return tail_file(Path::new(path), TRANSCRIPT_TAIL_MAX_BYTES).unwrap_or_default();
        }

        String::new()
    }
}
