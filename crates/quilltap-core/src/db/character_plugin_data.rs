//! The character-plugin-data repository — a Phase-2 repo port, after `folders`,
//! `tags`, `text_replacement_rules`, `prompt_templates`,
//! `conversation_annotations`, `provider_models`, `help_docs`,
//! `roleplay_templates`, `image_profiles`, and `connection_profiles`.
//! Ports v4's `lib/database/repositories/character-plugin-data.repository.ts`
//! (+ the `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, `delete` (the three abstract methods over the base
//! repo) plus `upsert`. The remaining custom query helpers (`findByCharacterId`,
//! `findByPluginName`, `getPluginDataMap`, …) and the `deleteBy*` bulk deletes
//! are out of scope; the private `find_id_by_character_and_plugin` here is the
//! narrowed `findByCharacterAndPlugin` `upsert` needs. There is **no built-in
//! guard** (unlike `prompt_templates`).
//!
//! `upsert(characterId, pluginName, data)` mints its own id / `now` internally
//! (its `create`/`update` calls go through the two pinned methods here, but the
//! orchestration is the remap-normalization form, not the pinned
//! zero-normalization form), so it has a SEPARATE minted-values tier-2 case
//! (`character_plugin_data_upsert_tier2_equivalence`).
//!
//! ## What this repo banks for the tier-2 marshaling surface
//!
//! `character_plugin_data` extends v4's plain `AbstractBaseRepository`, so it has
//! no Taggable/`userId` lineage. Its distinctive column is **`data`** — the
//! schema field is `z.unknown()`, i.e. an **open / arbitrary-JSON VALUE column**.
//! A non-null, non-Buffer, non-Date object/array value is stored by v4's
//! `prepareForStorage` as compact JSON text (`shouldStoreAsJson` → `toJson` =
//! `JSON.stringify`) — but a **string or number is stored verbatim**, which is
//! what a `.qtap` round-trip depends on (the export carries the column's raw
//! TEXT, and re-importing must not add a second layer of quoting). It is modeled
//! here as a [`serde_json::Value`] and stored through the same rule. The rest of
//! the row is plain strings
//! (`characterId`, `pluginName`) plus the timestamps — no booleans, no numbers,
//! no nullable columns. (This is the same `data`-as-open-JSON shape `image_profiles`
//! banked for its `parameters` column.)
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in the update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form
//! `folders` / `tags` / … / `image_profiles` use.
//!
//! The `data` open-JSON column is key-order-faithful: this crate enables
//! serde_json's `preserve_order`, so a `Value::Object` is an `IndexMap` emitting
//! INSERTION order, exactly as v4's `JSON.stringify` does. (This was once tracked
//! as deferred seam #5 — "open-JSON multi-key key order" — on the assumption that
//! `Value` sorts its keys; `preserve_order` closed it, here and in the same
//! family's `image_profiles.rs` / `connection_profiles.rs`.) The corpus stays
//! constrained to `{}` or single-key objects because nothing needs more (a
//! single-key fractional-number object exercises the float pass-through inside
//! the JSON text); a multi-key `data` op would marshal correctly.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;

