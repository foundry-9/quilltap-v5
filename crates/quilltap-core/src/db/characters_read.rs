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
//! `scenarios` / `systemPrompts` / `aliases` → `[]`, `talkativeness` → `0.5`,
//! `canChooseOutfit` → `false`, and
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

use std::collections::{HashMap, HashSet};

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

/// The three archive columns (v4 `d553f72a`, migration
/// `add-character-archive-fields-v1`), selected AFTER [`SLIM_COLUMNS`].
const ARCHIVE_COLUMNS: [&str; 3] = ["archivedAt", "archiveFileId", "archivedAvatarFileId"];

/// The full `SELECT` list: [`SLIM_COLUMNS`] plus the three archive columns —
/// or three literal `NULL`s when the table predates them.
///
/// v4 reads every collection with `SELECT *`, so a `characters` table that
/// predates `add-character-archive-fields-v1` simply returns no archive
/// columns and their `.nullable().optional()` schema keys parse as absent. v5
/// names its columns, so it has to ask the table first — otherwise every
/// pre-migration instance (and every differential fixture built before the
/// drift) would fail the read outright with `no such column`. Selecting
/// `NULL` rather than dropping the columns keeps the marshaling arity fixed,
/// and a NULL cell marshals to the same absent key v4's Zod produces. This is
/// the P4.D49 llm-logs tolerance, one shape tighter.
///
/// The WRITE side stays strict, exactly as v4's is: its `insertOne`/`update`
/// name the document's keys, so v4 cannot write an archive field into a
/// pre-migration table either. The boot ensure
/// ([`crate::db::character_archive_repair`]) is what gets a real instance the
/// columns; this is what keeps old fixtures readable.
fn select_columns(conn: &Connection) -> String {
    let mut list = String::from(SLIM_COLUMNS);
    // Probe PER COLUMN, not just the first: v4's migration (and v5's own boot
    // repair) guard each ALTER individually, so a table interrupted between
    // ALTERs can carry `archivedAt` without `archiveFileId` — a single-column
    // probe would then name a missing column and break every read (the round-1
    // unification review's finding 4).
    for col in ARCHIVE_COLUMNS {
        list.push_str(", ");
        if table_has_column(conn, col) {
            list.push_str(col);
        } else {
            list.push_str("NULL");
        }
    }
    list
}

/// `PRAGMA table_info(characters)` membership. Fail-soft: an unreadable pragma
/// (missing table) reports "absent", which lands the caller on the same
/// no-archive-columns path v4 takes.
fn table_has_column(conn: &Connection, column: &str) -> bool {
    conn.prepare("SELECT 1 FROM pragma_table_info('characters') WHERE name = ?1")
        .and_then(|mut stmt| stmt.exists([column]))
        .unwrap_or(false)
}

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
    // The three archive columns (v4 `d553f72a`). All
    // `.nullable().optional()`: a NULL cell — whether the row's or the literal
    // `NULL` [`select_columns`] substitutes on a pre-migration table — omits
    // the key, exactly as v4's Zod parse does.
    put_opt_string(&mut obj, "archivedAt", row.get(28)?);
    put_opt_string(&mut obj, "archiveFileId", row.get(29)?);
    put_opt_string(&mut obj, "archivedAvatarFileId", row.get(30)?);

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
    obj.insert("canChooseOutfit".into(), Value::Bool(false));

    Ok(Value::Object(obj))
}

