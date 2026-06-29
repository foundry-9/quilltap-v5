//! The roleplay-templates repository — a Phase-2 repo port, after `folders`,
//! `tags`, `text_replacement_rules`, `prompt_templates`, and
//! `conversation_annotations`. Ports v4's
//! `lib/database/repositories/roleplay-templates.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, and `delete`. The built-in *seeding*
//! (`seedBuiltInTemplates`) and the tag-mutation helpers (`addTag`/`removeTag`)
//! are out of scope. Like `prompt_templates`, `roleplay_templates` uses the plain
//! `AbstractBaseRepository` because `userId` is nullable (built-in templates have
//! `userId = null`). **No built-in read-only guard is ported here:** v4's
//! `roleplay-templates` `update`/`delete` DO carry an `isBuiltIn` guard, but the
//! task scopes this port to the base `_create`/`_update`/`_delete` delegation
//! shape used by the corpus — the corpus never targets a built-in row for a
//! mutation, so the guard is not exercised and is left to a later widening.
//!
//! ## What this repo banks for the tier-2 marshaling surface
//!
//! It is the **first repo with an array-of-objects JSON column**
//! (`renderingPatterns`), plus a **nullable JSON-object column**
//! (`dialogueDetection`):
//!
//!   - **`renderingPatterns: z.array(RenderingPatternSchema)`** → a JSON array of
//!     objects, stored compact via v4's `prepareForStorage` `JSON.stringify`. Each
//!     element's key order is the schema's field order (Zod `.parse()` rebuilds
//!     the object in shape order), so the elements are modeled with a typed
//!     [`RenderingPattern`] struct whose fields are declared in
//!     `RenderingPatternSchema` order — `pattern`, `className`, then the three
//!     optionals `flags` / `scope` / `hideDelimiters`. The optionals carry
//!     `skip_serializing_if = "Option::is_none"`, matching Zod's omission of an
//!     absent `.optional()` field (and `JSON.stringify`'s skip of `undefined`).
//!     `serde_json::Value` is deliberately NOT used (its `BTreeMap` would sort the
//!     keys and diverge from v4) — the same typed-struct rule `tags.visualStyle`
//!     established, now over an array of objects.
//!   - **`dialogueDetection: DialogueDetectionSchema.nullable().optional()`** → a
//!     nullable JSON object. `None` → SQL NULL; `Some` → compact JSON via
//!     [`DialogueDetection`], fields in schema order (`openingChars`,
//!     `closingChars`, `className`).
//!   - **`tags: z.array(UUIDSchema)`** → a JSON array of UUID strings, exactly as
//!     `prompt_templates` (order-preserving, no key-order subtlety).
//!   - several **nullable string columns** (`userId` null-for-built-in,
//!     `description`) and the `isBuiltIn` boolean → INTEGER 0/1.
//!
//! Two columns are deliberately kept to their *simple* forms (the task's scope
//! dodge) — modeling their full schemas buys no new marshaling coverage:
//!
//!   - **`delimiters`** is held EMPTY (`[]`) across the whole corpus. Its element
//!     schema is a preprocessed discriminated union (`wrap` / `linePrefix` /
//!     `tagPrefix`); the column is bound as the JSON text of an empty array.
//!   - **`narrationDelimiters`** uses the plain STRING form (e.g. `"*"`). The
//!     schema is `z.union([string, tuple])`; a string is stored as plain TEXT (not
//!     JSON), so it is bound as a `String`.
//!
//! Determinism: the tier-2 case pins the id and timestamps, so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form
//! `folders`/`tags`/`text_replacement_rules`/`prompt_templates`/
//! `conversation_annotations` use.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::DbError;

/// One rendering pattern — an element of the `renderingPatterns` JSON array.
/// Field order here is `RenderingPatternSchema`'s definition order in v4's
/// `template.types.ts`; `serde_json` serializes struct fields in declaration
/// order, so `to_string` reproduces v4's `JSON.stringify(zodParsed)` exactly.
/// The three optionals are `skip_serializing_if = "Option::is_none"` so an absent
/// `.optional()` field is omitted from the stored object (matching Zod +
/// `JSON.stringify`), letting the corpus exercise varied optional-field presence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderingPattern {
    pub pattern: String,
    pub class_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide_delimiters: Option<bool>,
}

/// The `dialogueDetection` JSON object (nullable column). Field order is
/// `DialogueDetectionSchema`'s definition order (`openingChars`, `closingChars`,
/// `className`); serialized compact, reproducing v4's `JSON.stringify`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogueDetection {
    pub opening_chars: Vec<String>,
    pub closing_chars: Vec<String>,
    pub class_name: String,
}

