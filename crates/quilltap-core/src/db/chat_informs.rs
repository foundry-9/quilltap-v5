//! The chat-informs repository — P4.D205, porting v4 `e7d77bb60`'s
//! `lib/database/repositories/chat-informs.repository.ts` (311 lines) and the
//! `chat_informs` table behind the Salon's **Inform** action.
//!
//! An *inform* is an out-of-character passage the operator hands to one or more
//! LLM-controlled seats: delivered verbatim as its own system block on each
//! target's next generation, and consumed once that turn produces a *persisted*
//! assistant message.
//!
//! ## One row per (batch × target)
//!
//! The body is duplicated per target on purpose (v4's schema header says so in
//! as many words): consumption is then a single-row write with no
//! read-modify-write of a shared array that a buffered job-child write could
//! clobber. `batchId` is what re-assembles the rows into the post the operator
//! made, and it is the unit cancel operates on.
//!
//! ## Two DDLs — and which one v5 follows (the order's item-2 measurement)
//!
//! v4 ships the table twice, and the shapes differ:
//!
//!   - **`generateDDL(ChatInformSchema)`** — reached through the repo's lazy
//!     `ensureCollection` on first access. **No foreign key**, and one index,
//!     `idx_chat_informs_createdAt` on `("createdAt" DESC)`.
//!   - **the `add-chat-informs-table-v1` migration** — hand-written, with
//!     `FOREIGN KEY ("chatId") REFERENCES "chats"("id") ON DELETE CASCADE` and
//!     three purpose-built indexes (`_pending`, `_batch`, `_consumedBy`).
//!
//! **Measured at the target pin `f45a517a9`, not inferred:** v4's real startup
//! runs the migration runner at `instrumentation.ts`'s PHASE 1 — "Run Migrations
//! FIRST - before anything else", and nothing ahead of it touches a repository —
//! so on a *real* v4 instance the MIGRATION creates the table and
//! `ensureCollection` later finds it present and no-ops. The generateDDL shape
//! is what lands wherever the migration runner does not run: jest, the tsx
//! oracles, and the D23 schema dump. (The order predicted the opposite —
//! "expect … the migration's `shouldRun` false". It is the `createdAt` index
//! that never lands on a real instance, not the migration's three. Recorded as
//! a v4-side quirk, filing candidate, in the lane record.)
//!
//! **v5 follows the generateDDL surface**, which is the same choice the port has
//! made for every other table since P4.4 (see
//! `harness/oracle/provision/dump-fresh-schema.ts`'s header: a long-lived v4
//! instance's schema is the hand-written base plus ~170 ALTER migrations, and
//! v4's repos — like v5's — are column-name-addressed, so the generateDDL
//! surface is a valid v4-compatible schema). Concretely:
//!
//!   - `services/provisioning/fresh_schema.json` carries the two statements,
//!     re-dumped from v4's live `generateDDL` at the pin (D23 — never by hand).
//!   - [`ensure_chat_informs_table`] emits the SAME two statements on the boot
//!     chain, so an existing instance gains the table on open.
//!   - **Because the v5 surface has no FK, the chat-delete cascade must delete
//!     these rows explicitly** — which is exactly what v4's own `deleteByChatId`
//!     exists for ("the FK already cascades; this is for callers that ask"). See
//!     [`ChatInformsRepository::delete_by_chat_id`] and its caller — the
//!     repository's own delete, `db::chats::ChatsRepository::delete`
//!     (`db/chats.rs`, the `P4.D205` fence beside the conversation-annotations
//!     sweep), NOT the `api/chat_delete.rs` handler, which never touches this
//!     table directly.
//!
//! ## Method names
//!
//! Kept as v4 spells them. v4 chose them so its background-job child proxy
//! classifies reads/writes without an override (`find` / `create` / `mark` /
//! `delete`); v5 has no such proxy, but the oracle cases call them by name and
//! the correspondence is worth more than a rename.
//!
//! ## Standing informs (P4.D249, v4 `52d6e7ecd`)
//!
//! A **standing** row (`permanent: true`) is never retired by consumption: it
//! is in force for every generation its seat makes in this chat until the
//! operator withdraws it. `consumedAt` / `consumedByMessageId` on such a row
//! record only its first delivery. [`is_inform_in_force`] is the one
//! definition of "still owed" — every read and delete below goes through it.
//! The column arrives on an existing instance through
//! `db::chat_informs_permanent_repair` (v4's migration, re-homed).
//!
//! ## Ordering
//!
//! Two comparators, reproducing v4's exactly. [`by_posting_order`]:
//! `new Date(createdAt).getTime()` ascending, tie-broken by
//! `id.localeCompare(...)` — `id` breaks the tie because a single post mints
//! several rows inside one millisecond. [`by_delivery_order`]: standing rows
//! first, then posting order — the two per-seat reads use it (the prompt path
//! stacks in this order); the batches read keeps posting order. See
//! [`by_posting_order`] for the NaN leg.

