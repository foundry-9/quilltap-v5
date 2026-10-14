//! Hand edits to a memory (the memory editor's `PUT /api/v1/memories/[id]`,
//! and search and replace) — v4 `lib/memory/admission/update-memory.ts`
//! (`af48f6cc2` Phase B; bug 203 from `c2795733f`).
//!
//! v4's why, carried: "A memory row carries derived state beside the text a
//! person edits: its vector, `reinforcedImportance`, the character's frozen
//! archive, the `memories` realtime topic and, for a consolidation digest,
//! the Commonplace mirror in the holder's vault. A raw `updateForCharacter`
//! left all of it describing the row as it was before the edit (bug 203).
//! `updateMemory` keeps every piece in step:
//!
//!   - a change to the embedded text (summary or content) embeds the edited
//!     text first and writes text and vector in one row update through
//!     `memoryIndex.upsert` (anchor line included, tier kept); a failed embed
//!     writes nothing;
//!   - an importance change recomputes `reinforcedImportance` from the row's
//!     reinforcement count;
//!   - `relatedMemoryIds` must name existing memories of the same character,
//!     and the link is written on both sides;
//!   - the frozen archive is invalidated and `memories` is published;
//!   - a digest's Commonplace mirror bucket(s) are rewritten through
//!     `mirrorCommonplaceForSubjects`, the same writer consolidation uses."

use std::collections::HashSet;

use serde_json::Value;

use crate::db::memories::MemUpdate;
use crate::db::memories_read;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::memory_embedding_text::{
    embedding_text_for, lexical_embedding_text_for, AnchorEntities, EmbeddingTextSource,
};
use crate::memory_scoring::reinforced_importance;
use crate::model::embedding::{EmbeddingCallOptions, EmbeddingProvider};
use crate::realtime::bus::publish_realtime;
use crate::realtime::types::RealtimeTopic;
use crate::services::commonplace_mirror::mirror_commonplace_for_subjects;
use crate::services::frozen_archive::invalidate_frozen_archive;
use crate::services::memory_index::{self, MemoryIndexUpsert};
use crate::services::memory_time_zone::refresh_memory_time_zone;

use super::now_ms;

/// v4's child-logger binding (`logger.child({ module: 'memory:update-memory'
/// })`).
const MODULE: &str = "memory:update-memory";

/// The fields a hand edit may change (v4 `MemoryEditPatch`). `None` is an
/// absent key; the two nullable ids are `Some(None)` for an explicit `null`.
/// Declaration order is v4's object order (the route's Zod schema and the
/// search-replace planner both build the patch in it), which `Memory edited`
/// reports.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemoryEditPatch {
    pub content: Option<String>,
    pub summary: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub importance: Option<f64>,
    pub about_character_id: Option<Option<String>>,
    pub chat_id: Option<Option<String>>,
    pub related_memory_ids: Option<Vec<String>>,
}

impl MemoryEditPatch {
    /// v4 `Object.keys(patch)` — the present keys, in order.
    pub fn keys(&self) -> Vec<&'static str> {
        let mut keys = Vec::new();
        let present = [
            ("content", self.content.is_some()),
            ("summary", self.summary.is_some()),
            ("keywords", self.keywords.is_some()),
            ("tags", self.tags.is_some()),
            ("importance", self.importance.is_some()),
            ("aboutCharacterId", self.about_character_id.is_some()),
            ("chatId", self.chat_id.is_some()),
            ("relatedMemoryIds", self.related_memory_ids.is_some()),
        ];
        for (key, is_present) in present {
            if is_present {
                keys.push(key);
            }
        }
        keys
    }
}

/// v4 `UpdateMemoryResult`.
#[derive(Clone, Debug, PartialEq)]
pub enum UpdateMemoryResult {
    /// The row as it now stands (with its vector).
    Ok {
        memory: Value,
    },
    NotFound,
    InvalidRelated {
        invalid_ids: Vec<String>,
    },
    /// The edited text could not be embedded; nothing was written.
    EmbeddingFailed {
        message: String,
    },
}

