//! The conversation-annotations repository — a Phase-2 repo port, after
//! `folders`, `tags`, `text_replacement_rules`, and `prompt_templates`. Ports
//! v4's `lib/database/repositories/conversation-annotations.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, and `delete` (the three abstract methods over the
//! base repo). The custom query/upsert helpers — `upsert`, `findByChatId`,
//! `findByMessageIndex`, `deleteAnnotation`, `deleteAllForChat` — are out of
//! scope here. Single source: `ConversationAnnotationSchema` from
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
/// explicitly. Carries only `content`/`characterName`/`messageIndex` — clearing
/// the nullable `sourceMessageId` to NULL is deferred (see the module header).
#[derive(Default)]
pub struct CaUpdate {
    pub content: Option<String>,
    pub character_name: Option<String>,
    pub message_index: Option<f64>,
    pub updated_at: String,
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
}
