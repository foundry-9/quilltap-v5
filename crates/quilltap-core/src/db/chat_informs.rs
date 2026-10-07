//! The chat-informs repository — P4.D205, porting v4 `e7d77bb60`'s
//! `lib/database/repositories/chat-informs.repository.ts` (311 lines) and the
//! `chat_informs` table behind the Salon's **Inform** action.
//!
//! An *inform* is an out-of-character passage the operator hands to one or more
//! LLM-controlled seats: delivered verbatim, under one vouching header, as a
//! trailing context section of each target's next generation (v4 `94fbb1ae3`,
//! P4.D254), and consumed once that turn produces a *persisted*
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
/// else in this module — or anywhere in the production tree — may open-code
/// it (pinned by the harness's `chat_informs_in_force_census`, which reads
/// the whole production zone on the shared lexer, SQL literals included).
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
    //
    // P4.156: every read is v4's TWO-layer fallback (`chat-informs.repository.
    // ts:87-183`) — the method's own `safeQuery(…, [])` around the base
    // `findByFilter`, itself a fallback. Outside the strict scope a failed SELECT
    // logs the INNER `Error finding entities by filter` and answers `Ok([])`
    // (the outer line is unreachable); inside it both lines log
    // `strictFailures=true` and the `Err` propagates. The signatures still
    // answer `Result`, because the strict callers (the `.qtap` export, the
    // importer, the restore) must see the failure.

    /// The base `findByFilter` over one `WHERE` — the raw SELECT, through the
    /// strict-aware inner home.
    fn select_where(
        &self,
        clause: &str,
        args: &[&dyn rusqlite::ToSql],
    ) -> Result<Vec<ChatInformRow>, DbError> {
        super::fallback::find_by_filter_strict_aware("chat_informs", || {
            let sql = format!("{SELECT_COLUMNS} WHERE {clause}");
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt.query_map(args, row_from)?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
    }

    /// v4 `findPendingForParticipant` — every row still in force for one seat,
    /// in delivery order: standing rows (whether or not they have been
    /// delivered before), then unconsumed one-shot rows, each oldest first.
    /// This is what the prompt path delivers.
    pub fn find_pending_for_participant(
        &self,
        chat_id: &str,
        participant_id: &str,
    ) -> Result<Vec<ChatInformRow>, DbError> {
        super::fallback::pending_informs_for_participant_or_empty(chat_id, participant_id, || {
            let mut out: Vec<ChatInformRow> = self
                .select_where(
                    "chatId = ?1 AND participantId = ?2",
                    params![chat_id, participant_id],
                )?
                .into_iter()
                .filter(is_inform_in_force)
                .collect();
            out.sort_by(by_delivery_order);
            Ok(out)
        })
    }

    /// v4 `findConsumedByMessages` — the swipe re-apply set: rows this seat
    /// already consumed on any of `message_ids`. A swipe re-rolls the line a
    /// message was, so it must see exactly the informs that generation saw and
    /// no others. An empty list answers `[]` without touching the database
    /// (v4's own early return, BEFORE its `safeQuery`).
    pub fn find_consumed_by_messages(
        &self,
        chat_id: &str,
        participant_id: &str,
        message_ids: &[String],
    ) -> Result<Vec<ChatInformRow>, DbError> {
        if message_ids.is_empty() {
            return Ok(Vec::new());
        }
        super::fallback::informs_consumed_by_messages_or_empty(
            chat_id,
            participant_id,
            message_ids.len(),
            || {
                let wanted: std::collections::HashSet<&str> =
                    message_ids.iter().map(String::as_str).collect();
                let mut out: Vec<ChatInformRow> = self
                    .select_where(
                        "chatId = ?1 AND participantId = ?2",
                        params![chat_id, participant_id],
                    )?
                    .into_iter()
                    .filter(|r| {
                        r.consumed_by_message_id
                            .as_deref()
                            .is_some_and(|m| wanted.contains(m))
                    })
                    .collect();
                out.sort_by(by_delivery_order);
                Ok(out)
            },
        )
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
        super::fallback::pending_inform_batches_or_empty(chat_id, || {
            let mut pending: Vec<ChatInformRow> = self
                .select_where("chatId = ?1", params![chat_id])?
                .into_iter()
                .filter(is_inform_in_force)
                .collect();
            pending.sort_by(by_posting_order);

            let mut batches: Vec<PendingInformBatch> = Vec::new();
            let mut seen: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
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
        })
    }

    /// v4 `findByChatId` — every row for a chat, consumed included. Export and
    /// backup read this. No sort: v4's `findByFilter` applies none, so the rows
    /// come back in rowid order (insertion order).
    pub fn find_by_chat_id(&self, chat_id: &str) -> Result<Vec<ChatInformRow>, DbError> {
        super::fallback::informs_by_chat_id_or_empty(chat_id, || {
            self.select_where("chatId = ?1", params![chat_id])
        })
    }

    /// v4 `findByBatchId` — every row of one batch, consumed included. Unsorted
    /// for the same reason as [`Self::find_by_chat_id`].
    pub fn find_by_batch_id(&self, batch_id: &str) -> Result<Vec<ChatInformRow>, DbError> {
        super::fallback::informs_by_batch_id_or_empty(batch_id, || {
            self.select_where("batchId = ?1", params![batch_id])
        })
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
            // P4.156: `createBatch` calls v4's pass-through `create` → `_create`,
            // a RETHROW `safeQuery`: the failed row logs the base `Error creating
            // entity` (C2's bytes, `strictFailures` inside the scope) and the
            // batch THROWS — `createBatch` has no wrap of its own. The public
            // [`Self::create`] stays silent: its import / restore callers log
            // the base line themselves (the restore's arm; the importer's is a
            // recorded handoff).
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
            })
            .inspect_err(|e| super::fallback::log_create_failure("chat_informs", e))?;
            created.push(row);
        }

        tracing::debug!(
            target: "quilltap::db",
            collection = "chat_informs",
            chatId = chat_id,
            batchId = batch_id,
            targetCount = created.len(),
            recordMessageId = record_message_id,
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
        // P4.156: v4's 4-argument FALLBACK (`:238-268`) around a loop of
        // `update` → `_update`, measured at `94fbb1ae3`: a failed UPDATE logs
        // the base `Error updating entity` and RETHROWS, so the first failed row
        // ends the loop and the wrap logs `Error marking informs consumed` and
        // answers `Ok(0)` (strict-aware, through the home). `_update` reads the
        // row FIRST through the fallback `findById`: a failed read logs `Error
        // finding entity by ID` and answers `null` → not counted, the loop goes
        // on (its `Entity not found for update` WARN is unported — P4.149's
        // Ruling R-A).
        //
        // P4.163 (R-D): that per-row read is v4's `_findById`, a fallback
        // `safeQuery` that honours the strict scope — measured at `94fbb1ae3`
        // (`chat_informs_tier2`, the strict per-row op): inside it the read's
        // line gains `strictFailures: true` and RETHROWS into `_update`'s
        // rethrow line, then this wrap's, all strict, and the consume throws.
        // `_findById` also Zod-validates the row it found (`Data validation
        // failed` → the same entity line → `null`); v5 reads the id only and
        // does NOT validate here — no `ChatInformSchema` twin exists on this
        // branch (P4.161 builds `zod_chat_inform_issues` this round; the
        // validating arm is a recorded HANDOFF to the unifier).
        super::fallback::informs_marked_consumed_or_zero(ids, message_id, || {
            let now = crate::clock::now_iso();
            let mut count = 0usize;
            for id in ids {
                let found = super::fallback::find_by_id_strict_aware("chat_informs", id, || {
                    self.conn
                        .query_row(
                            "SELECT id FROM chat_informs WHERE id = ?1",
                            params![id],
                            |r| r.get::<_, String>(0),
                        )
                        .map(Some)
                        .or_else(|e| match e {
                            rusqlite::Error::QueryReturnedNoRows => Ok(None),
                            other => Err(other.into()),
                        })
                })
                .inspect_err(|error| {
                    super::fallback::log_update_failure("chat_informs", id, error);
                })?;
                if found.is_none() {
                    continue;
                }
                let changed = self
                    .conn
                    .execute(
                        "UPDATE chat_informs SET consumedAt = ?1, consumedByMessageId = ?2, \
                           updatedAt = ?3 WHERE id = ?4",
                        params![now, message_id, now, id],
                    )
                    .map_err(|e| {
                        let error = DbError::from(e);
                        super::fallback::log_update_failure("chat_informs", id, &error);
                        error
                    })?;
                if changed > 0 {
                    count += 1;
                }
            }
            // v4 `logger.debug('Informs marked consumed', {collection, ids,
            // messageId, count})` (`:251-256`): `ids` is an ARRAY, so it rides
            // the file layer's `…Json` convention (P4.163 — it had been a Rust
            // `Debug` rendering under the bare key).
            let ids_json = serde_json::to_string(ids).unwrap_or_else(|_| "[]".to_string());
            tracing::debug!(
                target: "quilltap::db",
                collection = "chat_informs",
                idsJson = ids_json.as_str(),
                messageId = message_id,
                count,
                "Informs marked consumed",
            );
            Ok(count)
        })
    }

    /// v4 `deletePendingByBatch` — cancel: drop every row still in force — the
    /// one-shot rows nobody has had yet, and every row of a standing batch
    /// (withdrawing it is the only way it ends). A seat that already consumed a
    /// one-shot passage keeps its row, so a later swipe of that turn still
    /// re-applies it.
    ///
    /// P4.149: v4's 4-argument FALLBACK `safeQuery` — a failed row delete logs
    /// v4's lines (the base `Error deleting entity`, then the wrap's own) and
    /// answers `Ok(0)` through
    /// [`super::fallback::pending_informs_by_batch_deleted_or_zero`]; it
    /// propagates only inside the strict scope. A failed READ never reaches
    /// the wrap: v4's `findByBatchId` sits on the base `findByFilter` FALLBACK
    /// (`chat-informs.repository.ts:176-183` → `base.repository.ts:283-298`),
    /// which logs `Error finding entities by filter` and answers `[]`, so the
    /// loop runs zero times and the DEBUG fires with `count: 0` (the
    /// `07b8f0209` follow-ups unification — the lane had let the read propagate
    /// into the wrap's line).
    pub fn delete_pending_by_batch(&self, batch_id: &str) -> Result<usize, DbError> {
        super::fallback::pending_informs_by_batch_deleted_or_zero(batch_id, || {
            // P4.156: the read is v4's own fallback method — `Ok([])` with the
            // filter line outside the strict scope, its two strict lines and
            // the `Err` inside it (then this wrap's line).
            let rows = self.find_by_batch_id(batch_id)?;
            let mut count = 0usize;
            for row in rows {
                if !is_inform_in_force(&row) {
                    continue;
                }
                if self.delete_one(&row.id)? {
                    count += 1;
                }
            }
            tracing::debug!(
                target: "quilltap::db",
                collection = "chat_informs",
                batchId = batch_id,
                count,
                "Pending informs deleted by batch",
            );
            Ok(count)
        })
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
        // P4.149: v4's 4-argument FALLBACK wrap, as `delete_pending_by_batch`.
        super::fallback::pending_informs_for_participant_deleted_or_zero(
            chat_id,
            participant_id,
            || {
                // The read is v4's fallback (`:87-100` over `findByFilter`), as
                // `delete_pending_by_batch`'s above.
                let pending = self.find_pending_for_participant(chat_id, participant_id)?;
                let mut count = 0usize;
                for row in pending {
                    if self.delete_one(&row.id)? {
                        count += 1;
                    }
                }
                tracing::debug!(
                    target: "quilltap::db",
                    collection = "chat_informs",
                    chatId = chat_id,
                    participantId = participant_id,
                    count,
                    "Pending informs deleted for participant",
                );
                Ok(count)
            },
        )
    }

    /// v4 `deleteByChatId` — every row for a chat, consumed included.
    ///
    /// On v4 this mirrors a cascade the FK already performs. **On v5 it IS the
    /// cascade**: the generateDDL surface v5 follows carries no foreign key (see
    /// the module header), so the chat-delete path must call this or the rows
    /// outlive their chat.
    ///
    /// P4.149: v4 wraps this in a fallback too (`Error deleting informs by chat
    /// ID` → `0`), but NO v4 production code calls it (the FK cascades), so it
    /// keeps PROPAGATING: on v5 it is the cascade, and its one caller
    /// (`chats.rs`) logs its own v5-only WARN. Its row deletes still log v4's
    /// base line.
    pub fn delete_by_chat_id(&self, chat_id: &str) -> Result<usize, DbError> {
        // P4.156: v4's `findByChatId` is a fallback — a failed read answers `[]`
        // with the filter line, so the cascade deletes nothing and answers 0
        // (measured); only a strict-scope caller sees the `Err`.
        let rows = self.find_by_chat_id(chat_id)?;
        let mut count = 0usize;
        for row in rows {
            if self.delete_one(&row.id)? {
                count += 1;
            }
        }
        Ok(count)
    }

    /// v4 `delete` — a pass-through `_delete` (`chat-informs.repository.ts:72`):
    /// a failed DELETE logs the base rethrow line ([`super::fallback::
    /// log_delete_failure`]) and propagates; a miss answers `false` (v4's `Entity
    /// not found for deletion` WARN and the `Entity deleted` INFO are not ported
    /// — Ruling R-A).
    fn delete_one(&self, id: &str) -> Result<bool, DbError> {
        self.conn
            .execute("DELETE FROM chat_informs WHERE id = ?1", params![id])
            .map(|n| n > 0)
            .map_err(|e| {
                let error = DbError::from(e);
                super::fallback::log_delete_failure("chat_informs", id, &error);
                error
            })
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

    /// Install `global_capture`'s process-global subscriber once per binary
    /// (the `db/memories.rs` idiom): sibling tests hit the same `debug!`
    /// callsites with NO subscriber on their thread, and tracing's `Interest`
    /// cache can leave a callsite `never` for the thread that IS capturing —
    /// the first run of this pin captured the line, the second captured
    /// nothing (memory: a-second-global-default-silences-the-first). One
    /// permanently-live `always` dispatcher keeps every callsite interesting.
    fn arm_global_callsites() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            tokio::runtime::Builder::new_current_thread()
                .build()
                .expect("a current-thread runtime to arm the global capture layer")
                .block_on(crate::test_support::global_capture::capture_events(async {}));
        });
    }

    /// Item 9's capture pin (the P4.D249 lane recorded it without writing it):
    /// v4's `Inform batch created` debug carries `permanent` LAST, after
    /// `recordMessageId`; and item 18's `Pending informs deleted by batch`
    /// carries v4's `{collection, batchId, count}`. The keys are v4's
    /// camelCase (`chat-informs.repository.ts:218-307`) since the `07b8f0209`
    /// follow-ups unification — the four DEBUG lines had carried snake_case,
    /// and this pin had frozen it.
    #[test]
    fn the_two_repository_debug_lines_carry_v4s_fields_in_order() {
        arm_global_callsites();
        let c = conn();
        let repo = ChatInformsRepository::new(&c);
        let seats = vec!["p1".to_string(), "p2".to_string()];
        // A `Some` record id: tracing records an `Option` field only when it is
        // `Some`, so with `None` the line simply lacks `recordMessageId` where
        // v4 logs `recordMessageId: null` — a pre-existing (P4.D205) shape on
        // every v5 line that carries an `Option`, recorded, not this pin's
        // subject. The ORDER is what `52d6e7ecd` moved (`permanent` LAST).
        let (rows, lines) = crate::test_support::captured_with(|| {
            repo.create_batch("c1", "x", &seats, Some("m-rec"), true)
        });
        let rows = rows.unwrap();
        let created = lines
            .iter()
            .find(|l| l.contains("Inform batch created"))
            .unwrap_or_else(|| panic!("{lines:#?}"));
        assert!(created.starts_with("DEBUG quilltap::db"), "{created}");
        let keys: Vec<&str> = created
            .split_whitespace()
            .filter_map(|tok| tok.split_once('=').map(|(k, _)| k))
            .collect();
        assert_eq!(
            keys,
            [
                "collection",
                "chatId",
                "batchId",
                "targetCount",
                "recordMessageId",
                "permanent"
            ],
            "{created}"
        );
        assert!(created.contains("targetCount=2"), "{created}");
        assert!(created.contains("permanent=true"), "{created}");

        let batch = rows[0].batch_id.clone();
        let (count, lines) =
            crate::test_support::captured_with(|| repo.delete_pending_by_batch(&batch));
        assert_eq!(count.unwrap(), 2);
        let deleted = lines
            .iter()
            .find(|l| l.contains("Pending informs deleted by batch"))
            .unwrap_or_else(|| panic!("{lines:#?}"));
        assert!(deleted.starts_with("DEBUG quilltap::db"), "{deleted}");
        let keys: Vec<&str> = deleted
            .split_whitespace()
            .filter_map(|tok| tok.split_once('=').map(|(k, _)| k))
            .collect();
        assert_eq!(keys, ["collection", "batchId", "count"], "{deleted}");
        assert!(deleted.contains("count=2"), "{deleted}");
    }

    /// A failed READ inside the bulk deletes takes v4's base `findByFilter`
    /// fallback: the filter line, an empty loop, the DEBUG with `count=0`, and
    /// NOT the wrap's `Error deleting pending informs …` line (measured against
    /// `chat-informs.repository.ts:87-100,176-183` + `base.repository.ts:283-298`
    /// at the `07b8f0209` follow-ups unification).
    #[test]
    fn a_failed_read_inside_the_bulk_deletes_takes_the_filter_fallback_not_the_wrap() {
        arm_global_callsites();
        let c = conn();
        c.execute_batch("ALTER TABLE chat_informs RENAME COLUMN batchId TO batchId_x")
            .unwrap();
        let repo = ChatInformsRepository::new(&c);
        let (by_batch, lines) =
            crate::test_support::captured_with(|| repo.delete_pending_by_batch("b1"));
        assert_eq!(by_batch.unwrap(), 0);
        let filter = lines
            .iter()
            .find(|l| l.contains("Error finding entities by filter"))
            .unwrap_or_else(|| panic!("{lines:#?}"));
        assert!(
            filter.starts_with("ERROR quilltap::db Error finding entities by filter collection=chat_informs error=no such column: batchId"),
            "{filter}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains("Pending informs deleted by batch") && l.contains("count=0")),
            "{lines:#?}"
        );
        assert!(
            lines
                .iter()
                .all(|l| !l.contains("Error deleting pending informs")),
            "the wrap's line is v4-unreachable on a read failure: {lines:#?}"
        );
        let (for_seat, lines) =
            crate::test_support::captured_with(|| repo.delete_pending_for_participant("c1", "p1"));
        assert_eq!(for_seat.unwrap(), 0);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("Error finding entities by filter"))
                && lines
                    .iter()
                    .any(|l| l.contains("Pending informs deleted for participant")
                        && l.contains("count=0"))
                && lines
                    .iter()
                    .all(|l| !l.contains("Error deleting pending informs")),
            "{lines:#?}"
        );
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
