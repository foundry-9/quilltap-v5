//! The **memory-service cascade-delete family** — v4 `lib/memory/memory-service.ts`
//! `deleteMemoryWithVector` / `deleteMemoriesBySourceMessageWithVectors` /
//! `deleteMemoriesBySourceMessagesWithVectors` / `deleteMemoriesByChatIdWithVectors`.
//!
//! These are the vector-store-aware wrappers around the deletion chokepoint
//! ([`MemoriesRepository::delete_with_unlink`] / [`delete_many_with_unlink`]):
//! every path that deletes memories in bulk (a single UI delete, a source-message
//! cascade, a swipe-group cascade, a chat wipe) goes through one of these so the
//! rows are unlinked from neighbours' `relatedMemoryIds` **and** their entries are
//! removed from the per-character vector stores (with the store metadata bumped
//! only when something was actually removed — a store untouched by the sweep keeps
//! its `updatedAt`).
//!
//! No model call anywhere — this family is pure DB effect, so it is verified by a
//! plain tier-2 differential (`memory_cascade_tier2_equivalence`), not tier-3.
//!
//! ## Faithful v4 shapes
//!
//! * `delete_memory_with_vector` confirms ownership first (the chokepoint is
//!   characterId-agnostic), deletes through the chokepoint, **then** removes the
//!   vector — and the vector cleanup is non-fatal (v4 wraps it in try/catch and
//!   still returns `true`).
//! * The three cascades read the doomed set, group it by character in
//!   first-appearance order (v4's `Map` insertion order), remove each character's
//!   vectors (counting only ids the store actually held — `hasVector` first) with a
//!   per-character non-fatal guard, and only then run the chokepoint batch delete.
//! * The swipe-group variant gathers every memory across the whole group up front
//!   so the chokepoint's neighbour scan sweeps the `relatedMemoryIds` column once.
//!
//! [`MemoriesRepository::delete_with_unlink`]: crate::db::memories::MemoriesRepository::delete_with_unlink
//! [`delete_many_with_unlink`]: crate::db::memories::MemoriesRepository::delete_many_with_unlink

use serde_json::Value;

use crate::db::runtime::Db;
use crate::db::vector_store::CharacterVectorStore;
use crate::db::{memories_read, DbError};

/// Result of a source-message / swipe-group cascade (v4's
/// `{ deleted, vectorsRemoved }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CascadeDeleteResult {
    pub deleted: i64,
    pub vectors_removed: i64,
}

/// Result of a chat wipe (v4's `{ deleted, vectorsRemoved, characterCount }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChatCascadeDeleteResult {
    pub deleted: i64,
    pub vectors_removed: i64,
    pub character_count: usize,
}

/// Delete a memory and remove its vector (v4 `deleteMemoryWithVector`). Returns
/// `false` — writing nothing — when the memory does not exist or belongs to a
/// different character (the ownership check precedes the characterId-agnostic
/// chokepoint). The vector cleanup after a successful delete is non-fatal.
pub async fn delete_memory_with_vector(
    db: &Db,
    character_id: &str,
    memory_id: &str,
) -> Result<bool, DbError> {
    let id = memory_id.to_string();
    let existing = db.read_main(move |conn| memories_read::find_by_id(conn, &id))?;
    let owned = existing
        .as_ref()
        .and_then(|m| m.get("characterId"))
        .and_then(Value::as_str)
        == Some(character_id);
    if !owned {
        return Ok(false);
    }

    let id = memory_id.to_string();
    let deleted = db
        .write(move |writers| writers.main().memories().delete_with_unlink(&id))
        .await?;
    if !deleted {
        return Ok(false);
    }

    // Remove from the vector store — non-fatal (v4 logs a warn and still returns
    // true, so a store failure must not turn a completed delete into an error).
    let char_id = character_id.to_string();
    let id = memory_id.to_string();
    let _ = db
        .write(move |writers| {
            let main = writers.main();
            let mut store = CharacterVectorStore::load(main.connection(), &char_id)?;
            store.remove_vector(&id);
            store.flush(&main.vector_indices())?;
            Ok(())
        })
        .await;

    Ok(true)
}

