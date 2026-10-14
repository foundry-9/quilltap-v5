//! The memory extraction cursor repository — P4.D288 unit 1 (contract C1-1),
//! porting v4 `lib/database/repositories/memory-extraction-cursors.repository.ts`
//! (151 lines, `ac6348c5f`; the `turn` pass `64743ee13`) and the cursor schema
//! `lib/schemas/memory-extraction-cursor.types.ts` at the `83334b9a5` pin.
//!
//! One row per (chat, extraction pass): the last message that pass has read.
//! v4's header: "A pass advances its cursor only past messages its window
//! actually covered, so nothing is ever marked read unread (bugs 206, 207)";
//! the `turn` row is the turn grain's, "so no line is extracted twice (bug
//! 245)". The rows replace the two per-chat watermark COLUMNS
//! (`chats.otherExtractionWatermarkMessageId`, the episode pass's
//! `episodeExtractionWatermarkMessageId`) — `db::memory_extraction_cursors_repair`
//! copies them over and drops the columns on boot.
//!
//! ## The table shape (R-C, `CURSOR_TABLE_MIGRATION_SHAPE`)
//!
//! Every REAL v4 instance — fresh ones included — carries the table in the
//! MIGRATION's shape: `add-memory-extraction-cursors-v1` runs in v4's PHASE 1,
//! before any repository's `ensureCollection`, so its `UNIQUE ("chatId",
//! "pass")` and live `FOREIGN KEY ("chatId") … ON DELETE CASCADE` win. Measured
//! on a real `tsx server.ts` first boot at the pin (P4.D288 unit 0): the table
//! text below, `sqlite_autoindex_…_1` (the TEXT primary key) and `…_2` (the
//! UNIQUE). The generateDDL shape (no UNIQUE, no FK, `"id" TEXT PRIMARY KEY NOT
//! NULL`) exists only where the migration never runs — the D23 dump, jest, the
//! tsx oracles. v5 provisions and ensures [`MIGRATION_TABLE_DDL`]; every verb
//! here works on BOTH shapes (the upsert targets the PRIMARY KEY), which the
//! tier-2 family proves op by op.
//!
//! **No chat-delete path clears cursors** — on the migration shape the FK
//! cascade does (v4 relies on it; v5's writable connection runs `foreign_keys
//! = ON` exactly as v4's `client.ts:120` does), and a cursor for an absent chat
//! is REFUSED (`FOREIGN KEY constraint failed`).
//!
//! ## Reads: what fails and how (measured at the pin)
//!
//! v4's header says "Reads throw on failure rather than answering 'no
//! cursor'". Through v4's REAL repository that holds only inside
//! `withStrictRepositoryFailures`: each read wraps a base read (`findByFilter`,
//! `_findById`, `_findAll`) that is itself a FALLBACK `safeQuery`, so outside
//! the strict scope a failed read logs the BASE line and answers the empty
//! value, and the method's own RETHROW line is never reached. Inside the scope
//! the base line gains `strictFailures: true`, propagates, and the method's
//! line follows it. Both arms are ported through the strict-aware homes in
//! `db::fallback`.
//!
//! A stored row the cursor schema refuses — a `pass` outside the enum (a raw
//! write, R-X) or a non-UUID `chatId` — is DROPPED by every read with v4's
//! `Data validation failed` / `Safe validation failed` pair (`findByFilter` /
//! `_findAll`'s `validateSafe`), or answered `None` with `Data validation
//! failed` + `Error finding entity by ID` (`_findById`'s `validate`).
//!
//! ## Method names
//!
//! Kept as v4 spells them (its job-child proxy classifies `find*` / `set*` /
//! `clear*`); the oracle case calls them by name.

use rusqlite::types::ValueRef;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{Map, Value};

use super::fallback;
use super::DbError;

/// The table name (v4's collection name — every repository line's
/// `collection` field).
pub const TABLE: &str = "memory_extraction_cursors";

/// v4's MIGRATION text for the table, byte-identical to the `sqlite_master.sql`
/// a real v4 first boot stores (SQLite drops the migration's `IF NOT EXISTS`
/// and keeps the rest — newlines and indentation included). Measured at the
/// `83334b9a5` pin (P4.D288 unit 0).
pub const MIGRATION_TABLE_DDL: &str = "CREATE TABLE \"memory_extraction_cursors\" (\n          \"id\" TEXT PRIMARY KEY,\n          \"chatId\" TEXT NOT NULL,\n          \"pass\" TEXT NOT NULL,\n          \"messageId\" TEXT NOT NULL,\n          \"createdAt\" TEXT NOT NULL,\n          \"updatedAt\" TEXT NOT NULL,\n          UNIQUE (\"chatId\", \"pass\"),\n          FOREIGN KEY (\"chatId\") REFERENCES \"chats\"(\"id\") ON DELETE CASCADE\n        )";

