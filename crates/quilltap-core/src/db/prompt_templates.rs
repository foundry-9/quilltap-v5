//! The prompt-templates repository — the fourth Phase-2 repo port, after
//! `folders`, `tags`, and `text_replacement_rules`. Ports v4's
//! `lib/database/repositories/prompt-templates.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, `delete`, and (since P4.83) the three READS
//! v4's routes call — [`find_all_for_user`], [`find_built_in`] and
//! [`find_by_id`] — plus [`built_in_name_exists`], the lookup v4's
//! `seedSamplePrompts` uses to decide whether a sample prompt is already on
//! file. (The seeding itself is
//! [`crate::services::builtin_prompt_templates`]. The header used to say
//! seeding "is a startup concern, not a CRUD op": that was wrong about v4 —
//! v4 seeds LAZILY, from inside these very reads.)
//! `prompt_templates` uses the plain `AbstractBaseRepository`
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
use serde_json::Value;

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

/// v4 `promptTemplates.findByName(userId, name)` — `findOneByFilter({userId,
/// name})`, exact match, first row. Returns the matched id (the `.qtap`
/// importer's dedupe key and the import preview's `matchedExistingId`).
pub fn find_by_name(
    conn: &Connection,
    user_id: &str,
    name: &str,
) -> Result<Option<String>, DbError> {
    conn.query_row(
        "SELECT id FROM prompt_templates WHERE userId = ?1 AND name = ?2 LIMIT 1",
        params![user_id, name],
        |r| r.get::<_, String>(0),
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other.into()),
    })
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
/// header).
///
/// `name` / `content` / `tags` are NOT NULL columns, so `Some` sets and `None`
/// leaves alone. The three NULLABLE columns are a TRI-STATE (P4.83 closed the
/// former "clearing a nullable column is deferred" gap): `None` leaves the
/// column alone, `Some(None)` sets it to SQL NULL, `Some(Some(v))` sets `v`.
/// v4's PUT route reaches the middle state on every `description: ''` — it
/// assigns `validatedData.description || null`, and an empty string is falsy.
#[derive(Default)]
pub struct PtUpdate {
    pub name: Option<String>,
    pub content: Option<String>,
    pub description: Option<Option<String>>,
    pub category: Option<Option<String>>,
    pub model_hint: Option<Option<String>>,
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
            .map_err(|e| DbError::Internal(format!("tags serialize: {e}")))?;

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
                .map_err(|e| DbError::Internal(format!("tags serialize: {e}")))?;
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

// ===========================================================================
// P4.83: the three READS v4's routes call, plus the seeder's presence lookup.
// ===========================================================================

/// One `prompt_templates` row as v4's READ routes serialize it — the entity in
/// `PromptTemplateSchema`'s key order (`lib/schemas/template.types.ts:270-282`),
/// which is what `NextResponse.json` emits because Zod's object parse rebuilds
/// the value in shape order regardless of the hydrated row's column order
/// (measured 2026-09-07 against v4's real schema, scrambled input included).
///
/// ⚠ **A NULL column is OMITTED from the wire, not emitted as `null`.** v4's
/// SQLite backend hydrates every SQL NULL to `undefined` — `hydrateRow`
/// (`backend.ts:404-451`) does it three times over, each with the same comment,
/// "null → undefined for Zod .optional() compatibility" — and
/// `PromptTemplateSchema`'s four nullable columns are `.nullable().optional()`,
/// so Zod's output simply has no such key. Measured, not read off the schema:
/// `list_first_seeds_21`'s built-ins carry no `userId`, and its user template
/// carries no `description` / `category` / `modelHint`. Hence
/// `skip_serializing_if` on all four. (The work order's §B declared them
/// `string | null`; the wire says otherwise, and the wire wins. The CREATE 201
/// body DOES carry explicit `null`s — v4 validates the input object there
/// rather than re-reading the row — which is why that body is built separately
/// in `api::prompt_templates`.)
///
/// The rest of the marshaling is `hydrateRow`'s too: a numeric column whose name
/// starts with `is` becomes a boolean (NULL → `undefined` → Zod's
/// `.default(false)`), and the registered JSON column `tags` is parsed (NULL or
/// unparseable → `undefined` → Zod's `.default([])`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptTemplateRecord {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    pub name: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub is_built_in: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_hint: Option<String>,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// The `SELECT` every read shares — the column list is spelled out (not `*`) so
/// the positional reads below cannot silently re-map when the D23 re-dump adds a
/// column; the ORDER of the projection is irrelevant to the emitted JSON, which
/// is [`PromptTemplateRecord`]'s field order.
const SELECT_COLUMNS: &str = "SELECT id, userId, name, content, description, isBuiltIn, \
                              category, modelHint, tags, createdAt, updatedAt \
                              FROM prompt_templates";

/// The raw column tuple, before v4's `validateSafe` gate.
struct RawRow {
    id: String,
    user_id: Option<String>,
    name: String,
    content: String,
    description: Option<String>,
    is_built_in: i64,
    category: Option<String>,
    model_hint: Option<String>,
    tags: Option<String>,
    created_at: String,
    updated_at: String,
}

fn raw_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRow> {
    Ok(RawRow {
        id: row.get(0)?,
        user_id: row.get(1)?,
        name: row.get(2)?,
        content: row.get(3)?,
        description: row.get(4)?,
        is_built_in: row.get::<_, Option<i64>>(5)?.unwrap_or(0),
        category: row.get(6)?,
        model_hint: row.get(7)?,
        tags: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

/// v4's list reads run every hydrated row through `validateSafe` and **drop**
/// the ones that fail (`base.repository.ts:272-284` — `.map(validateSafe).
/// filter(Boolean)`), so a stored row that violates `PromptTemplateSchema` is
/// invisible to `GET /api/v1/prompt-templates` rather than an error. This is
/// that gate, over the constraints a persisted row can actually violate:
///
///   - `id` / `userId` / each `tags` entry — `UUIDSchema` (`z.uuid()`);
///   - `name` — `min(1).max(100)`, CODE POINTS (Zod 4.5);
///   - `content` — `min(1)`;
///   - `description` — `max(500)`, code points;
///   - `tags` — must parse to an array of UUID strings. A NULL column, an empty
///     string, the literal `'null'`, or text `JSON.parse` chokes on all reach
///     Zod as `undefined` (v4's `fromJsonSafe` + the null→undefined rule) and so
///     take `z.array().default([])` — `[]`, NOT a dropped row. Only a value that
///     parses to a NON-array (an object, a number) actually fails the schema.
///
/// `isBuiltIn` cannot fail: `hydrateRow` turns the INTEGER column into a
/// boolean before Zod sees it, and a NULL column reads as `false` (v4's
/// `value === 1`). The timestamp columns are `z.iso.datetime()`; that arm is
/// NOT re-validated here — recorded, unmeasured (no corpus case plants a
/// malformed timestamp).
///
/// Measured: `list_drops_schema_invalid_row` plants a 101-code-point name with
/// raw SQL (v4's own repo `create` will not write it) and v4's list comes back
/// one row shorter than the table.
fn validate_safe(raw: RawRow) -> Option<PromptTemplateRecord> {
    use crate::api::settings::zod_uuid_ok;
    use crate::jsstr::{zod_len_max_ok, zod_len_min_ok};

    if !zod_uuid_ok(&raw.id) {
        return None;
    }
    if let Some(uid) = &raw.user_id {
        if !zod_uuid_ok(uid) {
            return None;
        }
    }
    if !zod_len_min_ok(&raw.name, 1) || !zod_len_max_ok(&raw.name, 100) {
        return None;
    }
    if !zod_len_min_ok(&raw.content, 1) {
        return None;
    }
    if let Some(d) = &raw.description {
        if !zod_len_max_ok(d, 500) {
            return None;
        }
    }
    let tags: Vec<String> = match &raw.tags {
        // v4's `fromJsonSafe` answers `null` for an empty string or a parse
        // failure, and `hydrateRow` maps that (and a NULL column) to `undefined`
        // — which Zod's `.default([])` fills in. Only a well-formed non-array
        // reaches the schema and fails it.
        None => Vec::new(),
        Some(text) => match serde_json::from_str::<Value>(text) {
            Ok(Value::Null) | Err(_) => Vec::new(),
            Ok(Value::Array(items)) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(item.as_str()?.to_string());
                }
                out
            }
            Ok(_) => return None,
        },
    };
    if !tags.iter().all(|t| zod_uuid_ok(t)) {
        return None;
    }

    Some(PromptTemplateRecord {
        id: raw.id,
        user_id: raw.user_id,
        name: raw.name,
        content: raw.content,
        description: raw.description,
        is_built_in: raw.is_built_in == 1,
        category: raw.category,
        model_hint: raw.model_hint,
        tags,
        created_at: raw.created_at,
        updated_at: raw.updated_at,
    })
}

/// v4's `prompt_templates` DDL, verbatim from `fresh_schema.json` (v4's live
/// `generateDDL`) with `IF NOT EXISTS`. `generateDDL` emits NO index for this
/// table.
///
/// ⚠ **This exists because v4 creates the table LAZILY and v5 did not.** Every
/// v4 repo op goes through `AbstractBaseRepository.getCollection()`
/// (`base.repository.ts:100-114`), which `ensureCollection`s on first access —
/// so a v4 instance that has never opened the Import-from-Template modal simply
/// has no `prompt_templates` table, and gets one the moment it does. v5
/// provisions the table on a FRESH instance (it is in `fresh_schema.json`) but
/// had nothing for an OLDER one: the P4.83 e2e beat's first live run hit
/// `no such table: prompt_templates` on the shared fixture instance and the
/// catalogue came back empty — the `p4.9i2` `help_docs` gap, exactly
/// ([`crate::db::help_doc_chunks_repair::ensure_help_docs_table`] is the
/// precedent). The ensure runs at v4's OWN site — inside the seeding pass every
/// read triggers — rather than at boot, because that is where v4 does it.
pub const PROMPT_TEMPLATES_TABLE_DDL: &str = r#"CREATE TABLE IF NOT EXISTS "prompt_templates" (
  "id" TEXT PRIMARY KEY NOT NULL,
  "userId" TEXT,
  "name" TEXT NOT NULL,
  "content" TEXT NOT NULL,
  "description" TEXT,
  "isBuiltIn" INTEGER DEFAULT 0,
  "category" TEXT,
  "modelHint" TEXT,
  "tags" TEXT DEFAULT '[]',
  "createdAt" TEXT NOT NULL,
  "updatedAt" TEXT NOT NULL
)"#;

/// Create `prompt_templates` if the main partition lacks it (v4's
/// `ensureCollection`). Idempotent; a no-op on every instance that has one.
pub fn ensure_prompt_templates_table(main: &Connection) -> Result<(), DbError> {
    main.execute_batch(PROMPT_TEMPLATES_TABLE_DDL)?;
    Ok(())
}

/// v4 `promptTemplates.findAllForUser(userId)` (`:221-236`) — `findByFilter({
/// $or: [{isBuiltIn: true}, {userId}] })`, which the SQLite translator renders
/// as `WHERE ("isBuiltIn" = 1 OR "userId" = ?)` with **no `ORDER BY`** (no
/// `sort` option reaches `buildSelectQuery`), so the rows come back in rowid
/// order — insertion order. That is the order the Import-from-Template modal
/// lists in, and `prompt_templates_routes_equivalence` pins it with a case whose
/// built-in and user rows interleave.
///
/// The SEEDING v4 does first is NOT here: it is a write, and the caller
/// ([`crate::services::builtin_prompt_templates::seed_sample_prompts`]) runs it
/// on the writer before this read takes the pool.
pub fn find_all_for_user(
    conn: &Connection,
    user_id: &str,
) -> Result<Vec<PromptTemplateRecord>, DbError> {
    let sql = format!("{SELECT_COLUMNS} WHERE (isBuiltIn = 1 OR userId = ?1)");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![user_id], raw_row)?;
    let mut out = Vec::new();
    for r in rows {
        if let Some(rec) = validate_safe(r?) {
            out.push(rec);
        }
    }
    Ok(out)
}

