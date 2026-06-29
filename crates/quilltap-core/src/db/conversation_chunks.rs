//! The conversation-chunks repository — a Phase-2 repo port, after `folders`,
//! `tags`, `text_replacement_rules`, `prompt_templates`,
//! `conversation_annotations`, `provider_models`, `help_docs`, `image_profiles`,
//! and the rest. Ports v4's
//! `lib/database/repositories/conversation-chunks.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, and `delete` (the three abstract methods, each a
//! straight delegate to `_create`/`_update`/`_delete`). The custom helpers —
//! `countByChatIds`, `findByChatId`, `findByInterchangeIndex`, `upsert`,
//! `findAllWithEmbeddings`, `deleteAllForChat`, `updateEmbedding` — are out of
//! scope (the `upsert`/`updateEmbedding` paths need the remap-normalization form
//! for their internal `now`/`generateId()`, deferred like the other repos'
//! `upsert*`).
//!
//! ## A BLOB column, after `help_docs`
//!
//! `conversation_chunks.embedding` is the second tier-2 **BLOB column**, modeled
//! exactly like `help_docs.embedding`: a raw little-endian Float32 byte buffer
//! via [`crate::embedding_blob::float32_to_blob`]. v4's `documentToRow` blob path
//! (`embeddingToBlob`, wired by the repo auto-registering the `embedding` blob
//! column on first `getCollection()`) stores an **empty array or null as SQL
//! NULL**, never a zero-length blob; here `None` *or* an empty `Vec<f32>` binds
//! SQL NULL, only a non-empty vector is serialized. The canonical dump emits
//! BLOBs as lowercase hex on both sides, so a deterministic Float32 buffer
//! compares byte-for-byte (`[0.5,-0.25,0.75,0.125]` →
//! `0000003f000080be0000403f0000003e`).
//!
//! Following `help_docs` exactly, the BLOB is **not touchable through `update`**:
//! v4's `_update` whole-row rewrite re-persists the existing embedding unchanged,
//! so a text-only patch leaves it intact. This port models the patch as a partial
//! `UPDATE SET` over only the provided columns + `updatedAt`, never naming the
//! `embedding` column, so the stored BLOB survives untouched. The corpus exercises
//! this directly (a content/interchangeIndex/participantNames update on the
//! embedded seed row, asserted to still show the original embedding hex).
//!
//! ## The rest of the marshaling surface
//!
//!   - `interchangeIndex` is `z.number().int().min(0)` — a min but NO max — so
//!     v4's `mapToSQLiteType` lowers it to **REAL** (INTEGER affinity needs an
//!     integer min AND max). It binds `f64`; an integer-valued REAL (e.g. `0.0`)
//!     renders back as `0` in the canonical dump via [`super::js_number_to_json`],
//!     matching v4 byte-for-byte.
//!   - `participantNames` and `messageIds` are `z.array(z.string()).default([])`
//!     JSON **array** columns — `Vec<String>` → compact JSON text
//!     (`["a","b"]`, `[]`), the order-preserving array shape (no key-order
//!     subtlety, unlike JSON-object columns).
//!   - `chatId` is a UUID → TEXT; `content` is TEXT; timestamps TEXT.
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in each update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form
//! `folders`/`tags`/`help_docs`/`provider_models` use.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;
use crate::embedding_blob::float32_to_blob;

