//! Extraction cursors in bundles (`.qtap` exports and backups) — P4.D288
//! (C1-11), porting v4 `lib/memory/extraction/cursor-bundle.ts` (`ac6348c5f`,
//! memory refactor Phase D, + bug 245's `turn` grain) at the `83334b9a5` pin.
//!
//! "A chat's cursors live in `memory_extraction_cursors`, not on the chat row,
//! but bundles carry them on the chat under the two field names the chat row
//! used before Phase D — `otherExtractionWatermarkMessageId` and
//! `episodeExtractionWatermarkMessageId`. Keeping the names keeps every old
//! bundle readable and lets the existing message-id remapping carry them
//! unchanged. The turn grain's cursor (bug 245) travels under
//! `turnExtractionWatermarkMessageId`; a bundle written before it simply lacks
//! the field."
//!
//! - Writers call [`with_bundle_cursor_fields`] on each chat they emit
//!   (optionally over one [`load_bundle_cursors`] read).
//! - Readers call [`take_bundle_cursors`] on the chat they are about to create
//!   (it strips the fields) and [`restore_bundle_cursors`] once the chat
//!   exists.
//!
//! The three stateful wrappers are best-effort and never fail the writer:
//! their failures are v4's three `[ExtractionCursors]` WARNs and the restore's
//! DEBUG, every line on v4's child-logger binding `module:
//! 'memory:extraction:cursor-bundle'` first. **A read of an instance with no
//! cursor table answers "no cursors" without creating the table** (v4's
//! repository creates it on first touch, and reads it empty — the same
//! answer). Writing into an instance without the table cannot happen on a
//! booted v5 instance (the boot ensure creates it); were it to, the restore's
//! WARN would fire where v4 creates the table and writes.

use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::db::memory_extraction_cursors::{
    ExtractionCursorMap, ExtractionCursorPass, MemoryExtractionCursorsRepository, TABLE,
};
use crate::db::DbError;

/// v4's child-logger binding.
const MODULE: &str = "memory:extraction:cursor-bundle";

/// Every chat's cursors from one read ([`load_bundle_cursors`]) — v4's
/// `ReadonlyMap<string, ExtractionCursorMap>`, chats in first-row order.
pub type BundleCursors = Vec<(String, ExtractionCursorMap)>;

/// v4 `cursorsToBundleFields(cursors)`: a cursor map as bundle fields, in
/// [`ExtractionCursorPass::ALL`] order (other → episode → turn); a pass with
/// no cursor — or an empty one (v4's truthiness) — is left out, never `null`.
pub fn cursors_to_bundle_fields(cursors: &ExtractionCursorMap) -> Map<String, Value> {
    let mut fields = Map::new();
    for pass in ExtractionCursorPass::ALL {
        if let Some(id) = cursors.get(pass).filter(|id| !id.is_empty()) {
            fields.insert(
                pass.bundle_field().to_string(),
                Value::String(id.to_string()),
            );
        }
    }
    fields
}

/// v4 `takeBundleCursors(chat)`: split a bundle chat into the chat to create
/// and its cursors. All three bundle fields are DELETED from the chat (in
/// place, every other key keeping its position — `shift_remove`, v4's
/// `delete`); only a non-empty STRING value is a cursor.
pub fn take_bundle_cursors(chat: &mut Map<String, Value>) -> ExtractionCursorMap {
    let mut cursors = ExtractionCursorMap::default();
    for pass in ExtractionCursorPass::ALL {
        if let Some(Value::String(id)) = chat.shift_remove(pass.bundle_field()) {
            if !id.is_empty() {
                cursors.set(pass, id);
            }
        }
    }
    cursors
}

/// `true` when the cursor table exists; a failed probe is the read's failure.
fn cursor_table_exists(conn: &Connection) -> Result<bool, DbError> {
    crate::db::schema_probe::table_exists(conn, TABLE)
}

