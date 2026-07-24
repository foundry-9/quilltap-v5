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