/// Fields for creating a conversation chunk (the `Omit<ConversationChunk,'id'|
/// timestamps>` shape). `embedding` is the BLOB column (`None`/empty → SQL NULL,
/// non-empty → little-endian Float32 bytes); `participant_names`/`message_ids`
/// are the JSON array columns; `interchange_index` is the REAL number column.
pub struct CcCreate {
    pub chat_id: String,
    /// `z.number().int().min(0)` (min only, no max) → REAL → bound `f64`.
    pub interchange_index: f64,
    pub content: String,
    /// Stored as compact JSON array text (`["a","b"]`, `[]` when empty).
    pub participant_names: Vec<String>,
    /// Stored as compact JSON array text (`["id"]`, `[]` when empty).
    pub message_ids: Vec<String>,
    /// `None` or empty → SQL NULL; non-empty → little-endian Float32 bytes.
    pub embedding: Option<Vec<f32>>,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A conversation-chunk update patch. Mirrors v4 `update` over `_update`:
/// provided fields overwrite, id and createdAt are preserved, `updatedAt` is set
/// explicitly. Following `help_docs`, it deliberately has **no embedding field** —
/// the BLOB is never touched through `update` (v4's whole-row rewrite re-persists
/// the existing embedding unchanged; here the partial `UPDATE SET` simply never
/// names the `embedding` column). Each `Some` field sets that column.
#[derive(Default)]
pub struct CcUpdate {
    pub chat_id: Option<String>,
    /// REAL number column; `Some(n)` sets it (bound `f64`).
    pub interchange_index: Option<f64>,
    pub content: Option<String>,
    /// Re-serialized to compact JSON array text when provided.
    pub participant_names: Option<Vec<String>>,
    /// Re-serialized to compact JSON array text when provided.
    pub message_ids: Option<Vec<String>>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct ConversationChunksRepository<'c> {
    conn: &'c Connection,
}

impl<'c> ConversationChunksRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a conversation chunk with the given pinned id + timestamps. The
    /// embedding serializes to a little-endian Float32 BLOB (`None`/empty → SQL
    /// NULL); `participantNames`/`messageIds` → compact JSON array text;
    /// `interchangeIndex` binds `f64`.
    pub fn create(&self, data: &CcCreate, opts: &CreateOptions) -> Result<(), DbError> {
        // empty / null embedding -> SQL NULL; non-empty -> Float32 LE bytes.
        let embedding_blob: Option<Vec<u8>> = match &data.embedding {
            Some(v) if !v.is_empty() => Some(float32_to_blob(v)),
            _ => None,
        };
        let participant_names_json = serde_json::to_string(&data.participant_names)
            .map_err(|e| DbError::Key(format!("participantNames serialize: {e}")))?;
        let message_ids_json = serde_json::to_string(&data.message_ids)
            .map_err(|e| DbError::Key(format!("messageIds serialize: {e}")))?;

        self.conn.execute(
            "INSERT INTO conversation_chunks \
               (id, chatId, interchangeIndex, content, participantNames, messageIds, \
                embedding, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                opts.id,
                data.chat_id,
                data.interchange_index,
                data.content,
                participant_names_json,
                message_ids_json,
                embedding_blob,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the chunk `id`. Returns `Ok(false)` when no row
    /// matched (v4's "not found -> null"). id, createdAt, and the `embedding`
    /// BLOB are never touched. Each `Some` field sets that column; `updatedAt` is
    /// always set.
    pub fn update(&self, id: &str, patch: &CcUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`: the row must exist or the update is a
        // no-op (-> null). Mirror that so a missing target yields Ok(false)
        // rather than relying on the UPDATE affecting zero rows.
        if !self.row_exists(id)? {
            return Ok(false);
        }

        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(chat_id) = &patch.chat_id {
            assignments.push(format!("chatId = ?{}", values.len() + 1));
            values.push(Box::new(chat_id.clone()));
        }
        if let Some(interchange_index) = patch.interchange_index {
            assignments.push(format!("interchangeIndex = ?{}", values.len() + 1));
            values.push(Box::new(interchange_index));
        }
        if let Some(content) = &patch.content {
            assignments.push(format!("content = ?{}", values.len() + 1));
            values.push(Box::new(content.clone()));
        }
        if let Some(participant_names) = &patch.participant_names {
            let participant_names_json = serde_json::to_string(participant_names)
                .map_err(|e| DbError::Key(format!("participantNames serialize: {e}")))?;
            assignments.push(format!("participantNames = ?{}", values.len() + 1));
            values.push(Box::new(participant_names_json));
        }
        if let Some(message_ids) = &patch.message_ids {
            let message_ids_json = serde_json::to_string(message_ids)
                .map_err(|e| DbError::Key(format!("messageIds serialize: {e}")))?;
            assignments.push(format!("messageIds = ?{}", values.len() + 1));
            values.push(Box::new(message_ids_json));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE conversation_chunks SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the chunk `id`. Returns `Ok(false)` when no row matched (v4's
    /// `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM conversation_chunks WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// True iff a row with this id exists — v4's `_update` `findById` precondition
    /// (a missing target makes the update a no-op returning `null`).
    fn row_exists(&self, id: &str) -> Result<bool, DbError> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM conversation_chunks WHERE id = ?1",
                params![id],
                |row| row.get::<_, i64>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(found.is_some())
    }
}
