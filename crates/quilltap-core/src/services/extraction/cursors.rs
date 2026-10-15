//! Extraction cursors — port of v4 `lib/memory/extraction/cursors.ts`
//! (`ac6348c5f`, `'turn'` from `64743ee13`; pin `83334b9a5`): where a pass has
//! read a chat to.
//!
//! v4's why, carried: a pass reads through a [`CursorStore`]. The live store
//! reads and writes `memory_extraction_cursors` (the keystone's repository,
//! C1-1); the dry run's store holds them in memory, starting where a
//! regenerate would (no cursors), so a preview reads the whole history and
//! never moves a real cursor.
//!
//! v5 shape (work order P4.D296 Tier 1 #6, L-4):
//!  - an ENUM, not an async trait object;
//!  - the pass is the RAW string, as v4 carries it: v4's `memoryExtractionPassOf`
//!    lets any truthy `pass` through, and an unknown one reads and writes its
//!    own `<chatId>:<pass>` row (R-X). Every read and advance therefore goes
//!    through C1-1's raw arm (`find_cursor_raw` / `set_cursor_raw`) — for a
//!    known pass that is exactly `find_cursor` / `set_cursor` (they delegate
//!    to it), and the ruled table `UNKNOWN_PASS_CURSOR_ROW` pins the rest;
//!  - reads PROPAGATE (v4's repository reads throw — a failed read taken for
//!    "no cursor" would re-read history);
//!  - a live advance logs `[ExtractionCursors] Advanced` on the CALLER thread,
//!    after the repository's own `Cursor set` (P4.D288's line, emitted on the
//!    writer thread).
//!
//! v4's `clear(pass)` has no production caller at the pin (the regenerate
//! clears through the repository's `clearForChat`); it is not ported.

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::db::memory_extraction_cursors::MemoryExtractionCursorsRepository;
use crate::db::runtime::Db;
use crate::db::DbError;

/// The `turn` cursor pass string (bug 245).
pub const TURN_CURSOR_PASS: &str = "turn";

/// Where one pass has read one chat to (v4 `ExtractionCursor`); `None` means
/// from the start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractionCursor {
    pub chat_id: String,
    pub pass: String,
    pub after_message_id: Option<String>,
}

/// v4 `CursorStore` — `liveCursorStore(chatId)` / `memoryCursorStore(chatId,
/// initial)`.
pub enum CursorStore {
    /// The chat's real cursors.
    Live { chat_id: String },
    /// Cursors held in memory (the dry run), keyed by pass string.
    Memory {
        chat_id: String,
        cursors: Mutex<BTreeMap<String, String>>,
    },
}

impl CursorStore {
    /// v4 `liveCursorStore(chatId)`.
    pub fn live(chat_id: &str) -> Self {
        Self::Live {
            chat_id: chat_id.to_string(),
        }
    }

    /// v4 `memoryCursorStore(chatId, initial = {})`.
    pub fn memory(chat_id: &str, initial: BTreeMap<String, String>) -> Self {
        Self::Memory {
            chat_id: chat_id.to_string(),
            cursors: Mutex::new(initial),
        }
    }

    pub fn chat_id(&self) -> &str {
        match self {
            Self::Live { chat_id } | Self::Memory { chat_id, .. } => chat_id,
        }
    }

    pub fn is_live(&self) -> bool {
        matches!(self, Self::Live { .. })
    }

    /// v4 `read(pass)`. A live read failure propagates.
    pub fn read(&self, db: &Db, pass: &str) -> Result<ExtractionCursor, DbError> {
        let after_message_id = match self {
            Self::Live { chat_id } => {
                let (chat, p) = (chat_id.clone(), pass.to_string());
                db.read_main(move |c| {
                    MemoryExtractionCursorsRepository::new(c).find_cursor_raw(&chat, &p)
                })?
            }
            Self::Memory { cursors, .. } => cursors.lock().unwrap().get(pass).cloned(),
        };
        Ok(ExtractionCursor {
            chat_id: self.chat_id().to_string(),
            pass: pass.to_string(),
            after_message_id,
        })
    }

    /// v4 `advance(pass, messageId)` — move the cursor to `message_id`, only
    /// ever past messages the pass's window covered.
    pub async fn advance(&self, db: &Db, pass: &str, message_id: &str) -> Result<(), DbError> {
        match self {
            Self::Live { chat_id } => {
                let (chat, p, m) = (chat_id.clone(), pass.to_string(), message_id.to_string());
                db.write(move |w| {
                    w.main()
                        .memory_extraction_cursors()
                        .set_cursor_raw(&chat, &p, &m)
                })
                .await?;
                tracing::debug!(
                    chatId = %chat_id,
                    pass = %pass,
                    messageId = %message_id,
                    "[ExtractionCursors] Advanced"
                );
            }
            Self::Memory { cursors, .. } => {
                cursors
                    .lock()
                    .unwrap()
                    .insert(pass.to_string(), message_id.to_string());
            }
        }
        Ok(())
    }

    /// v4's memory store `snapshot()` (`None` for the live store).
    pub fn snapshot(&self) -> Option<BTreeMap<String, String>> {
        match self {
            Self::Live { .. } => None,
            Self::Memory { cursors, .. } => Some(cursors.lock().unwrap().clone()),
        }
    }
}