/// v4 `promptTemplates.findBuiltIn()` (`:204-217`) — `findByFilter({isBuiltIn:
/// true})`, same no-`ORDER BY` rowid order. (v4's own routes do not call it;
/// it is the sibling read the seeding site is shared with, and the `.qtap`
/// exporter's natural entry point.)
pub fn find_built_in(conn: &Connection) -> Result<Vec<PromptTemplateRecord>, DbError> {
    let sql = format!("{SELECT_COLUMNS} WHERE isBuiltIn = 1");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], raw_row)?;
    let mut out = Vec::new();
    for r in rows {
        if let Some(rec) = validate_safe(r?) {
            out.push(rec);
        }
    }
    Ok(out)
}

/// v4 `promptTemplates.findById(id)` (`:155-166`) — `_findById`, which
/// `validate`s (not `validateSafe`) the single row. A row that fails the schema
/// THROWS there and `safeQuery` answers `null`, which is observationally the
/// same as "not found"; [`validate_safe`] returning `None` reproduces that
/// collapse.
pub fn find_by_id(conn: &Connection, id: &str) -> Result<Option<PromptTemplateRecord>, DbError> {
    let sql = format!("{SELECT_COLUMNS} WHERE id = ?1 LIMIT 1");
    let raw = conn
        .query_row(&sql, params![id], raw_row)
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    Ok(raw.and_then(validate_safe))
}