/// Delete all memories for a source message with vector-store cleanup (v4
/// `deleteMemoriesBySourceMessageWithVectors`). Handles the multi-character case —
/// one message may have produced memories for several characters.
pub async fn delete_memories_by_source_message_with_vectors(
    db: &Db,
    source_message_id: &str,
) -> Result<CascadeDeleteResult, DbError> {
    let smid = source_message_id.to_string();
    let memories =
        db.read_main(move |conn| memories_read::find_by_source_message_id(conn, &smid))?;
    if memories.is_empty() {
        return Ok(CascadeDeleteResult::default());
    }
    cascade_delete(db, &memories).await
}

/// Delete all memories for a whole swipe group with vector cleanup (v4
/// `deleteMemoriesBySourceMessagesWithVectors`). Gathers every memory across the
/// group up front so the chokepoint scan sweeps `relatedMemoryIds` once.
pub async fn delete_memories_by_source_messages_with_vectors(
    db: &Db,
    source_message_ids: &[String],
) -> Result<CascadeDeleteResult, DbError> {
    if source_message_ids.is_empty() {
        return Ok(CascadeDeleteResult::default());
    }

    let mut all_memories: Vec<Value> = Vec::new();
    for smid in source_message_ids {
        let smid = smid.clone();
        let slice =
            db.read_main(move |conn| memories_read::find_by_source_message_id(conn, &smid))?;
        all_memories.extend(slice);
    }
    if all_memories.is_empty() {
        return Ok(CascadeDeleteResult::default());
    }
    cascade_delete(db, &all_memories).await
}

/// Delete every memory tied to a chat (across all characters) and remove their
/// vector entries (v4 `deleteMemoriesByChatIdWithVectors`) — the chat-wipe path.
pub async fn delete_memories_by_chat_id_with_vectors(
    db: &Db,
    chat_id: &str,
) -> Result<ChatCascadeDeleteResult, DbError> {
    let cid = chat_id.to_string();
    let memories = db.read_main(move |conn| memories_read::find_by_chat_id(conn, &cid))?;
    if memories.is_empty() {
        return Ok(ChatCascadeDeleteResult::default());
    }

    let character_count = group_by_character(&memories).len();
    let CascadeDeleteResult {
        deleted,
        vectors_removed,
    } = cascade_delete(db, &memories).await?;
    Ok(ChatCascadeDeleteResult {
        deleted,
        vectors_removed,
        character_count,
    })
}

/// The shared cascade body: group by character, remove each group's vectors, then
/// batch-delete every row through the chokepoint (v4's shared middle section).
async fn cascade_delete(db: &Db, memories: &[Value]) -> Result<CascadeDeleteResult, DbError> {
    let groups = group_by_character(memories);
    let vectors_removed = remove_vectors_grouped(db, groups).await;

    let all_ids: Vec<String> = memories.iter().filter_map(id_of).collect();
    let deleted = db
        .write(move |writers| writers.main().memories().delete_many_with_unlink(&all_ids))
        .await?;

    Ok(CascadeDeleteResult {
        deleted,
        vectors_removed,
    })
}

/// Group memory ids by `characterId` in first-appearance order (v4 builds a `Map`,
/// whose iteration follows insertion).
fn group_by_character(memories: &[Value]) -> Vec<(String, Vec<String>)> {
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for m in memories {
        let Some(char_id) = m.get("characterId").and_then(Value::as_str) else {
            continue;
        };
        let Some(id) = id_of(m) else { continue };
        match groups.iter_mut().find(|(c, _)| c == char_id) {
            Some((_, ids)) => ids.push(id),
            None => groups.push((char_id.to_string(), vec![id])),
        }
    }
    groups
}

/// Remove each character group's vectors, counting only ids the store actually
/// held (v4's `hasVector` check before `removeVector`). Each character's cleanup
/// is non-fatal (v4's per-character try/catch: a failed store must not abort the
/// cascade — the chokepoint delete still runs).
async fn remove_vectors_grouped(db: &Db, groups: Vec<(String, Vec<String>)>) -> i64 {
    let mut total = 0i64;
    for (character_id, memory_ids) in groups {
        let removed = db
            .write(move |writers| {
                let main = writers.main();
                let mut store = CharacterVectorStore::load(main.connection(), &character_id)?;
                let mut n = 0i64;
                for id in &memory_ids {
                    if store.has_vector(id) && store.remove_vector(id) {
                        n += 1;
                    }
                }
                store.flush(&main.vector_indices())?;
                Ok(n)
            })
            .await;
        total += removed.unwrap_or(0);
    }
    total
}

