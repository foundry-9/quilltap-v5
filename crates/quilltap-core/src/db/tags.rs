//! The tags repository — the second Phase-2 repo port, after `folders`. Ports
//! v4's `lib/database/repositories/tags.repository.ts` (+ the `_create`/
//! `_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, and `delete`. `tags` is, like `folders`, a pure
//! single-table user-owned repo, but it widens the tier-2 marshaling coverage
//! past `folders`' all-strings shape with three things this port reproduces:
//!
//!   - **`quickHide` boolean → INTEGER 0/1.** v4's `prepareForStorage` maps a JS
//!     boolean to 1/0 before insert (and the backend reads it back via the
//!     schema's boolean-column set). We bind the same 0/1.
//!   - **`visualStyle` object → JSON text (compact, schema field order).** v4
//!     stores it via `JSON.stringify` of the Zod-parsed object, whose key order
//!     is the schema's field order. We reproduce that byte-for-byte with a typed
//!     [`TagVisualStyle`] struct serialized by `serde_json::to_string` (fields
//!     in schema order; `serde_json::Value` is deliberately NOT used — its
//!     default `BTreeMap` would sort the keys and diverge from v4).
//!   - **`nameLower` derivation.** On create v4 sets
//!     `nameLower = (data.nameLower || data.name).toLowerCase()`; on update it
//!     re-derives `nameLower` from `name` whenever `name` is supplied.
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in the update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the same form `folders` uses.
//!
//! Unicode case mapping (seam CLOSED 2026-06-30): `nameLower` uses Rust's
//! `str::to_lowercase`, which is **byte-identical** to v4's JS
//! `String.prototype.toLowerCase` — both implement locale-independent Unicode
//! default case mapping, verified to agree even on the gnarly cases (İ → `i` +
//! combining dot, final Σ → ς, ß, titlecase digraphs). The tier-2 corpus carries
//! a non-ASCII tag (`İSTANBUL ÉCOLE ΣΟΦΟΣ Straße`) that proves it against the
//! oracle, so `findByName`'s case-insensitive lookup stays correct on real data.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::DbError;

/// A tag's visual style — the optional `visualStyle` JSON column. Field order
/// here is the schema definition order in v4's `TagVisualStyleSchema`
/// (`common.types.ts`); `serde_json` serializes struct fields in declaration
/// order, so `to_string` reproduces v4's `JSON.stringify(zodParsed)` exactly.
///
/// The pilot supplies fully-specified styles (every field present), so no Zod
/// inner-default expansion is involved — the stored JSON is the input verbatim.
/// Reproducing `TagVisualStyleSchema`'s own per-field defaults (e.g.
/// `foregroundColor` → `#1f2937`) is deferred until an op needs a partial style.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagVisualStyle {
    /// `null` is emitted explicitly (matches `JSON.stringify`'s `"emoji":null`).
    pub emoji: Option<String>,
    pub foreground_color: String,
    pub background_color: String,
    pub emoji_only: bool,
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
}

/// Fields for creating a tag (the `Omit<Tag,'id'|timestamps>` shape).
pub struct TagCreate {
    pub user_id: String,
    pub name: String,
    /// Explicit `nameLower`; `None`/empty falls back to `name` (v4's `||`).
    pub name_lower: Option<String>,
    /// `None` => v4's `false` default (`typeof … === 'boolean' ? … : false`).
    pub quick_hide: Option<bool>,
    /// `None` => SQL NULL (the `.optional()` column absent).
    pub visual_style: Option<TagVisualStyle>,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A tag update patch. Mirrors v4 `tags.update` over `_update`: when `name` is
/// supplied, `nameLower` is re-derived; `updatedAt` is set explicitly; id and
/// createdAt are preserved. The pilot patches `name` + `quickHide` + `updatedAt`;
/// `visualStyle` patching and the "updatedAt = now when absent" fallback land
/// when an op needs them.
pub struct TagUpdate {
    pub name: Option<String>,
    pub quick_hide: Option<bool>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct TagsRepository<'c> {
    conn: &'c Connection,
}

/// v4: `(data.nameLower || data.name).toLowerCase()`. The `||` treats an empty
/// string as falsy, so an empty `nameLower` also falls back to `name`.
fn derive_name_lower(name: &str, name_lower: &Option<String>) -> String {
    let base = match name_lower {
        Some(s) if !s.is_empty() => s.as_str(),
        _ => name,
    };
    base.to_lowercase()
}

impl<'c> TagsRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a tag with the given pinned id + timestamps.
    pub fn create(&self, data: &TagCreate, opts: &CreateOptions) -> Result<(), DbError> {
        let name_lower = derive_name_lower(&data.name, &data.name_lower);
        // v4: quickHide defaults to false when not a boolean. Stored as 0/1.
        let quick_hide: i64 = i64::from(data.quick_hide.unwrap_or(false));
        // visualStyle: object -> compact JSON text (schema order); None -> NULL.
        let visual_style: Option<String> = match &data.visual_style {
            Some(style) => Some(
                serde_json::to_string(style)
                    .map_err(|e| DbError::Key(format!("visualStyle serialize: {e}")))?,
            ),
            None => None,
        };

        self.conn.execute(
            "INSERT INTO tags \
               (id, userId, name, nameLower, quickHide, visualStyle, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                opts.id,
                data.user_id,
                data.name,
                name_lower,
                quick_hide,
                visual_style,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the tag `id`. Returns `false` when no row matched
    /// (v4's "not found -> null"). id and createdAt are never touched; when
    /// `name` is supplied, `nameLower` is re-derived from it.
    pub fn update(&self, id: &str, patch: &TagUpdate) -> Result<bool, DbError> {
        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(name) = &patch.name {
            assignments.push(format!("name = ?{}", values.len() + 1));
            values.push(Box::new(name.clone()));
            // v4: `if (data.name) updateData.nameLower = data.name.toLowerCase()`.
            assignments.push(format!("nameLower = ?{}", values.len() + 1));
            values.push(Box::new(name.to_lowercase()));
        }
        if let Some(quick_hide) = patch.quick_hide {
            assignments.push(format!("quickHide = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(quick_hide)));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE tags SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the tag `id`. Returns `false` when no row matched (v4's
    /// `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM tags WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }
}
