//! Help-documentation search — port of v4 `lib/help-search.ts` (`HelpSearch.search`)
//! + the keyword-fallback half of `lib/tools/handlers/help-search-handler.ts`.
//!
//! Read-only over the **main** DB `help_docs` table. Two paths, matching v4:
//!   - [`semantic_search`] — cosine over the stored embeddings with the
//!     literal-phrase boost (v4 `HelpSearch.search`);
//!   - [`keyword_search`] — the TF-IDF-ish keyword scorer over `title + content`
//!     (v4 handler `keywordSearch` + `getAllDocuments`), the fallback the tool
//!     runs when the embedding call throws.
//!
//! ## The disk-sync host seam (deferred)
//!
//! v4's `HelpSearch.loadFromDatabase()` calls `ensureHelpDocsSynced()`, which reads
//! bundled `help/*.md` from disk and upserts them (re-embedding changed docs
//! elsewhere). That is a **host-side startup index-build** and is a no-op once
//! `help_docs` already has rows (`if (existing.length > 0) return`). The tool's
//! search path only ever READS stored embeddings, so this port reproduces the read
//! and treats the disk sync as a host seam — the same discipline the knowledge
//! injector uses (its fixture seeds chunks directly; `linkDocumentContent` doesn't
//! chunk/embed). Production wires the sync at startup; here the caller supplies
//! populated `help_docs`.

use rusqlite::Connection;

use super::{help_docs::HelpDocsRepository, DbError};
use crate::embedding_vector::{cosine_similarity, extract_search_terms, text_similarity};
use crate::literal_boost::{apply_literal_boost, contains_literal_phrase, get_literal_phrase};

/// One help-search hit — the flat shape the tool handler maps to (v4's
/// `{ id, title, path, url, score, content }`).
#[derive(Debug, Clone)]
pub struct HelpSearchResult {
    pub id: String,
    pub title: String,
    pub path: String,
    pub url: String,
    pub score: f64,
    pub content: String,
}

/// v4 `HelpSearch.search` — cosine over the stored help-doc embeddings, with the
/// literal-phrase boost on a verbatim title/content hit. Docs without an embedding
/// or with a dimension mismatch are skipped (v4's `continue` guards, so no throw).
/// Sorted by score desc, sliced to `limit`.
pub fn semantic_search(
    conn: &Connection,
    query_embedding: &[f32],
    limit: usize,
    query: Option<&str>,
) -> Result<Vec<HelpSearchResult>, DbError> {
    let repo = HelpDocsRepository::new(conn);
    let embedded_docs = repo.find_all_with_embeddings()?;
    if embedded_docs.is_empty() {
        return Ok(Vec::new());
    }

    let literal_phrase = get_literal_phrase(query);

    let mut results: Vec<HelpSearchResult> = Vec::new();
    for doc in &embedded_docs {
        // v4: `if (!doc.embedding || doc.embedding.length === 0) continue`.
        if doc.embedding.is_empty() {
            continue;
        }
        // v4: dimension mismatch → `continue` (guarded, no throw).
        if query_embedding.len() != doc.embedding.len() {
            continue;
        }
        // Lengths are equal here, so cosine cannot mismatch.
        let raw_score = cosine_similarity(query_embedding, &doc.embedding).unwrap_or(0.0);
        let literal_hit = literal_phrase
            .as_ref()
            .map(|p| {
                contains_literal_phrase(Some(&doc.title), p)
                    || contains_literal_phrase(Some(&doc.content), p)
            })
            .unwrap_or(false);
        let score = if literal_hit {
            apply_literal_boost(raw_score, 0.5)
        } else {
            raw_score
        };
        results.push(HelpSearchResult {
            id: doc.id.clone(),
            title: doc.title.clone(),
            path: doc.path.clone(),
            url: doc.url.clone(),
            score,
            content: doc.content.clone(),
        });
    }

    // Stable sort by score desc, slice to limit.
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(limit);
    Ok(results)
}

/// v4 handler `keywordSearch` — score every doc's `title + content` with the
/// keyword/phrase fallback scorer, keep score > 0, sort desc, slice to `limit`.
/// The tool's fallback when the semantic path throws.
pub fn keyword_search(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<HelpSearchResult>, DbError> {
    let repo = HelpDocsRepository::new(conn);
    let terms = extract_search_terms(query);
    let all_docs = repo.find_all()?;

    let mut scored: Vec<HelpSearchResult> = all_docs
        .into_iter()
        .map(|doc| {
            let target = format!("{} {}", doc.title, doc.content);
            let score = text_similarity(&terms.keywords, &terms.exact_phrases, &target);
            HelpSearchResult {
                id: doc.id,
                title: doc.title,
                path: doc.path,
                url: doc.url,
                score,
                content: doc.content,
            }
        })
        .filter(|r| r.score > 0.0)
        .collect();

    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(limit);
    Ok(scored)
}
