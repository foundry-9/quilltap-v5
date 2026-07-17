//! v4 `importCharacters` (`import-characters.ts`) — the seed subset:
//! id-then-name existence check → `skip`, else `create_character` (mints a fresh
//! id, provisions the vault, projects the managed fields) + per-character
//! wardrobe. The legacy `scenario` string → `scenarios[]` migration FIRES for the
//! seed (both characters carry the legacy field). The archetype-skip and the
//! `legacyPresetToComposite` fold are NOT taken (no `outfitPresets`, all wardrobe
//! items have a `characterId`). Non-empty `pluginData` is a loud refusal.

use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{ConflictStrategy, IdMap, ImportError, ImportOptions};
use crate::db::character_vault::create_character;
use crate::db::characters::{AvatarOverride, CharacterCreate, PartnerLink, TimestampConfig};
use crate::db::characters_read;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::vault_character_write::CharacterVaultWriteInput;
use crate::db::vault_wardrobe_public::{create_vault_wardrobe_item, WardrobePublicError};
use crate::vault_overlay::WardrobeItem;

pub(super) struct Counts {
    pub imported: u32,
    pub skipped: u32,
}

/// v4 `migrateCharacterScenarios` (`import-characters.ts:24`). Returns the
/// character with a `scenarios` array: unchanged if it already has one; a single
/// `'Default'` scenario when it carries a legacy `scenario` string; else an empty
/// array. The minted scenario id + timestamps are irrelevant to the vault
/// projection (`ScenarioWrite` reads only title + content), so a fixed id keeps
/// this deterministic.
fn migrate_character_scenarios(character: &Value) -> Value {
    let mut out = character.clone();
    let obj = match out.as_object_mut() {
        Some(o) => o,
        None => return out,
    };
    // Already has scenarios → nothing to do.
    if obj.get("scenarios").map(|v| !v.is_null()).unwrap_or(false) {
        return out;
    }
    // Legacy `scenario` string → a single 'Default' scenario.
    if let Some(scenario) = obj.get("scenario").and_then(Value::as_str) {
        if !scenario.is_empty() {
            obj.insert(
                "scenarios".to_string(),
                json!([{
                    "title": "Default",
                    "content": scenario,
                }]),
            );
            return out;
        }
    }
    // No scenario field at all → empty scenarios.
    obj.insert("scenarios".to_string(), json!([]));
    out
}

/// The slim (non-managed) subset of an imported character, deserialized with v4's
/// `repos.characters.create` defaults (`?? []` arrays, `?? false` bools,
/// controlledBy `'llm'`, everything else absent → `None`). The vault-managed keys
/// in the same object are ignored (serde drops unknown fields).
#[derive(Deserialize)]
struct ImportedSlim {
    #[serde(default)]
    name: String,
    #[serde(default, rename = "defaultImageId")]
    default_image_id: Option<String>,
    #[serde(default, rename = "defaultConnectionProfileId")]
    default_connection_profile_id: Option<String>,
    #[serde(default, rename = "defaultPartnerId")]
    default_partner_id: Option<String>,
    #[serde(default, rename = "defaultRoleplayTemplateId")]
    default_roleplay_template_id: Option<String>,
    #[serde(default, rename = "defaultImageProfileId")]
    default_image_profile_id: Option<String>,
    #[serde(default, rename = "sillyTavernData")]
    silly_tavern_data: Option<Value>,
    #[serde(default, rename = "isFavorite")]
    is_favorite: bool,
    #[serde(default)]
    npc: bool,
    #[serde(default = "default_controlled_by", rename = "controlledBy")]
    controlled_by: String,
    #[serde(default, rename = "defaultAgentModeEnabled")]
    default_agent_mode_enabled: Option<bool>,
    #[serde(default, rename = "defaultHelpToolsEnabled")]
    default_help_tools_enabled: Option<bool>,
    #[serde(default, rename = "defaultTimestampConfig")]
    default_timestamp_config: Option<TimestampConfig>,
    #[serde(default, rename = "defaultScenarioId")]
    default_scenario_id: Option<String>,
    #[serde(default, rename = "defaultSystemPromptId")]
    default_system_prompt_id: Option<String>,
    #[serde(default, rename = "canDressThemselves")]
    can_dress_themselves: Option<bool>,
    #[serde(default, rename = "canCreateOutfits")]
    can_create_outfits: Option<bool>,
    #[serde(default, rename = "systemTransparency")]
    system_transparency: Option<bool>,
    #[serde(default, rename = "coreWhisperEnabled")]
    core_whisper_enabled: Option<bool>,
    #[serde(default, rename = "canBeCarina")]
    can_be_carina: Option<bool>,
    #[serde(default, rename = "partnerLinks")]
    partner_links: Vec<PartnerLink>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default, rename = "avatarOverrides")]
    avatar_overrides: Vec<AvatarOverride>,
}

