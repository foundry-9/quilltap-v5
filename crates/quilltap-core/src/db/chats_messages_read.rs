//! The `chat_messages` **read path** (the conversation capstone, sub-unit 3).
//! Ports the message-read surface of v4's `ChatMessagesOps`
//! (`lib/database/repositories/chats-messages.ops.ts`): `getMessages`,
//! `getMessageCount`, and `findChatIdForMessage`.
//!
//! Messages live in their own MAIN-db `chat_messages` table (one row per event,
//! `ensureCollection('chat_messages', ChatMessageRowSchema)`), NOT in the `chats`
//! row. `getMessages` reads every row for a chat ordered by `createdAt`, hydrates
//! it, and validates each through `ChatEventSchema` (a three-member union —
//! `MessageEvent` / `ContextSummaryEvent` / `SystemEvent`), skipping any row that
//! fails to parse. This sub-unit is the inverse marshaling: row → `ChatEvent`.
//!
//! ## Why no read-side default materialization
//!
//! v4 only ever writes a message through `addMessage`/`addMessages`, which run
//! `ChatEventSchema.parse(message)` **before** the insert — so every Zod
//! `.default(...)` (e.g. `attachments` → `[]`, a `DangerFlag`'s
//! `userOverridden` / `wasRerouted` → `false`) is already baked into the stored
//! bytes. The read therefore reconstructs the union member by:
//!
//!   - reading the required columns (`id`/`role`/`content`/`createdAt` for a
//!     message; `context`; `systemEventType`/`description`),
//!   - reading the nullable-optional columns and **omitting** them when `NULL`
//!     (v4 emits `undefined`, dropped by `JSON.stringify`),
//!   - parsing the JSON columns straight to [`serde_json::Value`] — the stored
//!     text already carries the materialized defaults and the exact int-vs-float
//!     number representation (so e.g. `reasoningSegments`/`summaryAnchor`/nested
//!     `score`s round-trip byte-for-byte without a struct re-serialization that
//!     would turn `1` into `1.0`).
//!
//! The JSON columns (parsed on read) are exactly the schema's array/object
//! fields — `rawResponse` (`z.record` → object), `attachments`,
//! `debugMemoryLogs`, `reasoningSegments`, `dangerFlags`, `targetParticipantIds`,
//! `hostEvent`, `customAnnouncer`, `carinaMeta`, `pendingExternalAttachments`,
//! `summaryAnchor` (v4's `mapToSQLiteType` + the backend's
//! `array|object → jsonColumns` detection). Top-level number columns are REAL
//! affinity, read as `f64` and rendered the JS way via [`js_number_to_json`].
//!
//! ## `isSilentMessage` (seam CLOSED — the "drop" premise was wrong)
//!
//! `ChatMessageRowSchema`'s `isSilentMessage` is
//! `z.union([z.boolean(), z.number().transform(v => v === 1)]).nullable().optional()`
//! → **TEXT affinity**, so a written boolean `true` is persisted as numeric TEXT
//! (`"1.0"`). The earlier deferral assumed `hydrateRow` leaves that as a string and
//! `MessageEventSchema.z.boolean()` then rejects it, dropping the message.
//! **Verified empirically against v4 that this is NOT what happens**: the read
//! applies the ROW-schema union (coerce to a number, `=== 1`) → a real `boolean`,
//! so `getMessages` KEEPS the message with `isSilentMessage: true`. So the port
//! reads the column and reproduces that coercion in [`put_is_silent`] (numeric-TEXT
//! `=== 1.0` → bool; `NULL` → the key is omitted as v4 `undefined`). Proven by a
//! silent-message row in the read corpus. (See "Deferred seams #8" in
//! `docs/developer/porting/phase-2-onramp.md`.)
//!
//! **Migration-affinity addendum (the first Friday dogfood finding, 2026-07-10):**
//! the TEXT affinity above is the FRESH-`generateDDL` shape. A real v4 instance
//! got the column from the `add-silent-message-field` migration —
//! `ALTER TABLE "chat_messages" ADD COLUMN "isSilentMessage" INTEGER DEFAULT NULL`
//! — so migrated cells are stored **INTEGER** `1`/`0` (numeric affinity converts
//! v4's bound REAL losslessly). v4's dynamically-typed read coerces either
//! storage class through the same union; the port therefore reads the RAW sql
//! value and coerces Integer/Real/Text uniformly. (A migrations audit found no
//! other fresh-vs-migration affinity divergence that a strictly-typed read
//! consumes: every other TEXT-read column is TEXT in both shapes, and the
//! numeric INTEGER-vs-REAL divergences are harmless under `f64` reads.)

