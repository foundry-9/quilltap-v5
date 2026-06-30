//! The characters repository — **slim-row SQL marshaling** (the store-backed
//! capstone, sub-unit 2). Ports the base-repository SQL CRUD
//! (`_create`/`_update`/`_delete`) of v4's
//! `lib/database/repositories/characters.repository.ts` over the **main-DB**
//! `characters` table.
//!
//! ## Why this ports only the SLIM row (not the public create/update)
//!
//! `CharactersRepository` is a `TaggableBaseRepository` whose public
//! `create`/`update` orchestrate a vault: `create` runs `_create` then
//! `ensureCharacterVault` (scaffold + `writeCharacterVaultManagedFields` + link)
//! then reloads through the read overlay; `update` routes managed fields to the
//! vault before `_update`. Those orchestrations are the NEXT sub-unit. What this
//! ports is the foundational leaf they both sit on: the `characters` SQL row.
//!
//! v4's overridden `_create`/`_update` strip the [`MANAGED_FIELDS`] set
//! (identity, description, manifesto, personality, exampleDialogues, pronouns,
//! aliases, title, firstMessage, talkativeness, physicalDescription,
//! systemPrompts, scenarios) from the validated document before the INSERT/UPDATE,
//! because the 4.6 vault cutover moved those out of the DB and into the character
//! vault. So the persisted row carries only the **non-managed** columns — the
//! "slim row" this module marshals.
//!
//! A FRESH fixture's table is still created by `ensureCollection('characters',
//! CharacterSchema)` from the CURRENT schema, which DOES declare the managed
//! fields, so the fixture table has those columns too — but both v4 and this port
//! omit them from every write, so they sit at their DDL defaults (NULL, or `'[]'`
//! / `0.5` for the defaulted ones) identically on both sides. The canonical dump
//! reads every column; the managed columns appear, identical, on both sides.
//!
//! ## Scope
//!
//! The base `_create` / `_update` / `_delete` against `characters`, driven in the
//! tier-2 oracle via a thin test subclass that exposes those protected internals
//! (the wardrobe-repo precedent). The vault-aware public `create`/`update`,
//! `ensureCharacterVault`, the array/partner sub-array ops, and the `findBy*`
//! queries are out of scope here (later characters sub-units).
//!
//! ### `update` is a partial SET that reproduces v4's full `$set`
//!
//! v4's overridden `_update` reads the raw row, merges the patch, re-validates,
//! strips managed fields, and `$set`s **every** slim column. This port instead
//! `SET`s only the provided columns (+ `updatedAt`). The on-disk result is
//! identical: the fixture rows were written by v4's own `_create` (already in
//! validated/canonical JSON-key order), so re-validating + rewriting an unchanged
//! column reproduces the same bytes that leaving it untouched does. The
//! partial form is the same shape `doc_mount_points`/`wardrobe` use.
//!
//! ## What this repo banks for the tier-2 marshaling surface
//!
//! The **widest nullable-boolean surface in Phase 2** and the first repo mixing a
//! typed JSON-object column with typed-struct array columns:
//!
//!   - **seven nullable boolean columns** (`defaultAgentModeEnabled`,
//!     `defaultHelpToolsEnabled`, `canDressThemselves`, `canCreateOutfits`,
//!     `systemTransparency`, `coreWhisperEnabled`, `canBeCarina`) —
//!     `z.boolean().nullable().optional()` → INTEGER 0/1 when present, SQL NULL
//!     when absent. Bound `Option<i64>` (`Some(0/1)` / `None`). Both present and
//!     absent banked.
//!   - **two boolean-default columns** (`isFavorite`, `npc`) → INTEGER 0/1.
//!   - a **typed JSON-object column** (`defaultTimestampConfig`,
//!     `TimestampConfigSchema.nullable()`): a nine-field typed struct in schema
//!     field order so the compact JSON matches v4's `JSON.stringify` key order
//!     (NOT `serde_json::Value`, which sorts). The corpus supplies all nine keys
//!     (including the four nullable ones, explicitly) so neither side drops a key
//!     — sidestepping the optional-key-order seam. `None` → SQL NULL banked too.
//!   - an **open JSON value column** (`sillyTavernData`, `JsonSchema.nullable()` =
//!     `z.record(z.unknown())`) → `Option<serde_json::Value>` compact text. Kept
//!     `null`/single-key in the corpus (the multi-key open-JSON key-order seam).
//!   - **two typed-struct array columns** (`partnerLinks` of `{partnerId,
//!     isDefault}`, `avatarOverrides` of `{chatId, imageId}`) → `Vec<_>` of typed
//!     structs in field order; arrays are order-preserving. Empty and non-empty
//!     banked.
//!   - a **string array column** (`tags`, the Taggable lineage) → `Vec<String>`.
//!   - an **enum TEXT column** (`controlledBy`, `'llm'`/`'user'`).
//!   - many **nullable UUID/TEXT columns** (`defaultImageId`,
//!     `defaultConnectionProfileId`, `defaultPartnerId`, `defaultRoleplayTemplateId`,
//!     `defaultImageProfileId`, `defaultScenarioId`, `defaultSystemPromptId`,
//!     `characterDocumentMountPointId`).
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in each update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form.
//!
//! Deferred (not in the corpus, mirroring the prior repos): clearing a nullable
//! column back to NULL via `update`; populated multi-key `sillyTavernData`;
//! omitting a nullable key from `defaultTimestampConfig` (the optional-key-order
//! seam). Each lands with the form an op needs.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::DbError;

