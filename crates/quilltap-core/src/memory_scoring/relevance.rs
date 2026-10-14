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

#[cfg(test)]
mod jest_twins {
    //! Case-for-case port of the scoring cases of v4
    //! `__tests__/unit/lib/memory/ranking-blend.test.ts` (the pin
    //! `83334b9a5`) — the "F1 — ranking blend" and "F2 — provider-aware
    //! cosine floor" describes ("Regression guard for the Commonplace Book
    //! recall ranking math").
    //!
    //! NOT twinned here (their modules are not this file's): "F2 — empty pool
    //! yields an empty head" (`recall/render`'s `renderHead`), "F4 —
    //! anti-repetition penalty" (`recall/tags`) and "F4 — recall-history ring
    //! buffer" (`recall/history`). jest's bare `toBeCloseTo(x)` is precision 2
    //! (`|a − x| < 0.005`).

    use super::*;

    // -- describe('F1 — ranking blend (relevance leads, priority decays)') --

    /// `it('weights sum to 1 with relevance dominant')`.
    #[test]
    fn weights_sum_to_1_with_relevance_dominant() {
        assert!((RANKING_RELEVANCE_WEIGHT + RANKING_PRIORITY_WEIGHT - 1.0).abs() < 0.005);
        const { assert!(RANKING_RELEVANCE_WEIGHT > RANKING_PRIORITY_WEIGHT) };
    }

    /// `it('on-topic-ordinary beats off-topic-important (the §1.1 failure mode)')`.
    #[test]
    fn on_topic_ordinary_beats_off_topic_important_the_1_1_failure_mode() {
        // On-topic ordinary: strong cosine, middling importance.
        let on_topic_ordinary = compute_ranking_blend(0.8, 0.5);
        // Off-topic important: weak cosine, high (even un-decayed) importance.
        let off_topic_important = compute_ranking_blend(0.3, 0.9);
        assert!(on_topic_ordinary > off_topic_important);
    }

    /// `it('on-topic-important still ranks highest')`.
    #[test]
    fn on_topic_important_still_ranks_highest() {
        let on_topic_important = compute_ranking_blend(0.8, 0.9);
        let on_topic_ordinary = compute_ranking_blend(0.8, 0.5);
        let off_topic_important = compute_ranking_blend(0.3, 0.9);
        assert!(on_topic_important > on_topic_ordinary);
        assert!(on_topic_important > off_topic_important);
    }

    /// `it('cosine ≥ 0.7 outranks cosine ≤ 0.35 regardless of importance (acceptance)')`.
    #[test]
    fn cosine_ge_0_7_outranks_cosine_le_0_35_regardless_of_importance_acceptance() {
        // Worst case: the relevant memory has fully-decayed priority (0), the
        // off-topic one has maxed priority (1). Relevance must still win.
        let relevant_decayed = compute_ranking_blend(0.7, 0.0);
        let irrelevant_max_importance = compute_ranking_blend(0.35, 1.0);
        assert!(relevant_decayed > irrelevant_max_importance);
    }

    // -- describe('F2 — provider-aware cosine floor') --

    /// `it('local TF-IDF floor is lower than the neural floor')`.
    #[test]
    fn local_tf_idf_floor_is_lower_than_the_neural_floor() {
        const { assert!(DEFAULT_MIN_COSINE_TFIDF < DEFAULT_MIN_COSINE_NEURAL) };
    }

    /// `it('resolves BUILTIN to the TF-IDF floor and everything else to neural')`.
    #[test]
    fn resolves_builtin_to_the_tf_idf_floor_and_everything_else_to_neural() {
        assert_eq!(
            default_min_cosine_for_provider(Some("BUILTIN")),
            DEFAULT_MIN_COSINE_TFIDF
        );
        assert_eq!(
            default_min_cosine_for_provider(Some("OPENAI")),
            DEFAULT_MIN_COSINE_NEURAL
        );
        assert_eq!(
            default_min_cosine_for_provider(Some("OLLAMA")),
            DEFAULT_MIN_COSINE_NEURAL
        );
        assert_eq!(
            default_min_cosine_for_provider(None),
            DEFAULT_MIN_COSINE_NEURAL
        );
    }
}
