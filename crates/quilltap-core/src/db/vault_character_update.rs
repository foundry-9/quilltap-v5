//! Character vault UPDATE integration (characters sub-unit 4a). Ports v4's
//! `applyDocumentStoreWriteOverlay` (`vault-overlay/managed-fields.ts`) + the
//! `CharactersRepository.update` orchestration.
//!
//! ## The write overlay vs. the create-time writer
//!
//! Sub-unit 1's [`super::vault_character_write::write_character_vault_managed_fields`]
//! projects **every** managed field unconditionally (the create path, onto a fresh
//! vault). The UPDATE path is different: it routes only the fields **present in the
//! patch**, and `properties.json` is a **read-modify-write** — a patch touching
//! only `title` must preserve the other four property keys. So this is a distinct
//! port, composing the same verified write leaves with per-field presence checks.
//!
//! ## `apply_document_store_write_overlay`
//!
//! Reads the character's vault FK, then for each managed descriptor present in the
//! patch writes the vault file and strips that key from the DB-bound patch:
//!
//!   - **markdown** (`identity`/`description`/`manifesto`/`personality`/
//!     `exampleDialogues`): `None`/absent value → `""`.
//!   - **properties** (`pronouns`/`aliases`/`title`/`firstMessage`/`talkativeness`):
//!     read the current `properties.json` (parse; fall back to the empty-managed
//!     default when missing), overlay only the touched keys, rewrite. The
//!     untouched keys are preserved — the RMW invariant.
//!   - **physical** (`physicalDescription`): a non-null value writes
//!     `physical-description.md` + `physical-prompts.json`; a **null** value leaves
//!     the vault files alone (clearing is a DB-side concern) — matching v4, which
//!     does NOT strip it from the patch in that case (the slim update ignores it
//!     anyway, since `physicalDescription` is not a slim column).
//!   - **prompts / scenarios** (`systemPrompts`/`scenarios`): reproject the
//!     `Prompts/` / `Scenarios/` folder from the incoming array (sweep + write).
//!
//! Returns the DB-bound remainder (the unmanaged patch keys). `update_character`
//! then runs the slim `_update` for that remainder (skipped when empty — so a
//! managed-only update does NOT bump the slim row's `updatedAt`), mirroring v4.
//!
//! **Provision-on-the-fly.** When a character has no vault FK but the patch
//! carries managed fields, v4 provisions a vault mid-write (a loud-logged bug path
//! — every character is supposed to have one), re-reads to pick up the new FK,
//! verifies it linked, then continues normal routing. This port does the same via
//! [`super::character_vault::ensure_character_vault`] over the raw slim character
//! (the pre-patch managed values, all at their post-cutover empty defaults — the
//! patch's managed fields are then routed on top).

use rusqlite::{params, Connection};
use serde_json::{Map, Value};

use super::characters::{CharacterUpdate, CharactersRepository, MANAGED_FIELDS};
use super::doc_mount_documents::DocMountDocumentsRepository;
use super::doc_mount_file_links::DocMountFileLinksRepository;
use super::vault_character_write::{render_physical_prompts_json, render_properties_json};
use super::DbError;
use crate::vault_overlay::{
    build_scenario_file, build_system_prompt_file, parse_vault_properties, sanitize_file_name,
    CharacterVaultProperties,
};

use super::vault_character_write::{
    CharacterVaultWriteInput, PhysicalDescriptionWrite, Pronouns, ScenarioWrite, SystemPromptWrite,
};
use super::vault_wardrobe_write::project_array_into_vault_folder;

const IDENTITY_MD_PATH: &str = "identity.md";
const DESCRIPTION_MD_PATH: &str = "description.md";
const MANIFESTO_MD_PATH: &str = "manifesto.md";
const PERSONALITY_MD_PATH: &str = "personality.md";
const EXAMPLE_DIALOGUES_MD_PATH: &str = "example-dialogues.md";
const PHYSICAL_DESCRIPTION_MD_PATH: &str = "physical-description.md";
const PHYSICAL_PROMPTS_JSON_PATH: &str = "physical-prompts.json";
const PROPERTIES_JSON_PATH: &str = "properties.json";
const PROMPTS_FOLDER: &str = "Prompts";
const SCENARIOS_FOLDER: &str = "Scenarios";

/// The markdown managed fields and their vault paths, in descriptor order.
const MARKDOWN_FIELDS: &[(&str, &str)] = &[
    ("identity", IDENTITY_MD_PATH),
    ("description", DESCRIPTION_MD_PATH),
    ("manifesto", MANIFESTO_MD_PATH),
    ("personality", PERSONALITY_MD_PATH),
    ("exampleDialogues", EXAMPLE_DIALOGUES_MD_PATH),
];

