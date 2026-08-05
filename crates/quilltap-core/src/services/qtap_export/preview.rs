//! v4 `previewExport` (`lib/export/quilltap-export-service.ts:63`) — the
//! pre-export preview (entity id/name pairs + an optional memory count) with no
//! writes and no payload materialization.
//!
//! Two v4 quirks are carried verbatim:
//!   - `scope !== 'all'` reads `selectedIds` and NEVER calls `findAll`, so a
//!     `'selected'` preview with an empty id list yields `entities: []`.
//!   - the response omits `memoryCount` entirely unless it is `> 0` (the `&&`
//!     spread at :262).

use rusqlite::Connection;
use serde_json::{Map, Value};

use super::{ExportError, ExportOptions};
use crate::db::{
    characters_read, chats_read, connection_profiles, embedding_profiles, image_profiles,
    memories_read, projects, roleplay_templates, tags,
};

/// v4 `previewExport(userId, options)`.
pub fn preview_export(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    options: &ExportOptions,
) -> Result<Value, ExportError> {
    let all = options.scope == "all";
    let entity_ids: Vec<String> = if all {
        Vec::new()
    } else {
        options.selected_ids.clone()
    };

    let mut entities: Vec<Value> = Vec::new();
    let mut memory_count: i64 = 0;

    match options.entity_type.as_str() {
        "characters" => {
            let ids = if all {
                super::id_list(&characters_read::find_all(main, mount)?)
            } else {
                entity_ids
            };
            for id in ids {
                let Some(c) = characters_read::find_by_id(main, mount, &id)? else {
                    continue;
                };
                push_entity(&mut entities, &c, "name");
                if options.include_memories {
                    // v4 `collectCharacterMemories` swallows failures to `[]`.
                    memory_count += memories_read::find_by_character_id(main, &id)
                        .map(|m| m.len() as i64)
                        .unwrap_or(0);
                }
            }
        }
        "chats" => {
            let ids = if all {
                super::id_list(&chats_read::find_all(main)?)
            } else {
                entity_ids
            };
            for id in ids {
                let Some(c) = chats_read::find_by_id(main, &id)? else {
                    continue;
                };
                push_entity(&mut entities, &c, "title");
                if options.include_memories {
                    memory_count += collect_chat_memories(main, mount, &id);
                }
            }
        }
        "roleplay-templates" => {
            let ids = if all {
                roleplay_templates::find_all_for_user(main, user_id)?
                    .iter()
                    .filter(|t| is_user_template(t, user_id))
                    .filter_map(|t| t.get("id").and_then(Value::as_str).map(str::to_string))
                    .collect()
            } else {
                entity_ids
            };
            for id in ids {
                let Some(t) = roleplay_templates::find_full_json_by_id(main, &id)? else {
                    continue;
                };
                if !is_user_template(&t, user_id) {
                    continue;
                }
                push_entity(&mut entities, &t, "name");
            }
        }
        "connection-profiles" => {
            let ids = if all {
                super::id_list(&connection_profiles::find_all(main)?)
            } else {
                entity_ids
            };
            for id in ids {
                if let Some(p) = connection_profiles::find_by_id(main, &id)? {
                    push_entity(&mut entities, &p, "name");
                }
            }
        }
        "image-profiles" => {
            let ids = if all {
                super::id_list(&image_profiles::find_all(main)?)
            } else {
                entity_ids
            };
            for id in ids {
                if let Some(p) = image_profiles::find_by_id(main, &id)? {
                    push_entity(&mut entities, &p, "name");
                }
            }
        }
        "embedding-profiles" => {
            let ids = if all {
                super::id_list(&embedding_profiles::find_all_full_json(main)?)
            } else {
                entity_ids
            };
            for id in ids {
                if let Some(p) = embedding_profiles::find_full_json_by_id(main, &id)? {
                    push_entity(&mut entities, &p, "name");
                }
            }
        }
        "tags" => {
            let ids = if all {
                super::id_list(&tags::find_all(main)?)
            } else {
                entity_ids
            };
            for id in ids {
                if let Some(t) = tags::find_full_by_id(main, &id)? {
                    push_entity(&mut entities, &t, "name");
                }
            }
        }
        "projects" => {
            let repo = projects::ProjectsRepository::new(main, mount);
            let ids = if all {
                super::id_list(&repo.find_all()?)
            } else {
                entity_ids
            };
            for id in ids {
                if let Some(p) = repo.find_by_id(&id)? {
                    push_entity(&mut entities, &p, "name");
                }
            }
        }
        "groups" => {
            let repo = crate::db::groups::GroupsRepository::new(main, mount);
            let ids = if all {
                super::id_list(&repo.find_all()?)
            } else {
                entity_ids
            };
            for id in ids {
                if let Some(g) = repo.find_by_id(&id)? {
                    push_entity(&mut entities, &g, "name");
                }
            }
        }
        "document-stores" => {
            let ids = if all {
                super::id_list(&crate::db::doc_mount_points::find_all_full_json(mount)?)
            } else {
                entity_ids
            };
            let repo = crate::db::doc_mount_points::DocMountPointsRepository::new(mount);
            for id in ids {
                if let Some(s) = repo.find_full_json_by_id(&id)? {
                    push_entity(&mut entities, &s, "name");
                }
            }
        }
        // The five `7189a968` additions (v4 `quilltap-export-service.ts:254-332`).
        "files" => {
            let all_files = crate::services::backup::marshal::query_all(
                main,
                "files",
                crate::services::backup::collect::FILES,
                "",
                &[],
            )?;
            let ids: Vec<String> = if all {
                all_files
                    .iter()
                    .filter(|f| {
                        f.get("category").and_then(Value::as_str) != Some("BACKUP")
                            && f.get("folderPath").and_then(Value::as_str) != Some("/backups")
                    })
                    .filter_map(|f| f.get("id").and_then(Value::as_str).map(str::to_string))
                    .collect()
            } else {
                entity_ids
            };
            for id in ids {
                // v4 re-reads per id (`repos.files.findById`).
                if let Some(f) = all_files
                    .iter()
                    .find(|f| f.get("id").and_then(Value::as_str) == Some(id.as_str()))
                {
                    push_entity(&mut entities, f, "originalFilename");
                }
            }
        }
        "prompt-templates" => {
            let all_templates = crate::services::backup::marshal::query_all(
                main,
                "prompt_templates",
                crate::services::backup::collect::PROMPT_TEMPLATES,
                "",
                &[],
            )?;
            let is_user = |t: &&Value| {
                !t.get("isBuiltIn").and_then(Value::as_bool).unwrap_or(false)
                    && t.get("userId").and_then(Value::as_str) == Some(user_id)
            };
            let ids: Vec<String> = if all {
                all_templates
                    .iter()
                    .filter(is_user)
                    .filter_map(|t| t.get("id").and_then(Value::as_str).map(str::to_string))
                    .collect()
            } else {
                entity_ids
            };
            for id in ids {
                if let Some(t) = all_templates
                    .iter()
                    .filter(is_user)
                    .find(|t| t.get("id").and_then(Value::as_str) == Some(id.as_str()))
                {
                    push_entity(&mut entities, t, "name");
                }
            }
        }
        "provider-models" => {
            // Instance-global catalogue, no user filter; `scope != 'all'`
            // filters by the selected-id SET over findAll order (not per-id).
            let wanted: Option<std::collections::HashSet<&str>> = if all {
                None
            } else {
                Some(entity_ids.iter().map(String::as_str).collect())
            };
            for model in crate::db::provider_models::find_all(main)? {
                let Some(id) = model.get("id").and_then(Value::as_str) else {
                    continue;
                };
                if wanted.as_ref().is_some_and(|w| !w.contains(id)) {
                    continue;
                }
                let name = format!(
                    "{} / {}",
                    model.get("provider").and_then(Value::as_str).unwrap_or(""),
                    model.get("modelId").and_then(Value::as_str).unwrap_or("")
                );
                let mut m = Map::new();
                m.insert("id".into(), Value::String(id.to_string()));
                m.insert("name".into(), Value::String(name));
                entities.push(Value::Object(m));
            }
        }
        "plugin-configs" => {
            let wanted: Option<std::collections::HashSet<&str>> = if all {
                None
            } else {
                Some(entity_ids.iter().map(String::as_str).collect())
            };
            for config in crate::services::backup::marshal::query_all(
                main,
                "plugin_configs",
                crate::services::backup::collect::PLUGIN_CONFIGS,
                "userId = ?1",
                &[&user_id],
            )? {
                let Some(id) = config.get("id").and_then(Value::as_str) else {
                    continue;
                };
                if wanted.as_ref().is_some_and(|w| !w.contains(id)) {
                    continue;
                }
                push_entity(&mut entities, &config, "pluginName");
            }
        }
        "instance-settings" => {
            // Keyed by setting key — the table has no id column.
            let wanted: Option<std::collections::HashSet<&str>> = if all {
                None
            } else {
                Some(entity_ids.iter().map(String::as_str).collect())
            };
            for (key, _) in crate::db::instance_settings::list_portable_instance_settings(main)? {
                if wanted.as_ref().is_some_and(|w| !w.contains(key.as_str())) {
                    continue;
                }
                let mut m = Map::new();
                m.insert("id".into(), Value::String(key.clone()));
                m.insert("name".into(), Value::String(key));
                entities.push(Value::Object(m));
            }
        }
        other => return Err(ExportError::UnknownType(other.to_string())),
    }

    let mut out = Map::new();
    out.insert("type".into(), Value::String(options.entity_type.clone()));
    out.insert("entities".into(), Value::Array(entities));
    if memory_count > 0 {
        out.insert("memoryCount".into(), Value::from(memory_count));
    }
    Ok(Value::Object(out))
}