use rusqlite::{params, Connection};

use super::DbError;

/// The two statements v4's `generateDDL(ChatInformSchema)` emits, verbatim from
/// the D23 re-dump #4 at `52d6e7ecd` (see the module header on why this shape
/// and not the migration's). Kept byte-identical to `fresh_schema.json`'s
/// entries — modulo the `IF NOT EXISTS` the ensure needs, which SQLite strips
/// from `sqlite_master` — so a fresh instance and an ensured one carry the same
/// text (pinned by `the_table_ddl_matches_the_d23_dump`).
///
/// `"permanent" INTEGER DEFAULT 0` is generateDDL's spelling of v4's
/// `z.boolean().default(false)`, in schema order and **without** `NOT NULL`;
/// v4's migration ALTER (`chat_informs_permanent_repair`) spells it `INTEGER
/// NOT NULL DEFAULT 0` and appends it. The two v4 shapes disagree; both are
/// carried.
pub(crate) const CHAT_INFORMS_TABLE_DDL: &str = r#"CREATE TABLE IF NOT EXISTS "chat_informs" (
  "id" TEXT PRIMARY KEY NOT NULL,
  "chatId" TEXT NOT NULL,
  "batchId" TEXT NOT NULL,
  "participantId" TEXT NOT NULL,
  "contentMarkdown" TEXT NOT NULL,
  "recordMessageId" TEXT,
  "permanent" INTEGER DEFAULT 0,
  "createdAt" TEXT NOT NULL,
  "updatedAt" TEXT NOT NULL,
  "consumedAt" TEXT,
  "consumedByMessageId" TEXT
);
CREATE INDEX IF NOT EXISTS "idx_chat_informs_createdAt" ON "chat_informs" ("createdAt" DESC)"#;

/// Create `chat_informs` if the main partition lacks it (v4's
/// `ensureCollection`, which the repo reaches lazily on first access).
/// Idempotent; a no-op on every instance that has one — including a fresh v5
/// instance, which arrives carrying both statements from the D23 re-dump.
///
/// Load-bearing on an *existing* instance: without it every read below would hit
/// `no such table: chat_informs`, and the composer's pending-chip poll runs on
/// every Salon open.
pub fn ensure_chat_informs_table(main: &Connection) -> Result<(), DbError> {
    main.execute_batch(CHAT_INFORMS_TABLE_DDL)?;
    Ok(())
}

/// One `chat_informs` row — every column of v4's `ChatInformSchema`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatInformRow {
    pub id: String,
    pub chat_id: String,
    /// Shared by every row one post produced — the unit the operator cancels.
    pub batch_id: String,
    /// A chat PARTICIPANT id, never a character id.
    pub participant_id: String,
    /// Exactly what the operator typed. Delivered verbatim; never framed.
    pub content_markdown: String,
    /// The Host transcript message documenting the post. Nullable so a
    /// record-write failure cannot orphan the batch.
    pub record_message_id: Option<String>,
    /// A standing inform: delivered on every generation this seat makes in this
    /// chat, never retired by consumption, until the operator withdraws it.
    /// `false` (the default) is the one-shot inform gone after the seat's next
    /// turn.
    pub permanent: bool,
    pub created_at: String,
    pub updated_at: String,
    /// `None` while pending. On a standing row it records the FIRST delivery
    /// only, and does not retire the row.
    pub consumed_at: Option<String>,
    /// The assistant message whose generation delivered this row (the first
    /// one, for a standing row) — what makes a swipe of that message re-apply
    /// the same inform.
    pub consumed_by_message_id: Option<String>,
}

