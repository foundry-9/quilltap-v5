//! v4 `reconcileRelationships` (`reconcile.ts:25`) — walk every imported entity
//! and rewrite its relationship FKs (tags, default profile/partner/template ids,
//! participants, project rosters, mount-point links) through the id maps now
//! that every phase has populated them.
//!
//! The loops iterate the id maps in insertion order and read each DESTINATION
//! row; a `duplicate`-arm phantom id (see the entity importers) simply reads to
//! `None` and is skipped, exactly as v4's `findById` returns null for it.
//! `remapId` misses resolve to null, and every `if (newX)` guard means a failed
//! remap leaves the stored value alone — the character-vault mount FK in
//! particular is preserved rather than nulled (orphaned-vault avoidance, v4's
//! own comment at `reconcile.ts:99-106`).

use rusqlite::Connection;
use serde_json::{Map, Value};

use super::{IdMap, IdMaps};
use crate::db::characters::{AvatarOverride, CharacterUpdate, CharactersRepository};
use crate::db::chats::{ChatParticipant, ChatUpdate, ChatsRepository};
use crate::db::roleplay_templates::{RoleplayTemplatesRepository, RtUpdate};
use crate::db::{
    characters_read, chats_read, connection_profiles, embedding_profiles, image_profiles, projects,
    roleplay_templates,
};

/// v4's `remapId`: null on a miss.
fn remap_id(id: Option<&str>, map: &IdMap) -> Option<String> {
    id.filter(|s| !s.is_empty())
        .and_then(|s| map.get(s))
        .map(|s| s.to_string())
}

/// v4's `remapIdArray`: `get(id) || id` — an unmapped id keeps itself.
fn remap_id_array(ids: &[String], map: &IdMap) -> Vec<String> {
    ids.iter()
        .map(|id| {
            map.get(id)
                .map(|s| s.to_string())
                .unwrap_or_else(|| id.clone())
        })
        .collect()
}

