//! v4 `importMemories` (`import-entities.ts:433-522`) — remap-only, always insert (no
//! conflict check). `characterId` MUST resolve through the character id-map (else
//! warn + skip); `aboutCharacterId` remaps through the SAME character map or →
//! null; `chatId`/`projectId` remap through the chats/projects maps or → null
//! (which, after a `duplicate` import, can be a PHANTOM id — the map quirk rides
//! straight into the stored FK, exactly as in v4); `tags` remap through the tags
//! map with `get(tag) || tag` (an unmapped tag keeps its ORIGINAL id, unlike the
//! null-on-miss FK remaps). Strips id/createdAt/updatedAt (create mints), then runs the whole entity
//! through `MemorySchema` ([`parse_create_memory`](crate::db::memories::parse_create_memory),
//! P4.161) BEFORE the write — a refused item logs v4's three repository
//! ERRORs, warns `Failed to import memory: <ZodError message>`, WARNs, and
//! counts `skipped` (v4 `:522`).
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
use crate::db::memories::{CreateOptions, MemoriesRepository};
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

        // v4 builds the create payload from the RAW item (`import-entities.ts:
        // 459-498`): the item minus `id` / `createdAt` / `updatedAt` /
        // `embedding`, with the four FKs remapped. Each FK remap runs only on a
        // TRUTHY value (`if (memory.aboutCharacterId)`) and answers the map's
        // hit `|| null`; a falsy value (`""`, `0`, `null`) rides through as it
        // came — so `aboutCharacterId: ""` reaches the schema, which refuses it.
        let mut payload = memory.as_object().cloned().unwrap_or_default();
        for k in ["id", "createdAt", "updatedAt", "embedding"] {
            payload.remove(k);
        }
        let remap = |k: &str, map: &super::IdMap| {
            let raw = memory.get(k)?;
            Some(if crate::api::system_qtap::js_truthy(Some(raw)) {
                raw.as_str()
                    .and_then(|v| map.get(v))
                    .map_or(Value::Null, |v| Value::String(v.to_string()))
            } else {
                raw.clone()
            })
        };
        payload.insert(
            "characterId".into(),
            Value::String(new_character_id.to_string()),
        );
        for (k, map) in [
            ("aboutCharacterId", &id_maps.characters),
            ("chatId", &id_maps.chats),
            ("projectId", &id_maps.projects),
        ] {
            if let Some(v) = remap(k, map) {
                payload.insert(k.into(), v);
            }
        }
        // tags: `if (memory.tags && memory.tags.length > 0)` → `get(tag) ||
        // tag` per element (an unmapped tag keeps its ORIGINAL id). A
        // non-empty STRING passes that guard and has no `.map` — v4's TypeError
        // is the item's failure.
        let tags_failure = match memory.get("tags") {
            Some(Value::Array(items)) => {
                let mapped = items
                    .iter()
                    .map(|t| match t.as_str().and_then(|t| id_maps.tags.get(t)) {
                        Some(hit) => Value::String(hit.to_string()),
                        None => t.clone(),
                    })
                    .collect();
                payload.insert("tags".into(), Value::Array(mapped));
                None
            }
            Some(Value::String(s)) if !s.is_empty() => {
                Some("memory.tags.map is not a function".to_string())
            }
            _ => None,
        };

        let (new_id, _now) = super::mint_or_preserve(options, &source_id);
        // v4's `_create` validates the WHOLE entity (`MemorySchema`) before the
        // insert (P4.161, dogfood #152 — v5 used to coerce: `importance: 5`
        // and `kind: "bogus-kind"` were written as they came).
        let parsed = match tags_failure {
            Some(type_error) => Err(type_error),
            None => {
                crate::db::memories::parse_create_memory(&Value::Object(payload), Some(&new_id))
                    .inspect_err(|zod| {
                        super::log_refused_create("memories", zod);
                        crate::db::fallback::log_memory_create_failure(
                            new_character_id,
                            &DbError::Internal(zod.clone()),
                        );
                    })
            }
        };
        let now = crate::clock::now_iso();
        let opts = CreateOptions {
            id: new_id,
            created_at: now.clone(),
            updated_at: now,
        };
        let created = parsed.and_then(|create| {
            repo.create(&create, &opts)
                .map_err(|e| super::item_error_text(&e))
        });
        match created {
            Ok(()) => {
                created_ids.push((opts.id.clone(), new_character_id.to_string()));
                imported += 1;
            }
            Err(text) => {
                warnings.push(format!("Failed to import memory: {text}"));
                // v4 `import-entities.ts:518` (P4.148 Tier 2 item 17).
                tracing::warn!(memoryId = super::id_field(memory), error = %text, "Failed to import memory");
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
