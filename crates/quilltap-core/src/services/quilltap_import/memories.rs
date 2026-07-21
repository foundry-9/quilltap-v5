//! v4 `importMemories` (`import-entities.ts:387`) — remap-only, always insert (no
//! conflict check). For the seed: `characterId` MUST resolve through the character
//! id-map (else warn + skip); `aboutCharacterId` remaps through the SAME character
//! map (both seed characters reference each other) or → null; `chatId`/`projectId`
//! remap through the (empty) chats/projects maps → null; `tags` remap through the
//! (empty) tags map → themselves. `sourceMessageId`/`lastReinforcedAt` and the
//! rest pass through. Strips id/createdAt/updatedAt (create mints a fresh id).

use rusqlite::Connection;
use serde_json::Value;

use super::{IdMap, ImportError};
use crate::db::memories::{CreateOptions, MemCreate, MemoriesRepository};

pub(super) struct Counts {
    pub imported: u32,
    pub skipped: u32,
}

pub(super) fn import_memories(
    main: &Connection,
    memories: &[Value],
    character_id_map: &IdMap,
    warnings: &mut Vec<String>,
) -> Result<Counts, ImportError> {
    let mut imported = 0u32;
    let mut skipped = 0u32;

    let repo = MemoriesRepository::new(main);

    for memory in memories {
        // Remap character ID (required — a memory with no destination character is
        // dropped with a warning).
        let source_character_id = memory
            .get("characterId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Some(new_character_id) = character_id_map.get(source_character_id) else {
            warnings.push(format!(
                "Memory references non-existent character {source_character_id}"
            ));
            skipped += 1;
            continue;
        };

        // Remap aboutCharacterId if present (Characters-Not-Personas: who the memory
        // is about). `idMaps.characters.get(...) || null`.
        let about_character_id = match memory.get("aboutCharacterId").and_then(Value::as_str) {
            Some(about) => character_id_map.get(about).map(|s| s.to_string()),
            None => None,
        };

        // chatId / projectId remap through empty maps → null. `tags` remap through
        // the empty tags map → unchanged (each `get(tag) || tag`).
        let chat_id: Option<String> = None; // idMaps.chats is empty
        let project_id: Option<String> = None; // idMaps.projects is empty
        let tags = str_array(memory, "tags"); // tags map empty → identity

        let create = MemCreate {
            character_id: new_character_id.to_string(),
            about_character_id,
            chat_id,
            project_id,
            content: opt_str(memory, "content").unwrap_or_default(),
            summary: opt_str(memory, "summary").unwrap_or_default(),
            keywords: str_array(memory, "keywords"),
            tags,
            // MemorySchema defaults (materialized by v4's `_create` validation):
            // importance 0.5, reinforcementCount 1, reinforcedImportance 0.5,
            // source 'MANUAL'. The seed carries every one explicitly.
            importance: memory
                .get("importance")
                .and_then(Value::as_f64)
                .unwrap_or(0.5),
            embedding: None,
            source: opt_str(memory, "source").unwrap_or_else(|| "MANUAL".to_string()),
            witnessed_context: opt_str(memory, "witnessedContext"),
            // Episodic spine (v4 8bf3cb5f): the seed rows predate the feature,
            // so these read straight from the JSON with the MemorySchema
            // defaults (occurredAt/narrativeTime absent → NULL, entities [],
            // kind 'semantic').
            occurred_at: opt_str(memory, "occurredAt"),
            narrative_time: opt_str(memory, "narrativeTime"),
            entities: str_array(memory, "entities"),
            kind: opt_str(memory, "kind").unwrap_or_else(|| "semantic".to_string()),
            source_message_id: opt_str(memory, "sourceMessageId"),
            last_accessed_at: opt_str(memory, "lastAccessedAt"),
            reinforcement_count: memory
                .get("reinforcementCount")
                .and_then(Value::as_f64)
                .unwrap_or(1.0),
            last_reinforced_at: opt_str(memory, "lastReinforcedAt"),
            related_memory_ids: str_array(memory, "relatedMemoryIds"),
            reinforced_importance: memory
                .get("reinforcedImportance")
                .and_then(Value::as_f64)
                .unwrap_or(0.5),
        };

        let now = crate::clock::now_iso();
        let opts = CreateOptions {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: now.clone(),
            updated_at: now,
        };
        match repo.create(&create, &opts) {
            Ok(()) => imported += 1,
            Err(e) => {
                warnings.push(format!("Failed to import memory: {e}"));
                skipped += 1;
            }
        }
    }

    Ok(Counts { imported, skipped })
}

fn opt_str(obj: &Value, key: &str) -> Option<String> {
    obj.get(key).and_then(Value::as_str).map(|s| s.to_string())
}

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