/// v4 `MemorySchema`'s key order (`lib/schemas/memory.types.ts`) — the order
/// a parsed `Memory` carries.
const MEMORY_SCHEMA_ORDER: &[&str] = &[
    "id",
    "characterId",
    "aboutCharacterId",
    "chatId",
    "projectId",
    "content",
    "summary",
    "keywords",
    "tags",
    "importance",
    "embedding",
    "source",
    "witnessedContext",
    "occurredAt",
    "narrativeTime",
    "entities",
    "kind",
    "sourceMessageId",
    "lastAccessedAt",
    "reinforcementCount",
    "lastReinforcedAt",
    "relatedMemoryIds",
    "reinforcedImportance",
    "tier",
    "supersededById",
    "supersededAt",
    "consolidatedFrom",
    "consolidatedAt",
    "createdAt",
    "updatedAt",
];

/// v4's update answers the parsed `{ ...existing, ...patch }`, so a key the
/// patch set to `null` is PRESENT as `null` — a re-read row omits it. Overlay
/// the patch's explicit nulls, in `MemorySchema` order.
fn with_explicit_nulls(memory: Value, patch: &MemoryEditPatch) -> Value {
    let nulls = [
        (
            "aboutCharacterId",
            matches!(patch.about_character_id, Some(None)),
        ),
        ("chatId", matches!(patch.chat_id, Some(None))),
    ];
    if !nulls.iter().any(|(_, n)| *n) {
        return memory;
    }
    let Value::Object(mut fields) = memory else {
        return memory;
    };
    for (key, is_null) in nulls {
        if is_null {
            fields.insert(key.to_string(), Value::Null);
        }
    }
    let mut ordered = serde_json::Map::new();
    for key in MEMORY_SCHEMA_ORDER {
        if let Some(v) = fields.shift_remove(*key) {
            ordered.insert((*key).to_string(), v);
        }
    }
    ordered.extend(fields);
    Value::Object(ordered)
}

fn str_field<'a>(memory: &'a Value, key: &str) -> Option<&'a str> {
    memory.get(key).and_then(Value::as_str)
}

/// Does the patch change the text the row is embedded as? (v4
/// `changesEmbeddedText` — a hand edit reaches only `summary` and `content`
/// of that text.)
fn changes_embedded_text(existing: &Value, patch: &MemoryEditPatch) -> bool {
    patch
        .content
        .as_deref()
        .is_some_and(|c| Some(c) != str_field(existing, "content"))
        || patch
            .summary
            .as_deref()
            .is_some_and(|s| Some(s) != str_field(existing, "summary"))
}

