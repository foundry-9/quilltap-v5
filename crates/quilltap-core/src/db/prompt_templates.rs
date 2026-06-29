//! The prompt-templates repository — the fourth Phase-2 repo port, after
//! `folders`, `tags`, and `text_replacement_rules`. Ports v4's
//! `lib/database/repositories/prompt-templates.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, and `delete`. (The repo's built-in *seeding* —
//! `seedSamplePrompts` — is a startup concern, not a CRUD op, and is out of
//! scope here.) `prompt_templates` uses the plain `AbstractBaseRepository`
//! because `userId` is nullable (built-in templates have `userId = null`). It
//! widens the tier-2 marshaling surface past `text_replacement_rules` with:
//!
//!   - the first **JSON array column** (`tags: z.array(UUIDSchema)`) — v4's
//!     `prepareForStorage` `JSON.stringify`s an array, so it lands as compact
//!     JSON text (`["id1","id2"]`, `[]` when empty). Reproduced with
//!     `serde_json::to_string` of a `Vec<String>`; arrays are order-preserving,
//!     so (unlike the `tags.visualStyle` object) there is no key-order subtlety.
//!   - several **nullable string columns** (`userId`, `description`, `category`,
//!     `modelHint`) — `None` → SQL NULL, `Some` → the text. `folders` had one
//!     nullable column; this is the first repo with several, including the
//!     null-for-built-in `userId`.
//!
//! ## The built-in read-only guard (the new behavior)
//!
//! v4's `update`/`delete` first `findById`, and if the row is a **built-in
//! template** (`isBuiltIn === true`) they refuse: `update` returns `null` and
//! `delete` returns `false`, leaving the row untouched. This is a read-then-
//! guard pattern like `text_replacement_rules`' conflict check, but it
//! *suppresses* the op (returns a not-modified result) rather than throwing.
//! The port reads only `isBuiltIn` for the target row (behaviorally identical to
//! v4's full `findById` for the guard *outcome* on valid data) and returns
//! `Ok(false)` for both "not found" and "built-in" — the same two cases v4
//! collapses to `null` / `false`.
//!
//! Determinism: the tier-2 case pins the id and timestamps, so the persisted
//! rows match v4's byte-for-byte with no normalization — the form
//! `folders`/`tags`/`text_replacement_rules` use.
//!
//! Deferred (not in the corpus): setting a nullable column **to NULL** via
//! `update` (the patch models a provided field as "set to this value"; clearing
//! a column lands when an op needs it), and Zod's `tags`/`isBuiltIn` defaults on
//! create (the corpus supplies both explicitly, as `tags`' `visualStyle` did).

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;

/// Fields for creating a prompt template (the `Omit<PromptTemplate,'id'|
/// timestamps>` shape). `tags` is the JSON array column; the four `Option`
/// string fields are the nullable columns.
pub struct PtCreate {
    /// `None` => SQL NULL (the null-for-built-in `userId`).
    pub user_id: Option<String>,
    pub name: String,
    pub content: String,
    pub description: Option<String>,
    pub is_built_in: bool,
    pub category: Option<String>,
    pub model_hint: Option<String>,
    /// Stored as compact JSON text (`["id1","id2"]`, `[]` when empty).
    pub tags: Vec<String>,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A prompt-template update patch. Mirrors v4 `update` over `_update`: provided
/// fields overwrite, id and createdAt are preserved, `updatedAt` is set
/// explicitly. A built-in target is rejected before any write (see the module
/// header). Each `Some` field sets that column; clearing a nullable column to
/// NULL is deferred (not in the corpus).
#[derive(Default)]
pub struct PtUpdate {
    pub name: Option<String>,
    pub content: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub model_hint: Option<String>,
    pub tags: Option<Vec<String>>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct PromptTemplatesRepository<'c> {
    conn: &'c Connection,
}

impl<'c> PromptTemplatesRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a prompt template with the given pinned id + timestamps. No guard
    /// on create (only `update`/`delete` reject built-ins).
    pub fn create(&self, data: &PtCreate, opts: &CreateOptions) -> Result<(), DbError> {
        let tags_json = serde_json::to_string(&data.tags)
            .map_err(|e| DbError::Key(format!("tags serialize: {e}")))?;

        self.conn.execute(
            "INSERT INTO prompt_templates \
               (id, userId, name, content, description, isBuiltIn, category, modelHint, tags, \
                createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                opts.id,
                data.user_id,
                data.name,
                data.content,
                data.description,
                i64::from(data.is_built_in),
                data.category,
                data.model_hint,
                tags_json,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the template `id`. Returns `Ok(false)` when no
    /// row matched OR the row is built-in (both of v4's "-> null" cases). id and
    /// createdAt are never touched.
    pub fn update(&self, id: &str, patch: &PtUpdate) -> Result<bool, DbError> {
        if !self.is_mutable(id)? {
            return Ok(false);
        }

        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(name) = &patch.name {
            assignments.push(format!("name = ?{}", values.len() + 1));
            values.push(Box::new(name.clone()));
        }
        if let Some(content) = &patch.content {
            assignments.push(format!("content = ?{}", values.len() + 1));
            values.push(Box::new(content.clone()));
        }
        if let Some(description) = &patch.description {
            assignments.push(format!("description = ?{}", values.len() + 1));
            values.push(Box::new(description.clone()));
        }
        if let Some(category) = &patch.category {
            assignments.push(format!("category = ?{}", values.len() + 1));
            values.push(Box::new(category.clone()));
        }
        if let Some(model_hint) = &patch.model_hint {
            assignments.push(format!("modelHint = ?{}", values.len() + 1));
            values.push(Box::new(model_hint.clone()));
        }
        if let Some(tags) = &patch.tags {
            let tags_json = serde_json::to_string(tags)
                .map_err(|e| DbError::Key(format!("tags serialize: {e}")))?;
            assignments.push(format!("tags = ?{}", values.len() + 1));
            values.push(Box::new(tags_json));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE prompt_templates SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the template `id`. Returns `Ok(false)` when no row matched OR the
    /// row is built-in (both of v4's "-> false" cases).
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        if !self.is_mutable(id)? {
            return Ok(false);
        }
        let affected = self
            .conn
            .execute("DELETE FROM prompt_templates WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// True iff the row exists and is not a built-in template — v4's `findById`
    /// + `isBuiltIn` guard, reading only the one column the guard needs.
    fn is_mutable(&self, id: &str) -> Result<bool, DbError> {
        let built_in: Option<i64> = self
            .conn
            .query_row(
                "SELECT isBuiltIn FROM prompt_templates WHERE id = ?1",
                params![id],
                |row| row.get::<_, i64>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(matches!(built_in, Some(0)))
    }
}
