//! The plugin-config repository — a Phase-2 repo port, after `folders`, `tags`,
//! `text_replacement_rules`, `prompt_templates`, `conversation_annotations`, and
//! the `image_profiles` / `roleplay_templates` / `connection_profiles` batch.
//! Ports v4's `lib/database/repositories/plugin-config.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, `delete` (the three abstract methods over the base
//! repo) and `upsertForUserPlugin` (the find-by-pair → MERGE-or-create helper,
//! tested in the remap / minted-values form — see `upsert_for_user_plugin`). The
//! remaining custom query helpers — `findByUserAndPlugin`, `findByUserId`,
//! `getOrCreate`, `deleteByPlugin` — are out of scope here. v4's `update` is a
//! plain `_update` (Partial spread): provided fields overwrite, id/createdAt are
//! preserved, `updatedAt` is set explicitly. There is **no built-in guard**
//! (unlike `prompt_templates`).
//!
//! ## What this repo banks for the tier-2 marshaling surface
//!
//! `plugin_config` extends v4's `UserOwnedBaseRepository`, so it carries a
//! user-scoped `userId` TEXT column. Its distinctive surface:
//!
//!   - the open / arbitrary-JSON object column (`config`,
//!     `z.record(z.string(), z.unknown())`). Like `image_profiles.parameters`,
//!     it has no fixed shape, so it can't be a typed struct — it is modeled as a
//!     [`serde_json::Value`] and stored via `serde_json::to_string`, mirroring
//!     v4's `JSON.stringify`. The empty default `{}` and a single-key object
//!     (e.g. `{"threshold":0.5}`) serialize identically on both sides; the
//!     fractional value exercises `js_number_to_json`'s float pass-through inside
//!     the JSON text.
//!   - an **optional boolean with NO default** (`enabled`,
//!     `z.boolean().optional()`). Present → INTEGER 0/1; **absent → SQL NULL**
//!     (the column is registered but the value is undefined). Modeled as
//!     `Option<bool>` → bound `Option<i64>` (`None` → SQL NULL,
//!     `Some(b)` → `i64::from(b)`).
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in the update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form
//! `folders`/`tags`/`image_profiles` use.
//!
//! Deferred seam (open-JSON multi-key key-order — TRACKED, seam #5 in
//! docs/developer/porting/phase-2-onramp.md): a `config` object with **two or
//! more keys** would expose a key-order divergence — `serde_json::Value`'s
//! default `BTreeMap` SORTS object keys, while v4's `JSON.stringify` preserves
//! INSERTION order. The corpus is deliberately constrained to `{}` or single-key
//! objects so the two serializations coincide. This is the same family as the
//! `localeCompare` (collation) / `toLowerCase` (case-mapping) deferrals noted in
//! docs/developer/porting/phase-2-onramp.md; close it (preserve-insertion-order
//! serializer) before a multi-key `config` op lands.
//!
//! Deferred (not in the corpus): setting `enabled` **back to NULL** via `update`
//! (clearing a present value) — the patch models a provided field as "set to
//! this value", so a nullable setter lands when an op needs it.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;
use crate::clock::now_iso;

