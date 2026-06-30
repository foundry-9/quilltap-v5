//! The characters **read path** (characters sub-unit 4c). Ports the slim-row read
//! marshaling (the inverse of sub-unit 2's write marshaling) + the `findBy*`
//! queries of v4's `lib/database/repositories/characters.repository.ts`, each
//! overlaying the character vault.
//!
//! ## The marshaling: row → `Character` (v4 `_findById` = hydrateRow + Zod parse)
//!
//! v4 reads a row through `SQLiteCollection.hydrateRow` (parse JSON columns,
//! coerce boolean columns from INTEGER 0/1, `NULL` → `undefined`) then
//! `schema.parse` (apply `.default(...)`, drop `undefined`/optional keys). The net
//! result for the **slim** (non-managed) columns:
//!
//!   - required strings (`id` / `userId` / `name` / `createdAt` / `updatedAt`):
//!     present.
//!   - `*.nullable().optional()` TEXT/UUID/JSON columns: a `NULL` cell → the key
//!     is **omitted** (v4 emits `undefined`, which `JSON.stringify` drops); a
//!     non-null cell → present (JSON columns parsed).
//!   - `.default(false)` booleans (`isFavorite` / `npc`): present (INTEGER → bool).
//!   - `.nullable().optional()` booleans (`defaultAgentModeEnabled` … `canBeCarina`):
//!     `NULL` → omitted; `0`/`1` → bool.
//!   - `.default([])` arrays (`partnerLinks` / `tags` / `avatarOverrides`): present
//!     (parsed; `NULL`/empty → `[]`).
//!   - `controlledBy` enum `.default('llm')`: present.
//!
//! The **managed** columns (`MANAGED_FIELDS`) sit at their DDL defaults (the 4.6
//! cutover writes never touch them), so `_findById` reads back their Zod defaults:
//! `scenarios` / `systemPrompts` / `aliases` → `[]`, `talkativeness` → `0.5`, and
//! the nullable managed columns (`title` / `identity` / … / `pronouns` /
//! `physicalDescription`) → omitted. We reproduce those four defaults directly
//! (the columns provably hold nothing else). For a vault-linked character the read
//! overlay then OVERWRITES every managed field from the vault; the marshaled
//! managed defaults are what `findByIdRaw` (no overlay) returns and the seed the
//! overlay builds on.
//!
//! Comparison in the differential is over `serde_json::Value` (key-order
//! independent), so JSON-object columns are parsed straight into `Value` here — the
//! write-side typed-struct key-order discipline does not apply to the read path.
//!
//! ## The queries
//!
//! `find_by_id` / `find_by_id_raw` / `find_all` / `find_by_user_id` /
//! `find_user_controlled` / `find_llm_controlled` / `find_by_ids` /
//! `find_by_default_image_id` / `find_by_avatar_override_image_id` / `find_by_tag`.
//! Each (except the `…_raw` variant) overlays the vault via
//! [`apply_document_store_overlay`] (batched) / [`apply_document_store_overlay_one`].
//! The JSON-array filters (`tags`, `avatarOverrides.imageId`) use SQLite
//! `json_each` — the same selection v4's query translator emits.

use rusqlite::{Connection, Row};
use serde_json::{Map, Value};

use super::doc_mount_documents::DocMountDocumentsRepository;
use super::vault_read_overlay::{
    apply_document_store_overlay, apply_document_store_overlay_one, OverlayOneError,
};
use super::DbError;

/// The slim column list, in marshaling order (managed columns excluded — they hold
/// DDL defaults reproduced separately).
const SLIM_COLUMNS: &str = "id, userId, name, defaultImageId, defaultConnectionProfileId, \
     defaultPartnerId, defaultRoleplayTemplateId, defaultImageProfileId, sillyTavernData, \
     isFavorite, npc, controlledBy, defaultAgentModeEnabled, defaultHelpToolsEnabled, \
     defaultTimestampConfig, defaultScenarioId, defaultSystemPromptId, \
     characterDocumentMountPointId, canDressThemselves, canCreateOutfits, systemTransparency, \
     coreWhisperEnabled, canBeCarina, partnerLinks, tags, avatarOverrides, createdAt, updatedAt";

/// Insert a nullable-optional TEXT/UUID value: `Some` → string, `None` → omit.
fn put_opt_string(obj: &mut Map<String, Value>, key: &str, v: Option<String>) {
    if let Some(s) = v {
        obj.insert(key.to_string(), Value::String(s));
    }
}

/// Insert a nullable-optional boolean column (`NULL` → omit, `0`/`1` → bool).
fn put_opt_bool(obj: &mut Map<String, Value>, key: &str, v: Option<i64>) {
    if let Some(n) = v {
        obj.insert(key.to_string(), Value::Bool(n == 1));
    }
}