fn id_of(memory: &Value) -> Option<String> {
    memory.get("id").and_then(Value::as_str).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::memories::{CreateOptions, MemCreate};
    use crate::db::vector_indices::VectorEntryInput;
    use crate::db::Writer;
    use tempfile::{tempdir, TempDir};

    /// A throwaway base64 pepper keys the fresh encrypted DB (never a real one).
    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const SENTINEL: &str = "2020-01-01T00:00:00.000Z";

    const DDL: &str = "
        CREATE TABLE memories (
            id TEXT PRIMARY KEY, characterId TEXT, aboutCharacterId TEXT, chatId TEXT,
            projectId TEXT, content TEXT, summary TEXT, keywords TEXT, tags TEXT,
            importance REAL, embedding BLOB, source TEXT, witnessedContext TEXT,
            sourceMessageId TEXT, lastAccessedAt TEXT, reinforcementCount REAL,
            lastReinforcedAt TEXT, relatedMemoryIds TEXT, reinforcedImportance REAL,
            createdAt TEXT, updatedAt TEXT);
        CREATE TABLE vector_indices (
            id TEXT PRIMARY KEY, characterId TEXT, version REAL, dimensions REAL,
            createdAt TEXT, updatedAt TEXT);
        CREATE TABLE vector_entries (
            id TEXT PRIMARY KEY, characterId TEXT, embedding BLOB, createdAt TEXT);
    ";

    struct Seed {
        id: &'static str,
        character_id: &'static str,
        chat_id: Option<&'static str>,
        source_message_id: Option<&'static str>,
        related: &'static [&'static str],
        vector: Option<Vec<f32>>,
    }

    fn make_db(seeds: &[Seed]) -> (TempDir, Db) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("main.db");
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            w.connection().execute_batch(DDL).unwrap();
            for s in seeds {
                w.memories()
                    .create(
                        &MemCreate {
                            character_id: s.character_id.to_string(),
                            about_character_id: None,
                            chat_id: s.chat_id.map(str::to_string),
                            project_id: None,
                            content: format!("content {}", s.id),
                            summary: format!("summary {}", s.id),
                            keywords: vec![],
                            tags: vec![],
                            importance: 0.5,
                            embedding: None,
                            source: "AUTO".to_string(),
                            witnessed_context: None,
                            source_message_id: s.source_message_id.map(str::to_string),
                            last_accessed_at: None,
                            reinforcement_count: 1.0,
                            last_reinforced_at: None,
                            related_memory_ids: s.related.iter().map(|r| r.to_string()).collect(),
                            reinforced_importance: 0.5,
                        },
                        &CreateOptions {
                            id: s.id.to_string(),
                            created_at: SENTINEL.to_string(),
                            updated_at: SENTINEL.to_string(),
                        },
                    )
                    .unwrap();
                if let Some(vec) = &s.vector {
                    let vi = w.vector_indices();
                    vi.save_meta(s.character_id, vec.len() as f64).unwrap();
                    vi.add_entry(&VectorEntryInput {
                        id: s.id.to_string(),
                        character_id: s.character_id.to_string(),
                        embedding: Some(vec.clone()),
                    })
                    .unwrap();
                }
            }
            // Pin the seed-minted vector timestamps to the sentinel so the tests
            // can tell a flush-time bump from the seeding itself.
            w.connection()
                .execute(
                    "UPDATE vector_indices SET createdAt = ?1, updatedAt = ?1",
                    [SENTINEL],
                )
                .unwrap();
        }
        let db = Db::open_main(&path, PEPPER).unwrap();
        (dir, db)
    }

    fn count(db: &Db, table: &str, where_clause: &str) -> i64 {
        let sql = format!("SELECT COUNT(*) FROM {table} {where_clause}");
        db.read_main(move |c| Ok(c.query_row(&sql, [], |r| r.get(0))?))
            .unwrap()
    }

    /// Ownership gate: a wrong-character or missing target returns false and
    /// writes nothing; the owned path deletes the row + its vector entry.
    #[tokio::test]
    async fn delete_memory_with_vector_checks_ownership() {
        let (_dir, db) = make_db(&[Seed {
            id: "m1",
            character_id: "char-a",
            chat_id: None,
            source_message_id: None,
            related: &[],
            vector: Some(vec![1.0, 0.0]),
        }]);

        assert!(!delete_memory_with_vector(&db, "char-b", "m1")
            .await
            .unwrap());
        assert!(!delete_memory_with_vector(&db, "char-a", "nope")
            .await
            .unwrap());
        assert_eq!(count(&db, "memories", ""), 1);
        assert_eq!(count(&db, "vector_entries", ""), 1);

        assert!(delete_memory_with_vector(&db, "char-a", "m1")
            .await
            .unwrap());
        assert_eq!(count(&db, "memories", ""), 0);
        assert_eq!(count(&db, "vector_entries", ""), 0);
    }

    /// A source-message cascade spans characters: rows deleted through the
    /// chokepoint (surviving neighbour unlinked), vectors counted only where the
    /// store held them, and an untouched store's metadata keeps its sentinel.
    #[tokio::test]
    async fn source_message_cascade_spans_characters() {
        let (_dir, db) = make_db(&[
            Seed {
                id: "m1",
                character_id: "char-a",
                chat_id: Some("chat-1"),
                source_message_id: Some("msg-1"),
                related: &[],
                vector: Some(vec![1.0, 0.0]),
            },
            Seed {
                id: "m2",
                character_id: "char-b",
                chat_id: Some("chat-1"),
                source_message_id: Some("msg-1"),
                related: &[],
                // char-b's store never held m2 (no vector) — the sweep must not
                // bump char-b's metadata.
                vector: None,
            },
            Seed {
                id: "m3",
                character_id: "char-b",
                chat_id: Some("chat-1"),
                source_message_id: Some("msg-keep"),
                related: &["m1"],
                vector: Some(vec![0.0, 1.0]),
            },
        ]);

        let r = delete_memories_by_source_message_with_vectors(&db, "msg-1")
            .await
            .unwrap();
        assert_eq!(r.deleted, 2);
        assert_eq!(r.vectors_removed, 1);

        // Survivor m3 got unlinked from the doomed m1.
        let related: String = db
            .read_main(|c| {
                Ok(c.query_row(
                    "SELECT relatedMemoryIds FROM memories WHERE id = 'm3'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(related, "[]");

        // char-a's store was swept (metadata bumped); char-b's held nothing
        // matching, so its save was a no-op and the sentinel survives.
        let meta_b: String = db
            .read_main(|c| {
                Ok(c.query_row(
                    "SELECT updatedAt FROM vector_indices WHERE characterId = 'char-b'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(meta_b, SENTINEL);
        let meta_a: String = db
            .read_main(|c| {
                Ok(c.query_row(
                    "SELECT updatedAt FROM vector_indices WHERE characterId = 'char-a'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_ne!(meta_a, SENTINEL);
    }

    /// The chat wipe reports the character count and the empty branches of all
    /// three cascades return zeroed results without writing.
    #[tokio::test]
    async fn chat_wipe_counts_characters_and_empty_branches_noop() {
        let (_dir, db) = make_db(&[
            Seed {
                id: "m1",
                character_id: "char-a",
                chat_id: Some("chat-1"),
                source_message_id: Some("msg-1"),
                related: &[],
                vector: Some(vec![1.0, 0.0]),
            },
            Seed {
                id: "m2",
                character_id: "char-b",
                chat_id: Some("chat-1"),
                source_message_id: Some("msg-2"),
                related: &[],
                vector: Some(vec![0.0, 1.0]),
            },
        ]);

        let none = delete_memories_by_source_message_with_vectors(&db, "msg-none")
            .await
            .unwrap();
        assert_eq!(none, CascadeDeleteResult::default());
        let none = delete_memories_by_source_messages_with_vectors(&db, &[])
            .await
            .unwrap();
        assert_eq!(none, CascadeDeleteResult::default());

        let r = delete_memories_by_chat_id_with_vectors(&db, "chat-1")
            .await
            .unwrap();
        assert_eq!(r.deleted, 2);
        assert_eq!(r.vectors_removed, 2);
        assert_eq!(r.character_count, 2);
        assert_eq!(count(&db, "memories", ""), 0);
        assert_eq!(count(&db, "vector_entries", ""), 0);
    }
}