fn is_user_template(t: &Value, user_id: &str) -> bool {
    !t.get("isBuiltIn").and_then(Value::as_bool).unwrap_or(false)
        && t.get("userId").and_then(Value::as_str) == Some(user_id)
}

/// `entities.push({ id, name })` — always exactly those two keys, in that order.
fn push_entity(entities: &mut Vec<Value>, row: &Value, name_key: &str) {
    let mut m = Map::new();
    m.insert("id".into(), row.get("id").cloned().unwrap_or(Value::Null));
    m.insert(
        "name".into(),
        row.get(name_key).cloned().unwrap_or(Value::Null),
    );
    entities.push(Value::Object(m));
}

/// v4 `collectChatMemories` (:38) — every character's memories, filtered by
/// `chatId`. Swallows failures to `0` the way v4 swallows to `[]`.
fn collect_chat_memories(main: &Connection, mount: &Connection, chat_id: &str) -> i64 {
    let Ok(characters) = characters_read::find_all(main, mount) else {
        return 0;
    };
    let mut n = 0i64;
    for c in characters {
        let Some(cid) = c.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Ok(memories) = memories_read::find_by_character_id(main, cid) else {
            return 0;
        };
        n += memories
            .iter()
            .filter(|m| m.get("chatId").and_then(Value::as_str) == Some(chat_id))
            .count() as i64;
    }
    n
}