/// v4 `isInformInForce` — whether a row is still owed: a standing row always
/// is, a one-shot row until its seat's turn consumes it. **The one definition
/// every reader and deleter of "what is still owed" goes through** — nothing
/// else in this module may open-code it (pinned by
/// `the_in_force_predicate_is_never_open_coded`).
pub fn is_inform_in_force(row: &ChatInformRow) -> bool {
    row.permanent || row.consumed_at.is_none()
}

/// One batch still in force, as the composer chip reads it: the body once,
/// plus the seats still owed it — every target, for a standing batch (v4
/// `PendingInformBatch`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingInformBatch {
    pub batch_id: String,
    pub content_markdown: String,
    pub created_at: String,
    pub record_message_id: Option<String>,
    /// A standing inform — the first-seen row's flag (v4
    /// `Boolean(row.permanent)`).
    pub permanent: bool,
    pub pending_participant_ids: Vec<String>,
}

/// Render a row the way v4's repository hands it to a serializer — **null
/// optionals OMITTED, not rendered `null`**.
///
/// Measured at the target pin, and it is a whole-backend rule rather than a
/// `chat_informs` quirk: v4's SQLite deserializer converts every NULL cell to
/// `undefined` before Zod sees it
/// (`lib/database/backends/sqlite/backend.ts:477-479`, "Convert null to
/// undefined for Zod .optional() compatibility"), and `JSON.stringify` drops an
/// `undefined` property. So the export NDJSON record and the backup JSON — both
/// of which serialize rows straight off `findByChatId` — carry no
/// `recordMessageId` / `consumedAt` / `consumedByMessageId` key at all when
/// those are NULL.
///
/// Where v4 wants an explicit null it writes one, which is exactly why
/// `findPendingBatches` coalesces `row.recordMessageId ?? null`.
pub fn row_to_json(r: &ChatInformRow) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    let mut put = |k: &str, v: serde_json::Value| {
        m.insert(k.to_string(), v);
    };
    put("id", serde_json::Value::String(r.id.clone()));
    put("chatId", serde_json::Value::String(r.chat_id.clone()));
    put("batchId", serde_json::Value::String(r.batch_id.clone()));
    put(
        "participantId",
        serde_json::Value::String(r.participant_id.clone()),
    );
    put(
        "contentMarkdown",
        serde_json::Value::String(r.content_markdown.clone()),
    );
    if let Some(v) = &r.record_message_id {
        put("recordMessageId", serde_json::Value::String(v.clone()));
    }
    // NOT NULL-omitted: v4's schema defaults it, so the parsed row always
    // carries a boolean, in schema order.
    put("permanent", serde_json::Value::Bool(r.permanent));
    put("createdAt", serde_json::Value::String(r.created_at.clone()));
    put("updatedAt", serde_json::Value::String(r.updated_at.clone()));
    if let Some(v) = &r.consumed_at {
        put("consumedAt", serde_json::Value::String(v.clone()));
    }
    if let Some(v) = &r.consumed_by_message_id {
        put("consumedByMessageId", serde_json::Value::String(v.clone()));
    }
    serde_json::Value::Object(m)
}

/// Fields for creating one row with its id and timestamps supplied by the caller
/// — the import/restore shape (v4 `create(data, { id })`), and what
/// [`ChatInformsRepository::create_batch`] fills in per target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatInformCreate {
    pub id: String,
    pub chat_id: String,
    pub batch_id: String,
    pub participant_id: String,
    pub content_markdown: String,
    pub record_message_id: Option<String>,
    pub permanent: bool,
    pub created_at: String,
    pub updated_at: String,
    pub consumed_at: Option<String>,
    pub consumed_by_message_id: Option<String>,
}

const SELECT_COLUMNS: &str = "SELECT id, chatId, batchId, participantId, contentMarkdown, \
     recordMessageId, permanent, createdAt, updatedAt, consumedAt, consumedByMessageId \
     FROM chat_informs";

fn row_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChatInformRow> {
    Ok(ChatInformRow {
        id: row.get(0)?,
        chat_id: row.get(1)?,
        batch_id: row.get(2)?,
        participant_id: row.get(3)?,
        content_markdown: row.get(4)?,
        record_message_id: row.get(5)?,
        // generateDDL's shape is nullable; v4's SQLite deserializer turns a
        // NULL cell into `undefined` and Zod's `.default(false)` fills it.
        permanent: row.get::<_, Option<bool>>(6)?.unwrap_or(false),
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        consumed_at: row.get(9)?,
        consumed_by_message_id: row.get(10)?,
    })
}