/// The five `properties.json` keys (in literal order).
const PROPERTY_KEYS: &[&str] = &[
    "pronouns",
    "aliases",
    "title",
    "firstMessage",
    "talkativeness",
];

/// Route the managed fields in `patch` to the character's vault and return the
/// DB-bound remainder (v4 `applyDocumentStoreWriteOverlay`). `main` holds the slim
/// row (the FK lookup); `mount` holds the store.
pub fn apply_document_store_write_overlay(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    patch: &Map<String, Value>,
) -> Result<Map<String, Value>, DbError> {
    let mount_point_id = match read_vault_fk(main, character_id)? {
        Some(id) => id,
        None => {
            // No linked vault. With no managed fields there's nothing to route —
            // let the unmanaged remainder flow to the DB.
            if !MANAGED_FIELDS.iter().any(|f| patch.contains_key(*f)) {
                return Ok(patch.clone());
            }
            // The post-4.6 cutover dropped the DB columns for managed fields, so a
            // character without a vault would silently lose them on the way through
            // `_update`. Every character is supposed to have a vault — the startup
            // backfill provisions one for any that don't — so reaching this branch
            // is a bug elsewhere. Provision a vault now (v4 logs loudly) so the
            // write doesn't get dropped, re-read the FK it set, and continue.
            provision_vault_on_the_fly(main, mount, character_id)?
        }
    };

    let links = DocMountFileLinksRepository::new(mount);
    let docs = DocMountDocumentsRepository::new(mount);
    let mut db_patch = patch.clone();

    // 1. Markdown fields (None/absent value → "").
    for (key, path) in MARKDOWN_FIELDS {
        if let Some(value) = patch.get(*key) {
            links.write_database_document(&mount_point_id, path, value.as_str().unwrap_or(""))?;
            db_patch.remove(*key);
        }
    }

    // 2. properties.json — read-modify-write over only the touched keys.
    let touched: Vec<&str> = PROPERTY_KEYS
        .iter()
        .copied()
        .filter(|k| patch.contains_key(*k))
        .collect();
    if !touched.is_empty() {
        // Seed from the current properties.json; fall back to the empty-managed
        // default (the raw slim row carries no managed values post-cutover).
        let current = read_current_properties(mount, &mount_point_id)?
            .unwrap_or_else(empty_properties_default);

        let pronouns: Option<Pronouns> = if patch.contains_key("pronouns") {
            patch_pronouns(patch.get("pronouns").unwrap())
        } else {
            // Convert the read-side `Pronouns` (vault_overlay) to the write-side one.
            current.pronouns.as_ref().map(|p| Pronouns {
                subject: p.subject.clone(),
                object: p.object.clone(),
                possessive: p.possessive.clone(),
            })
        };
        let aliases = if patch.contains_key("aliases") {
            patch_string_array(patch.get("aliases").unwrap())
        } else {
            current.aliases
        };
        let title = if patch.contains_key("title") {
            patch_nullable_string(patch.get("title").unwrap())
        } else {
            current.title
        };
        let first_message = if patch.contains_key("firstMessage") {
            patch_nullable_string(patch.get("firstMessage").unwrap())
        } else {
            current.first_message
        };
        let talkativeness = if patch.contains_key("talkativeness") {
            patch.get("talkativeness").unwrap().as_f64().unwrap_or(0.5)
        } else {
            current.talkativeness
        };

        let json = render_properties_json(
            pronouns.as_ref(),
            &aliases,
            title.as_deref(),
            first_message.as_deref(),
            talkativeness,
        );
        links.write_database_document(&mount_point_id, PROPERTIES_JSON_PATH, &json)?;
        for k in &touched {
            db_patch.remove(*k);
        }
    }

    // 3. physicalDescription — non-null writes the two files; null leaves them.
    if let Some(incoming) = patch.get("physicalDescription") {
        if !incoming.is_null() {
            let phys: PhysicalDescriptionWrite = serde_json::from_value(incoming.clone())
                .map_err(|e| DbError::Key(format!("physicalDescription parse: {e}")))?;
            links.write_database_document(
                &mount_point_id,
                PHYSICAL_DESCRIPTION_MD_PATH,
                phys.full_description.as_deref().unwrap_or(""),
            )?;
            links.write_database_document(
                &mount_point_id,
                PHYSICAL_PROMPTS_JSON_PATH,
                &render_physical_prompts_json(Some(&phys)),
            )?;
            db_patch.remove("physicalDescription");
        }
        // Null case: leave the vault files; v4 keeps the key in the DB patch (the
        // slim update ignores it — not a slim column).
    }

    // 4. systemPrompts / scenarios — reproject the folder from the incoming array.
    if let Some(value) = patch.get("systemPrompts") {
        let prompts: Vec<SystemPromptWrite> = parse_array(value, "systemPrompts")?;
        project_array_into_vault_folder(
            &links,
            &docs,
            &mount_point_id,
            PROMPTS_FOLDER,
            &prompts,
            |p| {
                (
                    format!("{}.md", sanitize_file_name(&p.name)),
                    build_system_prompt_file(&p.name, p.is_default, &p.content),
                )
            },
        )?;
        db_patch.remove("systemPrompts");
    }
    if let Some(value) = patch.get("scenarios") {
        let scenarios: Vec<ScenarioWrite> = parse_array(value, "scenarios")?;
        project_array_into_vault_folder(
            &links,
            &docs,
            &mount_point_id,
            SCENARIOS_FOLDER,
            &scenarios,
            |s| {
                (
                    format!("{}.md", sanitize_file_name(&s.title)),
                    build_scenario_file(&s.title, &s.content),
                )
            },
        )?;
        db_patch.remove("scenarios");
    }

    Ok(db_patch)
}