use rusqlite::{Connection, Row};
use serde_json::{Map, Value};

use super::js_number_to_json;
use super::DbError;

/// Every column the three union members consume, in a fixed SELECT order. The
/// `type` discriminator is column 1; `chatId` / `isSilentMessage` are not read
/// here (see module docs). Indices below match this list.
const COLUMNS: &str = "id, type, role, content, rawResponse, tokenCount, promptTokens, \
     completionTokens, swipeGroupId, swipeIndex, attachments, debugMemoryLogs, thoughtSignature, \
     reasoningContent, reasoningSegments, participantId, recoveryType, renderedHtml, dangerFlags, \
     targetParticipantIds, systemSender, systemKind, opaqueContent, hostEvent, customAnnouncer, \
     carinaMeta, pendingExternalPrompt, pendingExternalPromptFull, pendingExternalAttachments, \
     summaryAnchor, context, systemEventType, description, totalTokens, provider, modelName, \
     estimatedCostUSD, createdAt, isSilentMessage, confirmed, confirmationChecked, \
     confirmationRevised, confirmationNotes, confirmationOriginalContent";

/// Nullable-optional TEXT/UUID/enum column: `Some` → string, `None` → omit.
fn put_opt_string(obj: &mut Map<String, Value>, key: &str, v: Option<String>) {
    if let Some(s) = v {
        obj.insert(key.to_string(), Value::String(s));
    }
}

/// Nullable-optional number column (`NULL` → omit, else the JS rendering).
fn put_opt_number(obj: &mut Map<String, Value>, key: &str, v: Option<f64>) {
    if let Some(n) = v {
        obj.insert(key.to_string(), js_number_to_json(n));
    }
}

/// Nullable-optional boolean column (`NULL` → omit, `0`/`1` → bool). The backend
/// `find` hydration coerces the INTEGER cell to a JS boolean; the nullable-optional
/// field drops on NULL. Used for the answer-confirmation flags.
fn put_opt_bool(obj: &mut Map<String, Value>, key: &str, v: Option<i64>) {
    if let Some(n) = v {
        obj.insert(key.to_string(), Value::Bool(n == 1));
    }
}

/// Nullable-optional JSON column (`NULL`/empty/`"null"` → omit, else parsed —
/// v4 `fromJsonSafe` + the `.optional()` drop). Parsed straight to `Value` so the
/// stored bytes (defaults already baked, exact int/float) pass through unchanged.
fn put_opt_json(obj: &mut Map<String, Value>, key: &str, v: Option<String>) {
    let Some(raw) = v else { return };
    if raw.is_empty() || raw == "null" {
        return;
    }
    if let Ok(parsed) = serde_json::from_str::<Value>(&raw) {
        if !parsed.is_null() {
            obj.insert(key.to_string(), parsed);
        }
    }
}

/// `isSilentMessage` — its ROW schema (`ChatMessageRowSchema`) is
/// `z.union([z.boolean(), z.number().transform(v => v === 1)]).nullable().optional()`
/// → TEXT affinity, so a stored boolean `true` is persisted as numeric TEXT
/// (`"1.0"`). v4's read applies that union (coerce to a number, compare `=== 1`) →
/// a `boolean`, which passes `MessageEventSchema`'s `z.boolean()` (so the message
/// is KEPT, not dropped — verified empirically against v4). A `NULL` cell → the
/// key is omitted (v4 `undefined`, dropped by `JSON.stringify`). A non-numeric
/// stored value coerces to `NaN`, and `NaN === 1` is `false` (matching JS
/// `Number()`).
fn put_is_silent(obj: &mut Map<String, Value>, v: rusqlite::types::Value) {
    use rusqlite::types::Value as SqlValue;
    let is_one = match &v {
        // NULL → the key is omitted (v4 `undefined`).
        SqlValue::Null => return,
        // The migration-affinity cell (INTEGER 1/0 on a real instance).
        SqlValue::Integer(i) => *i == 1,
        SqlValue::Real(f) => *f == 1.0,
        // The fresh-DDL TEXT-affinity cell ("1.0"/"0.0").
        SqlValue::Text(s) => s.trim().parse::<f64>().map(|n| n == 1.0).unwrap_or(false),
        SqlValue::Blob(_) => false,
    };
    obj.insert("isSilentMessage".to_string(), Value::Bool(is_one));
}

