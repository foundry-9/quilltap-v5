//! The chat-documents repository — a Phase-2 repo port, after `folders`, `tags`,
//! `text_replacement_rules`, `prompt_templates`, `conversation_annotations`, and
//! `image_profiles`. Ports v4's
//! `lib/database/repositories/chat-documents.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, and `delete` (the three abstract methods over the
//! base repo). The custom query / mutation helpers — `findActiveForChat`,
//! `findOpenForChat`, `findByChatId`, `findRecentForChat`, `findRecentAcrossChats`,
//! `openDocument`, `closeDocumentById`, `closeDocument`, `renameFilePath`,
//! `renameFilePathInStore`, `renameFolderPathInStore`, `deleteByChatId` — are out
//! of scope here. v4's `update` strips `id` and `createdAt` before `_update`,
//! which is a no-op for this port since we preserve both anyway. There is **no
//! built-in guard** (unlike `prompt_templates`).
//!
//! ## What this repo banks for the tier-2 marshaling surface
//!
//! `chat_documents` is a plain (non-Taggable) `AbstractBaseRepository`. Its shape
//! is all-text-plus-one-boolean, with no number, JSON, or BLOB columns:
//!
//!   - `chatId` / `filePath`: required TEXT strings.
//!   - `scope`: an enum TEXT column (`DocScopeSchema =
//!     z.enum(['project','document_store','general'])`, default `'project'`).
//!     Modeled as a plain `String` set explicitly by every corpus op.
//!   - `mountPoint` / `displayTitle`: nullable TEXT
//!     (`z.string().nullable().optional()`) → `Option<String>` (`None` → SQL
//!     NULL).
//!   - `isActive`: a boolean (`z.boolean().default(true)`) → INTEGER 0/1 (the
//!     `tags.quickHide` / `image_profiles.isDefault` mapping).
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in the update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form
//! `folders`/`tags`/`text_replacement_rules`/`prompt_templates`/
//! `conversation_annotations`/`image_profiles` use.
//!
//! Deferred (not in the corpus, mirroring `image_profiles`): setting a nullable
//! column (`mountPoint`, `displayTitle`) **to NULL** via `update` — the patch
//! models a provided field as "set to this value", so a nullable setter lands
//! when an op needs it.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;

/// Fields for creating a chat document (the `Omit<ChatDocument,'id'|
/// timestamps>` shape). The two `Option` string fields are the nullable columns;
/// `is_active` lands as INTEGER 0/1; the rest pass through as TEXT.
pub struct CdCreate {
    pub chat_id: String,
    pub file_path: String,
    /// One of `'project' | 'document_store' | 'general'` (enum TEXT).
    pub scope: String,
    /// `None` => SQL NULL (the `.nullable().optional()` column absent).
    pub mount_point: Option<String>,
    /// `None` => SQL NULL.
    pub display_title: Option<String>,
    pub is_active: bool,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A chat-document update patch. Mirrors v4 `update` over `_update`: provided
/// fields overwrite, id and createdAt are preserved (v4 deletes them off the
/// patch; we never touch them), `updatedAt` is set explicitly. Each `Some` field
/// sets that column; clearing a nullable column to NULL is deferred (see header).
#[derive(Default)]
pub struct CdUpdate {
    pub chat_id: Option<String>,
    pub file_path: Option<String>,
    pub scope: Option<String>,
    pub mount_point: Option<String>,
    pub display_title: Option<String>,
    pub is_active: Option<bool>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct ChatDocumentsRepository<'c> {
    conn: &'c Connection,
}

impl<'c> ChatDocumentsRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a chat document with the given pinned id + timestamps. `isActive` →
    /// INTEGER 0/1; `mountPoint`/`displayTitle` as `Option<String>` (`None` → SQL
    /// NULL); the rest as TEXT.
    pub fn create(&self, data: &CdCreate, opts: &CreateOptions) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO chat_documents \
               (id, chatId, filePath, scope, mountPoint, displayTitle, isActive, \
                createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                opts.id,
                data.chat_id,
                data.file_path,
                data.scope,
                data.mount_point,
                data.display_title,
                i64::from(data.is_active),
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the chat document `id`. Returns `Ok(false)` when
    /// no row matched (v4's "not found -> null"). id and createdAt are never
    /// touched. Each `Some` field sets that column; `updatedAt` is always set.
    pub fn update(&self, id: &str, patch: &CdUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`s — the row must exist or it's a no-op.
        let exists: bool = self
            .conn
            .query_row(
                "SELECT 1 FROM chat_documents WHERE id = ?1",
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

        if let Some(chat_id) = &patch.chat_id {
            assignments.push(format!("chatId = ?{}", values.len() + 1));
            values.push(Box::new(chat_id.clone()));
        }
        if let Some(file_path) = &patch.file_path {
            assignments.push(format!("filePath = ?{}", values.len() + 1));
            values.push(Box::new(file_path.clone()));
        }
        if let Some(scope) = &patch.scope {
            assignments.push(format!("scope = ?{}", values.len() + 1));
            values.push(Box::new(scope.clone()));
        }
        if let Some(mount_point) = &patch.mount_point {
            assignments.push(format!("mountPoint = ?{}", values.len() + 1));
            values.push(Box::new(mount_point.clone()));
        }
        if let Some(display_title) = &patch.display_title {
            assignments.push(format!("displayTitle = ?{}", values.len() + 1));
            values.push(Box::new(display_title.clone()));
        }
        if let Some(is_active) = patch.is_active {
            assignments.push(format!("isActive = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(is_active)));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE chat_documents SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the chat document `id`. Returns `false` when no row matched (v4's
    /// `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM chat_documents WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }
}