fn str_array_of(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// v4 `discardScaffoldVault` (`reconcile.ts:36`, `01e481f6`, Bug 52) — tear down
/// the scaffold vault `characters.create()` provisioned, now that the character
/// points at the vault its bundle carried. v4's note, verbatim:
///
/// > Goes through `deleteStoreCascade` — the chokepoint that runs link-group
/// > orphan GC — never a bare mount-point delete.
/// >
/// > Also hands the canonical vault name back: store names live in one
/// > case-insensitive namespace, so with the scaffold holding
/// > "<name> Character Vault" the imported vault was uniquified to "… (2)" on
/// > the way in. With the scaffold gone the plain name is free, and a character
/// > whose vault is permanently named "(2)" is a visible wart in the
/// > Scriptorium.
///
/// v5's chokepoint is `api::mount_points::cascade_delete` (P4.31, dogfood #58) —
/// the same rule, in one transaction.
fn discard_scaffold_vault(
    mount: &Connection,
    scaffold_mount_id: &str,
    imported_vault_id: &str,
    character_id: &str,
    warnings: &mut Vec<String>,
) {
    let out: Result<(), String> = (|| {
        let points = crate::db::doc_mount_points::DocMountPointsRepository::new(mount);
        // v4 reads the scaffold BEFORE deleting it — the name handback needs it.
        let scaffold = points
            .find_full_json_by_id(scaffold_mount_id)
            .map_err(|e| e.to_string())?;
        crate::api::mount_points::cascade_delete(mount, scaffold_mount_id)
            .map_err(|e| e.to_string())?;
        tracing::debug!(
            character_id,
            scaffold_mount_id,
            imported_vault_id,
            "Discarded scaffold vault in favour of the imported one"
        );

        let Some(scaffold) = scaffold else {
            return Ok(());
        };
        let scaffold_name = scaffold
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let all_stores =
            crate::db::doc_mount_points::find_all_full_json(mount).map_err(|e| e.to_string())?;
        // v4: *"The scaffold itself never counts as holding the name — we just
        // deleted it, and a read served from a stale cache would otherwise block
        // the rename forever."*
        let name_taken = all_stores.iter().any(|mp| {
            let id = mp.get("id").and_then(Value::as_str).unwrap_or_default();
            id != imported_vault_id
                && id != scaffold_mount_id
                && mp
                    .get("name")
                    .and_then(Value::as_str)
                    .map(|n| n.to_lowercase())
                    == Some(scaffold_name.to_lowercase())
        });
        if name_taken {
            return Ok(());
        }
        let imported = all_stores
            .iter()
            .find(|mp| mp.get("id").and_then(Value::as_str) == Some(imported_vault_id));
        if let Some(imported) = imported {
            if imported.get("name").and_then(Value::as_str) != Some(scaffold_name.as_str()) {
                points
                    .update(
                        imported_vault_id,
                        &crate::db::doc_mount_points::DmpUpdate {
                            name: Some(scaffold_name.clone()),
                            updated_at: crate::clock::now_iso(),
                            ..Default::default()
                        },
                    )
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    })();
    if let Err(e) = out {
        warnings.push(format!(
            "Failed to remove the placeholder vault for an imported character: {e}"
        ));
        tracing::warn!(character_id, scaffold_mount_id, error = %e, "Failed to discard scaffold vault");
    }
}

/// v4 `reconcileRelationships`.
pub(super) fn reconcile_relationships(
    main: &Connection,
    mount: &Connection,
    id_maps: &IdMaps,
    warnings: &mut Vec<String>,
) {
    // ── Characters ─────────────────────────────────────────────────────────
    for (backup_id, new_id) in id_maps.characters.iter() {
        // Skip-if-present rehydrate (spec §6/F4, `01e481f6`): the surviving
        // character was never re-created — every id it references maps to
        // itself, so there is nothing to remap and no scaffold to tear down.
        // v4: *"Attempting the identity patch anyway would be refused by the
        // archived-row write guard and surface as a spurious warning."*
        if backup_id == new_id && id_maps.preserve_ids_skips.contains(new_id) {
            continue;
        }
        let out: Result<(), String> = (|| {
            let Some(character) =
                characters_read::find_by_id(main, mount, new_id).map_err(|e| e.to_string())?
            else {
                return Ok(());
            };

            let mut patch = CharacterUpdate::default();
            let mut has_updates = false;

            let tags = str_array_of(&character, "tags");
            if !tags.is_empty() {
                let remapped = remap_id_array(&tags, &id_maps.tags);
                if !remapped.is_empty() {
                    patch.tags = Some(remapped);
                    has_updates = true;
                }
            }

            if let Some(new_partner) =
                remap_id(str_of(&character, "defaultPartnerId"), &id_maps.characters)
            {
                patch.default_partner_id = Some(new_partner);
                has_updates = true;
            }

            if let Some(new_conn) = remap_id(
                str_of(&character, "defaultConnectionProfileId"),
                &id_maps.connection_profiles,
            ) {
                patch.default_connection_profile_id = Some(new_conn);
                has_updates = true;
            }

            if let Some(new_img) = remap_id(
                str_of(&character, "defaultImageProfileId"),
                &id_maps.image_profiles,
            ) {
                patch.default_image_profile_id = Some(new_img);
                has_updates = true;
            }

            if let Some(new_tpl) = remap_id(
                str_of(&character, "defaultRoleplayTemplateId"),
                &id_maps.roleplay_templates,
            ) {
                patch.default_roleplay_template_id = Some(new_tpl);
                has_updates = true;
            }

            // The bundle carried this character's whole vault (WP A2,
            // `01e481f6`), and `create()` has already provisioned a scaffold
            // vault whose fresh id is what the row currently holds. **Bundle
            // wins, whole-store**: repoint at the imported store, then tear the
            // scaffold down after the update lands. Never merge the two.
            //
            // `scaffold_mount_id` is the store to cascade-delete once the
            // repoint has landed.
            let mut scaffold_mount_id: Option<String> = None;
            let stored_vault = str_of(&character, "characterDocumentMountPointId")
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let bundle_vault_id = id_maps.character_vault_mounts.get(new_id);
            let imported_vault_id =
                bundle_vault_id.and_then(|b| remap_id(Some(b), &id_maps.mount_points));
            if let Some(imported) = imported_vault_id
                .clone()
                .filter(|v| Some(v.as_str()) != stored_vault.as_deref())
            {
                scaffold_mount_id = stored_vault.clone();
                patch.character_document_mount_point_id = Some(imported);
                has_updates = true;
            } else if stored_vault.is_some() {
                // Pre-A2 bundle (no vault records): only rewrite when the stored
                // value resolves to a remapped mount-point row — the
                // freshly-provisioned scaffold FK must survive a failed remap
                // (v4's orphaned-vault fix).
                if let Some(new_mount) = remap_id(stored_vault.as_deref(), &id_maps.mount_points) {
                    patch.character_document_mount_point_id = Some(new_mount);
                    has_updates = true;
                }
            }

            // [Bug 52] Remap the avatar ids through the imported document-store
            // link map. These values are `doc_mount_file_links.id` values in the
            // SOURCE instance's vault. v4: *"If the source used a legacy
            // files.id, leave it unchanged; otherwise null it with a warning so
            // the character never keeps a dangling reference after import."*
            let character_name = str_of(&character, "name").unwrap_or_default().to_string();
            let files_repo = crate::db::files::FilesRepository::new(main);
            let mut clear_default_image_id = false;
            if let Some(default_image_id) =
                str_of(&character, "defaultImageId").filter(|s| !s.is_empty())
            {
                if let Some(remapped) =
                    remap_id(Some(default_image_id), &id_maps.doc_mount_file_links)
                {
                    patch.default_image_id = Some(remapped);
                    has_updates = true;
                } else if files_repo
                    .find_by_id(default_image_id)
                    .map_err(|e| e.to_string())?
                    .is_none()
                {
                    warnings.push(format!(
                        "Character \"{character_name}\" defaultImageId could not be remapped and was cleared: {default_image_id}"
                    ));
                    clear_default_image_id = true;
                    has_updates = true;
                }
            }

            let overrides: Vec<AvatarOverride> = character
                .get("avatarOverrides")
                .and_then(Value::as_array)
                .map(|a| serde_json::from_value(Value::Array(a.clone())).unwrap_or_default())
                .unwrap_or_default();
            if !overrides.is_empty() {
                let mut overrides_changed = false;
                let mut remapped: Vec<AvatarOverride> = Vec::new();
                for override_entry in &overrides {
                    if let Some(new_image_id) = remap_id(
                        Some(override_entry.image_id.as_str()),
                        &id_maps.doc_mount_file_links,
                    ) {
                        overrides_changed = true;
                        remapped.push(AvatarOverride {
                            chat_id: override_entry.chat_id.clone(),
                            image_id: new_image_id,
                        });
                        continue;
                    }
                    if files_repo
                        .find_by_id(&override_entry.image_id)
                        .map_err(|e| e.to_string())?
                        .is_some()
                    {
                        remapped.push(override_entry.clone());
                        continue;
                    }
                    warnings.push(format!(
                        "Character \"{character_name}\" avatar override could not be remapped and was dropped: {}",
                        override_entry.image_id
                    ));
                    // v4: *"Drop the entry rather than nulling its imageId: the
                    // schema requires a string there, so a null would fail the
                    // next validated read of the character."*
                    overrides_changed = true;
                }
                // v4: *"Only touch the field when something actually moved — an
                // unconditional write forces a pointless update (and a vault
                // round-trip) on every character that merely happens to own
                // overrides."*
                if overrides_changed {
                    patch.avatar_overrides = Some(remapped);
                    has_updates = true;
                }
            }

            if has_updates {
                patch.updated_at = crate::clock::now_iso();
                CharactersRepository::new(main)
                    .update(new_id, &patch)
                    .map_err(|e| e.to_string())?;
                // ⚠ v5 lane-boundary workaround (the `document_stores.rs`
                // `originalFileName` precedent). `CharacterUpdate` cannot
                // express "clear this nullable column" — `default_image_id` is
                // `Option<String>`, where `None` means "leave alone" — and
                // widening it lives in `db/characters.rs`, a SIBLING lane's file
                // this round (P4.D63). The net row state is v4's exactly: the
                // repo update above stamps `updatedAt` (so the write happens
                // where v4's does), and this clears the one column it could not.
                // When that type is widened, this collapses into the patch.
                if clear_default_image_id {
                    main.execute(
                        "UPDATE characters SET defaultImageId = NULL WHERE id = ?1",
                        rusqlite::params![new_id],
                    )
                    .map_err(|e| crate::db::DbError::from(e).to_string())?;
                }
            }

            // v4: *"Only now that the character points at its imported vault is
            // the scaffold safe to remove: reversing the order would leave a
            // window where the row references a store that no longer exists, and
            // any overlay read in it throws CharacterVaultUnavailableError."*
            if let (Some(scaffold), Some(imported)) = (&scaffold_mount_id, &imported_vault_id) {
                discard_scaffold_vault(mount, scaffold, imported, new_id, warnings);
            }
            Ok(())
        })();
        if let Err(e) = out {
            warnings.push(format!("Failed to reconcile character relationships: {e}"));
            tracing::warn!(character_id = %new_id, error = %e, "Failed to reconcile character");
        }
    }

    // ── Chats ──────────────────────────────────────────────────────────────
    for (_backup_id, new_id) in id_maps.chats.iter() {
        let out: Result<(), String> = (|| {
            let Some(chat) = chats_read::find_by_id(main, new_id).map_err(|e| e.to_string())?
            else {
                return Ok(());
            };

            let mut patch = ChatUpdate::default();
            let mut has_updates = false;

            // v4 remaps (and re-writes) the participants array whenever it is
            // non-empty — even when nothing actually changed.
            if let Some(parts) = chat.get("participants").and_then(Value::as_array) {
                if !parts.is_empty() {
                    let mut participants: Vec<ChatParticipant> =
                        serde_json::from_value(Value::Array(parts.clone()))
                            .map_err(|e| e.to_string())?;
                    for p in &mut participants {
                        if !p.character_id.is_empty() {
                            if let Some(new_char) = id_maps.characters.get(&p.character_id) {
                                p.character_id = new_char.to_string();
                            }
                        }
                        if let Some(Some(conn)) = &p.connection_profile_id {
                            if !conn.is_empty() {
                                if let Some(new_conn) = id_maps.connection_profiles.get(conn) {
                                    p.connection_profile_id = Some(Some(new_conn.to_string()));
                                }
                            }
                        }
                        if let Some(Some(img)) = &p.image_profile_id {
                            if !img.is_empty() {
                                if let Some(new_img) = id_maps.image_profiles.get(img) {
                                    p.image_profile_id = Some(Some(new_img.to_string()));
                                }
                            }
                        }
                        if let Some(Some(tpl)) = &p.roleplay_template_id {
                            if !tpl.is_empty() {
                                if let Some(new_tpl) = id_maps.roleplay_templates.get(tpl) {
                                    p.roleplay_template_id = Some(Some(new_tpl.to_string()));
                                }
                            }
                        }
                    }
                    patch.participants = Some(participants);
                    has_updates = true;
                }
            }

            let tags = str_array_of(&chat, "tags");
            if !tags.is_empty() {
                let remapped = remap_id_array(&tags, &id_maps.tags);
                if !remapped.is_empty() {
                    patch.tags = Some(remapped);
                    has_updates = true;
                }
            }

            if let Some(new_project) = remap_id(str_of(&chat, "projectId"), &id_maps.projects) {
                patch.project_id = Some(Some(new_project));
                has_updates = true;
            }

            if has_updates {
                // v4's `chats.update` preserves `updatedAt` unless the caller
                // passes one — reconcile does not.
                ChatsRepository::new(main)
                    .update(new_id, &patch)
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        if let Err(e) = out {
            warnings.push(format!("Failed to reconcile chat relationships: {e}"));
            tracing::warn!(chat_id = %new_id, error = %e, "Failed to reconcile chat");
        }
    }

    // ── Projects ───────────────────────────────────────────────────────────
    for (_backup_id, new_id) in id_maps.projects.iter() {
        let out: Result<(), String> = (|| {
            let repo = projects::ProjectsRepository::new(main, mount);
            let Some(project) = repo.find_by_id(new_id).map_err(|e| e.to_string())? else {
                return Ok(());
            };

            let mut patch = Map::new();

            let roster = str_array_of(&project, "characterRoster");
            if !roster.is_empty() {
                let remapped = remap_id_array(&roster, &id_maps.characters);
                if !remapped.is_empty() {
                    patch.insert(
                        "characterRoster".to_string(),
                        Value::Array(remapped.into_iter().map(Value::String).collect()),
                    );
                }
            }

            if let Some(new_img) = remap_id(
                str_of(&project, "defaultImageProfileId"),
                &id_maps.image_profiles,
            ) {
                patch.insert("defaultImageProfileId".to_string(), Value::String(new_img));
            }

            if let Some(new_tpl) = remap_id(
                str_of(&project, "defaultRoleplayTemplateId"),
                &id_maps.roleplay_templates,
            ) {
                patch.insert(
                    "defaultRoleplayTemplateId".to_string(),
                    Value::String(new_tpl),
                );
            }

            if !patch.is_empty() {
                repo.update(new_id, &patch).map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        if let Err(e) = out {
            warnings.push(format!("Failed to reconcile project relationships: {e}"));
            tracing::warn!(project_id = %new_id, error = %e, "Failed to reconcile project");
        }
    }

    // ── Connection profiles (tags + the fallback understudy) ───────────────
    for (_backup_id, new_id) in id_maps.connection_profiles.iter() {
        let out: Result<(), String> = (|| {
            let Some(profile) =
                connection_profiles::find_by_id(main, new_id).map_err(|e| e.to_string())?
            else {
                return Ok(());
            };
            let mut patch = connection_profiles::CpUpdate {
                updated_at: crate::clock::now_iso(),
                ..Default::default()
            };
            let mut touched = false;

            let tags = str_array_of(&profile, "tags");
            if !tags.is_empty() {
                let remapped = remap_id_array(&tags, &id_maps.tags);
                if !remapped.is_empty() {
                    patch.tags = Some(remapped);
                    touched = true;
                }
            }

            // Remap `fallbackProfileId` (the understudy, v4 `65f5021c8`). This
            // has to happen in the reconcile pass rather than at insert time: a
            // profile may name an understudy that appears *later* in the
            // bundle, so the map is only complete once every profile has
            // landed. A reference the map cannot resolve is left alone —
            // `build_fallback_chain` drops a target that isn't there, and
            // clearing it would throw away a chain that a preserve-ids import
            // got right.
            if let Some(current) = profile.get("fallbackProfileId").and_then(Value::as_str) {
                let remapped = remap_id(Some(current), &id_maps.connection_profiles);
                if let Some(remapped) = remapped.filter(|r| r != current) {
                    patch.fallback_profile_id = Some(Some(remapped));
                    touched = true;
                }
            }

            if touched {
                connection_profiles::ConnectionProfilesRepository::new(main)
                    .update(new_id, &patch)
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        if let Err(e) = out {
            warnings.push(format!(
                "Failed to reconcile connection profile relationships: {e}"
            ));
            tracing::warn!(profile_id = %new_id, error = %e, "Failed to reconcile connection profile");
        }
    }

    // ── Image profiles (tags) ──────────────────────────────────────────────
    for (_backup_id, new_id) in id_maps.image_profiles.iter() {
        let out: Result<(), String> = (|| {
            let Some(profile) =
                image_profiles::find_by_id(main, new_id).map_err(|e| e.to_string())?
            else {
                return Ok(());
            };
            let tags = str_array_of(&profile, "tags");
            if !tags.is_empty() {
                let remapped = remap_id_array(&tags, &id_maps.tags);
                if !remapped.is_empty() {
                    let patch = image_profiles::IpUpdate {
                        tags: Some(remapped),
                        updated_at: crate::clock::now_iso(),
                        ..Default::default()
                    };
                    image_profiles::ImageProfilesRepository::new(main)
                        .update(new_id, &patch)
                        .map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        })();
        if let Err(e) = out {
            warnings.push(format!(
                "Failed to reconcile image profile relationships: {e}"
            ));
            tracing::warn!(profile_id = %new_id, error = %e, "Failed to reconcile image profile");
        }
    }

    // ── Embedding profiles (tags) ──────────────────────────────────────────
    for (_backup_id, new_id) in id_maps.embedding_profiles.iter() {
        let out: Result<(), String> = (|| {
            let Some(profile) = embedding_profiles::find_full_json_by_id(main, new_id)
                .map_err(|e| e.to_string())?
            else {
                return Ok(());
            };
            let tags = str_array_of(&profile, "tags");
            if !tags.is_empty() {
                let remapped = remap_id_array(&tags, &id_maps.tags);
                if !remapped.is_empty() {
                    let patch = embedding_profiles::EpUpdate {
                        tags: Some(remapped),
                        updated_at: crate::clock::now_iso(),
                        ..Default::default()
                    };
                    embedding_profiles::EmbeddingProfilesRepository::new(main)
                        .update(new_id, &patch)
                        .map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        })();
        if let Err(e) = out {
            warnings.push(format!(
                "Failed to reconcile embedding profile relationships: {e}"
            ));
            tracing::warn!(profile_id = %new_id, error = %e, "Failed to reconcile embedding profile");
        }
    }

    // ── Roleplay templates (tags; global repo) ─────────────────────────────
    for (_backup_id, new_id) in id_maps.roleplay_templates.iter() {
        let out: Result<(), String> = (|| {
            let Some(template) = roleplay_templates::find_full_json_by_id(main, new_id)
                .map_err(|e| e.to_string())?
            else {
                return Ok(());
            };
            let tags = str_array_of(&template, "tags");
            if !tags.is_empty() {
                let remapped = remap_id_array(&tags, &id_maps.tags);
                if !remapped.is_empty() {
                    let patch = RtUpdate {
                        tags: Some(remapped),
                        updated_at: crate::clock::now_iso(),
                        ..Default::default()
                    };
                    RoleplayTemplatesRepository::new(main)
                        .update(new_id, &patch)
                        .map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        })();
        if let Err(e) = out {
            warnings.push(format!(
                "Failed to reconcile roleplay template relationships: {e}"
            ));
            tracing::warn!(template_id = %new_id, error = %e, "Failed to reconcile roleplay template");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::quilltap_import::IdMap;

    /// The `connection_profiles` shape the reconcile pass reads and writes —
    /// the CpCreate INSERT's column list, in the MIGRATED (appended) order the
    /// boot ensure produces. SQLite's dynamic typing makes the affinities
    /// immaterial here.
    fn profiles_table(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE connection_profiles (\
               id TEXT PRIMARY KEY, userId TEXT, name TEXT, provider TEXT, \
               transport TEXT, courierDeltaMode INTEGER, apiKeyId TEXT, \
               baseUrl TEXT, modelName TEXT, parameters TEXT, isDefault INTEGER, \
               isCheap INTEGER, allowWebSearch INTEGER, useNativeWebSearch INTEGER, \
               allowToolUse INTEGER, pseudoToolMode TEXT, \
               \"multiCharacterPrefill\" INTEGER, modelClass TEXT, \
               maxContext REAL, maxTokens REAL, isDangerousCompatible INTEGER, \
               supportsImageUpload INTEGER, tags TEXT, sortIndex REAL, \
               totalTokens REAL, totalPromptTokens REAL, totalCompletionTokens REAL, \
               messageCount REAL, createdAt TEXT, updatedAt TEXT, \
               \"fallbackProfileId\" TEXT, \"allowTierFallback\" INTEGER DEFAULT 0)",
        )
        .unwrap();
    }

    fn insert(conn: &Connection, id: &str, fallback: Option<&str>) {
        conn.execute(
            "INSERT INTO connection_profiles \
               (id, userId, name, provider, transport, courierDeltaMode, modelName, \
                parameters, isDefault, isCheap, allowWebSearch, useNativeWebSearch, \
                allowToolUse, pseudoToolMode, isDangerousCompatible, supportsImageUpload, \
                tags, sortIndex, totalTokens, totalPromptTokens, totalCompletionTokens, \
                messageCount, createdAt, updatedAt, fallbackProfileId) \
             VALUES (?1, 'u1', 'P', 'OPENAI', 'api', 1, 'm', '{}', 0, 0, 0, 0, 1, 'auto', \
                     0, 0, '[]', 0, 0, 0, 0, 0, 't', 't', ?2)",
            rusqlite::params![id, fallback],
        )
        .unwrap();
    }

    fn understudy_of(conn: &Connection, id: &str) -> Option<String> {
        conn.query_row(
            "SELECT fallbackProfileId FROM connection_profiles WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get::<_, Option<String>>(0),
        )
        .unwrap()
    }

    /// **The unit pin the `.qtap` import differential cannot carry.**
    ///
    /// `system_import_state`'s connection-profile leg has been vacuous since v4
    /// `aa464abf`: the committed `system-data-main.db` predates
    /// `multiCharacterPrefill`, so every profile import fails on BOTH sides with
    /// `no column named …` and the arms stay green on matching failures (the
    /// family header records it; widening that fixture is cross-lane). The 4.10
    /// fallback columns land in the same hole, so the understudy remap is
    /// pinned here instead, at the unit tier, and named in both places.
    ///
    /// What it proves is v4 `65f5021c8`'s reason for putting the remap in the
    /// reconcile pass at all: the FORWARD reference. `cp-a` names `cp-b`, and
    /// `cp-b` lands after it — at insert time the map has no entry for `cp-b`
    /// yet, so only a pass that runs once everything has landed can resolve it.
    #[test]
    fn the_understudy_is_remapped_including_a_forward_reference() {
        let main = Connection::open_in_memory().unwrap();
        let mount = Connection::open_in_memory().unwrap();
        profiles_table(&main);
        // The bundle's `cp-a` named `cp-b`; both were re-created under new ids.
        insert(&main, "new-a", Some("cp-b"));
        insert(&main, "new-b", None);

        let mut id_maps = IdMaps::default();
        let mut cps = IdMap::default();
        cps.set("cp-a".into(), "new-a".into());
        cps.set("cp-b".into(), "new-b".into());
        id_maps.connection_profiles = cps;

        let mut warnings = Vec::new();
        reconcile_relationships(&main, &mount, &id_maps, &mut warnings);

        assert_eq!(understudy_of(&main, "new-a").as_deref(), Some("new-b"));
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    /// An id the map cannot resolve is LEFT ALONE, not cleared — v4's comment:
    /// `buildFallbackChain` drops a target that isn't there, and clearing it
    /// would throw away a chain a preserve-ids import got right.
    #[test]
    fn an_unresolvable_understudy_is_left_alone() {
        let main = Connection::open_in_memory().unwrap();
        let mount = Connection::open_in_memory().unwrap();
        profiles_table(&main);
        insert(&main, "new-a", Some("preserved-id"));

        let mut id_maps = IdMaps::default();
        let mut cps = IdMap::default();
        cps.set("cp-a".into(), "new-a".into());
        id_maps.connection_profiles = cps;

        let mut warnings = Vec::new();
        reconcile_relationships(&main, &mount, &id_maps, &mut warnings);

        assert_eq!(
            understudy_of(&main, "new-a").as_deref(),
            Some("preserved-id")
        );
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    /// A profile with no understudy is not written at all — the reconcile pass
    /// only issues an UPDATE when something actually moved (v4 builds an
    /// `updates` bag and skips an empty one).
    #[test]
    fn a_profile_with_nothing_to_remap_is_not_rewritten() {
        let main = Connection::open_in_memory().unwrap();
        let mount = Connection::open_in_memory().unwrap();
        profiles_table(&main);
        insert(&main, "new-a", None);
        let before: String = main
            .query_row(
                "SELECT updatedAt FROM connection_profiles WHERE id = 'new-a'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        let mut id_maps = IdMaps::default();
        let mut cps = IdMap::default();
        cps.set("cp-a".into(), "new-a".into());
        id_maps.connection_profiles = cps;

        let mut warnings = Vec::new();
        reconcile_relationships(&main, &mount, &id_maps, &mut warnings);

        let after: String = main
            .query_row(
                "SELECT updatedAt FROM connection_profiles WHERE id = 'new-a'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(before, after, "an untouched profile must not be re-stamped");
    }
}
