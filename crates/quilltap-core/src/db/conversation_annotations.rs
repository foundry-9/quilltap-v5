//! The conversation-annotations repository — a Phase-2 repo port, after
//! `folders`, `tags`, `text_replacement_rules`, and `prompt_templates`. Ports
//! v4's `lib/database/repositories/conversation-annotations.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, `delete` (the three abstract methods over the base
//! repo), and `upsert` (the custom method that find-by-unique-key then routes to
//! the update or create path — ported in the minted-values/remap tier-2 form,
//! since it mints its own id + timestamps). The remaining custom query helpers —
//! `findByChatId`, `findByMessageIndex`, `deleteAnnotation`, `deleteAllForChat` —
//! are out of scope here. Single source: `ConversationAnnotationSchema` from
//! `scriptorium.types`, used by Project Scriptorium's conversation rendering.
//!
//! ## What this repo banks for the tier-2 marshaling surface
//!
//!   - a **REAL-affinity unbounded-int column** (`messageIndex`). The Zod field
//!     is `z.number().int().min(0)` with **no `.max()`** — and v4's
//!     schema-translator (`mapToSQLiteType`) only assigns INTEGER affinity when a
//!     numeric field has BOTH an integer min AND an integer max. With only
//!     `.min(0)`, `messageIndex` maps to **REAL**. So this port binds it as an
//!     `f64`, not an `i64`. The canonical dump's `js_number_to_json` collapses an
//!     integer-valued REAL cell (`3.0`) back to `3` — matching how the v4 oracle
//!     (better-sqlite3 → JS `Number` → `JSON.stringify`) renders it — so an
//!     integer message index round-trips byte-for-byte.
//!   - a **nullable UUID column** (`sourceMessageId`, `UUIDSchema.nullable()
//!     .optional()`). `None` → SQL NULL, `Some` → the UUID text. The seed/create
//!     corpus exercises both a null and a non-null value.
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in the update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form
//! `folders`/`tags`/`text_replacement_rules`/`prompt_templates` use.
//!
//! Deferred (not in the corpus, mirroring `prompt_templates`): clearing
//! `sourceMessageId` back to NULL via `update` — the update patch models a
//! provided field as "set to this value", so to avoid an `Option<Option<_>>`
//! setter it carries only `content`/`characterName`/`messageIndex`; a nullable
//! setter lands when an op needs it.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;
use crate::clock;