/// Insert a nullable-optional JSON column (`NULL`/empty/`"null"` → omit, else
/// parsed — v4 `fromJsonSafe` + the `.optional()` drop). A non-empty cell that
/// fails to parse is also dropped (v4 logs + uses the default).
fn put_opt_json(obj: &mut Map<String, Value>, key: &str, v: Option<String>) {
    let Some(raw) = v else { return };
    if raw.is_empty() || raw == "null" {
        return;
    }
    if let Ok(parsed) = serde_json::from_str::<Value>(&raw) {
        if !parsed.is_null() {
            obj.insert(key.to_string(), parsed);
        }
    }
}

/// A `.default([])` array column: parsed array, or `[]` when `NULL`/empty/invalid.
fn array_or_empty(v: Option<String>) -> Value {
    v.as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .filter(Value::is_array)
        .unwrap_or_else(|| Value::Array(Vec::new()))
}

/// Marshal one `characters` slim row into a `Character` JSON object (v4 `_findById`
/// = hydrateRow + Zod parse over the slim columns + the managed Zod defaults).
fn marshal_row(row: &Row) -> Result<Value, rusqlite::Error> {
    let mut obj = Map::new();

    obj.insert("id".into(), Value::String(row.get::<_, String>(0)?));
    obj.insert("userId".into(), Value::String(row.get::<_, String>(1)?));
    obj.insert("name".into(), Value::String(row.get::<_, String>(2)?));
    put_opt_string(&mut obj, "defaultImageId", row.get(3)?);
    put_opt_string(&mut obj, "defaultConnectionProfileId", row.get(4)?);
    put_opt_string(&mut obj, "defaultPartnerId", row.get(5)?);
    put_opt_string(&mut obj, "defaultRoleplayTemplateId", row.get(6)?);
    put_opt_string(&mut obj, "defaultImageProfileId", row.get(7)?);
    put_opt_json(&mut obj, "sillyTavernData", row.get(8)?);
    // `.default(false)` booleans (NOT NULL DEFAULT 0; Option guards a stray NULL).
    obj.insert(
        "isFavorite".into(),
        Value::Bool(row.get::<_, Option<i64>>(9)?.unwrap_or(0) == 1),
    );
    obj.insert(
        "npc".into(),
        Value::Bool(row.get::<_, Option<i64>>(10)?.unwrap_or(0) == 1),
    );
    obj.insert(
        "controlledBy".into(),
        Value::String(
            row.get::<_, Option<String>>(11)?
                .unwrap_or_else(|| "llm".to_string()),
        ),
    );
    put_opt_bool(&mut obj, "defaultAgentModeEnabled", row.get(12)?);
    put_opt_bool(&mut obj, "defaultHelpToolsEnabled", row.get(13)?);
    put_opt_json(&mut obj, "defaultTimestampConfig", row.get(14)?);
    put_opt_string(&mut obj, "defaultScenarioId", row.get(15)?);
    put_opt_string(&mut obj, "defaultSystemPromptId", row.get(16)?);
    put_opt_string(&mut obj, "characterDocumentMountPointId", row.get(17)?);
    put_opt_bool(&mut obj, "canDressThemselves", row.get(18)?);
    put_opt_bool(&mut obj, "canCreateOutfits", row.get(19)?);
    put_opt_bool(&mut obj, "systemTransparency", row.get(20)?);
    put_opt_bool(&mut obj, "coreWhisperEnabled", row.get(21)?);
    put_opt_bool(&mut obj, "canBeCarina", row.get(22)?);
    obj.insert("partnerLinks".into(), array_or_empty(row.get(23)?));
    obj.insert("tags".into(), array_or_empty(row.get(24)?));
    obj.insert("avatarOverrides".into(), array_or_empty(row.get(25)?));
    obj.insert("createdAt".into(), Value::String(row.get::<_, String>(26)?));
    obj.insert("updatedAt".into(), Value::String(row.get::<_, String>(27)?));

    // Managed columns sit at their DDL = Zod defaults (writes strip them); reproduce
    // the materialized ones. The nullable managed fields (title/identity/…/pronouns/
    // physicalDescription) read back `undefined` → omitted. For a vault-linked
    // character the overlay overwrites all of these.
    obj.insert("scenarios".into(), Value::Array(Vec::new()));
    obj.insert("systemPrompts".into(), Value::Array(Vec::new()));
    obj.insert("aliases".into(), Value::Array(Vec::new()));
    obj.insert(
        "talkativeness".into(),
        Value::Number(serde_json::Number::from_f64(0.5).expect("0.5 is finite")),
    );

    Ok(Value::Object(obj))
}

