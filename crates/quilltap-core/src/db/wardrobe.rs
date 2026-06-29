//! The wardrobe repository — a Phase-2 repo port over the `wardrobe_items` table.
//! Ports the SQL marshaling of v4's `WardrobeItemSchema` through the base
//! repository internals (`_create`/`_update`/`_delete` in `base.repository.ts`).
//!
//! ## Why this ports the BASE SQL CRUD, not the public overrides
//!
//! v4's `WardrobeRepository` (`lib/database/repositories/wardrobe.repository.ts`)
//! is unusual: its public `create`/`update`/`delete` are **vault-only overrides**
//! that route every mutation into the character's document-store vault (`Wardrobe/
//! *.md`) and *throw* when no vault mount resolves — there is deliberately **no
//! SQL mirror** on the write path. So those overrides cannot be exercised against
//! a bare tier-2 fixture DB (they would throw immediately). What this port
//! reproduces is the **`wardrobe_items` SQL table** itself: the column set/affinity
//! the schema-translator generates from `WardrobeItemSchema`, and the
//! base-repository row marshaling (`documentToRow`/`prepareForStorage`) that still
//! backs the table's reads — `findByCharacterIdRaw` → `findByFilter` (used by the
//! one-time vault populator and the `cutover-characters-to-vault` migration).
//!
//! Scope: the base `_create` / `_update` / `_delete` against `wardrobe_items`.
//! The vault-overlay public methods and the archetype/vault query helpers
//! (`findByCharacterId`, `findArchetypes`, `archive`/`unarchive`, …) are out of
//! scope. The tier-2 oracle drives v4's REAL base repository via a thin test
//! subclass that exposes those protected internals (see the oracle case), so the
//! differential checks v4's actual base-CRUD marshaling for this table.
//!
//! ## What this repo banks for the tier-2 marshaling surface
//!
//! `wardrobe_items` is a clean main-DB repo (plain `AbstractBaseRepository` — its
//! `characterId` is nullable, archetypes have `characterId = null`; not
//! user-owned, not taggable). It banks:
//!
//!   - **two JSON array columns** (`types`, `componentItemIds`). v4's
//!     `prepareForStorage` `JSON.stringify`s an array, so each lands as compact
//!     JSON text (`["top"]`, `[]` when empty). Reproduced with
//!     `serde_json::to_string` of a `Vec<String>`; arrays are order-preserving, so
//!     (unlike the `tags.visualStyle` object) there is no key-order subtlety. This
//!     is the first repo with TWO array columns, and the `types` enum array
//!     (values from `WardrobeItemTypeEnum`) is the first JSON array of enum
//!     strings.
//!   - **two boolean columns** (`isDefault`, `replace`) → INTEGER 0/1 (the
//!     `tags.quickHide` mapping).
//!   - a **nullable timestamp / soft-delete column** (`archivedAt`,
//!     `TimestampSchema.nullable().optional()` → `'date'` affinity → TEXT). `None`
//!     → SQL NULL (active), `Some` → the ISO timestamp (archived). This is the
//!     wardrobe soft-delete (`archive`/`unarchive` set/clear it); the corpus
//!     exercises both an active row and an archive-then-read.
//!   - **several nullable string / UUID columns** (`characterId`,
//!     `description`, `imagePrompt`, `appropriateness`,
//!     `migratedFromClothingRecordId`) → `None` → SQL NULL.
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in the update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form
//! `folders`/`tags`/`text_replacement_rules`/`prompt_templates`/
//! `conversation_annotations`/`image_profiles` use.
//!
//! Deferred (not in the corpus, mirroring `prompt_templates`/`image_profiles`):
//! clearing a nullable column back to NULL via `update` (the update patch models a
//! provided field as "set to this value"; the soft-delete unarchive — which sets
//! `archivedAt` to null — needs the nullable-setter form, so the corpus sets
//! `archivedAt` only via create/non-null update). Zod's `componentItemIds`/
//! `isDefault`/`replace` create defaults are supplied explicitly by the corpus.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;