/// The repository pass's index (`ensureCollection`'s generateDDL `createdAt`
/// index, the D23 dump's text). v4 adds it on the repository's first touch;
/// the steady state of every real instance carries it.
pub const CREATED_AT_INDEX_DDL: &str = "CREATE INDEX \"idx_memory_extraction_cursors_createdAt\" ON \"memory_extraction_cursors\" (\"createdAt\" DESC)";

/// v4 `ExtractionCursorPassEnum` — `['other-fold', 'episode-fold', 'turn']`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExtractionCursorPass {
    OtherFold,
    EpisodeFold,
    Turn,
}

impl ExtractionCursorPass {
    /// v4 `EXTRACTION_CURSOR_PASSES` (`ExtractionCursorPassEnum.options`), in
    /// the enum's order.
    pub const ALL: [Self; 3] = [Self::OtherFold, Self::EpisodeFold, Self::Turn];

    /// The wire / column string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OtherFold => "other-fold",
            Self::EpisodeFold => "episode-fold",
            Self::Turn => "turn",
        }
    }

    /// The enum's own parse: exact strings only.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == s)
    }

    /// v4 `BUNDLE_CURSOR_FIELDS` (`lib/memory/extraction/cursor-bundle.ts:29-33`)
    /// — the legacy chat key a bundle (`.qtap`, backup) carries this pass's
    /// cursor under.
    pub fn bundle_field(self) -> &'static str {
        match self {
            Self::OtherFold => "otherExtractionWatermarkMessageId",
            Self::EpisodeFold => "episodeExtractionWatermarkMessageId",
            Self::Turn => "turnExtractionWatermarkMessageId",
        }
    }

    /// v4 `FoldCursorPass` (`Exclude<ExtractionCursorPass, 'turn'>`): the
    /// cursors that read a chat in stretches.
    pub fn is_fold(self) -> bool {
        !matches!(self, Self::Turn)
    }
}

/// v4 `extractionCursorId(chatId, pass)` — `"<chatId>:<pass>"`.
pub fn extraction_cursor_id(chat_id: &str, pass: ExtractionCursorPass) -> String {
    raw_cursor_id(chat_id, pass.as_str())
}

/// The id of a row whose pass the enum does not know (R-X's raw arm).
fn raw_cursor_id(chat_id: &str, pass: &str) -> String {
    format!("{chat_id}:{pass}")
}

/// v4 `ExtractionCursorMap` (`Partial<Record<ExtractionCursorPass, string>>`):
/// at most one message id per pass. A struct, so its key ORDER is not a
/// property v5 carries — v4's map order follows the query plan (an index-ordered
/// `WHERE "chatId" = ?` on the migration shape, rowid on the bare one,
/// measured) and no v4 consumer reads it in order (the bundle helpers walk
/// [`ExtractionCursorPass::ALL`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtractionCursorMap {
    pub other_fold: Option<String>,
    pub episode_fold: Option<String>,
    pub turn: Option<String>,
}

impl ExtractionCursorMap {
    /// The cursor for one pass.
    pub fn get(&self, pass: ExtractionCursorPass) -> Option<&str> {
        match pass {
            ExtractionCursorPass::OtherFold => self.other_fold.as_deref(),
            ExtractionCursorPass::EpisodeFold => self.episode_fold.as_deref(),
            ExtractionCursorPass::Turn => self.turn.as_deref(),
        }
    }

    /// Set one pass's cursor (v4 `map[row.pass] = row.messageId` — a later row
    /// for the same pass wins).
    pub fn set(&mut self, pass: ExtractionCursorPass, message_id: String) {
        match pass {
            ExtractionCursorPass::OtherFold => self.other_fold = Some(message_id),
            ExtractionCursorPass::EpisodeFold => self.episode_fold = Some(message_id),
            ExtractionCursorPass::Turn => self.turn = Some(message_id),
        }
    }

    /// No pass has a cursor.
    pub fn is_empty(&self) -> bool {
        self.other_fold.is_none() && self.episode_fold.is_none() && self.turn.is_none()
    }

    /// v4's map as JSON, keys in [`ExtractionCursorPass::ALL`] order.
    pub fn to_json(&self) -> Value {
        let mut out = Map::new();
        for pass in ExtractionCursorPass::ALL {
            if let Some(id) = self.get(pass) {
                out.insert(pass.as_str().to_string(), Value::String(id.to_string()));
            }
        }
        Value::Object(out)
    }
}