/// Update a character — v4 `CharactersRepository.update`. Routes managed fields to
/// the vault (the write overlay), then runs the slim `_update` for the unmanaged
/// remainder (skipped when empty, so a managed-only update does NOT bump the slim
/// row's `updatedAt`). The closing overlay re-read is a READ concern (no DB
/// mutation) and is left to the read overlay. Returns `true` if the slim row was
/// updated, `false` if the update was managed-only (or the row was absent).
pub fn update_character(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    patch: &Map<String, Value>,
) -> Result<bool, DbError> {
    let db_patch = apply_document_store_write_overlay(main, mount, character_id, patch)?;
    if db_patch.is_empty() {
        return Ok(false);
    }
    let update = slim_update_from_patch(&db_patch)?;
    CharactersRepository::new(main).update(character_id, &update)
}

/// Read the character's `characterDocumentMountPointId` (the vault FK), `None` when
/// absent / unset.
fn read_vault_fk(main: &Connection, character_id: &str) -> Result<Option<String>, DbError> {
    main.query_row(
        "SELECT characterDocumentMountPointId FROM characters WHERE id = ?1",
        params![character_id],
        |row| row.get::<_, Option<String>>(0),
    )
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    })
    .map_err(DbError::from)
}

/// Provision a vault mid-write for a character that has none (v4's
/// `ensureCharacterVault(character)` + reload branch). Reads the raw slim
/// character (its managed values sit at their post-cutover empty defaults), then
/// mints/scaffolds/projects/links the vault via
/// [`super::character_vault::ensure_character_vault`], re-reads the FK it set, and
/// confirms it linked. Returns the new mount-point id so the caller can route the
/// patch's managed fields onto the freshly-provisioned vault.
fn provision_vault_on_the_fly(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
) -> Result<String, DbError> {
    let raw = super::characters_read::find_by_id_raw(main, character_id)?.ok_or_else(|| {
        DbError::Key(format!(
            "applyDocumentStoreWriteOverlay: character {character_id} not found while \
             provisioning a vault on the fly"
        ))
    })?;
    let name = raw
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            DbError::Key(format!(
                "applyDocumentStoreWriteOverlay: character {character_id} row has no name"
            ))
        })?
        .to_string();
    // The raw slim row's managed keys are absent post-cutover, so every managed
    // field deserializes to its default (`None` markdown → `""`, talkativeness →
    // 0.5, …), matching v4's `?? <default>` coalescing off `findByIdRaw`.
    let vault: CharacterVaultWriteInput = serde_json::from_value(raw.clone())
        .map_err(|e| DbError::Key(format!("raw character → vault input: {e}")))?;

    super::character_vault::ensure_character_vault(main, mount, character_id, &name, &vault, None)?;

    // Reload — ensureCharacterVault set characterDocumentMountPointId. Confirm.
    read_vault_fk(main, character_id)?.ok_or_else(|| {
        DbError::Key(format!(
            "applyDocumentStoreWriteOverlay: failed to provision vault for {character_id}"
        ))
    })
}

/// Read + parse the current `properties.json` for the RMW seed.
fn read_current_properties(
    mount: &Connection,
    mount_point_id: &str,
) -> Result<Option<CharacterVaultProperties>, DbError> {
    let docs = DocMountDocumentsRepository::new(mount);
    match docs.find_by_mount_point_and_path(mount_point_id, PROPERTIES_JSON_PATH)? {
        Some(content) => Ok(parse_vault_properties(&content)),
        None => Ok(None),
    }
}

/// The RMW fallback when `properties.json` is missing — the empty-managed default
/// (v4 seeds from the raw slim row, which carries no managed values post-cutover).
fn empty_properties_default() -> CharacterVaultProperties {
    CharacterVaultProperties {
        pronouns: None,
        aliases: Vec::new(),
        title: None,
        first_message: None,
        talkativeness: 0.5,
    }
}

