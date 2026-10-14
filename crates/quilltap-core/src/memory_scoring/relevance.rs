//! Port of v4 `lib/memory/scoring/relevance.ts` — relevance and the recall
//! blend.
//!
//! v4's why, carried: relevance is the query cosine, named so every ranking
//! site is greppable. Retrieval ranks by one blended key, `0.75 × relevance +
//! 0.25 × salience`: relevance is the primary key, salience a decaying
//! tie-breaker with no floor. Recall-tag and anti-repetition multipliers are
//! applied to the blend afterwards (the recall lane).

use crate::embedding_scale::is_lexical_embedding_provider;

/// Relevance of a candidate to a query: the cosine itself.
pub fn relevance(cosine: f64) -> f64 {
    cosine
}

pub const RANKING_RELEVANCE_WEIGHT: f64 = 0.75;
pub const RANKING_PRIORITY_WEIGHT: f64 = 0.25;

/// The recall blend: `0.75 × relevance + 0.25 × salience`.
pub fn compute_ranking_blend(cosine: f64, salience: f64) -> f64 {
    RANKING_RELEVANCE_WEIGHT * relevance(cosine) + RANKING_PRIORITY_WEIGHT * salience
}

/// Default minimum cosine for a memory to be eligible for recall. v4's why:
/// neural API embeddings live in a compressed band (~0.25 unrelated,
/// ~0.45–0.75 related); the built-in TF-IDF profile produces much sparser
/// cosines, so the floor is provider-aware.
pub const DEFAULT_MIN_COSINE_NEURAL: f64 = 0.30;
pub const DEFAULT_MIN_COSINE_TFIDF: f64 = 0.10;

/// The relevance floor for an embedding profile's provider (the ONE scale
/// predicate decides).
pub fn default_min_cosine_for_provider(provider: Option<&str>) -> f64 {
    if is_lexical_embedding_provider(provider) {
        DEFAULT_MIN_COSINE_TFIDF
    } else {
        DEFAULT_MIN_COSINE_NEURAL
    }
}