/// One validated cursor row (v4 `MemoryExtractionCursor`).
#[derive(Clone, Debug, PartialEq, Eq)]
struct CursorRow {
    chat_id: String,
    pass: ExtractionCursorPass,
    message_id: String,
}

/// The six columns, in the table's order (both shapes agree).
const SELECT_COLUMNS: &str =
    "\"id\", \"chatId\", \"pass\", \"messageId\", \"createdAt\", \"updatedAt\"";

/// A raw row as v4's SQLite collection hydrates it: every column a JSON value
/// (TEXT → string, NULL → null, a number → a number). A BLOB cell — which no
/// writer of this table produces — fails the read (a recorded residual: v4
/// would hand Zod a `Buffer`).
fn raw_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Map<String, Value>> {
    let mut out = Map::new();
    for (i, name) in [
        "id",
        "chatId",
        "pass",
        "messageId",
        "createdAt",
        "updatedAt",
    ]
    .into_iter()
    .enumerate()
    {
        let v = match row.get_ref(i)? {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(n) => Value::from(n),
            ValueRef::Real(f) => serde_json::Number::from_f64(f)
                .map(Value::Number)
                .unwrap_or(Value::Null),
            ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
            ValueRef::Blob(_) => {
                return Err(rusqlite::Error::InvalidColumnType(
                    i,
                    name.to_string(),
                    rusqlite::types::Type::Blob,
                ))
            }
        };
        out.insert(name.to_string(), v);
    }
    Ok(out)
}

/// v4 `MemoryExtractionCursorSchema.parse` over one raw row: the row, or the
/// Zod error message v4's `validate` logs.
fn validate(raw: &Map<String, Value>) -> Result<CursorRow, String> {
    let issues = crate::api::zod_issues::zod_memory_extraction_cursor_issues(raw);
    if !issues.is_empty() {
        return Err(crate::api::zod_issues::zod_error_message(&issues));
    }
    let s = |k: &str| {
        raw.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    Ok(CursorRow {
        chat_id: s("chatId"),
        pass: ExtractionCursorPass::parse(&s("pass")).expect("validated pass"),
        message_id: s("messageId"),
    })
}

/// The memory extraction cursor repository over one connection (MAIN db).
pub struct MemoryExtractionCursorsRepository<'c> {
    conn: &'c Connection,
}

impl<'c> MemoryExtractionCursorsRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// v4 `findByFilter`'s read + per-row `validateSafe` (a refused row is
    /// dropped with v4's two lines).
    fn select_validated(&self, sql: &str, args: &[&str]) -> Result<Vec<CursorRow>, DbError> {
        let mut stmt = self.conn.prepare(sql)?;
        let raws = stmt
            .query_map(rusqlite::params_from_iter(args.iter()), raw_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(raws
            .iter()
            .filter_map(|raw| match validate(raw) {
                Ok(row) => Some(row),
                Err(zod) => {
                    fallback::log_safe_validation_drop(TABLE, &zod);
                    None
                }
            })
            .collect())
    }

    /// A chat's cursors, keyed by pass (v4 `findByChatId`, `:56-65`). Empty
    /// when no pass has read the chat — and, outside the strict scope, when
    /// the read FAILED (v4's inner `findByFilter` fallback; see the module
    /// header).
    pub fn find_by_chat_id(&self, chat_id: &str) -> Result<ExtractionCursorMap, DbError> {
        let rows = fallback::find_by_filter_strict_aware(TABLE, || {
            self.select_validated(
                &format!("SELECT {SELECT_COLUMNS} FROM \"{TABLE}\" WHERE \"chatId\" = ?1"),
                &[chat_id],
            )
        })
        .inspect_err(|e| fallback::log_cursor_read_failure_for_chat(chat_id, e))?;
        let mut map = ExtractionCursorMap::default();
        for row in rows {
            map.set(row.pass, row.message_id);
        }
        Ok(map)
    }

    /// One pass's cursor for a chat, or `None` (v4 `findCursor`, `:68-75`).
    pub fn find_cursor(
        &self,
        chat_id: &str,
        pass: ExtractionCursorPass,
    ) -> Result<Option<String>, DbError> {
        self.find_cursor_raw(chat_id, pass.as_str())
    }

    /// R-X's raw arm of [`Self::find_cursor`]: a pass string the enum does not
    /// know. v4's `findCursor` takes any string at run time; its `_findById`
    /// then REFUSES the stored row in Zod (the pass is not in the enum), so
    /// the answer is always `None`, with v4's `Data validation failed` +
    /// `Error finding entity by ID` lines.
    pub fn find_cursor_raw(&self, chat_id: &str, pass: &str) -> Result<Option<String>, DbError> {
        let id = raw_cursor_id(chat_id, pass);
        fallback::find_by_id_strict_aware(TABLE, &id, || {
            let raw = self
                .conn
                .query_row(
                    &format!("SELECT {SELECT_COLUMNS} FROM \"{TABLE}\" WHERE \"id\" = ?1"),
                    [&id],
                    raw_row,
                )
                .optional()?;
            match raw {
                None => Ok(None),
                Some(raw) => match validate(&raw) {
                    Ok(row) => Ok(Some(row.message_id)),
                    Err(zod) => {
                        // `validate` logs, then throws into `_findById`'s own
                        // fallback `safeQuery` — whose line carries the SAME
                        // message.
                        fallback::log_data_validation_failed(TABLE, &zod);
                        Err(DbError::Internal(zod))
                    }
                },
            }
        })
        .inspect_err(|e| fallback::log_cursor_read_failure(chat_id, pass, e))
    }

    /// Every chat's cursors, chats in first-row (rowid) order (v4
    /// `findAllByChat`, `:77-91` — a `Map` filled in `_findAll`'s order).
    pub fn find_all_by_chat(&self) -> Result<Vec<(String, ExtractionCursorMap)>, DbError> {
        let rows = fallback::find_all_strict_aware(TABLE, || {
            self.select_validated(&format!("SELECT {SELECT_COLUMNS} FROM \"{TABLE}\""), &[])
        })
        .inspect_err(fallback::log_cursor_list_failure)?;
        let mut out: Vec<(String, ExtractionCursorMap)> = Vec::new();
        for row in rows {
            match out.iter_mut().find(|(chat, _)| *chat == row.chat_id) {
                Some((_, map)) => map.set(row.pass, row.message_id),
                None => {
                    let mut map = ExtractionCursorMap::default();
                    map.set(row.pass, row.message_id);
                    out.push((row.chat_id, map));
                }
            }
        }
        Ok(out)
    }

    /// Move a chat's cursor for one pass to `message_id` (v4 `setCursor`,
    /// `:102-120`): ONE upsert statement — "so two jobs advancing the same
    /// cursor cannot collide on the primary key" — with ONE `now` for both
    /// stamps; an existing row keeps its `createdAt`. A RETHROW: a failure
    /// logs `Error setting extraction cursor` and propagates.
    pub fn set_cursor(
        &self,
        chat_id: &str,
        pass: ExtractionCursorPass,
        message_id: &str,
    ) -> Result<(), DbError> {
        self.set_cursor_raw(chat_id, pass.as_str(), message_id)
    }

    /// R-X's raw arm of [`Self::set_cursor`]: v4 writes a `<chatId>:<raw>` row
    /// for any pass string (its `pass` column is free text), which every typed
    /// read then drops in Zod. `UNKNOWN_PASS_CURSOR_ROW` (P4.D296) pins it.
    pub fn set_cursor_raw(
        &self,
        chat_id: &str,
        pass: &str,
        message_id: &str,
    ) -> Result<(), DbError> {
        let now = crate::clock::now_iso();
        self.conn
            .execute(
                &format!(
                    "INSERT INTO \"{TABLE}\" (\"id\", \"chatId\", \"pass\", \"messageId\", \"createdAt\", \"updatedAt\") \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
                     ON CONFLICT(\"id\") DO UPDATE SET \"messageId\" = excluded.\"messageId\", \"updatedAt\" = excluded.\"updatedAt\""
                ),
                params![raw_cursor_id(chat_id, pass), chat_id, pass, message_id, now, now],
            )
            .map_err(DbError::from)
            .inspect_err(|e| fallback::log_cursor_set_failure(chat_id, pass, e))?;
        tracing::debug!(
            target: "quilltap::db",
            chatId = %chat_id,
            pass = %pass,
            messageId = %message_id,
            "[ExtractionCursors] Cursor set"
        );
        Ok(())
    }

    /// v4 base `deleteMany(filter)`: the delete, INFO `Entities deleted
    /// {collection, count}`; a failure logs `Error deleting entities` and
    /// propagates (a RETHROW `safeQuery`).
    fn delete_many(&self, column: &str, value: &str) -> Result<usize, DbError> {
        let count = self
            .conn
            .execute(
                &format!("DELETE FROM \"{TABLE}\" WHERE \"{column}\" = ?1"),
                [value],
            )
            .map_err(DbError::from)
            .inspect_err(|e| fallback::log_delete_many_failure(TABLE, e))?;
        fallback::log_entities_deleted(TABLE, count);
        Ok(count)
    }

    /// Forget one pass's cursor for a chat: the pass reads the chat from the
    /// start again (v4 `clearCursor`, `:123-135`). A FALLBACK: a failure logs
    /// the base line then `Error clearing extraction cursor` and answers
    /// `false` (propagates inside the strict scope).
    pub fn clear_cursor(&self, chat_id: &str, pass: ExtractionCursorPass) -> Result<bool, DbError> {
        fallback::cursor_cleared_or_false(chat_id, pass.as_str(), || {
            let removed = self.delete_many("id", &extraction_cursor_id(chat_id, pass))?;
            tracing::debug!(
                target: "quilltap::db",
                chatId = %chat_id,
                pass = %pass.as_str(),
                removed = removed,
                "[ExtractionCursors] Cursor cleared"
            );
            Ok(removed > 0)
        })
    }

    /// Forget every cursor of a chat (v4 `clearForChat`, `:137-150`). A
    /// FALLBACK `0`.
    pub fn clear_for_chat(&self, chat_id: &str) -> Result<usize, DbError> {
        fallback::cursors_cleared_for_chat_or_zero(chat_id, || self.delete_many("chatId", chat_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn(migration_shape: bool) -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys = ON; CREATE TABLE chats (id TEXT PRIMARY KEY);")
            .unwrap();
        if migration_shape {
            c.execute_batch(MIGRATION_TABLE_DDL).unwrap();
        } else {
            c.execute_batch(
                "CREATE TABLE \"memory_extraction_cursors\" (\"id\" TEXT PRIMARY KEY NOT NULL, \"chatId\" TEXT NOT NULL, \"pass\" TEXT NOT NULL, \"messageId\" TEXT NOT NULL, \"createdAt\" TEXT NOT NULL, \"updatedAt\" TEXT NOT NULL)",
            )
            .unwrap();
        }
        c.execute(
            "INSERT INTO chats (id) VALUES ('c0000000-0000-4000-8000-00000000000a')",
            [],
        )
        .unwrap();
        c
    }

    const A: &str = "c0000000-0000-4000-8000-00000000000a";

    #[test]
    fn pass_strings_follow_v4s_enum_and_bundle_fields() {
        assert_eq!(
            ExtractionCursorPass::ALL.map(ExtractionCursorPass::as_str),
            ["other-fold", "episode-fold", "turn"]
        );
        assert_eq!(
            ExtractionCursorPass::ALL.map(ExtractionCursorPass::bundle_field),
            [
                "otherExtractionWatermarkMessageId",
                "episodeExtractionWatermarkMessageId",
                "turnExtractionWatermarkMessageId"
            ]
        );
        assert!(ExtractionCursorPass::OtherFold.is_fold());
        assert!(!ExtractionCursorPass::Turn.is_fold());
        assert_eq!(ExtractionCursorPass::parse("Turn"), None);
        assert_eq!(
            extraction_cursor_id("c", ExtractionCursorPass::Turn),
            "c:turn"
        );
    }

    #[test]
    fn the_upsert_keeps_created_at_on_both_shapes() {
        for shape in [true, false] {
            let c = conn(shape);
            let repo = MemoryExtractionCursorsRepository::new(&c);
            repo.set_cursor(A, ExtractionCursorPass::Turn, "m1")
                .unwrap();
            c.execute(
                "UPDATE memory_extraction_cursors SET createdAt = '2020-01-01T00:00:00.000Z'",
                [],
            )
            .unwrap();
            repo.set_cursor(A, ExtractionCursorPass::Turn, "m2")
                .unwrap();
            let (msg, created): (String, String) = c
                .query_row(
                    "SELECT messageId, createdAt FROM memory_extraction_cursors",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!(
                (msg.as_str(), created.as_str()),
                ("m2", "2020-01-01T00:00:00.000Z")
            );
            assert_eq!(
                repo.find_cursor(A, ExtractionCursorPass::Turn)
                    .unwrap()
                    .as_deref(),
                Some("m2")
            );
        }
    }

    #[test]
    fn the_migration_shape_refuses_a_cursor_for_an_absent_chat_and_cascades() {
        let c = conn(true);
        let repo = MemoryExtractionCursorsRepository::new(&c);
        assert!(repo
            .set_cursor(
                "c0000000-0000-4000-8000-0000000000ff",
                ExtractionCursorPass::Turn,
                "m"
            )
            .is_err());
        repo.set_cursor(A, ExtractionCursorPass::OtherFold, "m")
            .unwrap();
        c.execute("DELETE FROM chats", []).unwrap();
        assert!(repo.find_all_by_chat().unwrap().is_empty());
    }
}