/// `attachments` (`z.array(UUIDSchema).default([])`, baked on write): parsed
/// array, or `[]` when `NULL`/empty/invalid.
fn array_or_empty(v: Option<String>) -> Value {
    v.as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .filter(Value::is_array)
        .unwrap_or_else(|| Value::Array(Vec::new()))
}

/// Marshal a `type='message'` row into a `MessageEvent` JSON object.
fn marshal_message(row: &Row) -> Result<Value, rusqlite::Error> {
    let mut o = Map::new();
    o.insert("type".into(), Value::String("message".into()));
    o.insert("id".into(), Value::String(row.get::<_, String>(0)?));
    o.insert("role".into(), Value::String(row.get::<_, String>(2)?));
    o.insert("content".into(), Value::String(row.get::<_, String>(3)?));
    put_opt_json(&mut o, "rawResponse", row.get(4)?);
    put_opt_number(&mut o, "tokenCount", row.get(5)?);
    put_opt_number(&mut o, "promptTokens", row.get(6)?);
    put_opt_number(&mut o, "completionTokens", row.get(7)?);
    put_opt_string(&mut o, "swipeGroupId", row.get(8)?);
    put_opt_number(&mut o, "swipeIndex", row.get(9)?);
    o.insert("attachments".into(), array_or_empty(row.get(10)?));
    put_opt_json(&mut o, "debugMemoryLogs", row.get(11)?);
    put_opt_string(&mut o, "thoughtSignature", row.get(12)?);
    put_opt_string(&mut o, "reasoningContent", row.get(13)?);
    put_opt_json(&mut o, "reasoningSegments", row.get(14)?);
    put_opt_string(&mut o, "participantId", row.get(15)?);
    put_opt_string(&mut o, "recoveryType", row.get(16)?);
    put_opt_string(&mut o, "renderedHtml", row.get(17)?);
    put_opt_json(&mut o, "dangerFlags", row.get(18)?);
    put_opt_string(&mut o, "provider", row.get(34)?);
    put_opt_string(&mut o, "modelName", row.get(35)?);
    put_opt_json(&mut o, "targetParticipantIds", row.get(19)?);
    put_opt_string(&mut o, "systemSender", row.get(20)?);
    put_opt_string(&mut o, "opaqueContent", row.get(22)?);
    put_opt_string(&mut o, "systemKind", row.get(21)?);
    put_opt_json(&mut o, "hostEvent", row.get(23)?);
    put_opt_json(&mut o, "summaryAnchor", row.get(29)?);
    put_opt_json(&mut o, "customAnnouncer", row.get(24)?);
    put_opt_json(&mut o, "carinaMeta", row.get(25)?);
    put_opt_string(&mut o, "pendingExternalPrompt", row.get(26)?);
    put_opt_string(&mut o, "pendingExternalPromptFull", row.get(27)?);
    put_opt_json(&mut o, "pendingExternalAttachments", row.get(28)?);
    o.insert("createdAt".into(), Value::String(row.get::<_, String>(37)?));
    put_is_silent(&mut o, row.get(38)?);
    put_opt_bool(&mut o, "confirmed", row.get(39)?);
    put_opt_bool(&mut o, "confirmationChecked", row.get(40)?);
    put_opt_bool(&mut o, "confirmationRevised", row.get(41)?);
    put_opt_string(&mut o, "confirmationNotes", row.get(42)?);
    put_opt_string(&mut o, "confirmationOriginalContent", row.get(43)?);
    Ok(Value::Object(o))
}