/// Fields for creating an annotation (the `Omit<ConversationAnnotation,'id'|
/// timestamps>` shape). `message_index` is the REAL-affinity column (bound as
/// `f64`); `source_message_id` is the nullable UUID column (`None` → SQL NULL).
pub struct CaCreate {
    pub chat_id: String,
    pub message_index: f64,
    pub source_message_id: Option<String>,
    pub character_name: String,
    pub content: String,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// An annotation update patch. Mirrors v4 `update` over `_update`: provided
/// fields overwrite, id and createdAt are preserved, `updatedAt` is set
/// explicitly. `source_message_id` is the nullable-setter case the `upsert`
/// update path needs (v4's `_update({content, sourceMessageId})` sets the column
/// to whatever the input carries, including `null`): `None` → leave the column
/// untouched, `Some(inner)` → set it to `inner` (`Some(None)` writes SQL NULL).
#[derive(Default)]
pub struct CaUpdate {
    pub content: Option<String>,
    pub character_name: Option<String>,
    pub message_index: Option<f64>,
    /// Outer `Option`: present in the patch or not. Inner `Option`: the column
    /// value (`None` → SQL NULL). See the struct doc.
    pub source_message_id: Option<Option<String>>,
    pub updated_at: String,
}

/// Input for [`ConversationAnnotationsRepository::upsert`] — mirrors v4's
/// `ConversationAnnotationInput` (the four unique-key/payload fields, no id or
/// timestamps; the upsert mints those). `source_message_id` is the nullable UUID
/// (`None` → SQL NULL on both the create and the update path).
pub struct CaUpsertInput {
    pub chat_id: String,
    pub message_index: f64,
    pub source_message_id: Option<String>,
    pub character_name: String,
    pub content: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct ConversationAnnotationsRepository<'c> {
    conn: &'c Connection,
}

impl<'c> ConversationAnnotationsRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert an annotation with the given pinned id + timestamps. `messageIndex`
    /// is bound as `f64` (REAL affinity); `sourceMessageId` as `Option<String>`
    /// (`None` → SQL NULL).
    pub fn create(&self, data: &CaCreate, opts: &CreateOptions) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO conversation_annotations \
               (id, chatId, messageIndex, sourceMessageId, characterName, content, \
                createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                opts.id,
                data.chat_id,
                data.message_index,
                data.source_message_id,
                data.character_name,
                data.content,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the annotation `id`. Returns `Ok(false)` when no
    /// row matched (v4's "not found -> null"). id and createdAt are never
    /// touched. Each `Some` field sets that column; `updatedAt` is always set.
    pub fn update(&self, id: &str, patch: &CaUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`s — the row must exist or it's a no-op.
        let exists: bool = self
            .conn
            .query_row(
                "SELECT 1 FROM conversation_annotations WHERE id = ?1",
                params![id],
                |_| Ok(()),
            )
            .map(|_| true)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(false),
                other => Err(other),
            })?;
        if !exists {
            return Ok(false);
        }

        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(content) = &patch.content {
            assignments.push(format!("content = ?{}", values.len() + 1));
            values.push(Box::new(content.clone()));
        }
        if let Some(character_name) = &patch.character_name {
            assignments.push(format!("characterName = ?{}", values.len() + 1));
            values.push(Box::new(character_name.clone()));
        }
        if let Some(message_index) = patch.message_index {
            assignments.push(format!("messageIndex = ?{}", values.len() + 1));
            values.push(Box::new(message_index));
        }
        if let Some(source_message_id) = &patch.source_message_id {
            assignments.push(format!("sourceMessageId = ?{}", values.len() + 1));
            // Inner `Option<String>` binds the value (or SQL NULL).
            values.push(Box::new(source_message_id.clone()));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE conversation_annotations SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the annotation `id`. Returns `false` when no row matched (v4's
    /// `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self.conn.execute(
            "DELETE FROM conversation_annotations WHERE id = ?1",
            params![id],
        )?;
        Ok(affected > 0)
    }

    /// Insert or update an annotation (v4's `upsert`). Uses the unique key
    /// `(chatId, messageIndex, characterName)`: if one existing row matches it is
    /// updated (ONLY `content` + `sourceMessageId`, with `updatedAt` re-minted and
    /// id/createdAt preserved — v4's `_update({content, sourceMessageId})`);
    /// otherwise a new row is created (id + createdAt + updatedAt all minted to the
    /// SAME `now` — v4's `_create(input)`). The id used (existing or minted) is
    /// returned. Mints exactly as v4: `id` a v4-shape UUID, timestamps from the
    /// wall clock via [`crate::clock::now_iso`].
    pub fn upsert(&self, input: &CaUpsertInput) -> Result<String, DbError> {
        // Find ONE existing row by the unique key. `messageIndex` is bound as the
        // same REAL/f64 the column holds, so an integer-valued index matches.
        let existing: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM conversation_annotations \
                 WHERE chatId = ?1 AND messageIndex = ?2 AND characterName = ?3",
                params![input.chat_id, input.message_index, input.character_name],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;

        if let Some(id) = existing {
            // UPDATE path: v4 `_update(existing.id, {content, sourceMessageId})`.
            // Only these two columns change; `updatedAt` is re-minted (not in the
            // patch), id + createdAt preserved. `sourceMessageId` is set to the
            // input value, including NULL when the input carries none.
            let now = clock::now_iso();
            self.update(
                &id,
                &CaUpdate {
                    content: Some(input.content.clone()),
                    source_message_id: Some(input.source_message_id.clone()),
                    updated_at: now,
                    ..Default::default()
                },
            )?;
            Ok(id)
        } else {
            // CREATE path: v4 `_create(input)` — id + createdAt + updatedAt all
            // minted to the same `now`.
            let id = uuid::Uuid::new_v4().to_string();
            let now = clock::now_iso();
            self.create(
                &CaCreate {
                    chat_id: input.chat_id.clone(),
                    message_index: input.message_index,
                    source_message_id: input.source_message_id.clone(),
                    character_name: input.character_name.clone(),
                    content: input.content.clone(),
                },
                &CreateOptions {
                    id: id.clone(),
                    created_at: now.clone(),
                    updated_at: now,
                },
            )?;
            Ok(id)
        }
    }
}
