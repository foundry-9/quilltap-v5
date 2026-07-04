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

/// One `chat_documents` row restricted to the columns `self_inventory`'s
/// `buildContextFiles` reads.
#[derive(Clone, Debug)]
pub struct ChatDocumentRow {
    pub scope: String,
    pub mount_point: Option<String>,
    pub file_path: String,
    pub display_title: Option<String>,
}

/// The `chat_documents` fields the document-UI tools consume — v4's
/// `findOpenForChat` / `resolveOpenDocTarget` read `id` / `filePath` / `scope` /
/// `mountPoint` (and sort on `createdAt`). This is the row shape those handlers
/// operate over (a `ChatDocument` subset).
#[derive(Clone, Debug)]
pub struct OpenChatDocument {
    pub id: String,
    pub file_path: String,
    pub scope: String,
    pub mount_point: Option<String>,
    /// ISO-8601; the open set is sorted oldest-first on this (v4
    /// `findOpenForChat`).
    pub created_at: String,
}

/// Input for [`ChatDocumentsRepository::open_document`] — v4 `openDocument`'s
/// `data` bag (`filePath` / `scope` / `mountPoint?` / `displayTitle?`).
pub struct OpenDocumentInput {
    pub file_path: String,
    pub scope: String,
    /// `None` => SQL NULL (v4 `mountPoint ?? null`).
    pub mount_point: Option<String>,
    /// `None` => SQL NULL on create (v4 `displayTitle ?? null`).
    pub display_title: Option<String>,
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