/// v4 `loadBundleCursors()`: every chat's cursors in one read, for a writer
/// that emits many chats; `None` when the read fails — WARN `[ExtractionCursors]
/// Could not read the cursor table for a bundle; reading per chat {module,
/// error}` — and each chat then reads its own, best-effort. (Outside the
/// strict scope the repository's own fallback answers an empty read first, as
/// v4's does.)
pub fn load_bundle_cursors(conn: &Connection) -> Option<BundleCursors> {
    let read = cursor_table_exists(conn).and_then(|exists| {
        if !exists {
            return Ok(Vec::new());
        }
        MemoryExtractionCursorsRepository::new(conn).find_all_by_chat()
    });
    match read {
        Ok(by_chat) => Some(by_chat),
        Err(error) => {
            tracing::warn!(
                module = MODULE,
                error = %crate::db::fallback::error_text(&error),
                "[ExtractionCursors] Could not read the cursor table for a bundle; reading per chat"
            );
            None
        }
    }
}

/// v4 `withBundleCursorFields(chat, preloaded)`: the chat as a writer emits
/// it — `{...chat, ...fields}`, its cursors on the bundle fields (a key the
/// chat already carries is overwritten in place) — from `preloaded` when
/// given, else one read. A cursor that cannot be read is left out — WARN
/// `[ExtractionCursors] Could not read a chat's cursors for a bundle; leaving
/// them out {module, chatId, error}` — never a reason to fail the export or
/// backup. `chat` must carry its `id`.
pub fn with_bundle_cursor_fields(
    conn: &Connection,
    mut chat: Map<String, Value>,
    preloaded: Option<&BundleCursors>,
) -> Map<String, Value> {
    let chat_id = chat
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let cursors = match preloaded {
        Some(by_chat) => by_chat
            .iter()
            .find(|(id, _)| *id == chat_id)
            .map(|(_, map)| map.clone())
            .unwrap_or_default(),
        None => {
            let read = cursor_table_exists(conn).and_then(|exists| {
                if !exists {
                    return Ok(ExtractionCursorMap::default());
                }
                MemoryExtractionCursorsRepository::new(conn).find_by_chat_id(&chat_id)
            });
            match read {
                Ok(map) => map,
                Err(error) => {
                    tracing::warn!(
                        module = MODULE,
                        chatId = %chat_id,
                        error = %crate::db::fallback::error_text(&error),
                        "[ExtractionCursors] Could not read a chat's cursors for a bundle; leaving them out"
                    );
                    return chat;
                }
            }
        }
    };
    for (k, v) in cursors_to_bundle_fields(&cursors) {
        chat.insert(k, v);
    }
    chat
}