/// v4's seeding lookup — `collection.findOne({name, isBuiltIn: true})`
/// (`prompt-templates.repository.ts:76-79`). The `isBuiltIn` half is
/// load-bearing: a USER template that happens to share a sample prompt's name
/// does NOT suppress the seed, so both rows end up on file.
pub fn built_in_name_exists(conn: &Connection, name: &str) -> Result<bool, DbError> {
    let found: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM prompt_templates WHERE name = ?1 AND isBuiltIn = 1 LIMIT 1",
            params![name],
            |r| r.get::<_, i64>(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    Ok(found.is_some())
}

#[cfg(test)]
mod ensure_table_tests {
    use super::*;

    fn has_table(c: &Connection) -> bool {
        c.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'prompt_templates'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
            == 1
    }

    #[test]
    fn creates_the_missing_table_and_is_idempotent() {
        let c = Connection::open_in_memory().unwrap();
        assert!(!has_table(&c));
        ensure_prompt_templates_table(&c).unwrap();
        assert!(has_table(&c));

        // The shape the reads and the seeder expect, including the two column
        // DEFAULTs a bare INSERT relies on.
        c.execute(
            "INSERT INTO prompt_templates (id, name, content, createdAt, updatedAt) \
             VALUES ('p1', 'n', 'c', 'now', 'now')",
            [],
        )
        .unwrap();
        let (built_in, tags): (i64, String) = c
            .query_row(
                "SELECT isBuiltIn, tags FROM prompt_templates WHERE id = 'p1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(built_in, 0);
        assert_eq!(tags, "[]");

        // Idempotent, and it does not wipe what is already there.
        ensure_prompt_templates_table(&c).unwrap();
        let n: i64 = c
            .query_row("SELECT count(*) FROM prompt_templates", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }
}