    /// v4 `chatDocuments.findByChatId` — the documents open in a chat, in table
    /// (rowid) order (v4 `findByFilter({ chatId })` with no explicit sort; the
    /// `self_inventory` builder re-sorts by `filePath`). Returns only the columns
    /// `buildContextFiles` reads: `scope` / `mountPoint` / `filePath` /
    /// `displayTitle`. `scope`'s Zod `.default('project')` materializes on a NULL
    /// cell (never written NULL by `create`, but faithful anyway); the two nullable
    /// columns come back as `None` on NULL.
    pub fn find_by_chat_id(&self, chat_id: &str) -> Result<Vec<ChatDocumentRow>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT scope, mountPoint, filePath, displayTitle \
               FROM chat_documents WHERE chatId = ?1",
        )?;
        let rows = stmt
            .query_map(params![chat_id], |row| {
                let scope: Option<String> = row.get(0)?;
                Ok(ChatDocumentRow {
                    scope: scope.unwrap_or_else(|| "project".to_string()),
                    mount_point: row.get::<_, Option<String>>(1)?,
                    file_path: row.get::<_, String>(2)?,
                    display_title: row.get::<_, Option<String>>(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// v4 `findOpenForChat` (`chat-documents.repository.ts:84`): every open
    /// (`isActive == true`) document for the chat, **oldest-opened first** —
    /// `findByFilter({ chatId, isActive: true })` then a stable sort by
    /// `createdAt` ascending (so the last entry is the newest open, which
    /// `resolveOpenDocTarget` picks by default). Reads `id` / `filePath` / `scope`
    /// / `mountPoint` / `createdAt`; `scope`'s `.default('project')` materializes on
    /// a NULL cell (never written NULL by `create`, but faithful).
    pub fn find_open_for_chat(&self, chat_id: &str) -> Result<Vec<OpenChatDocument>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, filePath, scope, mountPoint, createdAt \
               FROM chat_documents WHERE chatId = ?1 AND isActive = 1",
        )?;
        let mut rows = stmt
            .query_map(params![chat_id], |row| {
                let scope: Option<String> = row.get(2)?;
                Ok(OpenChatDocument {
                    id: row.get::<_, String>(0)?,
                    file_path: row.get::<_, String>(1)?,
                    scope: scope.unwrap_or_else(|| "project".to_string()),
                    mount_point: row.get::<_, Option<String>>(3)?,
                    created_at: row.get::<_, String>(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        // v4 sorts by `new Date(a.createdAt).getTime()` ascending. A stable sort
        // keyed on the ISO string is byte-order-equivalent for our fixed-width
        // `YYYY-MM-DDTHH:MM:SS.sssZ` timestamps.
        rows.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(rows)
    }

    /// v4 `openDocument` (`chat-documents.repository.ts:162`): open a document for
    /// a chat. Several may be open at once, so previously-open documents are left
    /// active. If a row for the same `(filePath, scope, mountPoint||null)` already
    /// exists (active or not) it is **reactivated** (`update({ isActive: true,
    /// displayTitle })`, which mints a fresh `updatedAt`); otherwise a new row is
    /// created (minting `id`/`createdAt`/`updatedAt`). Both branches mint their
    /// timestamps → the differential placeholders them.
    ///
    /// Returns the row `id` (the one created or reactivated), the target
    /// `doc_close`/`doc_focus` later resolve.
    pub fn open_document(
        &self,
        chat_id: &str,
        input: &OpenDocumentInput,
    ) -> Result<String, DbError> {
        // Reactivate an existing (filePath, scope, mountPoint||null) match rather
        // than inserting a duplicate — v4 scans ALL rows (active + inactive) for
        // the chat and matches on the normalized-null mountPoint.
        let mut stmt = self.conn.prepare(
            "SELECT id, mountPoint, displayTitle FROM chat_documents \
               WHERE chatId = ?1 AND filePath = ?2 AND scope = ?3",
        )?;
        let existing: Vec<(String, Option<String>, Option<String>)> = stmt
            .query_map(params![chat_id, input.file_path, input.scope], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        let want_mount = input.mount_point.clone();
        if let Some((id, _existing_mount, existing_title)) = existing
            .into_iter()
            .find(|(_, m, _)| m.clone() == want_mount || (m.is_none() && want_mount.is_none()))
        {
            // Reactivate: displayTitle ?? existingDoc.displayTitle.
            let title = input.display_title.clone().or(existing_title);
            let patch = CdUpdate {
                is_active: Some(true),
                display_title: title,
                updated_at: crate::clock::now_iso(),
                ..Default::default()
            };
            self.update(&id, &patch)?;
            return Ok(id);
        }

        // Create a fresh association (mountPoint ?? null, displayTitle ?? null).
        let id = uuid::Uuid::new_v4().to_string();
        let now = crate::clock::now_iso();
        let data = CdCreate {
            chat_id: chat_id.to_string(),
            file_path: input.file_path.clone(),
            scope: input.scope.clone(),
            mount_point: input.mount_point.clone(),
            display_title: input.display_title.clone(),
            is_active: true,
        };
        self.create(
            &data,
            &CreateOptions {
                id: id.clone(),
                created_at: now.clone(),
                updated_at: now,
            },
        )?;
        Ok(id)
    }

    /// v4 `closeDocumentById` (`chat-documents.repository.ts:204`): close one open
    /// document by row id (deactivate, not delete — the row is kept for
    /// quick-reopen). Returns `false` when the row is missing or belongs to a
    /// different chat. `update({ isActive: false })` mints a fresh `updatedAt`.
    pub fn close_document_by_id(
        &self,
        chat_id: &str,
        chat_document_id: &str,
    ) -> Result<bool, DbError> {
        // Confirm the row exists AND belongs to this chat (v4 findById + chatId
        // guard).
        let owning_chat: Option<String> = self
            .conn
            .query_row(
                "SELECT chatId FROM chat_documents WHERE id = ?1",
                params![chat_document_id],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        if owning_chat.as_deref() != Some(chat_id) {
            return Ok(false);
        }
        let patch = CdUpdate {
            is_active: Some(false),
            updated_at: crate::clock::now_iso(),
            ..Default::default()
        };
        self.update(chat_document_id, &patch)?;
        Ok(true)
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
