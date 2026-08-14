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

use std::collections::HashMap;

use rusqlite::Connection;

use super::{help_doc_chunks::HelpDocChunksRepository, help_docs::HelpDocsRepository, DbError};
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
    /// v4 `HelpMatchedSection` (`24633026`) — the best-matching section, present
    /// only when the score came from a SECTION vector. Absent when the document
    /// matched only on its whole-document embedding, i.e. it has no chunk rows
    /// embedded yet.
    pub matched_section: Option<HelpMatchedSection>,
}

/// v4 `HelpMatchedSection`.
#[derive(Debug, Clone)]
pub struct HelpMatchedSection {
    /// Nearest Markdown heading above the matching text, if any.
    pub heading: Option<String>,
    /// The matching excerpt itself.
    pub content: String,
    /// 0-based position of this chunk within the document (a JS number — see
    /// [`super::help_doc_chunks::HelpDocChunkRow::chunk_index`]).
    pub chunk_index: f64,
}

/// The best section per document (v4 `scoreSections`'s map value).
#[derive(Debug, Clone)]
struct BestSection {
    score: f64,
    heading: Option<String>,
    content: String,
    chunk_index: f64,
}

/// v4 `HelpSearch.scoreSections` (`24633026`) — score every embedded section
/// chunk against the query and keep the best one per document.
///
/// v4's *why*, carried forward: dimension mismatches are skipped exactly as they
/// are for documents — a chunk embedded under a previous profile is unusable
/// rather than wrong. **Any failure here is swallowed**: section scoring is an
/// improvement on whole-document search, not a precondition for it. That is why
/// this returns a map rather than a `Result`.
///
/// The tie rule is v4's `score > current.score` — strictly greater, so among
/// equal-scoring chunks of one doc the FIRST in row order wins. Row order is
/// `find_all_with_embeddings`' (rowid/insertion), which is v4's `_findAll`
/// order.
fn score_sections(conn: &Connection, query_embedding: &[f32]) -> HashMap<String, BestSection> {
    let mut best: HashMap<String, BestSection> = HashMap::new();

    let chunks = match HelpDocChunksRepository::new(conn).find_all_with_embeddings() {
        Ok(chunks) => chunks,
        Err(e) => {
            eprintln!("[help-search] Section-level help scoring failed; falling back to whole-document scores: {e}");
            return best;
        }
    };

    for chunk in chunks {
        if chunk.embedding.len() != query_embedding.len() {
            continue;
        }
        // Lengths are equal here (guarded above), so cosine cannot mismatch.
        let score = cosine_similarity(query_embedding, &chunk.embedding).unwrap_or(0.0);
        let replace = match best.get(&chunk.doc_id) {
            None => true,
            Some(current) => score > current.score,
        };
        if replace {
            best.insert(
                chunk.doc_id.clone(),
                BestSection {
                    score,
                    heading: chunk.heading.clone(),
                    content: chunk.content.clone(),
                    chunk_index: chunk.chunk_index,
                },
            );
        }
    }

    best
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

    // Section-level scores FIRST (v4 `24633026`). v4's *why*: a whole-document
    // vector for a long, broad page (chat-settings.md spans a dozen subsystems)
    // is a smear that matches any specific question only weakly, so the best
    // section's score stands in for the document wherever sections exist.
    let best_section_by_doc = score_sections(conn, query_embedding);

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
        let doc_score = cosine_similarity(query_embedding, &doc.embedding).unwrap_or(0.0);
        let best = best_section_by_doc.get(&doc.id);

        // Take whichever vector spoke more strongly (v4's *why*): the
        // document's own score is kept in play so a doc that is BROADLY
        // on-topic isn't buried by an unlucky slicing, and so docs with no
        // chunks yet still rank at all.
        let raw_score = match best {
            Some(b) => doc_score.max(b.score),
            None => doc_score,
        };
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
            // v4 attaches the section ONLY when it was at least as strong as
            // the document's own vector — `best.score >= docScore`, so an exact
            // tie DOES attach.
            matched_section: best
                .filter(|b| b.score >= doc_score)
                .map(|b| HelpMatchedSection {
                    heading: b.heading.clone(),
                    content: b.content.clone(),
                    chunk_index: b.chunk_index,
                }),
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
                // The keyword fallback never scores sections (v4 maps its
                // results without a `matchedSection` key at all).
                matched_section: None,
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
