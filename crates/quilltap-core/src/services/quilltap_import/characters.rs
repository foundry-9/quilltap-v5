//! v4 `importCharacters` (`import-characters.ts`) — id-then-name existence check
//! (the name fallback is the cross-instance import path), all four conflict
//! strategies, the legacy `scenario` string → `scenarios[]` migration, per-
//! character wardrobe (folding pre-rework `outfitPresets` into composites), and
//! per-character plugin data.
//!
//! Strategy notes carried from v4:
//! - **skip** maps the source id onto the MATCHED EXISTING id (unlike every
//!   other kind, where the id itself is the match key).
//! - **overwrite** maps onto the existing id FIRST, deletes the existing slim
//!   row (repo-level `_delete` — the old vault is orphaned, exactly as in v4),
//!   drops the name-map entry so it can't re-match, then falls through to the
//!   plain create — whose REAL minted id then overwrites the map entry.
//! - **duplicate** creates under `<name> (imported)` and maps the REAL created
//!   id (characters do NOT take the phantom-id quirk the other kinds have).

use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{ConflictStrategy, IdMaps, ImportOptions};
use crate::db::character_plugin_data::CharacterPluginDataRepository;
use crate::db::character_vault::create_character_with_options;
use crate::db::characters::{
    AvatarOverride, CharacterCreate, CharactersRepository, PartnerLink, TimestampConfig,
};
use crate::db::characters_read;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::vault_character_write::CharacterVaultWriteInput;
use crate::db::vault_wardrobe_public::{create_vault_wardrobe_item, WardrobePublicError};
use crate::db::DbError;
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

