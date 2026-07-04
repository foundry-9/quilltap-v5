//! Conversation-chunk semantic search — port of v4
//! `lib/scriptorium/conversation-search.ts` (`searchConversationChunks`).
//!
//! Cosine similarity (over pre-computed embeddings) across the conversation
//! chunks of a character's chats (or, operator-wide, every chat a user owns).
//! Sibling of [`crate::services::knowledge_injector::document_search`] — it shares
//! the same cosine + literal-phrase-boost leaves, but v4 keeps the two as separate
//! modules over different tables (`conversation_chunks` vs `doc_mount_chunks`), so
//! this is a faithful sibling, not a merge.
//!
//! Read-only over the **main** DB. Scope resolution:
//!   - `character_id` → the chats that character participates in;
//!   - else `user_id` → every chat the user owns (the Brahma Console's operator
//!     surface);
//!   - else no scope → empty.
//!
//! A dimension mismatch surfaces as an error (v4's `cosineSimilarity` throws) —
//! the `search` tool's conversation branch swallows it and continues, matching v4.

use rusqlite::Connection;
use serde_json::Value;

use super::{chats_read, conversation_chunks::ConversationChunksRepository, DbError};
use crate::embedding_vector::cosine_similarity;
use crate::literal_boost::{apply_literal_boost, contains_literal_phrase, get_literal_phrase};

/// One conversation-search hit (v4 `ConversationSearchResult`).
#[derive(Debug, Clone)]
pub struct ConversationSearchResult {
    pub chunk_id: String,
    pub chat_id: String,
    pub conversation_title: String,
    /// `interchangeIndex` — a JS number; carried as `f64`.
    pub interchange_index: f64,
    pub content: String,
    pub participant_names: Vec<String>,
    pub score: f64,
}

/// Search options (v4 `ConversationSearchOptions`).
#[derive(Debug, Clone, Default)]
pub struct ConversationSearchOptions {
    /// Scope to this character's chats.
    pub character_id: Option<String>,
    /// Operator-wide scope (every chat the user owns) when no `character_id`.
    pub user_id: Option<String>,
    /// v4 default 10.
    pub limit: Option<usize>,
    /// v4 default 0.3.
    pub min_score: Option<f64>,
    /// Original query text — required for the literal-phrase boost.
    pub query: Option<String>,
    /// Lift verbatim (case-insensitive) content hits toward 1.0 before filtering.
    pub apply_literal_phrase_boost: bool,
}

/// v4 `searchConversationChunks`. `conn` is a **main**-db connection (read pool).
pub fn search_conversation_chunks(
    conn: &Connection,
    query_embedding: &[f32],
    options: &ConversationSearchOptions,
) -> Result<Vec<ConversationSearchResult>, DbError> {
    let limit = options.limit.unwrap_or(10);
    let min_score = options.min_score.unwrap_or(0.3);

    // Resolve the set of chats to search (character scope, else operator-wide).
    let character_chats: Vec<Value> = if let Some(cid) = &options.character_id {
        chats_read::find_by_character_id(conn, cid)?
    } else if let Some(uid) = &options.user_id {
        chats_read::find_by_user_id(conn, uid)?
    } else {
        Vec::new()
    };

    let character_chat_ids: std::collections::HashSet<String> = character_chats
        .iter()
        .filter_map(|c| c.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();
    if character_chat_ids.is_empty() {
        return Ok(Vec::new());
    }

    // Load every embedded chunk, then filter to the in-scope chats.
    let repo = ConversationChunksRepository::new(conn);
    let all_chunks: Vec<_> = repo
        .find_all_with_embeddings()?
        .into_iter()
        .filter(|chunk| character_chat_ids.contains(&chunk.chat_id))
        .collect();
    if all_chunks.is_empty() {
        return Ok(Vec::new());
    }

    // Compute cosine (with optional literal boost) BEFORE minScore filtering.
    let literal_phrase = if options.apply_literal_phrase_boost {
        get_literal_phrase(options.query.as_deref())
    } else {
        None
    };

    struct Scored {
        idx: usize,
        score: f64,
    }
    let mut scored_all: Vec<Scored> = Vec::with_capacity(all_chunks.len());
    for (idx, chunk) in all_chunks.iter().enumerate() {
        // v4's `cosineSimilarity` throws on a dimension mismatch; the error
        // propagates to the tool's conversation branch, which swallows it.
        let raw_score = cosine_similarity(query_embedding, &chunk.embedding)
            .map_err(|e| DbError::Sqlite(rusqlite::Error::InvalidParameterName(e.message())))?;
        let literal_hit = literal_phrase
            .as_ref()
            .map(|p| contains_literal_phrase(Some(&chunk.content), p))
            .unwrap_or(false);
        let score = if literal_hit {
            apply_literal_boost(raw_score, 0.5)
        } else {
            raw_score
        };
        scored_all.push(Scored { idx, score });
    }

    // filter >= minScore, stable sort by score desc, slice to limit.
    let mut scored: Vec<Scored> = scored_all
        .into_iter()
        .filter(|s| s.score >= min_score)
        .collect();
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(limit);
    if scored.is_empty() {
        return Ok(Vec::new());
    }

    // Title map from the already-loaded in-scope chats. `chat.title || 'Untitled'`.
    let mut chat_titles: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for chat in &character_chats {
        if let Some(id) = chat.get("id").and_then(Value::as_str) {
            let title = chat.get("title").and_then(Value::as_str).unwrap_or("");
            chat_titles.insert(id.to_string(), title.to_string());
        }
    }

    let results = scored
        .into_iter()
        .map(|s| {
            let chunk = &all_chunks[s.idx];
            let title = chat_titles
                .get(&chunk.chat_id)
                .filter(|t| !t.is_empty())
                .cloned()
                .unwrap_or_else(|| "Untitled".to_string());
            ConversationSearchResult {
                chunk_id: chunk.id.clone(),
                chat_id: chunk.chat_id.clone(),
                conversation_title: title,
                interchange_index: chunk.interchange_index,
                content: chunk.content.clone(),
                participant_names: chunk.participant_names.clone(),
                score: s.score,
            }
        })
        .collect();

    Ok(results)
}
