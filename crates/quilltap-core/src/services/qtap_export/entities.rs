//! v4 `handleExportEntities` (`app/api/v1/system/tools/route.ts:440`) — the
//! `?action=export-entities&type=` listing that seeds the export dialog's
//! entity picker.
//!
//! ⚠ v4's switch covers only EIGHT of the ten export types: `groups` and
//! `document-stores` fall through to `badRequest(\`Unknown entity type: ${type}\`)`.
//! That asymmetry with `previewExport` (which handles all ten) is faithful — the
//! dialog never offers a picker for those two.

use rusqlite::Connection;
use serde_json::{Map, Value};

use super::ExportError;
use crate::db::{
    characters_read, chats_read, connection_profiles, embedding_profiles, image_profiles,
    memories_read, projects, roleplay_templates, tags,
};

/// v4's `Unknown entity type: <t>` 400 — distinct from the writer's
/// `Unknown export type` throw, and carried with v4's exact wording.
pub fn unknown_entity_type_message(entity_type: &str) -> String {
    format!("Unknown entity type: {entity_type}")
}

/// v4 `handleExportEntities` — returns `{entities, memoryCount}` where each entity
/// is `{id, name}` plus `memoryCount` for the two memory-bearing types.
pub fn export_entities(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    entity_type: &str,
) -> Result<Value, ExportError> {
    let mut entities: Vec<Value> = Vec::new();
    let mut total_memory_count: i64 = 0;

    match entity_type {
        "characters" => {
            for c in characters_read::find_all(main, mount)? {
                let Some(id) = c.get("id").and_then(Value::as_str) else {
                    continue;
                };
                let n = memories_read::find_by_character_id(main, id)?.len() as i64;
                total_memory_count += n;
                let mut m = Map::new();
                m.insert("id".into(), Value::String(id.to_string()));
                m.insert("name".into(), c.get("name").cloned().unwrap_or(Value::Null));
                m.insert("memoryCount".into(), Value::from(n));
                entities.push(Value::Object(m));
            }
        }
        "chats" => {
            let chats = chats_read::find_all(main)?;
            let characters = characters_read::find_all(main, mount)?;
            let mut all_memories: Vec<Value> = Vec::new();
            for c in &characters {
                let Some(cid) = c.get("id").and_then(Value::as_str) else {
                    continue;
                };
                all_memories.extend(memories_read::find_by_character_id(main, cid)?);
            }
            for chat in chats {
                let Some(id) = chat.get("id").and_then(Value::as_str) else {
                    continue;
                };
                let n = all_memories
                    .iter()
                    .filter(|m| m.get("chatId").and_then(Value::as_str) == Some(id))
                    .count() as i64;
                total_memory_count += n;
                let mut m = Map::new();
                m.insert("id".into(), Value::String(id.to_string()));
                m.insert(
                    "name".into(),
                    chat.get("title").cloned().unwrap_or(Value::Null),
                );
                m.insert("memoryCount".into(), Value::from(n));
                entities.push(Value::Object(m));
            }
        }
        "roleplay-templates" => {
            for t in roleplay_templates::find_all_for_user(main, user_id)? {
                let built_in = t.get("isBuiltIn").and_then(Value::as_bool).unwrap_or(false);
                if built_in || t.get("userId").and_then(Value::as_str) != Some(user_id) {
                    continue;
                }
                entities.push(id_name(&t, "name"));
            }
        }
        "connection-profiles" => {
            for p in connection_profiles::find_all(main)? {
                entities.push(id_name(&p, "name"));
            }
        }
        "image-profiles" => {
            for p in image_profiles::find_all(main)? {
                entities.push(id_name(&p, "name"));
            }
        }
        "embedding-profiles" => {
            for p in embedding_profiles::find_all_full_json(main)? {
                entities.push(id_name(&p, "name"));
            }
        }
        "tags" => {
            for t in tags::find_all(main)? {
                entities.push(id_name(&t, "name"));
            }
        }
        "projects" => {
            for p in projects::ProjectsRepository::new(main, mount).find_all()? {
                entities.push(id_name(&p, "name"));
            }
        }
        // `groups` + `document-stores` (and anything else) hit v4's default arm.
        other => return Err(ExportError::UnknownType(other.to_string())),
    }

    let mut out = Map::new();
    out.insert("entities".into(), Value::Array(entities));
    out.insert("memoryCount".into(), Value::from(total_memory_count));
    Ok(Value::Object(out))
}

fn id_name(row: &Value, name_key: &str) -> Value {
    let mut m = Map::new();
    m.insert("id".into(), row.get("id").cloned().unwrap_or(Value::Null));
    m.insert(
        "name".into(),
        row.get(name_key).cloned().unwrap_or(Value::Null),
    );
    Value::Object(m)
}