fn default_controlled_by() -> String {
    "llm".to_string()
}

impl ImportedSlim {
    fn into_create(self, user_id: &str) -> CharacterCreate {
        CharacterCreate {
            user_id: user_id.to_string(),
            name: self.name,
            default_image_id: self.default_image_id,
            default_connection_profile_id: self.default_connection_profile_id,
            default_partner_id: self.default_partner_id,
            default_roleplay_template_id: self.default_roleplay_template_id,
            default_image_profile_id: self.default_image_profile_id,
            silly_tavern_data: self.silly_tavern_data,
            is_favorite: self.is_favorite,
            npc: self.npc,
            controlled_by: self.controlled_by,
            default_agent_mode_enabled: self.default_agent_mode_enabled,
            default_help_tools_enabled: self.default_help_tools_enabled,
            default_timestamp_config: self.default_timestamp_config,
            default_scenario_id: self.default_scenario_id,
            default_system_prompt_id: self.default_system_prompt_id,
            // create always provisions a fresh vault; the FK is nulled internally.
            character_document_mount_point_id: None,
            can_dress_themselves: self.can_dress_themselves,
            can_create_outfits: self.can_create_outfits,
            system_transparency: self.system_transparency,
            core_whisper_enabled: self.core_whisper_enabled,
            can_be_carina: self.can_be_carina,
            partner_links: self.partner_links,
            tags: self.tags,
            avatar_overrides: self.avatar_overrides,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn import_characters(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    characters: &[Value],
    options: &ImportOptions,
    id_map: &mut IdMap,
    warnings: &mut Vec<String>,
) -> Result<Counts, ImportError> {
    let mut imported = 0u32;
    let mut skipped = 0u32;

    // Pre-fetch existing characters for name-based matching (cross-instance
    // imports). v4 lowercases the name for the map key.
    let existing = characters_read::find_all_raw(main)?;
    let existing_by_name: Vec<(String, String)> = existing
        .iter()
        .filter_map(|c| {
            let id = c.get("id").and_then(Value::as_str)?;
            let name = c.get("name").and_then(Value::as_str)?;
            Some((name.to_lowercase(), id.to_string()))
        })
        .collect();

    for raw_character in characters {
        let character = migrate_character_scenarios(raw_character);
        let source_id = character
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let name = character
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        // Check by ID first (same-instance re-import), then by name
        // (cross-instance).
        let existing_id: Option<String> = match characters_read::find_by_id_raw(main, &source_id)? {
            Some(_) => Some(source_id.clone()),
            None => existing_by_name
                .iter()
                .find(|(n, _)| *n == name.to_lowercase())
                .map(|(_, id)| id.clone()),
        };

        if let Some(existing_id) = existing_id {
            match options.conflict_strategy {
                ConflictStrategy::Skip => {
                    skipped += 1;
                    id_map.set(source_id.clone(), existing_id);
                    continue;
                }
                // Dead for both seed consumers — refuse loudly (the subset guard in
                // `execute_import` already rejects these strategies up front, so
                // this is belt-and-braces).
                other => {
                    return Err(ImportError::UnsupportedConflictStrategy(other));
                }
            }
        }

        // Loud refusal: non-empty pluginData is outside the subset. The seed
        // characters carry none (the key is absent).
        if let Some(plugin_data) = raw_character.get("pluginData").and_then(Value::as_object) {
            if !plugin_data.is_empty() {
                return Err(ImportError::UnsupportedPluginData {
                    character_name: name,
                });
            }
        }

        // The create path: build the slim row + the vault-managed inputs from the
        // (migrated) character, then create end-to-end. `create_character` mints a
        // fresh id (v4's `repos.characters.create` strips the source id).
        let slim: ImportedSlim = match serde_json::from_value(character.clone()) {
            Ok(s) => s,
            Err(e) => {
                warnings.push(format!("Failed to import character \"{name}\": {e}"));
                continue;
            }
        };
        // Deserializing the WHOLE character here is what threads `metadata` through
        // the qtap round-trip: `CharacterVaultWriteInput` carries the fact sheet, so
        // `create_character` → `write_character_vault_managed_fields` projects it into
        // `metadata.json` (the guarded write). `into_create` deliberately does NOT —
        // there is no DB column. Omitting the field here would silently drop the fact
        // sheet on every import.
        let vault: CharacterVaultWriteInput = match serde_json::from_value(character.clone()) {
            Ok(v) => v,
            Err(e) => {
                warnings.push(format!("Failed to import character \"{name}\": {e}"));
                continue;
            }
        };

        let new_id = match create_character(main, mount, &slim.into_create(user_id), &vault) {
            Ok(id) => id,
            Err(e) => {
                // v4 wraps each character in a try that pushes to warnings and
                // continues. But a store failure here means the vault write broke
                // mid-transaction — surface it as the top-level catch (success:false)
                // by propagating the DbError.
                return Err(ImportError::Db(e));
            }
        };
        id_map.set(source_id.clone(), new_id.clone());

        // Per-character wardrobe items (folding legacy outfitPresets is NOT taken —
        // the seed carries none).
        import_character_wardrobe_items(main, mount, raw_character, &new_id, warnings)?;

        imported += 1;
    }

    Ok(Counts { imported, skipped })
}

/// v4 `importCharacterWardrobeItems` (the seed path): each item with a
/// `characterId` (archetype items — `characterId: null` — are skipped) is
/// re-created under `newCharacterId` with `migratedFromClothingRecordId: null`,
/// via the **vault-backed** `repos.wardrobe.create` (each item is projected into
/// the character vault's `Wardrobe/` folder — there is no slim `wardrobe_items`
/// row). Strips id/characterId/createdAt/updatedAt (create mints a fresh id).
fn import_character_wardrobe_items(
    main: &Connection,
    mount: &Connection,
    raw_character: &Value,
    new_character_id: &str,
    warnings: &mut Vec<String>,
) -> Result<(), ImportError> {
    let items = match raw_character.get("wardrobeItems").and_then(Value::as_array) {
        Some(items) if !items.is_empty() => items,
        _ => return Ok(()),
    };

    let links = DocMountFileLinksRepository::new(mount);
    let docs = DocMountDocumentsRepository::new(mount);
    for item in items {
        // Skip archetype items (characterId = null) — shared, not per-character.
        let has_character_id = item
            .get("characterId")
            .map(|v| !v.is_null())
            .unwrap_or(false);
        if !has_character_id {
            continue;
        }

        let title = item
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        // Mint id/timestamps (v4 `repos.wardrobe.create` materializes them before
        // the vault projection). `migratedFromClothingRecordId` forced null on
        // import; `characterId` set to the new character; the rest passed through.
        let now = crate::clock::now_iso();
        let stored = WardrobeItem {
            id: uuid::Uuid::new_v4().to_string(),
            character_id: Some(Some(new_character_id.to_string())),
            title: title.clone(),
            description: Some(opt_string(item, "description")),
            image_prompt: Some(opt_string(item, "imagePrompt")),
            types: str_array(item, "types"),
            component_item_ids: str_array(item, "componentItemIds"),
            appropriateness: Some(opt_string(item, "appropriateness")),
            is_default: item
                .get("isDefault")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            // `replace` is a create-time flag; the import payload never sets it.
            replace: item
                .get("replace")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            migrated_from_clothing_record_id: Some(None),
            archived_at: Some(opt_string(item, "archivedAt")),
            created_at: now.clone(),
            updated_at: now,
        };

        match create_vault_wardrobe_item(main, &links, &docs, &stored) {
            Ok(_) => {}
            // Per-item failure → warning + continue (v4). `NoMount` would mean the
            // freshly-provisioned vault didn't resolve — surface it as a warning.
            Err(WardrobePublicError::NoMount) => {
                warnings.push(format!(
                    "Failed to import wardrobe item \"{title}\": character has no wardrobe vault"
                ));
            }
            Err(WardrobePublicError::Cycle(msg)) => {
                warnings.push(format!("Failed to import wardrobe item \"{title}\": {msg}"));
            }
            Err(WardrobePublicError::Db(e)) => {
                warnings.push(format!("Failed to import wardrobe item \"{title}\": {e}"));
            }
        }
    }

    Ok(())
}

/// `Some(string)` when the key holds a string, else `None` — v4's `?? null` on
/// optional string fields.
fn opt_string(obj: &Value, key: &str) -> Option<String> {
    obj.get(key).and_then(Value::as_str).map(|s| s.to_string())
}

/// A JSON string-array column (`types` / `componentItemIds`) → `Vec<String>`
/// (empty when absent).
fn str_array(obj: &Value, key: &str) -> Vec<String> {
    obj.get(key)
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default()
}