/// The top-level character keys v4 routes to the vault and strips before every
/// SQL write (`MANAGED_FIELDS` in `vault-overlay/schema.ts`). Recorded here for
/// documentation parity — this module never writes them, so the slim row carries
/// only the complement. `systemTransparency` is intentionally NOT managed (it is
/// access-control application state and stays a DB column).
pub const MANAGED_FIELDS: &[&str] = &[
    "identity",
    "description",
    "manifesto",
    "personality",
    "exampleDialogues",
    "pronouns",
    "aliases",
    "title",
    "firstMessage",
    "talkativeness",
    "physicalDescription",
    "systemPrompts",
    "scenarios",
];

/// A relationship link to a partner character (`partnerLinks[]` element). Stored
/// inside the `partnerLinks` JSON array; field order (`partnerId`, `isDefault`)
/// mirrors the Zod object so the serialized text matches `JSON.stringify`.
#[derive(Serialize, Deserialize, Clone)]
pub struct PartnerLink {
    #[serde(rename = "partnerId")]
    pub partner_id: String,
    #[serde(rename = "isDefault")]
    pub is_default: bool,
}

/// A per-chat avatar override (`avatarOverrides[]` element). Field order
/// (`chatId`, `imageId`) mirrors the Zod object.
#[derive(Serialize, Deserialize, Clone)]
pub struct AvatarOverride {
    #[serde(rename = "chatId")]
    pub chat_id: String,
    #[serde(rename = "imageId")]
    pub image_id: String,
}

/// The per-chat timestamp-injection config (`defaultTimestampConfig`, a
/// `TimestampConfigSchema` object). A typed struct in schema field order — the
/// compact JSON must match v4's `JSON.stringify` key order, so this is NOT a
/// `serde_json::Value` (which would sort keys). All nine keys are emitted (no
/// `skip_serializing_if`): the corpus supplies every key explicitly, matching v4
/// (where a nullable key provided as `null` stays present). Omitting a nullable
/// key entirely is the optional-key-order seam — out of corpus.
#[derive(Serialize, Deserialize, Clone)]
pub struct TimestampConfig {
    pub mode: String,
    pub format: String,
    #[serde(rename = "customFormat")]
    pub custom_format: Option<String>,
    #[serde(rename = "useFictionalTime")]
    pub use_fictional_time: bool,
    #[serde(rename = "fictionalBaseTimestamp")]
    pub fictional_base_timestamp: Option<String>,
    #[serde(rename = "fictionalBaseRealTime")]
    pub fictional_base_real_time: Option<String>,
    #[serde(rename = "autoPrepend")]
    pub auto_prepend: bool,
    pub timezone: Option<String>,
    /// `z.number().int()` inside the object → a JSON integer (`15`), bound here as
    /// `i64` so it renders bare (not `15.0`), matching `JSON.stringify`.
    #[serde(rename = "intervalMinutes")]
    pub interval_minutes: i64,
}