/// Marshal a `type='context-summary'` row into a `ContextSummaryEvent` (only
/// `type` / `id` / `context` / `createdAt`; every other column stripped).
fn marshal_context_summary(row: &Row) -> Result<Value, rusqlite::Error> {
    let mut o = Map::new();
    o.insert("type".into(), Value::String("context-summary".into()));
    o.insert("id".into(), Value::String(row.get::<_, String>(0)?));
    o.insert("context".into(), Value::String(row.get::<_, String>(30)?));
    o.insert("createdAt".into(), Value::String(row.get::<_, String>(37)?));
    Ok(Value::Object(o))
}

/// Marshal a `type='system'` row into a `SystemEvent`.
fn marshal_system(row: &Row) -> Result<Value, rusqlite::Error> {
    let mut o = Map::new();
    o.insert("type".into(), Value::String("system".into()));
    o.insert("id".into(), Value::String(row.get::<_, String>(0)?));
    o.insert(
        "systemEventType".into(),
        Value::String(row.get::<_, String>(31)?),
    );
    o.insert(
        "description".into(),
        Value::String(row.get::<_, String>(32)?),
    );
    put_opt_number(&mut o, "promptTokens", row.get(6)?);
    put_opt_number(&mut o, "completionTokens", row.get(7)?);
    put_opt_number(&mut o, "totalTokens", row.get(33)?);
    put_opt_string(&mut o, "provider", row.get(34)?);
    put_opt_string(&mut o, "modelName", row.get(35)?);
    put_opt_number(&mut o, "estimatedCostUSD", row.get(36)?);
    o.insert("createdAt".into(), Value::String(row.get::<_, String>(37)?));
    Ok(Value::Object(o))
}

/// Marshal one `chat_messages` row by its `type` discriminator. An unrecognized
/// `type` yields `None` (v4 `ChatEventSchema.safeParse` would fail → the row is
/// skipped as corrupted).
fn marshal_row(row: &Row) -> Result<Option<Value>, rusqlite::Error> {
    let typ: String = row.get(1)?;
    Ok(match typ.as_str() {
        "message" => Some(marshal_message(row)?),
        "context-summary" => Some(marshal_context_summary(row)?),
        "system" => Some(marshal_system(row)?),
        _ => None,
    })
}

/// `getMessages` — all events for a chat, ordered by `createdAt` ascending
/// (v4 `find({ chatId }, { sort: { createdAt: 1 } })`), each marshaled through
/// its union member; unrecognized rows are skipped.
pub fn get_messages(conn: &Connection, chat_id: &str) -> Result<Vec<Value>, DbError> {
    let sql =
        format!("SELECT {COLUMNS} FROM chat_messages WHERE chatId = ?1 ORDER BY createdAt ASC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([chat_id], marshal_row)?;
    let mut out = Vec::new();
    for r in rows {
        if let Some(v) = r? {
            out.push(v);
        }
    }
    Ok(out)
}

/// `getMessageCount` — `getMessages(chatId).length`, i.e. the count of rows that
/// parse (corrupted rows are excluded, matching v4).
pub fn get_message_count(conn: &Connection, chat_id: &str) -> Result<i64, DbError> {
    Ok(get_messages(conn, chat_id)?.len() as i64)
}

/// v4 `getLastPlayedMessageAt(chatId)` (`chats-messages.ops.ts`, the SQLite
/// branch): the `createdAt` of the newest **played** message — a `type:'message'`
/// row authored by a participant character or the human user, i.e. with
/// `systemSender IS NULL` (excludes Staff / personified-feature announcements that
/// persist as `type:'message'` and would otherwise keep a quiet chat's assets
/// alive). `None` when the chat has no played messages. Consumed only by the
/// stale-chat maintenance sweep (v4 `42242a3e`).
pub fn get_last_played_message_at(
    conn: &Connection,
    chat_id: &str,
) -> Result<Option<String>, DbError> {
    conn.query_row(
        "SELECT createdAt FROM chat_messages \
         WHERE chatId = ?1 AND type = 'message' AND systemSender IS NULL \
         ORDER BY createdAt DESC LIMIT 1",
        [chat_id],
        |r| r.get::<_, String>(0),
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    })
    .map_err(DbError::from)
}