/// Apply a hand edit to one memory and bring its derived state with it — v4
/// `updateMemory(characterId, memoryId, patch, { userId })`.
///
/// v4's order: find (`NotFound`) → an importance change recomputes
/// `reinforcedImportance` (DEBUG `Recomputed reinforcedImportance for edited
/// importance`) → `relatedMemoryIds` validated (deduped, self dropped, each
/// through `findByIdForCharacter`; DEBUG `Refusing memory edit: related ids
/// missing or belong to another character` → `InvalidRelated`) → a text
/// change embeds FIRST (interactive — no `priority`; `lexical_text`; a
/// failure WARNs `Memory edit refused: the edited text could not be
/// embedded` and answers `EmbeddingFailed`, nothing written) and writes text
/// and vector in ONE `memory_index::upsert` (DEBUG `Edited memory
/// re-embedded`), else one `update_for_character` → link deltas both ways
/// (DEBUG `Mirrored edited memory links onto neighbours`) → the frozen
/// archive invalidated → `memories` published → a hot digest's Commonplace
/// mirror rewritten (LR-6, DEBUG `Refreshed Commonplace mirror after digest
/// edit`) → DEBUG `Memory edited {module, characterId, memoryId,
/// fieldsJson}`. `host_zone_fallback` is the memory-calendar fallback the
/// anchor line renders in (LR-10).
#[allow(clippy::too_many_arguments)]
pub async fn update_memory<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: &P,
    character_id: &str,
    memory_id: &str,
    patch: MemoryEditPatch,
    user_id: &str,
    host_zone_fallback: &str,
) -> Result<UpdateMemoryResult, DbError> {
    let Some(existing) =
        db.read_main(|c| memories_read::find_by_id_for_character(c, character_id, memory_id))?
    else {
        return Ok(UpdateMemoryResult::NotFound);
    };

    let mut update = MemUpdate {
        content: patch.content.clone(),
        summary: patch.summary.clone(),
        keywords: patch.keywords.clone(),
        tags: patch.tags.clone(),
        importance: patch.importance,
        about_character_id: patch.about_character_id.clone(),
        chat_id: patch.chat_id.clone(),
        ..Default::default()
    };

    // --- importance → reinforcedImportance ----------------------------------
    let existing_importance = existing.get("importance").and_then(Value::as_f64);
    if let Some(importance) = patch.importance.filter(|i| Some(*i) != existing_importance) {
        let count = existing
            .get("reinforcementCount")
            .and_then(Value::as_f64)
            .unwrap_or(1.0);
        let recomputed = reinforced_importance(importance, count);
        update.reinforced_importance = Some(recomputed);
        tracing::debug!(
            module = MODULE,
            characterId = %character_id,
            memoryId = %memory_id,
            importance = importance,
            reinforcementCount = count,
            reinforcedImportance = recomputed,
            "Recomputed reinforcedImportance for edited importance"
        );
    }

    // --- relatedMemoryIds: validate, then link both sides -------------------
    let mut added: Vec<String> = Vec::new();
    let mut removed: Vec<String> = Vec::new();
    if let Some(related) = &patch.related_memory_ids {
        let mut seen = HashSet::new();
        let requested: Vec<String> = related
            .iter()
            .filter(|id| seen.insert(id.as_str()))
            .filter(|id| id.as_str() != memory_id)
            .cloned()
            .collect();
        let mut invalid_ids = Vec::new();
        for id in &requested {
            let neighbour =
                db.read_main(|c| memories_read::find_by_id_for_character(c, character_id, id))?;
            if neighbour.is_none() {
                invalid_ids.push(id.clone());
            }
        }
        if !invalid_ids.is_empty() {
            tracing::debug!(
                module = MODULE,
                characterId = %character_id,
                memoryId = %memory_id,
                invalidIdsJson = %serde_json::to_string(&invalid_ids).unwrap_or_default(),
                "Refusing memory edit: related ids missing or belong to another character"
            );
            return Ok(UpdateMemoryResult::InvalidRelated { invalid_ids });
        }
        // v4 `new Set(existing.relatedMemoryIds ?? [])` — first-seen order.
        let mut before_seen = HashSet::new();
        let before: Vec<String> = existing
            .get("relatedMemoryIds")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|id| before_seen.insert(*id))
            .map(str::to_string)
            .collect();
        let after: HashSet<&String> = requested.iter().collect();
        added = requested
            .iter()
            .filter(|id| !before.contains(id))
            .cloned()
            .collect();
        removed = before
            .iter()
            .filter(|id| !after.contains(id))
            .cloned()
            .collect();
        update.related_memory_ids = Some(requested);
    }

    // --- the row, re-embedded when its embedded text changed -----------------
    // Embed first, write second: a failed embed leaves the row (and so the
    // links below, the archive and the mirror) exactly as they were. The
    // vector rides in the same row update as the text, so they never disagree
    // on disk.
    let (cid, mid) = (character_id.to_string(), memory_id.to_string());
    let memory = if changes_embedded_text(&existing, &patch) {
        let summary = patch
            .summary
            .as_deref()
            .or_else(|| str_field(&existing, "summary"))
            .unwrap_or_default();
        let content = patch
            .content
            .as_deref()
            .or_else(|| str_field(&existing, "content"))
            .unwrap_or_default();
        let source = EmbeddingTextSource {
            summary,
            content,
            occurred_at: str_field(&existing, "occurredAt"),
            narrative_time: str_field(&existing, "narrativeTime"),
            entities: existing
                .get("entities")
                .and_then(Value::as_array)
                .map_or(AnchorEntities::None, |a| AnchorEntities::Json(a.as_slice())),
        };
        let time_zone = refresh_memory_time_zone(db, host_zone_fallback, now_ms());
        let text = embedding_text_for(&source, &time_zone);
        let call = EmbeddingCallOptions::interactive()
            .with_lexical_text(lexical_embedding_text_for(&source));
        let embedding = match provider
            .generate_embedding_for_user(&text, user_id, None, call)
            .await
        {
            Ok(result) => result.embedding,
            Err(e) => {
                tracing::warn!(
                    module = MODULE,
                    characterId = %character_id,
                    memoryId = %memory_id,
                    error = %e.message,
                    "Memory edit refused: the edited text could not be embedded"
                );
                return Ok(UpdateMemoryResult::EmbeddingFailed {
                    message: format!("Could not embed the edited memory: {}", e.message),
                });
            }
        };
        let written = embedding.clone();
        let row = db
            .write(move |w| {
                let conn = w.main().connection();
                let found = memory_index::upsert(
                    conn,
                    &cid,
                    vec![MemoryIndexUpsert {
                        id: mid.clone(),
                        embedding,
                        tier: None,
                        alongside: update,
                    }],
                )?;
                if found.first().copied() != Some(true) {
                    return Ok(None);
                }
                memories_read::find_by_id(conn, &mid)
            })
            .await?
            // v4's upsert answers the row with the vector it was handed (the
            // stored copy is int8-quantised; a re-read would dequantise it).
            .map(|mut row| {
                if let Some(m) = row.as_object_mut() {
                    m.insert("embedding".into(), super::apply::embedding_value(&written));
                }
                row
            });
        tracing::debug!(
            module = MODULE,
            characterId = %character_id,
            memoryId = %memory_id,
            "Edited memory re-embedded"
        );
        row
    } else {
        db.write(move |w| {
            let main = w.main();
            if !main.memories().update_for_character(&cid, &mid, &update)? {
                return Ok(None);
            }
            memories_read::find_by_id(main.connection(), &mid)
        })
        .await?
    };
    let Some(memory) = memory else {
        return Ok(UpdateMemoryResult::NotFound);
    };
    let memory = with_explicit_nulls(memory, &patch);

    // Neighbour links are deltas against the rows at write time, so an
    // extraction job linking the same neighbour keeps its link (bug 231).
    if !added.is_empty() || !removed.is_empty() {
        let (cid, mid, add, remove) = (
            character_id.to_string(),
            memory_id.to_string(),
            added.clone(),
            removed.clone(),
        );
        db.write(move |w| {
            let repo = w.main().memories();
            for id in &add {
                repo.add_related_memory_ids(&cid, id, std::slice::from_ref(&mid))?;
            }
            for id in &remove {
                repo.remove_related_memory_ids(&cid, id, std::slice::from_ref(&mid))?;
            }
            Ok(())
        })
        .await?;
        tracing::debug!(
            module = MODULE,
            characterId = %character_id,
            memoryId = %memory_id,
            addedJson = %serde_json::to_string(&added).unwrap_or_default(),
            removedJson = %serde_json::to_string(&removed).unwrap_or_default(),
            "Mirrored edited memory links onto neighbours"
        );
    }

    // --- caches and mirrors --------------------------------------------------
    invalidate_frozen_archive(character_id);
    publish_realtime(RealtimeTopic::Memories, None);

    let is_digest = str_field(&existing, "source") == Some("CONSOLIDATED");
    let is_hot = str_field(&existing, "tier").unwrap_or("hot") == "hot";
    if is_digest && is_hot {
        // A digest's bucket is its subject (consolidation's `bucketKeyOf`); an
        // edit that moves `aboutCharacterId` touches both the old and new
        // file. The no-subject bucket has no mirror file. Never fails.
        let subjects: Vec<String> = [
            str_field(&existing, "aboutCharacterId"),
            str_field(&memory, "aboutCharacterId"),
        ]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
        let written = mirror_commonplace_for_subjects(db, character_id, &subjects, None).await;
        tracing::debug!(
            module = MODULE,
            characterId = %character_id,
            memoryId = %memory_id,
            subjectsJson = %serde_json::to_string(&subjects).unwrap_or_default(),
            written = written,
            "Refreshed Commonplace mirror after digest edit"
        );
    }

    tracing::debug!(
        module = MODULE,
        characterId = %character_id,
        memoryId = %memory_id,
        fieldsJson = %serde_json::to_string(&patch.keys()).unwrap_or_default(),
        "Memory edited"
    );
    Ok(UpdateMemoryResult::Ok { memory })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_keys_are_the_present_ones_in_v4s_order() {
        let patch = MemoryEditPatch {
            related_memory_ids: Some(vec![]),
            summary: Some("s".into()),
            chat_id: Some(None),
            ..Default::default()
        };
        assert_eq!(patch.keys(), ["summary", "chatId", "relatedMemoryIds"]);
        assert!(MemoryEditPatch::default().keys().is_empty());
    }
}