/// A patch `pronouns` value → `Option<Pronouns>` (null → None; an object → parsed
/// into the write-side `Pronouns`).
fn patch_pronouns(v: &Value) -> Option<Pronouns> {
    if v.is_null() {
        return None;
    }
    serde_json::from_value(v.clone()).ok()
}

/// A patch string-array value → `Vec<String>` (null → empty).
fn patch_string_array(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// A patch nullable-string value → `Option<String>` (null/non-string → None).
fn patch_nullable_string(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
}

/// Parse a patch array (`systemPrompts` / `scenarios`) into the typed write inputs.
fn parse_array<T: serde::de::DeserializeOwned>(v: &Value, label: &str) -> Result<Vec<T>, DbError> {
    if v.is_null() {
        return Ok(Vec::new());
    }
    serde_json::from_value(v.clone()).map_err(|e| DbError::Key(format!("{label} parse: {e}")))
}

/// Build the slim-row update patch from the DB-bound remainder. Only the slim
/// (non-managed) columns are recognized; any leftover keys are ignored (v4's
/// `_update` strips `MANAGED_FIELDS` defensively, and a managed key never survives
/// the overlay anyway). `updatedAt` is minted (v4 `_update` mints `now` when the
/// patch carries none).
fn slim_update_from_patch(db_patch: &Map<String, Value>) -> Result<CharacterUpdate, DbError> {
    // Deserialize the recognized slim columns; unknown/managed keys are ignored.
    #[derive(serde::Deserialize, Default)]
    #[serde(rename_all = "camelCase")]
    struct SlimPatch {
        #[serde(default)]
        user_id: Option<String>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        default_image_id: Option<String>,
        #[serde(default)]
        default_connection_profile_id: Option<String>,
        #[serde(default)]
        default_partner_id: Option<String>,
        #[serde(default)]
        default_roleplay_template_id: Option<String>,
        #[serde(default)]
        default_image_profile_id: Option<String>,
        #[serde(default)]
        silly_tavern_data: Option<Value>,
        #[serde(default)]
        is_favorite: Option<bool>,
        #[serde(default)]
        npc: Option<bool>,
        #[serde(default)]
        controlled_by: Option<String>,
        #[serde(default)]
        default_agent_mode_enabled: Option<bool>,
        #[serde(default)]
        default_help_tools_enabled: Option<bool>,
        #[serde(default)]
        default_timestamp_config: Option<super::characters::TimestampConfig>,
        #[serde(default)]
        default_scenario_id: Option<String>,
        #[serde(default)]
        default_system_prompt_id: Option<String>,
        #[serde(default)]
        character_document_mount_point_id: Option<String>,
        #[serde(default)]
        can_dress_themselves: Option<bool>,
        #[serde(default)]
        can_create_outfits: Option<bool>,
        #[serde(default)]
        system_transparency: Option<bool>,
        #[serde(default)]
        core_whisper_enabled: Option<bool>,
        #[serde(default)]
        can_be_carina: Option<bool>,
        #[serde(default)]
        partner_links: Option<Vec<super::characters::PartnerLink>>,
        #[serde(default)]
        tags: Option<Vec<String>>,
        #[serde(default)]
        avatar_overrides: Option<Vec<super::characters::AvatarOverride>>,
    }

    let p: SlimPatch = serde_json::from_value(Value::Object(db_patch.clone()))
        .map_err(|e| DbError::Key(format!("slim patch parse: {e}")))?;

    Ok(CharacterUpdate {
        user_id: p.user_id,
        name: p.name,
        default_image_id: p.default_image_id,
        default_connection_profile_id: p.default_connection_profile_id,
        default_partner_id: p.default_partner_id,
        default_roleplay_template_id: p.default_roleplay_template_id,
        default_image_profile_id: p.default_image_profile_id,
        silly_tavern_data: p.silly_tavern_data,
        is_favorite: p.is_favorite,
        npc: p.npc,
        controlled_by: p.controlled_by,
        default_agent_mode_enabled: p.default_agent_mode_enabled,
        default_help_tools_enabled: p.default_help_tools_enabled,
        default_timestamp_config: p.default_timestamp_config,
        default_scenario_id: p.default_scenario_id,
        default_system_prompt_id: p.default_system_prompt_id,
        character_document_mount_point_id: p.character_document_mount_point_id,
        can_dress_themselves: p.can_dress_themselves,
        can_create_outfits: p.can_create_outfits,
        system_transparency: p.system_transparency,
        core_whisper_enabled: p.core_whisper_enabled,
        can_be_carina: p.can_be_carina,
        partner_links: p.partner_links,
        tags: p.tags,
        avatar_overrides: p.avatar_overrides,
        updated_at: crate::clock::now_iso(),
    })
}