/// `findChatIdForMessage` — the chat owning a message, via a direct indexed
/// lookup on the message id (v4 `findOne({ id })` → `chatId`). `None` when no
/// such message.
pub fn find_chat_id_for_message(
    conn: &Connection,
    message_id: &str,
) -> Result<Option<String>, DbError> {
    conn.query_row(
        "SELECT chatId FROM chat_messages WHERE id = ?1",
        [message_id],
        |r| r.get::<_, String>(0),
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    })
    .map_err(DbError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The MIGRATION-shape table (`add-silent-message-field`:
    /// `ADD COLUMN "isSilentMessage" INTEGER DEFAULT NULL`) — a real v4 instance
    /// stores INTEGER 1/0 cells where a fresh-`generateDDL` table stores numeric
    /// TEXT. The first Friday dogfood run surfaced this: a strictly-`String`
    /// read errors with "Invalid column type Integer … isSilentMessage".
    const MIGRATED_DDL: &str = "CREATE TABLE chat_messages (\
        id TEXT PRIMARY KEY, chatId TEXT, type TEXT, role TEXT, content TEXT, \
        rawResponse TEXT, tokenCount REAL, promptTokens REAL, completionTokens REAL, \
        swipeGroupId TEXT, swipeIndex REAL, attachments TEXT DEFAULT '[]', \
        debugMemoryLogs TEXT, thoughtSignature TEXT, reasoningContent TEXT, \
        reasoningSegments TEXT, participantId TEXT, recoveryType TEXT, renderedHtml TEXT, \
        dangerFlags TEXT, targetParticipantIds TEXT, isSilentMessage INTEGER, systemSender TEXT, \
        systemKind TEXT, opaqueContent TEXT, hostEvent TEXT, customAnnouncer TEXT, \
        carinaMeta TEXT, pendingExternalPrompt TEXT, pendingExternalPromptFull TEXT, \
        pendingExternalAttachments TEXT, summaryAnchor TEXT, context TEXT, \
        systemEventType TEXT, description TEXT, totalTokens REAL, provider TEXT, \
        modelName TEXT, estimatedCostUSD REAL, createdAt TEXT, confirmed INTEGER, \
        confirmationChecked INTEGER, confirmationRevised INTEGER, confirmationNotes TEXT, \
        confirmationOriginalContent TEXT);";

    #[test]
    fn is_silent_reads_integer_cells_from_a_migrated_instance() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(MIGRATED_DDL).unwrap();
        let insert = |id: &str, silent: Option<i64>, at: &str| {
            conn.execute(
                "INSERT INTO chat_messages (id, chatId, type, role, content, createdAt, \
                 isSilentMessage) VALUES (?1, 'c1', 'message', 'ASSISTANT', 'hi', ?2, ?3)",
                rusqlite::params![id, at, silent],
            )
            .unwrap();
        };
        insert("m1", Some(1), "2026-07-10T00:00:01.000Z");
        insert("m2", Some(0), "2026-07-10T00:00:02.000Z");
        insert("m3", None, "2026-07-10T00:00:03.000Z");

        let msgs = get_messages(&conn, "c1").unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0]["isSilentMessage"], Value::Bool(true));
        assert_eq!(msgs[1]["isSilentMessage"], Value::Bool(false));
        // NULL → the key is omitted (v4 `undefined`).
        assert!(msgs[2].get("isSilentMessage").is_none());
    }

    #[test]
    fn is_silent_still_reads_fresh_ddl_text_cells() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            &MIGRATED_DDL.replace("isSilentMessage INTEGER", "isSilentMessage TEXT"),
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chat_messages (id, chatId, type, role, content, createdAt, \
             isSilentMessage) VALUES ('m1', 'c1', 'message', 'ASSISTANT', 'hi', \
             '2026-07-10T00:00:01.000Z', '1.0')",
            [],
        )
        .unwrap();
        let msgs = get_messages(&conn, "c1").unwrap();
        assert_eq!(msgs[0]["isSilentMessage"], Value::Bool(true));
    }
}