/// v4 `restoreBundleCursors(chatId, cursors)`: write a created chat's cursors,
/// pass by pass in [`ExtractionCursorPass::ALL`] order, each through
/// `setCursor`. Best-effort — "a lost cursor only means a pass re-reads": a
/// failed write logs WARN `[ExtractionCursors] Could not restore a bundled
/// cursor; the pass will re-read {module, chatId, pass, error}` and the next
/// pass goes on. When anything was written, DEBUG `[ExtractionCursors]
/// Restored bundled cursors {module, chatId, written}`. Returns the cursors
/// written. On the WRITER's connection.
pub fn restore_bundle_cursors(
    conn: &Connection,
    chat_id: &str,
    cursors: &ExtractionCursorMap,
) -> usize {
    let repo = MemoryExtractionCursorsRepository::new(conn);
    let mut written = 0usize;
    for pass in ExtractionCursorPass::ALL {
        let Some(message_id) = cursors.get(pass).filter(|id| !id.is_empty()) else {
            continue;
        };
        match repo.set_cursor(chat_id, pass, message_id) {
            Ok(()) => written += 1,
            Err(error) => tracing::warn!(
                module = MODULE,
                chatId = %chat_id,
                pass = pass.as_str(),
                error = %crate::db::fallback::error_text(&error),
                "[ExtractionCursors] Could not restore a bundled cursor; the pass will re-read"
            ),
        }
    }
    if written > 0 {
        tracing::debug!(
            module = MODULE,
            chatId = %chat_id,
            written = written,
            "[ExtractionCursors] Restored bundled cursors"
        );
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::fallback::with_strict_repository_failures;
    use crate::db::memory_extraction_cursors::MIGRATION_TABLE_DDL;
    use crate::test_support::captured_with;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys = ON; CREATE TABLE chats (id TEXT PRIMARY KEY);")
            .unwrap();
        c.execute_batch(MIGRATION_TABLE_DDL).unwrap();
        c.execute_batch("INSERT INTO chats (id) VALUES ('c1'), ('c2');")
            .unwrap();
        c
    }

    fn lines_with(lines: &[String], needle: &str) -> usize {
        lines.iter().filter(|l| l.contains(needle)).count()
    }

    /// The pure pair: other → episode → turn, a blank cursor left out; the take
    /// deletes every bundle key in place and keeps only non-empty strings.
    #[test]
    fn the_pure_pair_follows_v4() {
        let mut map = ExtractionCursorMap::default();
        map.set(ExtractionCursorPass::Turn, "t".into());
        map.set(ExtractionCursorPass::OtherFold, "o".into());
        map.set(ExtractionCursorPass::EpisodeFold, String::new());
        assert_eq!(
            Value::Object(cursors_to_bundle_fields(&map)).to_string(),
            r#"{"otherExtractionWatermarkMessageId":"o","turnExtractionWatermarkMessageId":"t"}"#
        );
        let mut chat: Map<String, Value> = serde_json::from_str(
            r#"{"turnExtractionWatermarkMessageId":"t1","id":"c","otherExtractionWatermarkMessageId":7,"title":"T","episodeExtractionWatermarkMessageId":"e"}"#,
        )
        .unwrap();
        let cursors = take_bundle_cursors(&mut chat);
        assert_eq!(Value::Object(chat).to_string(), r#"{"id":"c","title":"T"}"#);
        assert_eq!(
            cursors.to_json().to_string(),
            r#"{"episode-fold":"e","turn":"t1"}"#
        );
    }

    /// The three WARN arms over reads / writes that FAIL in v5's build (a BLOB
    /// `messageId` the row read refuses; a restore into a chat with no `chats`
    /// row — the foreign key), and the restore's DEBUG; the reads' WARNs fire
    /// only inside the strict scope (outside it the repository's fallback
    /// answers first, as v4's does).
    #[test]
    fn the_warn_arms_fire_and_never_fail_the_writer() {
        let c = conn();
        let mut cursors = ExtractionCursorMap::default();
        cursors.set(ExtractionCursorPass::OtherFold, "m1".into());
        cursors.set(ExtractionCursorPass::Turn, "m3".into());
        let (written, lines) = captured_with(|| restore_bundle_cursors(&c, "c1", &cursors));
        assert_eq!(written, 2);
        assert_eq!(
            lines_with(&lines, "[ExtractionCursors] Restored bundled cursors"),
            1
        );
        let (written, lines) = captured_with(|| restore_bundle_cursors(&c, "c9", &cursors));
        assert_eq!(written, 0);
        assert_eq!(
            lines_with(
                &lines,
                "Could not restore a bundled cursor; the pass will re-read"
            ),
            2
        );
        assert_eq!(lines_with(&lines, "Restored bundled cursors"), 0);

        c.execute_batch(
            "UPDATE memory_extraction_cursors SET messageId = x'00' WHERE chatId = 'c1'",
        )
        .unwrap();
        let (loaded, lines) = captured_with(|| load_bundle_cursors(&c));
        assert_eq!(
            loaded,
            Some(Vec::new()),
            "outside the scope the fallback answers"
        );
        assert_eq!(lines_with(&lines, "Could not read the cursor table"), 0);
        let (loaded, lines) =
            captured_with(|| with_strict_repository_failures(|| load_bundle_cursors(&c)));
        assert_eq!(loaded, None);
        assert_eq!(
            lines_with(&lines, "Could not read the cursor table for a bundle"),
            1
        );

        let chat: Map<String, Value> = serde_json::from_str(r#"{"id":"c1","title":"A"}"#).unwrap();
        let (out, lines) = captured_with(|| {
            with_strict_repository_failures(|| with_bundle_cursor_fields(&c, chat.clone(), None))
        });
        assert_eq!(Value::Object(out), Value::Object(chat));
        assert_eq!(
            lines_with(&lines, "Could not read a chat's cursors for a bundle"),
            1
        );
    }

    /// An instance with no cursor table reads as "no cursors" and the read
    /// creates nothing.
    #[test]
    fn an_absent_table_reads_as_none_and_creates_nothing() {
        let c = Connection::open_in_memory().unwrap();
        assert_eq!(load_bundle_cursors(&c), Some(Vec::new()));
        let chat: Map<String, Value> = serde_json::from_str(r#"{"id":"c1"}"#).unwrap();
        assert_eq!(with_bundle_cursor_fields(&c, chat.clone(), None), chat);
        assert!(!crate::db::schema_probe::table_exists(&c, TABLE).unwrap());
    }
}
