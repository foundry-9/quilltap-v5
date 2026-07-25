//! v4 `importMemories` (`import-entities.ts:387`) — remap-only, always insert (no
//! conflict check). `characterId` MUST resolve through the character id-map (else
//! warn + skip); `aboutCharacterId` remaps through the SAME character map or →
//! null; `chatId`/`projectId` remap through the chats/projects maps or → null
//! (which, after a `duplicate` import, can be a PHANTOM id — the map quirk rides
//! straight into the stored FK, exactly as in v4); `tags` remap through the tags
//! map with `get(tag) || tag` (an unmapped tag keeps its ORIGINAL id, unlike the
//! null-on-miss FK remaps). Strips id/createdAt/updatedAt (create mints).
//!
//! ## The embedding shape trap, carried faithfully
//!
//! v4's NDJSON writer emits a memory's embedding as `JSON.stringify(Float32Array)`
//! — the `{"0":v0,"1":v1,…}` object — and v4's `MemorySchema.embedding` union
//! (Float32Array | number[] | Buffer | string) accepts NONE of that, so a v4
//! export that carries embeddings cannot re-import its memories: each one lands
//! in the per-item catch as `Failed to import memory: <ZodError>` + `skipped++`.
//! v5 reproduces the reject (warn + skip) for any embedding that is neither
//! null/absent nor a plain number array; the differential masks the engine
//! sentence (the standing parser-wording seam).

use rusqlite::Connection;
use serde_json::Value;

use super::IdMaps;
use crate::db::memories::{CreateOptions, MemCreate, MemoriesRepository};
use crate::db::DbError;

pub(super) struct Counts {
    pub imported: u32,
    pub skipped: u32,
}

/// The remapped embedding, or `Err` for a shape v4's Zod union rejects.
fn parse_embedding(memory: &Value) -> Result<Option<Vec<f32>>, String> {
    match memory.get("embedding") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(arr)) => {
            let mut out = Vec::with_capacity(arr.len());
            for v in arr {
                match v.as_f64() {
                    Some(f) => out.push(f as f32),
                    None => {
                        return Err(
                            "invalid embedding: expected an array of numbers or null".to_string()
                        )
                    }
                }
            }
            Ok(Some(out))
        }
        // The `{"0":…}` stringified-Float32Array shape (and anything else) — v4's
        // union rejects it (see the module header).
        Some(_) => Err("invalid embedding: expected an array of numbers or null".to_string()),
    }
}

pub(super) fn import_memories(
    main: &Connection,
    memories: &[Value],
    id_maps: &IdMaps,
    warnings: &mut Vec<String>,
) -> Result<Counts, DbError> {
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
        let Some(new_character_id) = id_maps.characters.get(source_character_id) else {
            warnings.push(format!(
                "Memory references non-existent character {source_character_id}"
            ));
            skipped += 1;
            continue;
        };

        // Remap aboutCharacterId if present (Characters-Not-Personas: who the memory
        // is about). `idMaps.characters.get(...) || null`.
        let about_character_id = match memory.get("aboutCharacterId").and_then(Value::as_str) {
            Some(about) => id_maps.characters.get(about).map(|s| s.to_string()),
            None => None,
        };

        // chatId / projectId: `idMaps.<kind>.get(...) || null` when present (which
        // may surface a phantom `duplicate`-arm id — the carried quirk).
        let chat_id = memory
            .get("chatId")
            .and_then(Value::as_str)
            .and_then(|c| id_maps.chats.get(c))
            .map(|s| s.to_string());
        let project_id = memory
            .get("projectId")
            .and_then(Value::as_str)
            .and_then(|p| id_maps.projects.get(p))
            .map(|s| s.to_string());

        // tags: `get(tag) || tag` — an unmapped tag keeps its original id.
        let tags: Vec<String> = str_array(memory, "tags")
            .into_iter()
            .map(|t| id_maps.tags.get(&t).map(|s| s.to_string()).unwrap_or(t))
            .collect();

        let embedding = match parse_embedding(memory) {
            Ok(e) => e,
            Err(msg) => {
                warnings.push(format!("Failed to import memory: {msg}"));
                skipped += 1;
                continue;
            }
        };

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
            // source 'MANUAL'.
            importance: memory
                .get("importance")
                .and_then(Value::as_f64)
                .unwrap_or(0.5),
            embedding,
            source: opt_str(memory, "source").unwrap_or_else(|| "MANUAL".to_string()),
            witnessed_context: opt_str(memory, "witnessedContext"),
            // Episodic spine (v4 8bf3cb5f): occurredAt/narrativeTime absent →
            // NULL, entities [], kind 'semantic'.
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