/// Fields for creating a character-plugin-data entry (the
/// `Omit<CharacterPluginData,'id'|timestamps>` shape). `data` is the open-JSON
/// VALUE column (bound as compact JSON text); the other two are plain strings.
pub struct CpdCreate {
    pub character_id: String,
    pub plugin_name: String,
    /// Open/arbitrary JSON value (`{}` or a single-key object — see the module
    /// header's multi-key key-order deferral). Stored via `serde_json::to_string`.
    pub data: serde_json::Value,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A character-plugin-data update patch. Mirrors v4 `update` over `_update`:
/// provided fields overwrite, id and createdAt are preserved (v4 spreads
/// `existing` then `data`, forcing `id`/`createdAt` back; we never touch them),
/// `updatedAt` is set explicitly. Each `Some` field sets that column.
#[derive(Default)]
pub struct CpdUpdate {
    pub character_id: Option<String>,
    pub plugin_name: Option<String>,
    /// Re-serialized to compact JSON text when provided.
    pub data: Option<serde_json::Value>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct CharacterPluginDataRepository<'c> {
    conn: &'c Connection,
}

/// Marshal one `character_plugin_data` row → the v4-shaped entity
/// `{id, characterId, pluginName, data, createdAt, updatedAt}` (with `data`
/// parsed back from its JSON-text column).
fn marshal_row(r: &rusqlite::Row) -> Result<serde_json::Value, rusqlite::Error> {
    let id: String = r.get(0)?;
    let character_id: String = r.get(1)?;
    let plugin_name: String = r.get(2)?;
    let data_json: String = r.get(3)?;
    let created_at: String = r.get(4)?;
    let updated_at: String = r.get(5)?;
    // v4 keeps `data` as its stored TEXT value — it is NOT re-parsed on read
    // (the schema column is a plain string), so the marshaled entity carries the
    // raw JSON string, not a parsed object. Confirmed against the oracle.
    Ok(serde_json::json!({
        "id": id,
        "characterId": character_id,
        "pluginName": plugin_name,
        "data": data_json,
        "createdAt": created_at,
        "updatedAt": updated_at,
    }))
}

/// v4 `findByCharacterId` — every plugin-data entry for a character (insertion
/// order). Each entry's `data` is parsed back from its JSON-text column.
pub fn find_by_character_id(
    conn: &Connection,
    character_id: &str,
) -> Result<Vec<serde_json::Value>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, characterId, pluginName, data, createdAt, updatedAt \
         FROM character_plugin_data WHERE characterId = ?1 ORDER BY createdAt ASC, id ASC",
    )?;
    let rows = stmt.query_map(params![character_id], marshal_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// v4 `getPluginDataMap` — `{ pluginName: data }` over the character's entries.
pub fn get_plugin_data_map(
    conn: &Connection,
    character_id: &str,
) -> Result<serde_json::Value, DbError> {
    let mut map = serde_json::Map::new();
    for entry in find_by_character_id(conn, character_id)? {
        if let (Some(name), Some(data)) = (
            entry.get("pluginName").and_then(|v| v.as_str()),
            entry.get("data"),
        ) {
            map.insert(name.to_string(), data.clone());
        }
    }
    Ok(serde_json::Value::Object(map))
}

/// v4 `deleteByCharacterAndPlugin` — delete the entry for a `(characterId,
/// pluginName)` pair; `false` when no row matched.
pub fn delete_by_character_and_plugin(
    conn: &Connection,
    character_id: &str,
    plugin_name: &str,
) -> Result<bool, DbError> {
    let affected = conn.execute(
        "DELETE FROM character_plugin_data WHERE characterId = ?1 AND pluginName = ?2",
        params![character_id, plugin_name],
    )?;
    Ok(affected > 0)
}

/// v4 `findByCharacterAndPlugin` — the entry for a `(characterId, pluginName)`
/// pair, or `None`.
pub fn find_by_character_and_plugin(
    conn: &Connection,
    character_id: &str,
    plugin_name: &str,
) -> Result<Option<serde_json::Value>, DbError> {
    conn.query_row(
        "SELECT id, characterId, pluginName, data, createdAt, updatedAt \
         FROM character_plugin_data WHERE characterId = ?1 AND pluginName = ?2",
        params![character_id, plugin_name],
        marshal_row,
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other.into()),
    })
}

