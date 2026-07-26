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
use crate::db::characters::{CharacterUpdate, CharactersRepository};
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

/// v4 `reconcileRelationships`.
pub(super) fn reconcile_relationships(
    main: &Connection,
    mount: &Connection,
    id_maps: &IdMaps,
    warnings: &mut Vec<String>,
) {
    // ── Characters ─────────────────────────────────────────────────────────
    for (_backup_id, new_id) in id_maps.characters.iter() {
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

            // Only rewrite when the imported value resolves to a remapped
            // mount-point row — the freshly-provisioned vault FK must survive a
            // failed remap (v4's orphaned-vault fix).
            if let Some(new_mount) = remap_id(
                str_of(&character, "characterDocumentMountPointId"),
                &id_maps.mount_points,
            ) {
                patch.character_document_mount_point_id = Some(new_mount);
                has_updates = true;
            }

            if has_updates {
                patch.updated_at = crate::clock::now_iso();
                CharactersRepository::new(main)
                    .update(new_id, &patch)
                    .map_err(|e| e.to_string())?;
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

    // ── Connection profiles (tags) ─────────────────────────────────────────
    for (_backup_id, new_id) in id_maps.connection_profiles.iter() {
        let out: Result<(), String> = (|| {
            let Some(profile) =
                connection_profiles::find_by_id(main, new_id).map_err(|e| e.to_string())?
            else {
                return Ok(());
            };
            let tags = str_array_of(&profile, "tags");
            if !tags.is_empty() {
                let remapped = remap_id_array(&tags, &id_maps.tags);
                if !remapped.is_empty() {
                    let patch = connection_profiles::CpUpdate {
                        tags: Some(remapped),
                        updated_at: crate::clock::now_iso(),
                        ..Default::default()
                    };
                    connection_profiles::ConnectionProfilesRepository::new(main)
                        .update(new_id, &patch)
                        .map_err(|e| e.to_string())?;
                }
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
