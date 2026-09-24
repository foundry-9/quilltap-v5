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
//! `hostEvent`, `customAnnouncer`, `carinaMeta`, `pascalMeta`,
//! `pendingExternalAttachments`,
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
use super::text_compression::CompressedText;
use super::DbError;

/// Every column the three union members consume, in a fixed SELECT order. The
/// `type` discriminator is column 1; `chatId` / `isSilentMessage` are not read
/// here (see module docs). Indices below match this list.
///
/// New columns are APPENDED (`pascalMeta` at 44, `routeTrail` at 45), never
/// spliced in beside their schema neighbour: the marshaling below is
/// positional, so inserting one mid-list would silently re-point every later
/// index. The SELECT order is free — the emitted key order is fixed by the
/// `Map` insertion order in [`marshal_message`], not by this list.
const COLUMNS: &str = "id, type, role, content, rawResponse, tokenCount, promptTokens, \
     completionTokens, swipeGroupId, swipeIndex, attachments, debugMemoryLogs, thoughtSignature, \
     reasoningContent, reasoningSegments, participantId, recoveryType, renderedHtml, dangerFlags, \
     targetParticipantIds, systemSender, systemKind, opaqueContent, hostEvent, customAnnouncer, \
     carinaMeta, pendingExternalPrompt, pendingExternalPromptFull, pendingExternalAttachments, \
     summaryAnchor, context, systemEventType, description, totalTokens, provider, modelName, \
     estimatedCostUSD, createdAt, isSilentMessage, confirmed, confirmationChecked, \
     confirmationRevised, confirmationNotes, confirmationOriginalContent, pascalMeta, routeTrail";

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

/// `Option<CompressedText>` → `Option<String>` for the nullable compressed
/// columns, so a call site reads exactly as it did before the codec landed.
fn opt_text(v: Option<CompressedText>) -> Option<String> {
    v.map(|c| c.0)
}