/// v4 `rememberBundleVault` (`import-characters.ts:50`, `01e481f6`) — record
/// which vault mount point the bundle claimed for a character we just created.
/// v4's note, verbatim:
///
/// > `characters.create()` deliberately drops the incoming pointer and
/// > provisions a scaffold vault, so this is the only surviving trace of the
/// > store the bundle meant — reconciliation needs it to repoint the character
/// > at its imported vault and tear the scaffold down.
/// >
/// > Characters exported before WP A2 carry no vault, so nothing is recorded
/// > and reconciliation leaves the scaffold in place.
fn remember_bundle_vault(id_maps: &mut IdMaps, exported: &Value, new_character_id: &str) {
    if let Some(vault_id) = exported
        .get("characterDocumentMountPointId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        id_maps
            .character_vault_mounts
            .set(new_character_id.to_string(), vault_id.to_string());
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn import_characters(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    characters: &[Value],
    options: &ImportOptions,
    id_maps: &mut IdMaps,
    warnings: &mut Vec<String>,
) -> Result<Counts, DbError> {
    let mut imported = 0u32;
    let mut skipped = 0u32;

    // Pre-fetch existing characters for name-based matching (cross-instance
    // imports). v4 lowercases the name for the map key; a later duplicate name
    // overwrites the earlier entry (JS `Map.set` — the LAST one wins).
    let existing = characters_read::find_all_raw(main)?;
    let mut existing_by_name: Vec<(String, String)> = Vec::new();
    for c in &existing {
        let (Some(id), Some(name)) = (
            c.get("id").and_then(Value::as_str),
            c.get("name").and_then(Value::as_str),
        ) else {
            continue;
        };
        let key = name.to_lowercase();
        match existing_by_name.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = id.to_string(),
            None => existing_by_name.push((key, id.to_string())),
        }
    }

    for raw_character in characters {
        let mut character = migrate_character_scenarios(raw_character);
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

        let out: Result<(), String> = (|| {
            // Skip-if-present rehydrate (spec §6/F4, `01e481f6`): this IS the
            // character being rehydrated — its row survived the archive. Map it
            // to itself and move on. v4's note on why this is NOT the
            // conflict-strategy `skip` branch below, verbatim:
            //
            // > that one adds the vault to skippedCharacterVaults, which would
            // > drop the very store records this import exists to restore. Nor
            // > rememberBundleVault: the character still points at its own
            // > vault, so reconciliation must not repoint anything or tear down
            // > a "scaffold".
            if options.preserve_ids && id_maps.preserve_ids_skips.contains(&source_id) {
                id_maps.characters.set(source_id.clone(), source_id.clone());
                skipped += 1;
                return Ok(());
            }

            // Check by ID first (same-instance re-import), then by name
            // (cross-instance).
            let existing_id: Option<String> =
                match characters_read::find_by_id_raw(main, &source_id)
                    .map_err(|e| e.to_string())?
                {
                    Some(_) => Some(source_id.clone()),
                    None => existing_by_name
                        .iter()
                        .find(|(n, _)| *n == name.to_lowercase())
                        .map(|(_, id)| id.clone()),
                };

            let mut duplicate_rename = false;
            if let Some(existing_id) = existing_id {
                match options.conflict_strategy {
                    ConflictStrategy::Skip => {
                        skipped += 1;
                        id_maps.characters.set(source_id.clone(), existing_id);
                        // The existing character keeps its own vault untouched,
                        // so the bundle's store must not be imported at all —
                        // it would land as a store nothing points at.
                        if let Some(vault_id) = character
                            .get("characterDocumentMountPointId")
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                        {
                            id_maps
                                .skipped_character_vaults
                                .insert(vault_id.to_string());
                        }
                        return Ok(());
                    }
                    ConflictStrategy::Overwrite => {
                        // Map old import id to the existing id before deleting, so
                        // related entities get re-linked to the replacement should
                        // the create below fail; the create's real id then
                        // overwrites this entry.
                        id_maps
                            .characters
                            .set(source_id.clone(), existing_id.clone());
                        CharactersRepository::new(main)
                            .delete(&existing_id)
                            .map_err(|e| e.to_string())?;
                        // Remove from the name map so we don't re-match.
                        existing_by_name.retain(|(n, _)| *n != name.to_lowercase());
                    }
                    ConflictStrategy::Duplicate => {
                        duplicate_rename = true;
                    }
                }
            }

            if duplicate_rename {
                // v4: create({...charData, name: `${name} (imported)`}) — the
                // override reaches the slim row AND the vault provisioning.
                if let Some(obj) = character.as_object_mut() {
                    obj.insert(
                        "name".to_string(),
                        Value::String(format!("{name} (imported)")),
                    );
                }
            }

            // Build the slim row + the vault-managed inputs from the (migrated)
            // character, then create end-to-end. `create_character` mints a fresh
            // id (v4's `repos.characters.create` strips the source id).
            let slim: ImportedSlim =
                serde_json::from_value(character.clone()).map_err(|e| e.to_string())?;
            // Deserializing the WHOLE character threads `metadata` through the
            // round-trip: `CharacterVaultWriteInput` carries the fact sheet, so
            // `create_character` projects it into `metadata.json`.
            let vault: CharacterVaultWriteInput =
                serde_json::from_value(character.clone()).map_err(|e| e.to_string())?;

            // v4 `01e481f6`: under `preserveIds` the PLAIN create claims the
            // bundle's own character id (`create(createData, { id })`) — but
            // the name-conflict `duplicate` branch does NOT: v4's
            // `import-characters.ts:141-153` creates the "(imported)" copy with
            // no createOptions, so it always mints (the same discipline
            // `entities.rs` names `DUPLICATE_MINTS`). Caught by the round-1
            // unification review: passing the bundle id here would claim it
            // for the renamed copy where v4 leaves it free.
            let duplicate_mints = ""; // the `entities.rs` `DUPLICATE_MINTS` discipline
            let (new_row_id, now) = if duplicate_rename {
                super::mint_or_preserve(options, duplicate_mints)
            } else {
                super::mint_or_preserve(options, &source_id)
            };
            let new_id = create_character_with_options(
                main,
                mount,
                &slim.into_create(user_id),
                &vault,
                &crate::db::characters::CreateOptions {
                    id: new_row_id,
                    created_at: now.clone(),
                    updated_at: now,
                },
            )
            .map_err(|e| e.to_string())?;
            id_maps.characters.set(source_id.clone(), new_id.clone());
            remember_bundle_vault(id_maps, &character, &new_id);

            // Per-character wardrobe items (folding any legacy outfitPresets into
            // composites for pre-rework `.qtap` exports).
            import_character_wardrobe_items(main, mount, raw_character, &new_id, warnings);

            // Per-character plugin data.
            import_character_plugin_data(main, raw_character, &new_id, warnings);

            imported += 1;
            Ok(())
        })();
        if let Err(e) = out {
            // v4 wraps each character in a try that pushes to warnings and
            // continues.
            warnings.push(format!("Failed to import character \"{name}\": {e}"));
            tracing::warn!(character_id = %source_id, error = %e, "Failed to import character");
        }
    }

    Ok(Counts { imported, skipped })
}

/// v4 `importCharacterWardrobeItems` (`import-characters.ts:187`): each item with
/// a `characterId` (archetype items — `characterId: null` — are skipped) is
/// re-created under `newCharacterId` with `migratedFromClothingRecordId: null`,
/// via the **vault-backed** `repos.wardrobe.create`. Strips id/characterId/
/// createdAt/updatedAt (create mints).
///
/// Back-compat: pre-rework exports may carry `outfitPresets`; each is folded
/// into a composite item — UNLESS the import already contains wardrobe items
/// with a non-empty `componentItemIds` (a post-rework export), in which case the
/// fold is skipped so the same composite isn't double-created.
fn import_character_wardrobe_items(
    main: &Connection,
    mount: &Connection,
    raw_character: &Value,
    new_character_id: &str,
    warnings: &mut Vec<String>,
) {
    let mut combined: Vec<Value> = raw_character
        .get("wardrobeItems")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    if let Some(presets) = raw_character
        .get("outfitPresets")
        .and_then(Value::as_array)
        .filter(|p| !p.is_empty())
    {
        let has_composites = combined.iter().any(|item| {
            item.get("componentItemIds")
                .and_then(Value::as_array)
                .map(|a| !a.is_empty())
                .unwrap_or(false)
        });
        if has_composites {
            tracing::debug!(
                new_character_id,
                "Skipping legacy outfit-preset fold; export already contains composite wardrobe items"
            );
        } else {
            combined.extend(
                presets
                    .iter()
                    .map(super::legacy_presets::legacy_preset_to_composite),
            );
        }
    }

    if combined.is_empty() {
        return;
    }

    let links = DocMountFileLinksRepository::new(mount);
    let docs = DocMountDocumentsRepository::new(mount);

    // Skip archetype items (characterId = null) — shared, not per-character.
    let importable: Vec<&Value> = combined
        .iter()
        .filter(|item| {
            item.get("characterId")
                .map(|v| !v.is_null())
                .unwrap_or(false)
        })
        .collect();

    // Item ids are re-minted on import, so composite `componentItemIds` — which
    // reference the export's original ids — must be remapped to the new ids
    // (v4 Bug 75, `40d507cc`). Pre-assign every new id, remap the references,
    // and create leaf items before the composites that bundle them so no
    // composite is ever written ahead of its components. References that don't
    // resolve within this character's own items (e.g. archetype components) are
    // dropped with a warning rather than left dangling.
    let new_id_by_old_id: std::collections::HashMap<String, String> = importable
        .iter()
        .filter_map(|item| {
            item.get("id")
                .and_then(Value::as_str)
                .map(|id| (id.to_string(), uuid::Uuid::new_v4().to_string()))
        })
        .collect();

    // v4 `compositeDepth(item, seen)` — 0 for a leaf or an already-seen id (the
    // shared `seen` set is the cycle guard); otherwise 1 + the deepest resolved
    // component.
    fn composite_depth(
        item: &Value,
        importable: &[&Value],
        seen: &mut std::collections::HashSet<String>,
    ) -> usize {
        let component_ids: Vec<&str> = item
            .get("componentItemIds")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
        if component_ids.is_empty() || seen.contains(id) {
            return 0;
        }
        seen.insert(id.to_string());
        let mut max = 0;
        for component_id in component_ids {
            if let Some(component) = importable
                .iter()
                .find(|i| i.get("id").and_then(Value::as_str) == Some(component_id))
            {
                max = max.max(composite_depth(component, importable, seen) + 1);
            }
        }
        max
    }

    // v4 sorts with a fresh `seen` per comparison; a STABLE sort on the
    // precomputed depth yields the same order (ties keep export order).
    let mut ordered: Vec<&Value> = importable.clone();
    ordered.sort_by_key(|item| {
        composite_depth(item, &importable, &mut std::collections::HashSet::new())
    });

    for item in ordered {
        let title = item
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        let original_component_ids = str_array(item, "componentItemIds");
        let remapped_component_ids: Vec<String> = original_component_ids
            .iter()
            .filter_map(|old_id| new_id_by_old_id.get(old_id).cloned())
            .collect();
        if remapped_component_ids.len() != original_component_ids.len() {
            warnings.push(format!(
                "Wardrobe item \"{}\" referenced {} component item(s) not present in the import; those references were dropped.",
                title,
                original_component_ids.len() - remapped_component_ids.len()
            ));
        }

        // Timestamps minted here (v4 `repos.wardrobe.create` materializes them
        // before the vault projection); the id is the PRE-MINTED one from the
        // remap table (v4 `wardrobe.create({...}, { id })`), so composite
        // references written above resolve. `migratedFromClothingRecordId`
        // forced null on import; `characterId` set to the new character; the
        // rest passed through.
        let now = crate::clock::now_iso();
        let stored = WardrobeItem {
            id: item
                .get("id")
                .and_then(Value::as_str)
                .and_then(|id| new_id_by_old_id.get(id).cloned())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            character_id: Some(Some(new_character_id.to_string())),
            title: title.clone(),
            description: Some(opt_string(item, "description")),
            image_prompt: Some(opt_string(item, "imagePrompt")),
            types: str_array(item, "types"),
            component_item_ids: remapped_component_ids,
            appropriateness: Some(opt_string(item, "appropriateness")),
            is_default: item
                .get("isDefault")
                .and_then(Value::as_bool)
                .unwrap_or(false),
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
}

/// v4 `importCharacterPluginData` (`import-characters.ts:264`): upsert each
/// plugin's payload under the new character id; per-plugin failure → warning +
/// continue.
fn import_character_plugin_data(
    main: &Connection,
    raw_character: &Value,
    new_character_id: &str,
    warnings: &mut Vec<String>,
) {
    let Some(plugin_data) = raw_character.get("pluginData").and_then(Value::as_object) else {
        return;
    };
    if plugin_data.is_empty() {
        return;
    }
    let repo = CharacterPluginDataRepository::new(main);
    for (plugin_name, data) in plugin_data {
        if let Err(e) = repo.upsert(new_character_id, plugin_name, data.clone()) {
            warnings.push(format!(
                "Failed to import plugin data for \"{plugin_name}\": {e}"
            ));
            tracing::warn!(plugin_name = %plugin_name, character_id = %new_character_id, error = %e,
                "Failed to import plugin data");
        }
    }
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