/// Fields for creating a plugin config (the `Omit<PluginConfig,'id'|
/// timestamps>` shape). `config` is the open-JSON object column (bound as
/// compact JSON text); `enabled` is the optional boolean (`None` => SQL NULL,
/// `Some(b)` => INTEGER 0/1).
pub struct PcCreate {
    pub user_id: String,
    pub plugin_name: String,
    /// Open/arbitrary JSON object (`{}` or a single-key object — see the module
    /// header's multi-key key-order deferral). Stored via `serde_json::to_string`.
    pub config: serde_json::Value,
    /// `None` => SQL NULL (the optional boolean absent — v4 stores no value);
    /// `Some(b)` => INTEGER 0/1.
    pub enabled: Option<bool>,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A plugin-config update patch. Mirrors v4 `update` over `_update`: provided
/// fields overwrite, id and createdAt are preserved (the base spread keeps
/// `existing.id` / `existing.createdAt`), `updatedAt` is set explicitly. Each
/// `Some` field sets that column; clearing `enabled` back to NULL is deferred
/// (see header).
#[derive(Default)]
pub struct PcUpdate {
    pub plugin_name: Option<String>,
    /// Re-serialized to compact JSON text when provided.
    pub config: Option<serde_json::Value>,
    /// `Some(b)` sets the column to INTEGER 0/1.
    pub enabled: Option<bool>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct PluginConfigRepository<'c> {
    conn: &'c Connection,
}

impl<'c> PluginConfigRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a plugin config with the given pinned id + timestamps. `config`
    /// → compact JSON text; `enabled` as `Option<i64>` (`None` → SQL NULL).
    pub fn create(&self, data: &PcCreate, opts: &CreateOptions) -> Result<(), DbError> {
        let config_json = serde_json::to_string(&data.config)
            .map_err(|e| DbError::Key(format!("config serialize: {e}")))?;
        let enabled: Option<i64> = data.enabled.map(i64::from);

        self.conn.execute(
            "INSERT INTO plugin_configs \
               (id, userId, pluginName, config, enabled, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                opts.id,
                data.user_id,
                data.plugin_name,
                config_json,
                enabled,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the plugin config `id`. Returns `Ok(false)` when
    /// no row matched (v4's "not found -> null"). id and createdAt are never
    /// touched. Each `Some` field sets that column; `updatedAt` is always set.
    pub fn update(&self, id: &str, patch: &PcUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`s — the row must exist or it's a no-op.
        let exists: bool = self
            .conn
            .query_row(
                "SELECT 1 FROM plugin_configs WHERE id = ?1",
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

        if let Some(plugin_name) = &patch.plugin_name {
            assignments.push(format!("pluginName = ?{}", values.len() + 1));
            values.push(Box::new(plugin_name.clone()));
        }
        if let Some(config) = &patch.config {
            let config_json = serde_json::to_string(config)
                .map_err(|e| DbError::Key(format!("config serialize: {e}")))?;
            assignments.push(format!("config = ?{}", values.len() + 1));
            values.push(Box::new(config_json));
        }
        if let Some(enabled) = patch.enabled {
            assignments.push(format!("enabled = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(enabled)));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE plugin_configs SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the plugin config `id`. Returns `false` when no row matched (v4's
    /// `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM plugin_configs WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// Set the config for a `(user_id, plugin_name)` pair, creating the row if it
    /// doesn't exist (v4's `upsertForUserPlugin`). MINTED-VALUES path: this op
    /// reads no pinned id/timestamps — it mints its own (the remap-normalization
    /// form), so it is differential-tested via the first-seen id remap +
    /// timestamp placeholder, not the pinned zero-normalization form.
    ///
    /// Semantics (v4):
    ///   - find existing by `(userId, pluginName)`;
    ///   - if found → **MERGE** `{ ...existing.config, ...config }` (existing
    ///     keys first, each overwritten by the new value when the key overlaps,
    ///     then any new keys appended — a JS object spread), then `update(id,
    ///     {config: merged})` (id + createdAt preserved, `updatedAt` minted);
    ///   - else → `create({userId, pluginName, config})` (mints id + both
    ///     timestamps; `enabled` is never set → SQL NULL).
    ///
    /// Returns the id of the affected row (the existing id on the update path,
    /// the freshly minted id on the create path).
    ///
    /// OPEN-JSON MERGE + SEAM (tracked seam #5): the MERGE is performed on
    /// `serde_json::Value` objects, which sort keys; v4 merges via spread and
    /// re-stringifies in INSERTION order. To keep the two byte-identical, the
    /// corpus is constrained so every stored config (including every MERGE
    /// result) is `{}` or a SINGLE key — either the existing and new config share
    /// the same single key (the merge overwrites the value, staying single-key)
    /// or an empty existing merges with a single-key new. A 2+-key merge result
    /// would expose the key-order divergence; close the seam
    /// (preserve-insertion-order serializer) before such an op lands.
    pub fn upsert_for_user_plugin(
        &self,
        user_id: &str,
        plugin_name: &str,
        config: &serde_json::Value,
    ) -> Result<String, DbError> {
        // Private find-by-(userId, pluginName): the existing row's id + config.
        let existing: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT id, config FROM plugin_configs WHERE userId = ?1 AND pluginName = ?2",
                params![user_id, plugin_name],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;

        let now = now_iso();

        if let Some((id, existing_config_json)) = existing {
            // MERGE `{ ...existing.config, ...config }`: start from the existing
            // object, then insert/overwrite each key from the new config. The
            // corpus keeps the result `{}`/single-key (see header), so the
            // serde_json key-sorting and v4's insertion order coincide.
            let existing_config: serde_json::Value = serde_json::from_str(&existing_config_json)
                .map_err(|e| DbError::Key(format!("existing config parse: {e}")))?;
            let mut merged: serde_json::Map<String, serde_json::Value> =
                existing_config.as_object().cloned().unwrap_or_default();
            if let Some(new_obj) = config.as_object() {
                for (k, v) in new_obj {
                    merged.insert(k.clone(), v.clone());
                }
            }
            let merged_value = serde_json::Value::Object(merged);

            self.update(
                &id,
                &PcUpdate {
                    plugin_name: None,
                    config: Some(merged_value),
                    enabled: None,
                    updated_at: now,
                },
            )?;
            Ok(id)
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            self.create(
                &PcCreate {
                    user_id: user_id.to_string(),
                    plugin_name: plugin_name.to_string(),
                    config: config.clone(),
                    enabled: None,
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
