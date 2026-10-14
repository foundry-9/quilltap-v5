//! The extraction dry run's scratch book — v4
//! `lib/memory/admission/dry-run-book.ts` (`ac6348c5f`, the review of PR 94).
//!
//! v4's why, carried: "A dry run previews a regenerate, which first retires the
//! chat's extracted memories and then admits each candidate against a book that
//! grows as it goes. Admission in a dry run writes nothing, so without help it
//! would decide against rows the regenerate retires first, and never let a
//! later candidate fold into an earlier one. The book supplies both halves:
//!
//!  - `retiring` — the ids the chat wipe would retire (`previewChatWipe`): the
//!    neighbour search goes past them;
//!  - `inserted` — what this dry run would have inserted, per holder, with its
//!    vector: a later candidate is decided against these too."

use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use crate::admission::{AdmissionNeighbour, AdmissionRow};
use crate::embedding_vector::cosine_similarity;

/// One would-be insert: the row admission would have written, and its vector.
#[derive(Clone, Debug, PartialEq)]
pub struct BookEntry {
    pub memory: Value,
    pub embedding: Vec<f32>,
}

/// v4 `DryRunBook`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DryRunBook {
    pub retiring: HashSet<String>,
    /// Per holder, in insertion order.
    pub inserted: HashMap<String, Vec<BookEntry>>,
}

/// v4 `createDryRunBook(retiring = [])`.
pub fn create_dry_run_book(retiring: impl IntoIterator<Item = String>) -> DryRunBook {
    DryRunBook {
        retiring: retiring.into_iter().collect(),
        inserted: HashMap::new(),
    }
}

/// The holder's would-be inserts as neighbours of `embedding`, closest first
/// (v4 `bookNeighbours`: same-length vectors only; a stable sort, so ties keep
/// their insertion order).
pub fn book_neighbours(
    book: &DryRunBook,
    character_id: &str,
    embedding: &[f32],
) -> Vec<AdmissionNeighbour> {
    let mut out: Vec<AdmissionNeighbour> = book
        .inserted
        .get(character_id)
        .map(|entries| {
            entries
                .iter()
                .filter(|e| e.embedding.len() == embedding.len())
                .filter_map(|e| {
                    cosine_similarity(embedding, &e.embedding)
                        .ok()
                        .map(|cosine| AdmissionNeighbour {
                            memory: e.memory.clone(),
                            cosine,
                            via: None,
                            dangling_digest: false,
                        })
                })
                .collect()
        })
        .unwrap_or_default();
    crate::memory_scoring::js_sort::js_array_sort_by(&mut out, |a, b| b.cosine - a.cosine);
    out
}

/// Record a would-be insert, as the row admission would have written (v4
/// `recordBookInsert`): id `dry-run:<characterId>:<n>`, `source ?? 'AUTO'`,
/// hot, count 1, stamped now.
pub fn record_book_insert(book: &mut DryRunBook, row: &AdmissionRow, embedding: &[f32]) -> Value {
    let list = book.inserted.entry(row.character_id.clone()).or_default();
    let now = crate::clock::now_iso();
    let memory = json!({
        "id": format!("dry-run:{}:{}", row.character_id, list.len() + 1),
        "characterId": row.character_id,
        "aboutCharacterId": row.about_character_id,
        "chatId": row.chat_id,
        "projectId": row.project_id,
        "content": row.content,
        "summary": row.summary,
        "keywords": row.keywords.clone().unwrap_or_default(),
        "tags": row.tags.clone().unwrap_or_default(),
        "importance": crate::db::js_number_to_json(row.importance.unwrap_or(0.5)),
        "source": row.source.clone().unwrap_or_else(|| "AUTO".to_string()),
        "kind": row.kind.clone().unwrap_or_else(|| "semantic".to_string()),
        "occurredAt": row.occurred_at,
        "narrativeTime": row.narrative_time,
        "entities": row.entities.clone().unwrap_or_default(),
        "tier": "hot",
        "reinforcementCount": 1,
        "relatedMemoryIds": [],
        "createdAt": now,
        "updatedAt": now,
    });
    list.push(BookEntry {
        memory: memory.clone(),
        embedding: embedding.to_vec(),
    });
    memory
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(character_id: &str, content: &str) -> AdmissionRow {
        AdmissionRow {
            character_id: character_id.to_string(),
            content: content.to_string(),
            summary: format!("{content} (summary)"),
            ..Default::default()
        }
    }

    /// v4 `admission-dry-run.test.ts` "lets a later candidate fold into an
    /// earlier would-be insert" — the book half: ids count per holder, the
    /// row defaults to AUTO / hot / count 1.
    #[test]
    fn records_would_be_inserts_per_holder_with_v4s_ids_and_defaults() {
        let mut book = create_dry_run_book(Vec::<String>::new());
        let a = record_book_insert(&mut book, &row("c1", "first"), &[1.0, 0.0]);
        let b = record_book_insert(&mut book, &row("c1", "second"), &[0.0, 1.0]);
        let c = record_book_insert(&mut book, &row("c2", "third"), &[1.0, 0.0]);
        assert_eq!(a["id"], "dry-run:c1:1");
        assert_eq!(b["id"], "dry-run:c1:2");
        assert_eq!(c["id"], "dry-run:c2:1");
        assert_eq!(a["source"], "AUTO");
        assert_eq!(a["tier"], "hot");
        assert_eq!(a["reinforcementCount"], 1);
        assert_eq!(a["importance"], 0.5);
        assert_eq!(a["kind"], "semantic");
    }

    #[test]
    fn book_neighbours_are_the_holders_same_length_rows_closest_first() {
        let mut book = create_dry_run_book(["gone".to_string()]);
        assert!(book.retiring.contains("gone"));
        record_book_insert(&mut book, &row("c1", "far"), &[0.0, 1.0]);
        record_book_insert(&mut book, &row("c1", "near"), &[1.0, 0.0]);
        record_book_insert(&mut book, &row("c1", "other width"), &[1.0, 0.0, 0.0]);
        record_book_insert(&mut book, &row("c2", "other holder"), &[1.0, 0.0]);
        let found = book_neighbours(&book, "c1", &[1.0, 0.0]);
        let ids: Vec<&str> = found
            .iter()
            .map(|n| n.memory["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["dry-run:c1:2", "dry-run:c1:1"]);
        assert_eq!(found[0].cosine, 1.0);
        assert!(book_neighbours(&book, "nobody", &[1.0, 0.0]).is_empty());
    }
}