/// Run a `WHERE`-clause query over `characters` and marshal each row (no overlay).
fn query_raw(
    conn: &Connection,
    where_clause: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Vec<Value>, DbError> {
    let sql = if where_clause.is_empty() {
        format!("SELECT {SLIM_COLUMNS} FROM characters")
    } else {
        format!("SELECT {SLIM_COLUMNS} FROM characters WHERE {where_clause}")
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params, marshal_row)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Apply the batched vault read overlay to a marshaled list (v4
/// `applyDocumentStoreOverlay`).
fn overlay_many(mount: &Connection, characters: Vec<Value>) -> Result<Vec<Value>, DbError> {
    let repo = DocMountDocumentsRepository::new(mount);
    apply_document_store_overlay(&repo, characters)
}

// ============================================================================
// findBy* queries (each overlays the vault unless named `_raw`)
// ============================================================================

/// Find a character by id, overlaid (v4 `findById`). `None` when absent; errors if
/// the linked vault is unavailable (missing `properties.json` keystone).
pub fn find_by_id(
    main: &Connection,
    mount: &Connection,
    id: &str,
) -> Result<Option<Value>, DbError> {
    let mut rows = query_raw(main, "id = ?1", &[&id])?;
    let raw = rows.pop();
    let repo = DocMountDocumentsRepository::new(mount);
    match apply_document_store_overlay_one(&repo, raw) {
        Ok(v) => Ok(v),
        Err(OverlayOneError::Db(e)) => Err(e),
        Err(OverlayOneError::Unavailable(u)) => Err(DbError::Key(format!(
            "applyDocumentStoreOverlayOne: vault unavailable for character {} (mount {})",
            u.character_id, u.mount_id
        ))),
    }
}

/// Find a character by id **without** the vault overlay (v4 `findByIdRaw`): the
/// managed fields are at their Zod defaults. Reserved for backfills/migrations and
/// the overlay's own bootstrap.
pub fn find_by_id_raw(main: &Connection, id: &str) -> Result<Option<Value>, DbError> {
    Ok(query_raw(main, "id = ?1", &[&id])?.pop())
}

/// Find all characters, overlaid (v4 `findAll`). A character whose vault is
/// unavailable is dropped.
pub fn find_all(main: &Connection, mount: &Connection) -> Result<Vec<Value>, DbError> {
    overlay_many(mount, query_raw(main, "", &[])?)
}

/// Find characters by user id, overlaid (v4 `findByUserId`).
pub fn find_by_user_id(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
) -> Result<Vec<Value>, DbError> {
    overlay_many(mount, query_raw(main, "userId = ?1", &[&user_id])?)
}

/// Find user-controlled characters for a user (v4 `findUserControlled`).
pub fn find_user_controlled(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
) -> Result<Vec<Value>, DbError> {
    overlay_many(
        mount,
        query_raw(main, "userId = ?1 AND controlledBy = 'user'", &[&user_id])?,
    )
}

/// Find LLM-controlled characters for a user (v4 `findLLMControlled` — `controlledBy
/// = 'llm'` OR unset/NULL, which defaults to llm).
pub fn find_llm_controlled(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
) -> Result<Vec<Value>, DbError> {
    overlay_many(
        mount,
        query_raw(
            main,
            "userId = ?1 AND (controlledBy = 'llm' OR controlledBy IS NULL)",
            &[&user_id],
        )?,
    )
}

/// Find characters by a set of ids, overlaid (v4 `findByIds`). Empty input → `[]`.
pub fn find_by_ids(
    main: &Connection,
    mount: &Connection,
    ids: &[String],
) -> Result<Vec<Value>, DbError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = (1..=ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let params: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    overlay_many(
        mount,
        query_raw(main, &format!("id IN ({placeholders})"), &params)?,
    )
}

/// Find characters using an image as their default (v4 `findByDefaultImageId`).
pub fn find_by_default_image_id(
    main: &Connection,
    mount: &Connection,
    image_id: &str,
) -> Result<Vec<Value>, DbError> {
    overlay_many(mount, query_raw(main, "defaultImageId = ?1", &[&image_id])?)
}

/// Find characters whose `avatarOverrides` reference an image (v4
/// `findByAvatarOverrideImageId` — the `avatarOverrides.imageId` nested match,
/// `json_each` + `json_extract`).
pub fn find_by_avatar_override_image_id(
    main: &Connection,
    mount: &Connection,
    image_id: &str,
) -> Result<Vec<Value>, DbError> {
    overlay_many(
        mount,
        query_raw(
            main,
            "EXISTS (SELECT 1 FROM json_each(avatarOverrides) \
                 WHERE json_extract(value, '$.imageId') = ?1)",
            &[&image_id],
        )?,
    )
}

/// Find characters carrying a tag (v4 `findByTag` — `tags` array contains, via
/// `json_each`).
pub fn find_by_tag(
    main: &Connection,
    mount: &Connection,
    tag_id: &str,
) -> Result<Vec<Value>, DbError> {
    overlay_many(
        mount,
        query_raw(
            main,
            "EXISTS (SELECT 1 FROM json_each(tags) WHERE value = ?1)",
            &[&tag_id],
        )?,
    )
}