impl<'c> CharacterPluginDataRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// v4's generic column serializer for this TEXT column
    /// (`backends/sqlite/json-columns.ts:110-126` `serializeValue`): an array or
    /// object is `JSON.stringify`d, but **a string or number passes through
    /// unchanged**. That matters here because `data` is an opaque plugin
    /// payload: v4 reads the column back as raw TEXT (never re-parsed — see
    /// [`marshal_row`]), so a `.qtap` export carries whatever text was stored,
    /// and re-importing it must store the SAME text. A blanket
    /// `serde_json::to_string` would wrap an already-JSON string in a second
    /// layer of quoting and escapes on every round-trip.
    fn serialize_column_value(v: &serde_json::Value) -> Result<String, DbError> {
        match v {
            serde_json::Value::String(s) => Ok(s.clone()),
            serde_json::Value::Number(n) => Ok(n.to_string()),
            other => serde_json::to_string(other)
                .map_err(|e| DbError::Internal(format!("data serialize: {e}"))),
        }
    }

    /// Insert an entry with the given pinned id + timestamps. `data` is stored
    /// through v4's column serializer ([`Self::serialize_column_value`]);
    /// `characterId`/`pluginName` pass through as strings.
    pub fn create(&self, data: &CpdCreate, opts: &CreateOptions) -> Result<(), DbError> {
        let data_json = Self::serialize_column_value(&data.data)?;

        self.conn.execute(
            "INSERT INTO character_plugin_data \
               (id, characterId, pluginName, data, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                opts.id,
                data.character_id,
                data.plugin_name,
                data_json,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the entry `id`. Returns `Ok(false)` when no row
    /// matched (v4's "not found -> null"). id and createdAt are never touched.
    /// Each `Some` field sets that column; `updatedAt` is always set.
    pub fn update(&self, id: &str, patch: &CpdUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`s — the row must exist or it's a no-op.
        let exists: bool = self
            .conn
            .query_row(
                "SELECT 1 FROM character_plugin_data WHERE id = ?1",
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

        if let Some(character_id) = &patch.character_id {
            assignments.push(format!("characterId = ?{}", values.len() + 1));
            values.push(Box::new(character_id.clone()));
        }
        if let Some(plugin_name) = &patch.plugin_name {
            assignments.push(format!("pluginName = ?{}", values.len() + 1));
            values.push(Box::new(plugin_name.clone()));
        }
        if let Some(data) = &patch.data {
            let data_json = Self::serialize_column_value(data)?;
            assignments.push(format!("data = ?{}", values.len() + 1));
            values.push(Box::new(data_json));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE character_plugin_data SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the entry `id`. Returns `false` when no row matched (v4's `_delete`
    /// "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self.conn.execute(
            "DELETE FROM character_plugin_data WHERE id = ?1",
            params![id],
        )?;
        Ok(affected > 0)
    }

    /// Find the id of the row for a `(characterId, pluginName)` pair, if any.
    /// Mirrors v4's `findByCharacterAndPlugin` (`findOneByFilter`), narrowed to
    /// just the id — all `upsert` needs to decide create-vs-update.
    fn find_id_by_character_and_plugin(
        &self,
        character_id: &str,
        plugin_name: &str,
    ) -> Result<Option<String>, DbError> {
        self.conn
            .query_row(
                "SELECT id FROM character_plugin_data \
                 WHERE characterId = ?1 AND pluginName = ?2",
                params![character_id, plugin_name],
                |r| r.get::<_, String>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
    }

    /// Create-or-update plugin data for a `(characterId, pluginName)` pair —
    /// v4's `upsert(characterId, pluginName, data)`.
    ///
    /// Semantics ported from v4: find the existing row for the pair; if found,
    /// `update` ONLY the `data` field (id + createdAt preserved, updatedAt minted
    /// to `now`); else `create` a fresh row (mints id + createdAt + updatedAt, all
    /// to the same `now`). Returns the id of the affected row.
    ///
    /// This mints its own id + timestamps (the remap-normalization form), so it
    /// belongs to the minted-values tier-2 case, not the pinned one. The `data`
    /// open-JSON key-order seam (#5) still applies — keep `data` to `{}` /
    /// single-key.
    pub fn upsert(
        &self,
        character_id: &str,
        plugin_name: &str,
        data: serde_json::Value,
    ) -> Result<String, DbError> {
        let now = crate::clock::now_iso();

        if let Some(existing_id) =
            self.find_id_by_character_and_plugin(character_id, plugin_name)?
        {
            // v4: this.update(existing.id, { data }) — only `data` changes;
            // updatedAt is minted, id + createdAt are preserved.
            self.update(
                &existing_id,
                &CpdUpdate {
                    data: Some(data),
                    updated_at: now,
                    ..Default::default()
                },
            )?;
            Ok(existing_id)
        } else {
            // v4: this.create({ characterId, pluginName, data }) — mints id +
            // both timestamps (all the same `now`).
            let id = uuid::Uuid::new_v4().to_string();
            self.create(
                &CpdCreate {
                    character_id: character_id.to_string(),
                    plugin_name: plugin_name.to_string(),
                    data,
                },
                &CreateOptions {
                    id: id.clone(),
                    created_at: now.clone(),
                    updated_at: now,
                },
            )?;
            Ok(id)
        }
    }
}
