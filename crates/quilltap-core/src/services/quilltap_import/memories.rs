//! v4 `importMemories` (`import-entities.ts:387`) — remap-only, always insert (no
//! conflict check). `characterId` MUST resolve through the character id-map (else
//! warn + skip); `aboutCharacterId` remaps through the SAME character map or →
//! null; `chatId`/`projectId` remap through the chats/projects maps or → null
//! (which, after a `duplicate` import, can be a PHANTOM id — the map quirk rides
//! straight into the stored FK, exactly as in v4); `tags` remap through the tags
//! map with `get(tag) || tag` (an unmapped tag keeps its ORIGINAL id, unlike the
//! null-on-miss FK remaps). Strips id/createdAt/updatedAt (create mints).
//!
//! ## Embeddings are dropped, not validated (v4 `7189a968`)
//!
//! `embedding` is excluded on purpose. A vector is only meaningful against the
//! model that produced it, so a foreign one silently corrupts semantic search
//! whenever the dimensionality happens to match — and v4's boot repair
//! (`repair-text-embeddings.ts`) would *preserve* a bad vector by converting it
//! to a valid blob rather than discarding it. Import time is the only correct
//! place to drop it. The orchestrator enqueues an `EMBEDDING_GENERATE` per
//! created row (see `mod.rs`'s `enqueue_imported_memory_embeddings`).
//!
//! (History: before `7189a968`, v4's Zod union REJECTED the NDJSON writer's
//! `JSON.stringify(Float32Array)` object shape, so an embedding-bearing export
//! could not re-import its memories at all — each landed in the per-item catch.
//! The destructure-before-validate at `import-entities.ts:458` retired that
//! trap on both sides: any embedding shape is now silently dropped. The
//! standing irony note that used to live here is therefore history too.)

use rusqlite::Connection;
use serde_json::Value;

use super::{IdMaps, ImportOptions};
use crate::db::memories::{CreateOptions, MemCreate, MemoriesRepository};
use crate::db::DbError;

pub(super) struct Counts {
    pub imported: u32,
    pub skipped: u32,
    /// v4 `createdIds` (`import-entities.ts:400-406`) — `{id, characterId}` per
    /// created row, in creation order, for the post-reconcile embedding enqueue.
    pub created_ids: Vec<(String, String)>,
}

pub(super) fn import_memories(
    main: &Connection,
    memories: &[Value],
    options: &ImportOptions,
    id_maps: &IdMaps,
    warnings: &mut Vec<String>,
) -> Result<Counts, DbError> {
    let mut imported = 0u32;
    let mut skipped = 0u32;
    let mut created_ids: Vec<(String, String)> = Vec::new();

    let repo = MemoriesRepository::new(main);

    for memory in memories {
        // Skip-if-present rehydrate (spec §6/F4, `01e481f6`): the memory is
        // already back — a partial restore being re-run. The surviving row
        // wins. v4 checks this BEFORE the character remap, so a sanctioned skip
        // never trips the "references non-existent character" warning.
        let source_id = super::id_of(memory);
        if options.preserve_ids && id_maps.preserve_ids_skips.contains(&source_id) {
            skipped += 1;
            continue;
        }

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
            // Excluded on purpose — see the module header (v4 `7189a968`).
            embedding: None,
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
        let (new_id, _now) = super::mint_or_preserve(options, &source_id);
        let opts = CreateOptions {
            id: new_id,
            created_at: now.clone(),
            updated_at: now,
        };
        match repo.create(&create, &opts) {
            Ok(()) => {
                created_ids.push((opts.id.clone(), new_character_id.to_string()));
                imported += 1;
            }
            Err(e) => {
                warnings.push(format!("Failed to import memory: {e}"));
                skipped += 1;
            }
        }
    }

    Ok(Counts {
        imported,
        skipped,
        created_ids,
    })
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