/// Fields for creating a wardrobe item (the `Omit<WardrobeItem,'id'|timestamps>`
/// shape). `types`/`component_item_ids` are the JSON array columns; the bools
/// land as INTEGER 0/1; the `Option` string/timestamp fields are the nullable
/// columns (`None` → SQL NULL).
pub struct WardrobeCreate {
    /// `None` => SQL NULL (archetype — shared across characters).
    pub character_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub image_prompt: Option<String>,
    /// Coverage tags (enum strings). Stored as compact JSON text (`["top"]`).
    pub types: Vec<String>,
    /// Composite components. Stored as compact JSON text (`[]` when empty/leaf).
    pub component_item_ids: Vec<String>,
    pub appropriateness: Option<String>,
    pub is_default: bool,
    pub replace: bool,
    pub migrated_from_clothing_record_id: Option<String>,
    /// Soft-delete marker. `None` → SQL NULL (active), `Some` → ISO timestamp.
    pub archived_at: Option<String>,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A wardrobe-item update patch. Mirrors v4 `_update`: provided fields overwrite,
/// id and createdAt are preserved, `updatedAt` is set explicitly. Each `Some`
/// field sets that column; `archived_at` is the nullable-setter case the
/// soft-delete archive path needs (v4's `update({archivedAt: now|null})` sets the
/// column to whatever the input carries, including `null`): `None` → leave the
/// column untouched, `Some(inner)` → set it (`Some(None)` writes SQL NULL — the
/// unarchive path; not in the corpus, see header).
#[derive(Default)]
pub struct WardrobeUpdate {
    pub title: Option<String>,
    pub description: Option<String>,
    pub image_prompt: Option<String>,
    /// Re-serialized to compact JSON text when provided.
    pub types: Option<Vec<String>>,
    /// Re-serialized to compact JSON text when provided.
    pub component_item_ids: Option<Vec<String>>,
    pub appropriateness: Option<String>,
    pub is_default: Option<bool>,
    pub replace: Option<bool>,
    /// Outer `Option`: present in the patch or not. Inner `Option`: the column
    /// value (`None` → SQL NULL). See the struct doc.
    pub archived_at: Option<Option<String>>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct WardrobeRepository<'c> {
    conn: &'c Connection,
}

impl<'c> WardrobeRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a wardrobe item with the given pinned id + timestamps. `types` and
    /// `componentItemIds` → compact JSON array text; bools → INTEGER 0/1; the
    /// `Option` columns as themselves (`None` → SQL NULL).
    pub fn create(&self, data: &WardrobeCreate, opts: &CreateOptions) -> Result<(), DbError> {
        let types_json = serde_json::to_string(&data.types)
            .map_err(|e| DbError::Key(format!("types serialize: {e}")))?;
        let component_item_ids_json = serde_json::to_string(&data.component_item_ids)
            .map_err(|e| DbError::Key(format!("componentItemIds serialize: {e}")))?;

        self.conn.execute(
            "INSERT INTO wardrobe_items \
               (id, characterId, title, description, imagePrompt, types, componentItemIds, \
                appropriateness, isDefault, replace, migratedFromClothingRecordId, archivedAt, \
                createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                opts.id,
                data.character_id,
                data.title,
                data.description,
                data.image_prompt,
                types_json,
                component_item_ids_json,
                data.appropriateness,
                i64::from(data.is_default),
                i64::from(data.replace),
                data.migrated_from_clothing_record_id,
                data.archived_at,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the wardrobe item `id`. Returns `Ok(false)` when no
    /// row matched (v4's `_update` "not found -> null"). id and createdAt are never
    /// touched. Each `Some` field sets that column; `updatedAt` is always set.
    pub fn update(&self, id: &str, patch: &WardrobeUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`s — the row must exist or it's a no-op.
        let exists: bool = self
            .conn
            .query_row(
                "SELECT 1 FROM wardrobe_items WHERE id = ?1",
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

        if let Some(title) = &patch.title {
            assignments.push(format!("title = ?{}", values.len() + 1));
            values.push(Box::new(title.clone()));
        }
        if let Some(description) = &patch.description {
            assignments.push(format!("description = ?{}", values.len() + 1));
            values.push(Box::new(description.clone()));
        }
        if let Some(image_prompt) = &patch.image_prompt {
            assignments.push(format!("imagePrompt = ?{}", values.len() + 1));
            values.push(Box::new(image_prompt.clone()));
        }
        if let Some(types) = &patch.types {
            let types_json = serde_json::to_string(types)
                .map_err(|e| DbError::Key(format!("types serialize: {e}")))?;
            assignments.push(format!("types = ?{}", values.len() + 1));
            values.push(Box::new(types_json));
        }
        if let Some(component_item_ids) = &patch.component_item_ids {
            let component_item_ids_json = serde_json::to_string(component_item_ids)
                .map_err(|e| DbError::Key(format!("componentItemIds serialize: {e}")))?;
            assignments.push(format!("componentItemIds = ?{}", values.len() + 1));
            values.push(Box::new(component_item_ids_json));
        }
        if let Some(appropriateness) = &patch.appropriateness {
            assignments.push(format!("appropriateness = ?{}", values.len() + 1));
            values.push(Box::new(appropriateness.clone()));
        }
        if let Some(is_default) = patch.is_default {
            assignments.push(format!("isDefault = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(is_default)));
        }
        if let Some(replace) = patch.replace {
            assignments.push(format!("replace = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(replace)));
        }
        if let Some(archived_at) = &patch.archived_at {
            assignments.push(format!("archivedAt = ?{}", values.len() + 1));
            // Inner `Option<String>` binds the value (or SQL NULL).
            values.push(Box::new(archived_at.clone()));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE wardrobe_items SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the wardrobe item `id`. Returns `false` when no row matched (v4's
    /// `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM wardrobe_items WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }
}