/// Fields for creating a roleplay template (the `Omit<RoleplayTemplate,'id'|
/// timestamps>` shape). `tags`/`renderingPatterns` are the JSON array columns;
/// `dialogue_detection` is the nullable JSON-object column; `user_id` /
/// `description` are the nullable string columns. `delimiters` is held empty and
/// `narration_delimiters` to a plain string (see the module header).
pub struct RtCreate {
    /// `None` => SQL NULL (the null-for-built-in `userId`).
    pub user_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub system_prompt: String,
    pub is_built_in: bool,
    /// Stored as compact JSON text (`["id1","id2"]`, `[]` when empty).
    pub tags: Vec<String>,
    /// Array of pattern objects → compact JSON array of objects.
    pub rendering_patterns: Vec<RenderingPattern>,
    /// `None` => SQL NULL; `Some` => compact JSON object.
    pub dialogue_detection: Option<DialogueDetection>,
    /// The plain-string narration delimiter (e.g. `"*"`), stored as TEXT.
    pub narration_delimiters: String,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A roleplay-template update patch. Mirrors v4 `update` over `_update`: provided
/// fields overwrite, id and createdAt are preserved, `updatedAt` is set
/// explicitly. Each `Some` field sets that column (JSON columns re-serialized).
#[derive(Default)]
pub struct RtUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub system_prompt: Option<String>,
    pub tags: Option<Vec<String>>,
    pub rendering_patterns: Option<Vec<RenderingPattern>>,
    pub dialogue_detection: Option<DialogueDetection>,
    pub narration_delimiters: Option<String>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct RoleplayTemplatesRepository<'c> {
    conn: &'c Connection,
}

impl<'c> RoleplayTemplatesRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a roleplay template with the given pinned id + timestamps. All 13
    /// columns are written; JSON columns via `serde_json::to_string`, the bool via
    /// `i64::from`, nullable columns via `Option`. `delimiters` is the JSON text of
    /// an empty array (held empty across the corpus).
    pub fn create(&self, data: &RtCreate, opts: &CreateOptions) -> Result<(), DbError> {
        let tags_json = serde_json::to_string(&data.tags)
            .map_err(|e| DbError::Key(format!("tags serialize: {e}")))?;
        let rendering_patterns_json = serde_json::to_string(&data.rendering_patterns)
            .map_err(|e| DbError::Key(format!("renderingPatterns serialize: {e}")))?;
        // delimiters: kept empty across the corpus (see the module header).
        let delimiters_json = "[]".to_string();
        let dialogue_detection_json: Option<String> = match &data.dialogue_detection {
            Some(dd) => Some(
                serde_json::to_string(dd)
                    .map_err(|e| DbError::Key(format!("dialogueDetection serialize: {e}")))?,
            ),
            None => None,
        };

        self.conn.execute(
            "INSERT INTO roleplay_templates \
               (id, userId, name, description, systemPrompt, isBuiltIn, tags, delimiters, \
                renderingPatterns, dialogueDetection, narrationDelimiters, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                opts.id,
                data.user_id,
                data.name,
                data.description,
                data.system_prompt,
                i64::from(data.is_built_in),
                tags_json,
                delimiters_json,
                rendering_patterns_json,
                dialogue_detection_json,
                data.narration_delimiters,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the template `id`. Returns `Ok(false)` when no row
    /// matched (v4's "not found -> null"). id and createdAt are never touched;
    /// `updatedAt` is always set. JSON columns are re-serialized when provided.
    pub fn update(&self, id: &str, patch: &RtUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`s — the row must exist or it's a no-op.
        let exists: bool = self
            .conn
            .query_row(
                "SELECT 1 FROM roleplay_templates WHERE id = ?1",
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

        if let Some(name) = &patch.name {
            assignments.push(format!("name = ?{}", values.len() + 1));
            values.push(Box::new(name.clone()));
        }
        if let Some(description) = &patch.description {
            assignments.push(format!("description = ?{}", values.len() + 1));
            values.push(Box::new(description.clone()));
        }
        if let Some(system_prompt) = &patch.system_prompt {
            assignments.push(format!("systemPrompt = ?{}", values.len() + 1));
            values.push(Box::new(system_prompt.clone()));
        }
        if let Some(tags) = &patch.tags {
            let tags_json = serde_json::to_string(tags)
                .map_err(|e| DbError::Key(format!("tags serialize: {e}")))?;
            assignments.push(format!("tags = ?{}", values.len() + 1));
            values.push(Box::new(tags_json));
        }
        if let Some(rendering_patterns) = &patch.rendering_patterns {
            let json = serde_json::to_string(rendering_patterns)
                .map_err(|e| DbError::Key(format!("renderingPatterns serialize: {e}")))?;
            assignments.push(format!("renderingPatterns = ?{}", values.len() + 1));
            values.push(Box::new(json));
        }
        if let Some(dialogue_detection) = &patch.dialogue_detection {
            let json = serde_json::to_string(dialogue_detection)
                .map_err(|e| DbError::Key(format!("dialogueDetection serialize: {e}")))?;
            assignments.push(format!("dialogueDetection = ?{}", values.len() + 1));
            values.push(Box::new(json));
        }
        if let Some(narration_delimiters) = &patch.narration_delimiters {
            assignments.push(format!("narrationDelimiters = ?{}", values.len() + 1));
            values.push(Box::new(narration_delimiters.clone()));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE roleplay_templates SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the template `id`. Returns `false` when no row matched (v4's
    /// `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM roleplay_templates WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }
}