/// Run a `WHERE`-clause query over `characters` and marshal each row (no overlay).
fn query_raw(
    conn: &Connection,
    where_clause: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Vec<Value>, DbError> {
    let columns = select_columns(conn);
    let sql = if where_clause.is_empty() {
        format!("SELECT {columns} FROM characters")
    } else {
        format!("SELECT {columns} FROM characters WHERE {where_clause}")
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
        // The absent-keystone read refusal survives structurally (P4.23), so
        // the api layer answers v4's contextful 503 (v4's middleware maps
        // `CharacterVaultUnavailableError` the same way) instead of a 500.
        Err(OverlayOneError::Unavailable(u)) => Err(DbError::StoreUnavailable {
            entity_label: "character",
            id: u.character_id.clone(),
            message: format!(
                "applyDocumentStoreOverlayOne: vault unavailable for character {} (mount {})",
                u.character_id, u.mount_id
            ),
        }),
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

/// Find all characters **without** the vault overlay (v4 `findAllRaw` → `_findAll`):
/// slim rows only, managed columns at their Zod defaults. `self_inventory`'s Carina
/// section uses this so one broken character vault can't sink the whole answerer
/// listing (it reads only `id` / `name` / `canBeCarina`, all slim columns).
pub fn find_all_raw(main: &Connection) -> Result<Vec<Value>, DbError> {
    query_raw(main, "", &[])
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

/// Resolve character ids to display names — **without the vault overlay** (v4
/// `d883a5ee1` / bug 122, `CharactersRepository.findNamesByIds`).
///
/// `name` is a plain slim column, so the overlay has nothing to add here, and
/// skipping it is the point: this runs on the per-turn context path (the
/// memory-subject prefix), where a character whose vault is unreadable must
/// cost the caller a *name*, not the whole turn — [`find_by_id`] answers
/// `StoreUnavailable` on that shelf by design, and [`find_by_ids`] silently
/// drops the row. **The signature is the proof:** there is no mount
/// connection to reach a vault with. Ids with no row, or with a blank name,
/// are simply absent from the map; callers degrade rather than assume a hit.
///
/// Ids are deduped and blanks dropped first (v4 filters `typeof id ===
/// 'string' && id.length > 0` into a `Set`); an empty list queries nothing.
/// A read failure logs v4's `Error resolving character names` and yields an
/// EMPTY map rather than an `Err` — v4 wraps the body in `safeQuery(…, new
/// Map())`, and this port keeps the fail-soft leg because the turn is what it
/// protects. (v4's `withStrictRepositoryFailures` scope, which suspends that
/// fallback, is not on this path.)
pub fn find_names_by_ids(main: &Connection, ids: &[String]) -> HashMap<String, String> {
    let mut unique: Vec<String> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for id in ids {
        if !id.is_empty() && seen.insert(id.as_str()) {
            unique.push(id.clone());
        }
    }
    if unique.is_empty() {
        return HashMap::new();
    }

    let placeholders = (1..=unique.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let params: Vec<&dyn rusqlite::ToSql> =
        unique.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    let rows = match query_raw(main, &format!("id IN ({placeholders})"), &params) {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(
                target: "quilltap::memory",
                collection = "characters",
                count = unique.len(),
                error = %e,
                "Error resolving character names"
            );
            return HashMap::new();
        }
    };

    let mut names = HashMap::new();
    for row in rows {
        // v4 reads the hydrated row: `typeof row.name === 'string' ? row.name.trim() : ''`,
        // and keeps the pair only when the id is truthy and the trimmed name non-empty.
        let obj = match row.as_object() {
            Some(o) => o,
            None => continue,
        };
        let id = match obj.get("id").and_then(Value::as_str) {
            Some(s) if !s.is_empty() => s,
            _ => continue,
        };
        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .map(crate::jsstr::js_trim)
            .unwrap_or("");
        if !name.is_empty() {
            names.insert(id.to_string(), name.to_string());
        }
    }
    names
}

/// Count characters whose `defaultImageId` equals `image_id` — the length of v4's
/// `findByDefaultImageId(image_id)` result, WITHOUT the vault overlay (the sweep's
/// `skipReason` only consults `.length > 0`, and the overlay never adds or removes
/// rows, only enriches them, so the raw count is length-equivalent). Main-DB only.
pub fn count_by_default_image_id(main: &Connection, image_id: &str) -> Result<i64, DbError> {
    Ok(main.query_row(
        "SELECT COUNT(*) FROM characters WHERE defaultImageId = ?1",
        [image_id],
        |r| r.get::<_, i64>(0),
    )?)
}

/// Count characters whose `avatarOverrides` reference `image_id` — the length of
/// v4's `findByAvatarOverrideImageId(image_id)` result (see
/// [`count_by_default_image_id`] for the overlay-free rationale). Main-DB only.
pub fn count_by_avatar_override_image_id(
    main: &Connection,
    image_id: &str,
) -> Result<i64, DbError> {
    Ok(main.query_row(
        "SELECT COUNT(*) FROM characters \
         WHERE EXISTS (SELECT 1 FROM json_each(avatarOverrides) \
             WHERE json_extract(value, '$.imageId') = ?1)",
        [image_id],
        |r| r.get::<_, i64>(0),
    )?)
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

/// Scoped read of a character's `coreWhisperEnabled` override (v4
/// `character.coreWhisperEnabled`, nullable) — the per-character input to
/// `resolveCoreWhisperConfig`. `Some(None)` = column NULL; `Ok(None)` = missing
/// row. Reads the MAIN slim column directly (no vault overlay — `coreWhisperEnabled`
/// is not a managed field). Used by the build_context Core-whisper feeder (W4.6a).
pub fn find_core_whisper_enabled(
    main: &rusqlite::Connection,
    id: &str,
) -> Result<Option<Option<bool>>, DbError> {
    main.query_row(
        "SELECT coreWhisperEnabled FROM characters WHERE id = ?1",
        rusqlite::params![id],
        |row| {
            let v: Option<i64> = row.get(0)?;
            Ok(v.map(|n| n != 0))
        },
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other.into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    /// A `characters` table at the post-`d553f72a` shape, plus the vault link
    /// column the overlay keys on.
    fn main_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE characters (id TEXT PRIMARY KEY NOT NULL, userId TEXT NOT NULL, \
             name TEXT NOT NULL, defaultImageId TEXT, defaultConnectionProfileId TEXT, \
             defaultPartnerId TEXT, defaultRoleplayTemplateId TEXT, defaultImageProfileId TEXT, \
             sillyTavernData TEXT, isFavorite INTEGER, npc INTEGER, controlledBy TEXT, \
             defaultAgentModeEnabled INTEGER, defaultHelpToolsEnabled INTEGER, \
             defaultTimestampConfig TEXT, defaultScenarioId TEXT, defaultSystemPromptId TEXT, \
             characterDocumentMountPointId TEXT, canDressThemselves INTEGER, \
             canCreateOutfits INTEGER, systemTransparency TEXT, coreWhisperEnabled INTEGER, \
             canBeCarina INTEGER, partnerLinks TEXT, tags TEXT, avatarOverrides TEXT, \
             createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL, archivedAt TEXT, \
             archiveFileId TEXT, archivedAvatarFileId TEXT);",
        )
        .unwrap();
        conn
    }

    fn insert(conn: &Connection, id: &str, name: &str, mount: Option<&str>) {
        conn.execute(
            "INSERT INTO characters (id, userId, name, characterDocumentMountPointId, \
             partnerLinks, tags, avatarOverrides, createdAt, updatedAt) \
             VALUES (?1, 'u1', ?2, ?3, '[]', '[]', '[]', \
             '2026-09-05T00:00:00.000Z', '2026-09-05T00:00:00.000Z')",
            params![id, name, mount],
        )
        .unwrap();
    }

    /// The mount-index side with the three overlay tables present but EMPTY:
    /// a linked character's `properties.json` keystone is missing, so its vault
    /// is unavailable — the shelf v4's `findById` throws
    /// `CharacterVaultUnavailableError` from and `findByIds` drops.
    fn unreadable_mount() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE doc_mount_files (id TEXT PRIMARY KEY NOT NULL, fileType TEXT);
             CREATE TABLE doc_mount_documents (id TEXT PRIMARY KEY NOT NULL, \
                fileId TEXT NOT NULL, content TEXT, createdAt TEXT, updatedAt TEXT);
             CREATE TABLE doc_mount_file_links (id TEXT PRIMARY KEY NOT NULL, \
                fileId TEXT NOT NULL, mountPointId TEXT NOT NULL, relativePath TEXT NOT NULL, \
                fileName TEXT, lastModified TEXT);",
        )
        .unwrap();
        conn
    }

    /// The proof that the name lookup does not go through the vault: the SAME
    /// row, on the SAME pair of connections, is dropped by the overlaid
    /// `find_by_ids` and still resolved by `find_names_by_ids`. Without the
    /// first assertion the second would be vacuous — it is what establishes
    /// that this vault really is unreadable.
    #[test]
    fn names_resolve_through_an_unreadable_vault() {
        let main = main_db();
        insert(&main, "c-marion", "Marion", Some("mount-1"));
        let mount = unreadable_mount();

        let overlaid = find_by_ids(&main, &mount, &["c-marion".to_string()]).unwrap();
        assert!(
            overlaid.is_empty(),
            "the overlaid read must drop this row — otherwise the vault is readable \
             and the test below proves nothing"
        );

        let names = find_names_by_ids(&main, &["c-marion".to_string()]);
        assert_eq!(names.get("c-marion").map(String::as_str), Some("Marion"));
    }

    #[test]
    fn empty_and_blank_ids_query_nothing() {
        // A table-less connection: any query at all would error (and fail soft
        // to an empty map, which would look identical) — so build the main DB
        // and assert on the ids instead.
        let main = main_db();
        assert!(find_names_by_ids(&main, &[]).is_empty());
        assert!(find_names_by_ids(&main, &[String::new()]).is_empty());
    }

    #[test]
    fn dedupes_ids_and_skips_missing_rows_and_blank_names() {
        let main = main_db();
        insert(&main, "c1", "  Ada  ", None);
        insert(&main, "c2", "   ", None);

        let names = find_names_by_ids(
            &main,
            &[
                "c1".to_string(),
                "c1".to_string(),
                "c2".to_string(),
                "c-absent".to_string(),
            ],
        );
        // Trimmed (v4 `.trim()`), blank-after-trim dropped, absent id absent.
        assert_eq!(names.len(), 1);
        assert_eq!(names.get("c1").map(String::as_str), Some("Ada"));
    }

    /// A read failure yields an empty map, never an `Err` up the turn (v4's
    /// `safeQuery(..., new Map())`).
    #[test]
    fn a_read_failure_is_an_empty_map() {
        let main = Connection::open_in_memory().unwrap(); // no `characters` table
        assert!(find_names_by_ids(&main, &["c1".to_string()]).is_empty());
    }
}