/// Marshal a `type='message'` row into a `MessageEvent` JSON object.
fn marshal_message(row: &Row) -> Result<Value, rusqlite::Error> {
    let mut o = Map::new();
    o.insert("type".into(), Value::String("message".into()));
    o.insert("id".into(), Value::String(row.get::<_, String>(0)?));
    o.insert("role".into(), Value::String(row.get::<_, String>(2)?));
    // `content` is a REGISTERED COMPRESSED COLUMN (v4 `manager.ts:134-139`):
    // on a v4-4.10 instance any cell of 512 bytes or more is a brotli BLOB,
    // which a `String` bind rejects with `InvalidColumnType`. Because
    // `get_messages` propagates the `?` out of the whole `query_map`, ONE
    // compressed cell used to fail the ENTIRE chat read.
    o.insert(
        "content".into(),
        Value::String(row.get::<_, CompressedText>(3)?.0),
    );
    put_opt_json(&mut o, "rawResponse", row.get(4)?);
    put_opt_number(&mut o, "tokenCount", row.get(5)?);
    put_opt_number(&mut o, "promptTokens", row.get(6)?);
    put_opt_number(&mut o, "completionTokens", row.get(7)?);
    put_opt_string(&mut o, "swipeGroupId", row.get(8)?);
    put_opt_number(&mut o, "swipeIndex", row.get(9)?);
    o.insert("attachments".into(), array_or_empty(row.get(10)?));
    // v4's `MessageEventSchema` declares `createdAt` HERE, directly after
    // `attachments` and before `debugMemoryLogs` (`lib/schemas/chat.types.ts:
    // 234`), and v4's hydration follows the schema — so this is where the key
    // lands on the wire. v5 had emitted it 20 keys later, between
    // `pendingExternalAttachments` and `isSilentMessage`, which nothing could
    // see until P4.D183's message-EVENTS listing gave the raw marshal a wire
    // of its own (every other consumer either sorts keys or re-projects). The
    // sibling `marshal_context_summary` / `marshal_system` always had it in
    // schema position, which is what identified this as the slip.
    o.insert("createdAt".into(), Value::String(row.get::<_, String>(37)?));
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
    // The route trail (v4 `5841a8c62`): every model tried, in order, for this
    // reply. `RouteAttemptSchema.array().nullable().optional()` — NULL (nearly
    // every message) or absent both omit the key here, matching every other
    // nullable-optional JSON column above; the Chat GET's `|| null` force-
    // present is `api::salon::assemble_chat_get`'s concern, not this raw event
    // marshal's.
    put_opt_json(&mut o, "routeTrail", row.get(45)?);
    put_opt_json(&mut o, "targetParticipantIds", row.get(19)?);
    put_opt_string(&mut o, "systemSender", row.get(20)?);
    // REGISTERED COMPRESSED COLUMN — see `content` above.
    put_opt_string(&mut o, "opaqueContent", opt_text(row.get(22)?));
    put_opt_string(&mut o, "systemKind", row.get(21)?);
    put_opt_json(&mut o, "hostEvent", row.get(23)?);
    put_opt_json(&mut o, "summaryAnchor", row.get(29)?);
    put_opt_json(&mut o, "customAnnouncer", row.get(24)?);
    put_opt_json(&mut o, "carinaMeta", row.get(25)?);
    // Emitted here — `pascalMeta` follows `carinaMeta` in v4's MessageEventSchema
    // declaration — though it is SELECTed last (see `COLUMNS`).
    put_opt_json(&mut o, "pascalMeta", row.get(44)?);
    put_opt_string(&mut o, "pendingExternalPrompt", row.get(26)?);
    put_opt_string(&mut o, "pendingExternalPromptFull", row.get(27)?);
    put_opt_json(&mut o, "pendingExternalAttachments", row.get(28)?);
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
    // REGISTERED COMPRESSED COLUMN.
    o.insert(
        "context".into(),
        Value::String(row.get::<_, CompressedText>(30)?.0),
    );
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
    // REGISTERED COMPRESSED COLUMN.
    o.insert(
        "description".into(),
        Value::String(row.get::<_, CompressedText>(32)?.0),
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
/// `type` yields `None` (v4 `ChatEventSchema.safeParse` fails → [`read_row`]
/// reports the row CORRUPTED, so it is skipped with v4's WARN).
fn marshal_row(row: &Row) -> Result<Option<Value>, rusqlite::Error> {
    let typ: String = row.get(1)?;
    Ok(match typ.as_str() {
        "message" => Some(marshal_message(row)?),
        "context-summary" => Some(marshal_context_summary(row)?),
        "system" => Some(marshal_system(row)?),
        _ => None,
    })
}

/// v4's `RoleEnum` (`lib/schemas/common.types.ts:38`).
const ROLE_ENUM: [&str; 4] = ["SYSTEM", "USER", "ASSISTANT", "TOOL"];

/// `hostEvent.toStatus`'s enum (`lib/schemas/chat.types.ts`, `MessageEventSchema`).
const HOST_EVENT_STATUSES: [&str; 4] = ["active", "silent", "absent", "removed"];

/// P4.112 — the Zod-only half of v4's per-row `ChatEventSchema.safeParse`: the
/// shapes a raw cell can carry that still MARSHAL here (every cell reads as
/// its column's type) but that v4's schema rejects, so v4 skips the row with
/// `Skipping corrupted chat message` exactly as it does a bad cell. The
/// `00c290c9a` unification review named three, and those three are what this
/// checks: an `id` that is not a Zod uuid (every member's `id: UUIDSchema`), a
/// message `role` outside `RoleEnum`, and a message `hostEvent` that is not
/// its object shape (`{ participantId?: uuid, toStatus?: enum,
/// introducedCharacterIds?: uuid[] }` — `.optional()`, not `.nullable()`, so a
/// PRESENT `null` inside it fails too). P4.113 adds the last two a raw cell
/// can carry: `createdAt` (`TimestampSchema`, all three members — through the
/// ONE [`zod_iso_datetime_ok`](crate::api::zod_issues::zod_iso_datetime_ok)
/// home) and the message's `participantId` (`UUIDSchema.nullable().optional()`);
/// and makes this the ONE home of the check for BOTH the per-row skip and
/// `updateMessage`'s `ChatEventSchema.parse` of the MERGED event
/// (`chats_messages.rs`). Returns the failing path + reason, the
/// stand-in for v4's issue list (v5 has no Zod issue source here). Every
/// other `MessageEventSchema` field v4 can reject on a raw cell (`recoveryType`,
/// `attachments`' uuids, `systemEventType`, the nested JSON shapes …) is NOT
/// checked — named in P4.112's lane record, never silently claimed.
pub(crate) fn zod_shape_failure(event: &Value) -> Option<String> {
    use crate::api::zod_issues::{zod_iso_datetime_ok, zod_uuid_ok};
    let obj = event.as_object()?;
    let is_uuid = |v: &Value| v.as_str().is_some_and(zod_uuid_ok);
    if !obj.get("id").is_some_and(is_uuid) {
        return Some("id: Invalid UUID".to_string());
    }
    // P4.113: `createdAt: TimestampSchema` on ALL THREE members —
    // `z.iso.datetime().or(z.date())`; a JSON value is never a `Date`, so the
    // string arm is the whole check.
    if !obj
        .get("createdAt")
        .and_then(Value::as_str)
        .is_some_and(zod_iso_datetime_ok)
    {
        return Some(format!(
            "createdAt: Invalid ISO datetime ({})",
            obj.get("createdAt").unwrap_or(&Value::Null)
        ));
    }
    if obj.get("type").and_then(Value::as_str) != Some("message") {
        return None;
    }
    let role = obj.get("role").and_then(Value::as_str).unwrap_or_default();
    if !ROLE_ENUM.contains(&role) {
        return Some(format!("role: invalid option {role:?}"));
    }
    // P4.113: `participantId: UUIDSchema.nullable().optional()` (message only).
    if let Some(p) = obj.get("participantId") {
        if !p.is_null() && !is_uuid(p) {
            return Some(format!("participantId: Invalid UUID ({p})"));
        }
    }
    // `.nullable().optional()` on the object itself: a `null` hostEvent passes
    // (the read path never sees one — `put_opt_json` drops it — but a MERGED
    // update event does: `{ hostEvent: null }` is the repair, P4.113).
    if let Some(host) = obj.get("hostEvent").filter(|h| !h.is_null()) {
        let ok = host.as_object().is_some_and(|h| {
            h.get("participantId").is_none_or(is_uuid)
                && h.get("toStatus")
                    .is_none_or(|s| s.as_str().is_some_and(|s| HOST_EVENT_STATUSES.contains(&s)))
                && h.get("introducedCharacterIds")
                    .is_none_or(|ids| ids.as_array().is_some_and(|a| a.iter().all(is_uuid)))
        });
        if !ok {
            return Some(format!("hostEvent: not its object shape ({host})"));
        }
    }
    None
}

/// The `context` field on this module's `safeQuery`-arm lines (the house shape of
/// `db::chats_search`'s `LOG_CONTEXT`; v4's lines come from
/// `chats-messages.ops.ts`).
const LOG_CONTEXT: &str = "db.chats-messages";

/// One row as `get_messages` sees it: kept, or CORRUPTED — an unknown `type`,
/// a cell `marshal_row` cannot read as its member's type, or (P4.112) a
/// marshaled event [`zod_shape_failure`] rejects. All are v4's per-row
/// `ChatEventSchema.safeParse` failing, so all take its WARN.
enum RowOutcome {
    Event(Value),
    Corrupted {
        id: Option<String>,
        typ: Option<String>,
        error: String,
    },
}

/// Is this a per-CELL read failure (the row's own data does not fit its
/// member — v4's `ChatEventSchema.safeParse` failing on that one row) rather
/// than a failure of the statement itself?
fn is_cell_error(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::InvalidColumnType(..)
            | rusqlite::Error::FromSqlConversionFailure(..)
            | rusqlite::Error::IntegralValueOutOfRange(..)
    )
}

fn read_row(row: &Row) -> Result<RowOutcome, rusqlite::Error> {
    let corrupted = |error: String| RowOutcome::Corrupted {
        id: row.get::<_, Option<String>>(0).ok().flatten(),
        typ: row.get::<_, Option<String>>(1).ok().flatten(),
        error,
    };
    match marshal_row(row) {
        // P4.112: a row whose cells all read can still fail v4's Zod shape.
        Ok(Some(v)) => Ok(match zod_shape_failure(&v) {
            None => RowOutcome::Event(v),
            Some(error) => corrupted(error),
        }),
        // The `00c290c9a` unification: an unknown `type` fails v4's
        // discriminated-union `safeParse` exactly like a bad cell, so it takes
        // the same WARN (P4.109 had dropped it silently).
        Ok(None) => Ok(corrupted(format!(
            "unrecognized chat event type {:?}",
            row.get::<_, Option<String>>(1)
                .ok()
                .flatten()
                .unwrap_or_default()
        ))),
        Err(e) if is_cell_error(&e) => Ok(corrupted(e.to_string())),
        Err(e) => Err(e),
    }
}

/// `getMessages` — all events for a chat, ordered by `createdAt` ascending
/// (v4 `find({ chatId }, { sort: { createdAt: 1 } })`), each marshaled through
/// its union member; unrecognized rows are skipped.
///
/// **Never answers `Err`** (P4.109). v4's `getMessages`
/// (`chats-messages.ops.ts:315-366`) is `safeQuery(…, 'Failed to get messages
/// for chat', { chatId }, [])` — FALLBACK mode — so a read that throws logs one
/// ERROR and answers `[]`. That is where v4's `countMessagesWithText`,
/// `findMessagesWithText` and `replaceInMessages` actually swallow on SQLite:
/// their own `safeQuery` arms are unreachable, because the only thing in them
/// that can throw is this call. The `Result` stays in the signature (the
/// P4.105 precedent) so no caller moves.
///
/// A caller whose v4 counterpart does NOT read through `getMessages` — it
/// rethrows, or reads another way — calls [`get_messages_strict`] instead. The
/// choice is made per call site by the caller census (P4.109's lane record,
/// guarded by `get_messages_caller_census`), never by blanket rule.
pub fn get_messages(conn: &Connection, chat_id: &str) -> Result<Vec<Value>, DbError> {
    match get_messages_strict(conn, chat_id) {
        Ok(events) => Ok(events),
        Err(err) => {
            tracing::error!(
                target: "quilltap::db",
                context = LOG_CONTEXT,
                chatId = chat_id,
                error = %err,
                "Failed to get messages for chat",
            );
            Ok(Vec::new())
        }
    }
}

/// P4.113 — v4 `updateMessage`'s `messagesCollection.findOne({ id: messageId,
/// chatId })` (`chats-messages.ops.ts:524`): ONE row by `(id, chatId)`,
/// hydrated, with **NO Zod** — so a row the per-row skip would reject (a bad
/// `role`, a non-uuid `id`, a malformed `createdAt` …) is still FOUND here,
/// and its fate is decided by `ChatEventSchema.parse` of the MERGED event
/// ([`zod_shape_failure`], the caller's job): a repairing update writes, a
/// non-repairing one throws into `Failed to update message in chat`. No WARN
/// is ever logged here, for this row or any sibling — the whole-chat strict
/// read this replaced emitted every corrupted SIBLING's `Skipping corrupted
/// chat message` on a healthy update, and answered a corrupted target as
/// not-found.
///
/// An unknown `type` answers `Err` (v4's hydrated row then fails the parse —
/// the same ERROR). ⚠ **A per-CELL failure also answers `Err`** — a recorded
/// divergence, not taken by P4.113: v4's `hydrateRow` has no member types, so
/// a NULL `content` row hydrates and a `{ content }` update REPAIRS it, where
/// v5's typed marshal fails before the merge and logs the ERROR (named in
/// P4.113's lane record).
pub(crate) fn find_event_raw(
    conn: &Connection,
    chat_id: &str,
    message_id: &str,
) -> Result<Option<Value>, DbError> {
    let sql = format!("SELECT {COLUMNS} FROM chat_messages WHERE id = ?1 AND chatId = ?2 LIMIT 1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([message_id, chat_id])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    match marshal_row(row)? {
        Some(event) => Ok(Some(event)),
        None => Err(DbError::Internal(format!(
            "updateMessage parse: unrecognized chat event type {:?}",
            row.get::<_, Option<String>>(1)?.unwrap_or_default()
        ))),
    }
}

/// [`get_messages`] WITHOUT v4's `safeQuery` fallback: a failing read answers
/// `Err`. For the call sites whose v4 counterpart rethrows or reads the rows
/// some other way (see [`get_messages`]).
///
/// A CORRUPTED row — one whose cells do not fit its member, e.g. a NULL
/// `content` — is skipped with v4's WARN `Skipping corrupted chat message`
/// (`chats-messages.ops.ts:346`, the per-row `ChatEventSchema.safeParse`), not
/// allowed to fail the whole chat (P4.105's finding 1). v4's `errors` field
/// carries Zod issue strings v5 has no source for; v5 logs the cell error in
/// its place.
pub fn get_messages_strict(conn: &Connection, chat_id: &str) -> Result<Vec<Value>, DbError> {
    let sql =
        format!("SELECT {COLUMNS} FROM chat_messages WHERE chatId = ?1 ORDER BY createdAt ASC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([chat_id], read_row)?;
    let mut out = Vec::new();
    for r in rows {
        match r? {
            RowOutcome::Event(v) => out.push(v),
            RowOutcome::Corrupted { id, typ, error } => {
                // v4's `msg?.id || 'unknown'`: JS `||`, so an EMPTY string
                // falls back too, not only an absent one.
                let or_unknown = |v: &Option<String>| {
                    v.clone()
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "unknown".into())
                };
                tracing::warn!(
                    target: "quilltap::db",
                    context = LOG_CONTEXT,
                    chatId = chat_id,
                    messageId = or_unknown(&id).as_str(),
                    messageType = or_unknown(&typ).as_str(),
                    error = %error,
                    "Skipping corrupted chat message",
                );
            }
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
/// branch): the `createdAt` of the newest message a character posted as
/// content, per [`crate::chat_activity::is_character_authored_message`], which
/// is THE definition of chat activity and the thing to change if this needs to
/// move. In short: `type === 'message'`, role `USER`/`ASSISTANT`, no
/// `systemSender`, no `customAnnouncer`. Whispers count; Staff announcements,
/// announcement bubbles, and raw tool rows don't.
///
/// `None` when the chat has no character-authored messages at all. This is the
/// value mirrored into the chat's `lastMessageAt` column (which every list and
/// sort reads — v4 `735d9408c`), and the value the stale-chat maintenance sweep
/// uses to decide whether a chat has gone quiet (v4 `42242a3e`).
pub fn get_last_played_message_at(
    conn: &Connection,
    chat_id: &str,
) -> Result<Option<String>, DbError> {
    conn.query_row(
        &format!(
            "SELECT createdAt FROM chat_messages WHERE chatId = ?1 AND {} \
             ORDER BY createdAt DESC LIMIT 1",
            crate::chat_activity::CHARACTER_AUTHORED_MESSAGE_FILTER
        ),
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

    // P4.112: message ids must be Zod uuids — v4's `ChatEventSchema` (and so
    // `get_messages`) skips any other id as a corrupted row.
    const M1: &str = "a0000000-0000-4000-8000-000000000001";
    const M2: &str = "a0000000-0000-4000-8000-000000000002";
    const M3: &str = "a0000000-0000-4000-8000-000000000003";

    /// The MIGRATION-shape table (`add-silent-message-field`:
    /// `ADD COLUMN "isSilentMessage" INTEGER DEFAULT NULL`) — a real v4 instance
    /// stores INTEGER 1/0 cells where a fresh-`generateDDL` table stores numeric
    /// TEXT. The first Friday dogfood run surfaced this: a strictly-`String`
    /// read errors with "Invalid column type Integer … isSilentMessage".
    /// P4.88: `routeTrail` is NOT listed — every fixture in the tree gets the
    /// two `78b381a96` columns from the ONE home,
    /// [`crate::test_support::ensure_p4d171_columns`], so a re-dump moves them
    /// in one place instead of seven.
    const MIGRATED_DDL: &str = "CREATE TABLE chat_messages (\
        id TEXT PRIMARY KEY, chatId TEXT, type TEXT, role TEXT, content TEXT, \
        rawResponse TEXT, tokenCount REAL, promptTokens REAL, completionTokens REAL, \
        swipeGroupId TEXT, swipeIndex REAL, attachments TEXT DEFAULT '[]', \
        debugMemoryLogs TEXT, thoughtSignature TEXT, reasoningContent TEXT, \
        reasoningSegments TEXT, participantId TEXT, recoveryType TEXT, renderedHtml TEXT, \
        dangerFlags TEXT, targetParticipantIds TEXT, isSilentMessage INTEGER, systemSender TEXT, \
        systemKind TEXT, opaqueContent TEXT, hostEvent TEXT, customAnnouncer TEXT, \
        carinaMeta TEXT, pascalMeta TEXT, pendingExternalPrompt TEXT, pendingExternalPromptFull TEXT, \
        pendingExternalAttachments TEXT, summaryAnchor TEXT, context TEXT, \
        systemEventType TEXT, description TEXT, totalTokens REAL, provider TEXT, \
        modelName TEXT, estimatedCostUSD REAL, createdAt TEXT, confirmed INTEGER, \
        confirmationChecked INTEGER, confirmationRevised INTEGER, confirmationNotes TEXT, \
        confirmationOriginalContent TEXT);";

    #[test]
    fn is_silent_reads_integer_cells_from_a_migrated_instance() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(MIGRATED_DDL).unwrap();
        crate::test_support::ensure_p4d171_columns(&conn);
        let insert = |id: &str, silent: Option<i64>, at: &str| {
            conn.execute(
                "INSERT INTO chat_messages (id, chatId, type, role, content, createdAt, \
                 isSilentMessage) VALUES (?1, 'c1', 'message', 'ASSISTANT', 'hi', ?2, ?3)",
                rusqlite::params![id, at, silent],
            )
            .unwrap();
        };
        insert(M1, Some(1), "2026-07-10T00:00:01.000Z");
        insert(M2, Some(0), "2026-07-10T00:00:02.000Z");
        insert(M3, None, "2026-07-10T00:00:03.000Z");

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
        crate::test_support::ensure_p4d171_columns(&conn);
        conn.execute(
            "INSERT INTO chat_messages (id, chatId, type, role, content, createdAt, \
             isSilentMessage) VALUES ('a0000000-0000-4000-8000-000000000001', 'c1', 'message', 'ASSISTANT', 'hi', \
             '2026-07-10T00:00:01.000Z', '1.0')",
            [],
        )
        .unwrap();
        let msgs = get_messages(&conn, "c1").unwrap();
        assert_eq!(msgs[0]["isSilentMessage"], Value::Bool(true));
    }

    /// A three-row chat on the migrated DDL: `M1`, `M2`, `M3` in order.
    fn three_rows() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(MIGRATED_DDL).unwrap();
        crate::test_support::ensure_p4d171_columns(&conn);
        for (id, at) in [
            (M1, "2026-09-23T00:00:01.000Z"),
            (M2, "2026-09-23T00:00:02.000Z"),
            (M3, "2026-09-23T00:00:03.000Z"),
        ] {
            conn.execute(
                "INSERT INTO chat_messages (id, chatId, type, role, content, createdAt) \
                 VALUES (?1, 'c1', 'message', 'USER', 'hi', ?2)",
                rusqlite::params![id, at],
            )
            .unwrap();
        }
        conn
    }

    fn ids(events: &[Value]) -> Vec<&str> {
        events.iter().filter_map(|e| e["id"].as_str()).collect()
    }

    /// P4.109: v4's `getMessages` is a FALLBACK `safeQuery` — a read that
    /// throws logs ONE `Failed to get messages for chat` ERROR `{chatId,
    /// error}` and answers `[]`. The strict sibling still answers `Err` and
    /// logs nothing.
    #[test]
    fn a_read_that_throws_answers_empty_and_logs_once() {
        let conn = three_rows();
        conn.execute_batch(r#"ALTER TABLE "chat_messages" RENAME TO "chat_messages_gone""#)
            .expect("plant");
        let (events, lines) = crate::test_support::captured_with(|| get_messages(&conn, "c1"));
        assert_eq!(
            events.expect("v4's safeQuery never throws here"),
            Vec::<Value>::new()
        );
        let errors: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("Failed to get messages for chat"))
            .collect();
        assert_eq!(errors.len(), 1, "one ERROR: {lines:?}");
        let line = errors[0];
        assert!(
            line.starts_with("ERROR quilltap::db"),
            "level/target: {line}"
        );
        assert!(line.contains("context=db.chats-messages"), "{line}");
        assert!(line.contains("chatId=c1"), "{line}");
        assert!(line.contains("no such table: chat_messages"), "{line}");

        let (strict, lines) =
            crate::test_support::captured_with(|| get_messages_strict(&conn, "c1"));
        assert!(strict.is_err(), "the strict variant rethrows");
        assert!(lines.is_empty(), "and logs nothing of its own: {lines:?}");
    }

    /// The silence leg: a healthy read never reaches the `safeQuery` ERROR.
    #[test]
    fn a_healthy_read_does_not_log_the_safe_query_error() {
        let conn = three_rows();
        let (events, lines) = crate::test_support::captured_with(|| get_messages(&conn, "c1"));
        assert_eq!(ids(&events.unwrap()), [M1, M2, M3]);
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// P4.105's finding 1: ONE corrupted row — a NULL `content`, which v4's
    /// per-row `ChatEventSchema.safeParse` rejects — is skipped with v4's WARN
    /// `Skipping corrupted chat message` `{chatId, messageId, messageType}`;
    /// the rest of the chat still reads. Both variants skip (the skip is inside
    /// v4's `safeQuery` operation, not its fallback).
    #[test]
    fn a_null_content_row_is_skipped_with_a_warn_not_fatal() {
        let conn = three_rows();
        conn.execute(
            "UPDATE chat_messages SET content = NULL WHERE id = 'a0000000-0000-4000-8000-000000000002'",
            [],
        )
        .unwrap();
        for strict in [false, true] {
            let (events, lines) = crate::test_support::captured_with(|| {
                if strict {
                    get_messages_strict(&conn, "c1")
                } else {
                    get_messages(&conn, "c1")
                }
            });
            assert_eq!(
                ids(&events.expect("one bad row never fails the chat")),
                [M1, M3]
            );
            assert_eq!(lines.len(), 1, "strict={strict}: {lines:?}");
            let line = &lines[0];
            assert!(
                line.starts_with("WARN quilltap::db"),
                "level/target: {line}"
            );
            assert!(line.contains("Skipping corrupted chat message"), "{line}");
            assert!(line.contains("chatId=c1"), "{line}");
            assert!(line.contains(&format!("messageId={M2}")), "{line}");
            assert!(line.contains("messageType=message"), "{line}");
        }
    }

    /// The `00c290c9a` unification (the §3 review): a row whose `type` is no
    /// union member fails v4's `ChatEventSchema.safeParse` exactly as a bad
    /// cell does, so it takes the same WARN — P4.109 had skipped it SILENTLY.
    /// And v4's `msg?.id || 'unknown'` is JS `||`: an EMPTY id falls back too.
    #[test]
    fn an_unknown_type_row_is_skipped_with_the_same_warn() {
        let conn = three_rows();
        conn.execute(
            "UPDATE chat_messages SET type = 'bogus', id = '' WHERE id = 'a0000000-0000-4000-8000-000000000002'",
            [],
        )
        .unwrap();
        for strict in [false, true] {
            let (events, lines) = crate::test_support::captured_with(|| {
                if strict {
                    get_messages_strict(&conn, "c1")
                } else {
                    get_messages(&conn, "c1")
                }
            });
            assert_eq!(ids(&events.unwrap()), [M1, M3]);
            assert_eq!(lines.len(), 1, "strict={strict}: {lines:?}");
            let line = &lines[0];
            assert!(line.starts_with("WARN quilltap::db"), "{line}");
            assert!(line.contains("Skipping corrupted chat message"), "{line}");
            assert!(line.contains("messageId=unknown"), "{line}");
            assert!(line.contains("messageType=bogus"), "{line}");
        }
    }
}