/// v4 `byPostingOrder`, exactly:
///
/// ```js
/// const delta = new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime();
/// return delta !== 0 ? delta : a.id.localeCompare(b.id);
/// ```
///
/// The NaN leg is reproduced rather than papered over: an unparseable timestamp
/// makes `delta` NaN in V8, `NaN !== 0` is **true**, so the comparator returns
/// NaN — the `id` tiebreak is never reached and V8's sort reads NaN as 0
/// ("equal"). So a row pair whose timestamps do not parse compares `Equal` here,
/// and the `id` tiebreak applies only when BOTH parse. (`a-nan-comparator-is-not-
/// a-total-order`: nothing in v5 may promote this to a total order without
/// changing the observable order.)
fn by_posting_order(a: &ChatInformRow, b: &ChatInformRow) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (Some(am), Some(bm)) = (
        crate::clock::iso_to_ms(&a.created_at),
        crate::clock::iso_to_ms(&b.created_at),
    ) else {
        return Ordering::Equal;
    };
    match am.cmp(&bm) {
        Ordering::Equal => crate::collation::locale_compare(&a.id, &b.id),
        other => other,
    }
}

/// v4 `byDeliveryOrder`: standing rows first, then one-shot rows, each in
/// posting order. Standing passages are the same turn after turn, so putting
/// them ahead of the one-shots keeps the front of the block stable for
/// providers that cache by prefix.
///
/// ```js
/// if (Boolean(a.permanent) !== Boolean(b.permanent)) return a.permanent ? -1 : 1;
/// return byPostingOrder(a, b);
/// ```
fn by_delivery_order(a: &ChatInformRow, b: &ChatInformRow) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    if a.permanent != b.permanent {
        return if a.permanent {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    by_posting_order(a, b)
}

/// Repository over a borrowed connection.
pub struct ChatInformsRepository<'c> {
    conn: &'c Connection,
}