/// Fields for creating a character slim row (the non-managed `Omit<Character,
/// 'id'|timestamps>` complement). All slim columns are present so the corpus can
/// set them explicitly (mirrors `doc_mount_points`/`connection_profiles`).
pub struct CharacterCreate {
    pub user_id: String,
    pub name: String,
    pub default_image_id: Option<String>,
    pub default_connection_profile_id: Option<String>,
    pub default_partner_id: Option<String>,
    pub default_roleplay_template_id: Option<String>,
    pub default_image_profile_id: Option<String>,
    /// Open arbitrary-JSON object (`z.record(z.unknown())`). `None` → SQL NULL.
    pub silly_tavern_data: Option<serde_json::Value>,
    pub is_favorite: bool,
    pub npc: bool,
    /// `'llm'` | `'user'`.
    pub controlled_by: String,
    pub default_agent_mode_enabled: Option<bool>,
    pub default_help_tools_enabled: Option<bool>,
    pub default_timestamp_config: Option<TimestampConfig>,
    pub default_scenario_id: Option<String>,
    pub default_system_prompt_id: Option<String>,
    pub character_document_mount_point_id: Option<String>,
    pub can_dress_themselves: Option<bool>,
    pub can_create_outfits: Option<bool>,
    pub system_transparency: Option<bool>,
    pub core_whisper_enabled: Option<bool>,
    pub can_be_carina: Option<bool>,
    pub partner_links: Vec<PartnerLink>,
    pub tags: Vec<String>,
    pub avatar_overrides: Vec<AvatarOverride>,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A character slim-row update patch. Each `Some` field sets that column; id and
/// createdAt are never touched; `updatedAt` is always set. Provided fields set
/// values (clearing a nullable column to NULL is deferred — see header). Covers
/// every settable slim column. See the header for why a partial SET reproduces
/// v4's full `$set` on-disk result.
#[derive(Default)]
pub struct CharacterUpdate {
    pub user_id: Option<String>,
    pub name: Option<String>,
    pub default_image_id: Option<String>,
    pub default_connection_profile_id: Option<String>,
    pub default_partner_id: Option<String>,
    pub default_roleplay_template_id: Option<String>,
    pub default_image_profile_id: Option<String>,
    pub silly_tavern_data: Option<serde_json::Value>,
    pub is_favorite: Option<bool>,
    pub npc: Option<bool>,
    pub controlled_by: Option<String>,
    pub default_agent_mode_enabled: Option<bool>,
    pub default_help_tools_enabled: Option<bool>,
    pub default_timestamp_config: Option<TimestampConfig>,
    pub default_scenario_id: Option<String>,
    pub default_system_prompt_id: Option<String>,
    pub character_document_mount_point_id: Option<String>,
    pub can_dress_themselves: Option<bool>,
    pub can_create_outfits: Option<bool>,
    pub system_transparency: Option<bool>,
    pub core_whisper_enabled: Option<bool>,
    pub can_be_carina: Option<bool>,
    pub partner_links: Option<Vec<PartnerLink>>,
    pub tags: Option<Vec<String>>,
    pub avatar_overrides: Option<Vec<AvatarOverride>>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`], MAIN db).
pub struct CharactersRepository<'c> {
    conn: &'c Connection,
}

impl<'c> CharactersRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a character slim row with the given pinned id + timestamps. The JSON
    /// columns → compact JSON text; booleans → INTEGER 0/1; nullable booleans →
    /// `Option<i64>` (`None` → SQL NULL); the `Option` strings/objects pass through
    /// (`None` → SQL NULL). Columns are bound in schema field order (managed
    /// columns omitted — they take their DDL defaults, as in v4).
    pub fn create(&self, data: &CharacterCreate, opts: &CreateOptions) -> Result<(), DbError> {
        let silly = json_opt(data.silly_tavern_data.as_ref(), "sillyTavernData")?;
        let ts_config = json_opt(
            data.default_timestamp_config.as_ref(),
            "defaultTimestampConfig",
        )?;
        let partner_links = serde_json::to_string(&data.partner_links)
            .map_err(|e| DbError::Key(format!("partnerLinks serialize: {e}")))?;
        let tags = serde_json::to_string(&data.tags)
            .map_err(|e| DbError::Key(format!("tags serialize: {e}")))?;
        let avatar_overrides = serde_json::to_string(&data.avatar_overrides)
            .map_err(|e| DbError::Key(format!("avatarOverrides serialize: {e}")))?;

        self.conn.execute(
            "INSERT INTO characters \
               (id, userId, name, defaultImageId, defaultConnectionProfileId, defaultPartnerId, \
                defaultRoleplayTemplateId, defaultImageProfileId, sillyTavernData, isFavorite, npc, \
                controlledBy, defaultAgentModeEnabled, defaultHelpToolsEnabled, defaultTimestampConfig, \
                defaultScenarioId, defaultSystemPromptId, characterDocumentMountPointId, \
                canDressThemselves, canCreateOutfits, systemTransparency, coreWhisperEnabled, \
                canBeCarina, partnerLinks, tags, avatarOverrides, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, \
                     ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28)",
            params![
                opts.id,
                data.user_id,
                data.name,
                data.default_image_id,
                data.default_connection_profile_id,
                data.default_partner_id,
                data.default_roleplay_template_id,
                data.default_image_profile_id,
                silly,
                i64::from(data.is_favorite),
                i64::from(data.npc),
                data.controlled_by,
                data.default_agent_mode_enabled.map(i64::from),
                data.default_help_tools_enabled.map(i64::from),
                ts_config,
                data.default_scenario_id,
                data.default_system_prompt_id,
                data.character_document_mount_point_id,
                data.can_dress_themselves.map(i64::from),
                data.can_create_outfits.map(i64::from),
                data.system_transparency.map(i64::from),
                data.core_whisper_enabled.map(i64::from),
                data.can_be_carina.map(i64::from),
                partner_links,
                tags,
                avatar_overrides,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the character `id`. Returns `Ok(false)` when no row
    /// matched (v4's `_update` "not found -> null"). id and createdAt are never
    /// touched. Each `Some` field sets that column; `updatedAt` is always set.
    pub fn update(&self, id: &str, patch: &CharacterUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findByIdRaw`s — the row must exist or it's a no-op.
        if !self.row_exists(id)? {
            return Ok(false);
        }

        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        macro_rules! set_text {
            ($field:expr, $col:literal) => {
                if let Some(v) = &$field {
                    assignments.push(format!("{} = ?{}", $col, values.len() + 1));
                    values.push(Box::new(v.clone()));
                }
            };
        }
        macro_rules! set_bool {
            ($field:expr, $col:literal) => {
                if let Some(v) = $field {
                    assignments.push(format!("{} = ?{}", $col, values.len() + 1));
                    values.push(Box::new(i64::from(v)));
                }
            };
        }
        macro_rules! set_json {
            ($field:expr, $col:literal, $label:literal) => {
                if let Some(v) = &$field {
                    let text = serde_json::to_string(v)
                        .map_err(|e| DbError::Key(format!("{} serialize: {e}", $label)))?;
                    assignments.push(format!("{} = ?{}", $col, values.len() + 1));
                    values.push(Box::new(text));
                }
            };
        }

        set_text!(patch.user_id, "userId");
        set_text!(patch.name, "name");
        set_text!(patch.default_image_id, "defaultImageId");
        set_text!(
            patch.default_connection_profile_id,
            "defaultConnectionProfileId"
        );
        set_text!(patch.default_partner_id, "defaultPartnerId");
        set_text!(
            patch.default_roleplay_template_id,
            "defaultRoleplayTemplateId"
        );
        set_text!(patch.default_image_profile_id, "defaultImageProfileId");
        set_json!(
            patch.silly_tavern_data,
            "sillyTavernData",
            "sillyTavernData"
        );
        set_bool!(patch.is_favorite, "isFavorite");
        set_bool!(patch.npc, "npc");
        set_text!(patch.controlled_by, "controlledBy");
        set_bool!(patch.default_agent_mode_enabled, "defaultAgentModeEnabled");
        set_bool!(patch.default_help_tools_enabled, "defaultHelpToolsEnabled");
        set_json!(
            patch.default_timestamp_config,
            "defaultTimestampConfig",
            "defaultTimestampConfig"
        );
        set_text!(patch.default_scenario_id, "defaultScenarioId");
        set_text!(patch.default_system_prompt_id, "defaultSystemPromptId");
        set_text!(
            patch.character_document_mount_point_id,
            "characterDocumentMountPointId"
        );
        set_bool!(patch.can_dress_themselves, "canDressThemselves");
        set_bool!(patch.can_create_outfits, "canCreateOutfits");
        set_bool!(patch.system_transparency, "systemTransparency");
        set_bool!(patch.core_whisper_enabled, "coreWhisperEnabled");
        set_bool!(patch.can_be_carina, "canBeCarina");
        set_json!(patch.partner_links, "partnerLinks", "partnerLinks");
        set_json!(patch.tags, "tags", "tags");
        set_json!(patch.avatar_overrides, "avatarOverrides", "avatarOverrides");

        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE characters SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the character `id`. Returns `Ok(false)` when no row matched (v4's
    /// `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM characters WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// True iff a row with this id exists — v4's `_update` `findByIdRaw`
    /// precondition (a missing target makes the update a no-op returning `null`).
    fn row_exists(&self, id: &str) -> Result<bool, DbError> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM characters WHERE id = ?1",
                params![id],
                |row| row.get::<_, i64>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(found.is_some())
    }
}

/// Serialize an optional JSON-bearing value to compact text, `None` → SQL NULL.
fn json_opt<T: Serialize>(value: Option<&T>, label: &str) -> Result<Option<String>, DbError> {
    value
        .map(|v| serde_json::to_string(v))
        .transpose()
        .map_err(|e| DbError::Key(format!("{label} serialize: {e}")))
}