impl<'c> ChatInformsRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    // ========================================================================
    // Reads
    // ========================================================================

    /// v4 `findPendingForParticipant` — every row still in force for one seat,
    /// in delivery order: standing rows (whether or not they have been
    /// delivered before), then unconsumed one-shot rows, each oldest first.
    /// This is what the prompt path delivers.
    pub fn find_pending_for_participant(
        &self,
        chat_id: &str,
        participant_id: &str,
    ) -> Result<Vec<ChatInformRow>, DbError> {
        let sql = format!("{SELECT_COLUMNS} WHERE chatId = ?1 AND participantId = ?2");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![chat_id, participant_id], row_from)?;
        let mut out: Vec<ChatInformRow> = Vec::new();
        for r in rows {
            let r = r?;
            if is_inform_in_force(&r) {
                out.push(r);
            }
        }
        out.sort_by(by_delivery_order);
        Ok(out)
    }

    /// v4 `findConsumedByMessages` — the swipe re-apply set: rows this seat
    /// already consumed on any of `message_ids`. A swipe re-rolls the line a
    /// message was, so it must see exactly the informs that generation saw and
    /// no others. An empty list answers `[]` without touching the database
    /// (v4's own early return).
    pub fn find_consumed_by_messages(
        &self,
        chat_id: &str,
        participant_id: &str,
        message_ids: &[String],
    ) -> Result<Vec<ChatInformRow>, DbError> {
        if message_ids.is_empty() {
            return Ok(Vec::new());
        }
        let wanted: std::collections::HashSet<&str> =
            message_ids.iter().map(String::as_str).collect();
        let sql = format!("{SELECT_COLUMNS} WHERE chatId = ?1 AND participantId = ?2");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![chat_id, participant_id], row_from)?;
        let mut out: Vec<ChatInformRow> = Vec::new();
        for r in rows {
            let r = r?;
            if r.consumed_by_message_id
                .as_deref()
                .is_some_and(|m| wanted.contains(m))
            {
                out.push(r);
            }
        }
        out.sort_by(by_delivery_order);
        Ok(out)
    }

    /// v4 `findPendingBatches` — the chat's rows still in force folded back into
    /// the batches they were posted as, one entry per post carrying the seats
    /// still owed it (every target, for a standing batch). Drives the
    /// composer's pending chip. Sorted in POSTING order, not delivery order.
    ///
    /// The fold walks the rows in sorted order and keeps first-seen wins for the
    /// body/`createdAt`/`recordMessageId`, appending each later row's
    /// participant — so `pendingParticipantIds` is in the same
    /// createdAt-then-id order, and the batches come back in the order their
    /// FIRST row sorts (v4 returns `[...map.values()]`, and a JS Map iterates in
    /// insertion order — reproduced here by pushing into a `Vec` and keeping a
    /// side index, rather than by taking a new crate dependency).
    pub fn find_pending_batches(&self, chat_id: &str) -> Result<Vec<PendingInformBatch>, DbError> {
        let sql = format!("{SELECT_COLUMNS} WHERE chatId = ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![chat_id], row_from)?;
        let mut pending: Vec<ChatInformRow> = Vec::new();
        for r in rows {
            let r = r?;
            if is_inform_in_force(&r) {
                pending.push(r);
            }
        }
        pending.sort_by(by_posting_order);

        let mut batches: Vec<PendingInformBatch> = Vec::new();
        let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for row in pending {
            if let Some(&at) = seen.get(&row.batch_id) {
                batches[at].pending_participant_ids.push(row.participant_id);
                continue;
            }
            seen.insert(row.batch_id.clone(), batches.len());
            batches.push(PendingInformBatch {
                batch_id: row.batch_id,
                content_markdown: row.content_markdown,
                created_at: row.created_at,
                record_message_id: row.record_message_id,
                permanent: row.permanent,
                pending_participant_ids: vec![row.participant_id],
            });
        }
        Ok(batches)
    }

    /// v4 `findByChatId` — every row for a chat, consumed included. Export and
    /// backup read this. No sort: v4's `findByFilter` applies none, so the rows
    /// come back in rowid order (insertion order).
    pub fn find_by_chat_id(&self, chat_id: &str) -> Result<Vec<ChatInformRow>, DbError> {
        let sql = format!("{SELECT_COLUMNS} WHERE chatId = ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![chat_id], row_from)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// v4 `findByBatchId` — every row of one batch, consumed included. Unsorted
    /// for the same reason as [`Self::find_by_chat_id`].
    pub fn find_by_batch_id(&self, batch_id: &str) -> Result<Vec<ChatInformRow>, DbError> {
        let sql = format!("{SELECT_COLUMNS} WHERE batchId = ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![batch_id], row_from)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    // ========================================================================
    // Writes
    // ========================================================================

    /// One row with everything supplied — v4's `create(data, { id })`, the shape
    /// import and restore use to preserve a source id.
    pub fn create(&self, data: &ChatInformCreate) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO chat_informs \
               (id, chatId, batchId, participantId, contentMarkdown, recordMessageId, \
                permanent, createdAt, updatedAt, consumedAt, consumedByMessageId) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                data.id,
                data.chat_id,
                data.batch_id,
                data.participant_id,
                data.content_markdown,
                data.record_message_id,
                data.permanent,
                data.created_at,
                data.updated_at,
                data.consumed_at,
                data.consumed_by_message_id,
            ],
        )?;
        Ok(())
    }

    /// v4 `createBatch` — mint ONE `batchId` and one row per target, all
    /// carrying the same body, all pending. `permanent` makes it a standing
    /// inform for this chat (v4 `params.permanent === true`, written on every
    /// row). Returns the created rows in target order.
    ///
    /// The single batch id across many rows is the whole design: it is what
    /// cancel addresses and what the composer chip folds on.
    ///
    /// **The `batchId` is minted once, before the loop; the timestamps are
    /// minted per row, inside it.** That is v4's shape — `createBatch` computes
    /// `randomUUID()` once and then calls `this.create(...)` per target, and each
    /// `_create` reads its own `getCurrentTimestamp()`. Measured at the pin
    /// rather than assumed: one oracle batch came back with its two rows five
    /// milliseconds apart (`…42.573Z` / `…42.578Z`). Hoisting the clock out of
    /// the loop would be a tidier lie.
    pub fn create_batch(
        &self,
        chat_id: &str,
        content_markdown: &str,
        participant_ids: &[String],
        record_message_id: Option<&str>,
        permanent: bool,
    ) -> Result<Vec<ChatInformRow>, DbError> {
        let batch_id = uuid::Uuid::new_v4().to_string();
        let mut created = Vec::with_capacity(participant_ids.len());
        for participant_id in participant_ids {
            let now = crate::clock::now_iso();
            let row = ChatInformRow {
                id: uuid::Uuid::new_v4().to_string(),
                chat_id: chat_id.to_string(),
                batch_id: batch_id.clone(),
                participant_id: participant_id.clone(),
                content_markdown: content_markdown.to_string(),
                record_message_id: record_message_id.map(str::to_string),
                permanent,
                created_at: now.clone(),
                updated_at: now,
                consumed_at: None,
                consumed_by_message_id: None,
            };
            self.create(&ChatInformCreate {
                id: row.id.clone(),
                chat_id: row.chat_id.clone(),
                batch_id: row.batch_id.clone(),
                participant_id: row.participant_id.clone(),
                content_markdown: row.content_markdown.clone(),
                record_message_id: row.record_message_id.clone(),
                permanent,
                created_at: row.created_at.clone(),
                updated_at: row.updated_at.clone(),
                consumed_at: None,
                consumed_by_message_id: None,
            })?;
            created.push(row);
        }

        tracing::debug!(
            target: "quilltap::db",
            collection = "chat_informs",
            chat_id,
            batch_id,
            target_count = created.len(),
            record_message_id,
            permanent,
            "Inform batch created",
        );

        Ok(created)
    }

    /// v4 `markConsumed` — mark exactly these rows consumed by `message_id`,
    /// returning how many rows moved. On a standing row this stamps its first
    /// delivery and leaves it in force; `build_inform_block` only hands over
    /// rows not yet stamped, so a later turn never moves the stamp.
    ///
    /// Called once a generation has produced a **persisted** assistant message —
    /// never from context building, so a provider failure that saves nothing
    /// leaves the rows pending for the seat's next attempt. An empty list is a
    /// no-op answering 0 (v4's early return, before the timestamp is even read).
    pub fn mark_consumed(&self, ids: &[String], message_id: &str) -> Result<usize, DbError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let now = crate::clock::now_iso();
        let mut count = 0usize;
        for id in ids {
            // v4's `_update` finds the row first and answers null when absent;
            // the row count is the same signal.
            let changed = self.conn.execute(
                "UPDATE chat_informs SET consumedAt = ?1, consumedByMessageId = ?2, \
                   updatedAt = ?3 WHERE id = ?4",
                params![now, message_id, now, id],
            )?;
            if changed > 0 {
                count += 1;
            }
        }
        tracing::debug!(
            target: "quilltap::db",
            collection = "chat_informs",
            ids = ?ids,
            message_id,
            count,
            "Informs marked consumed",
        );
        Ok(count)
    }

    /// v4 `deletePendingByBatch` — cancel: drop every row still in force — the
    /// one-shot rows nobody has had yet, and every row of a standing batch
    /// (withdrawing it is the only way it ends). A seat that already consumed a
    /// one-shot passage keeps its row, so a later swipe of that turn still
    /// re-applies it.
    pub fn delete_pending_by_batch(&self, batch_id: &str) -> Result<usize, DbError> {
        let rows = self.find_by_batch_id(batch_id)?;
        let mut count = 0usize;
        for row in rows {
            if !is_inform_in_force(&row) {
                continue;
            }
            count += self
                .conn
                .execute("DELETE FROM chat_informs WHERE id = ?1", params![row.id])?;
        }
        tracing::debug!(
            target: "quilltap::db",
            collection = "chat_informs",
            batch_id,
            count,
            "Pending informs deleted by batch",
        );
        Ok(count)
    }

    /// v4 `deletePendingForParticipant` — a seat has left the chat: it can never
    /// collect what it was owed, standing or not. No code of its own moved at
    /// `52d6e7ecd`: it reads through [`Self::find_pending_for_participant`],
    /// so it inherits the in-force predicate and now also removes the seat's
    /// DELIVERED standing rows.
    pub fn delete_pending_for_participant(
        &self,
        chat_id: &str,
        participant_id: &str,
    ) -> Result<usize, DbError> {
        let pending = self.find_pending_for_participant(chat_id, participant_id)?;
        let mut count = 0usize;
        for row in pending {
            count += self
                .conn
                .execute("DELETE FROM chat_informs WHERE id = ?1", params![row.id])?;
        }
        tracing::debug!(
            target: "quilltap::db",
            collection = "chat_informs",
            chat_id,
            participant_id,
            count,
            "Pending informs deleted for participant",
        );
        Ok(count)
    }

    /// v4 `deleteByChatId` — every row for a chat, consumed included.
    ///
    /// On v4 this mirrors a cascade the FK already performs. **On v5 it IS the
    /// cascade**: the generateDDL surface v5 follows carries no foreign key (see
    /// the module header), so the chat-delete path must call this or the rows
    /// outlive their chat.
    pub fn delete_by_chat_id(&self, chat_id: &str) -> Result<usize, DbError> {
        let rows = self.find_by_chat_id(chat_id)?;
        let mut count = 0usize;
        for row in rows {
            count += self
                .conn
                .execute("DELETE FROM chat_informs WHERE id = ?1", params![row.id])?;
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        ensure_chat_informs_table(&c).unwrap();
        c
    }

    fn plant(c: &Connection, id: &str, batch: &str, seat: &str, at: &str, permanent: bool) {
        ChatInformsRepository::new(c)
            .create(&ChatInformCreate {
                id: id.into(),
                chat_id: "c1".into(),
                batch_id: batch.into(),
                participant_id: seat.into(),
                content_markdown: format!("body {id}"),
                record_message_id: None,
                permanent,
                created_at: at.into(),
                updated_at: at.into(),
                consumed_at: None,
                consumed_by_message_id: None,
            })
            .unwrap();
    }

    fn ids(rows: &[ChatInformRow]) -> Vec<&str> {
        rows.iter().map(|r| r.id.as_str()).collect()
    }

    /// The table DDL is byte-identical to the D23 dump's two `chat_informs`
    /// statements once SQLite's own `IF NOT EXISTS` strip is applied — which is
    /// exactly what `sqlite_master` stores for a fresh-provisioned table.
    #[test]
    fn the_table_ddl_matches_the_d23_dump() {
        let dump: serde_json::Value =
            serde_json::from_str(include_str!("../services/provisioning/fresh_schema.json"))
                .unwrap();
        let main: Vec<&str> = dump["main"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .filter(|s| s.contains("\"chat_informs\""))
            .collect();
        let ours: Vec<String> = CHAT_INFORMS_TABLE_DDL
            .replace(" IF NOT EXISTS", "")
            .split(";\n")
            .map(str::to_string)
            .collect();
        assert_eq!(main, ours);
    }

    /// Item 19's census: the in-force predicate lives in ONE function. A read
    /// or delete that open-codes `consumed_at.is_none()` / `.is_some()` is the
    /// drift class `52d6e7ecd` closed in v4.
    #[test]
    fn the_in_force_predicate_is_never_open_coded() {
        let src = include_str!("chat_informs.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap();
        let needle_none = concat!("consumed_at", ".is_none()");
        let needle_some = concat!("consumed_at", ".is_some()");
        assert_eq!(
            prod.matches(needle_none).count(),
            1,
            "only is_inform_in_force"
        );
        assert_eq!(prod.matches(needle_some).count(), 0);
        let home = prod.find("pub fn is_inform_in_force").unwrap();
        let at = prod.find(needle_none).unwrap();
        assert!(
            at > home && at < home + 120,
            "the one use is inside the predicate"
        );
    }

    #[test]
    fn the_seat_read_puts_standing_first_and_keeps_a_delivered_standing_row() {
        let c = conn();
        plant(&c, "a", "b1", "p1", "2026-01-01T00:00:00.000Z", false);
        plant(&c, "b", "b2", "p1", "2026-01-01T00:00:01.000Z", true);
        plant(&c, "c", "b3", "p1", "2026-01-01T00:00:02.000Z", true);
        let repo = ChatInformsRepository::new(&c);
        repo.mark_consumed(&["c".into()], "m1").unwrap();
        repo.mark_consumed(&["a".into()], "m1").unwrap();
        // `a` (one-shot, consumed) drops; `c` (standing, delivered) stays.
        assert_eq!(
            ids(&repo.find_pending_for_participant("c1", "p1").unwrap()),
            ["b", "c"]
        );
        // The consumed-by read sorts by delivery order too.
        assert_eq!(
            ids(&repo
                .find_consumed_by_messages("c1", "p1", &["m1".into()])
                .unwrap()),
            ["c", "a"]
        );
    }

    #[test]
    fn the_batch_read_is_posting_order_and_carries_the_flag() {
        let c = conn();
        plant(&c, "a", "b1", "p1", "2026-01-01T00:00:00.000Z", false);
        plant(&c, "b", "b2", "p1", "2026-01-01T00:00:01.000Z", true);
        let repo = ChatInformsRepository::new(&c);
        repo.mark_consumed(&["b".into()], "m1").unwrap();
        let batches = repo.find_pending_batches("c1").unwrap();
        let got: Vec<(&str, bool)> = batches
            .iter()
            .map(|b| (b.batch_id.as_str(), b.permanent))
            .collect();
        assert_eq!(got, [("b1", false), ("b2", true)]);
    }

    #[test]
    fn cancel_withdraws_a_standing_batch_whole_and_spares_a_consumed_one_shot() {
        let c = conn();
        plant(&c, "s1", "bs", "p1", "2026-01-01T00:00:00.000Z", true);
        plant(&c, "s2", "bs", "p2", "2026-01-01T00:00:00.000Z", true);
        plant(&c, "o1", "bo", "p1", "2026-01-01T00:00:00.000Z", false);
        plant(&c, "o2", "bo", "p2", "2026-01-01T00:00:00.000Z", false);
        let repo = ChatInformsRepository::new(&c);
        repo.mark_consumed(&["s1".into(), "o1".into()], "m1")
            .unwrap();
        assert_eq!(repo.delete_pending_by_batch("bs").unwrap(), 2);
        assert_eq!(repo.delete_pending_by_batch("bo").unwrap(), 1);
        assert_eq!(ids(&repo.find_by_chat_id("c1").unwrap()), ["o1"]);
    }

    /// §R.4(e): no code hunk, but seat removal now also takes the seat's
    /// DELIVERED standing row.
    #[test]
    fn seat_removal_takes_a_delivered_standing_row() {
        let c = conn();
        plant(&c, "s1", "bs", "p1", "2026-01-01T00:00:00.000Z", true);
        plant(&c, "o1", "bo", "p1", "2026-01-01T00:00:00.000Z", false);
        let repo = ChatInformsRepository::new(&c);
        repo.mark_consumed(&["s1".into(), "o1".into()], "m1")
            .unwrap();
        assert_eq!(repo.delete_pending_for_participant("c1", "p1").unwrap(), 1);
        assert_eq!(ids(&repo.find_by_chat_id("c1").unwrap()), ["o1"]);
    }

    #[test]
    fn create_batch_writes_the_flag_on_every_row() {
        let c = conn();
        let repo = ChatInformsRepository::new(&c);
        let seats = vec!["p1".to_string(), "p2".to_string()];
        repo.create_batch("c1", "x", &seats, None, true).unwrap();
        repo.create_batch("c1", "y", &seats, None, false).unwrap();
        let flags: Vec<bool> = repo
            .find_by_chat_id("c1")
            .unwrap()
            .iter()
            .map(|r| r.permanent)
            .collect();
        assert_eq!(flags, [true, true, false, false]);
    }

    /// A NULL cell (permitted by generateDDL's nullable shape) reads `false`,
    /// as v4's Zod default makes of it; `row_to_json` emits the flag in schema
    /// order, never omitted.
    #[test]
    fn a_null_flag_reads_false_and_the_json_carries_it_in_schema_order() {
        let c = conn();
        c.execute(
            "INSERT INTO chat_informs (id, chatId, batchId, participantId, contentMarkdown, \
               permanent, createdAt, updatedAt) VALUES ('n', 'c1', 'b', 'p', 'x', NULL, 't', 't')",
            [],
        )
        .unwrap();
        let row = ChatInformsRepository::new(&c)
            .find_by_chat_id("c1")
            .unwrap()
            .remove(0);
        assert!(!row.permanent);
        let keys: Vec<String> = row_to_json(&row)
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(
            keys,
            [
                "id",
                "chatId",
                "batchId",
                "participantId",
                "contentMarkdown",
                "permanent",
                "createdAt",
                "updatedAt"
            ]
        );
    }
}
