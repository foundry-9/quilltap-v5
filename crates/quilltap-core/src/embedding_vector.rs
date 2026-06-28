//! Port of the pure vector-math hot paths in v4's
//! lib/embedding/embedding-service.ts: L2 normalisation, the embedding-profile
//! storage policy (Matryoshka truncate + optional normalise), the cosine
//! similarity (dot product, vectors assumed unit-length), the dimension-match
//! guard with its exact error message, and the fallback keyword/phrase
//! `textSimilarity` scorer.
//!
//! Numeric-fidelity notes — these reproduce JS exactly:
//!   * Embeddings are `Float32Array` in v4. Reading an element widens the f32 to
//!     a f64; arithmetic (the norm sum, the dot product, the `* inv` scale) runs
//!     in f64. Writing back into a `Float32Array` rounds to f32. So we carry the
//!     vectors as `f32`, accumulate in f64 (`x as f64`), and round each stored
//!     element with `(.. as f64) as f32` — the same nearest-ties-to-even round
//!     a typed-array store performs.
//!   * `normalizeVector` returns the input unchanged when the norm is exactly
//!     zero (no division). The zero test is on the f64 sum, not per-element.
//!   * `cosineSimilarity` accumulates the dot product in f64 and returns a JS
//!     number (f64); no normalisation, since both inputs are unit vectors.

/// Raised when two embeddings being compared have different dimensionality.
/// Mirrors v4's `EmbeddingDimensionMismatchError`; [`message`](Self::message)
/// reproduces its thrown message byte-for-byte so the tier-1 oracle can check
/// the string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmbeddingDimensionMismatch {
    pub query_length: usize,
    pub stored_length: usize,
    pub context: Option<String>,
}

impl EmbeddingDimensionMismatch {
    /// The exact message v4 throws (`Error.message`), context-parenthesised when
    /// present — matches the template in embedding-service.ts.
    pub fn message(&self) -> String {
        let ctx = match &self.context {
            Some(c) => format!(" ({c})"),
            None => String::new(),
        };
        format!(
            "Embedding dimension mismatch{ctx}: \
             query is {}-d, stored is {}-d. \
             The active embedding profile differs from the on-disk corpus. \
             Re-apply the embedding profile to migrate stored vectors.",
            self.query_length, self.stored_length
        )
    }
}

/// Normalise a vector to unit length (L2). An all-zero vector (norm exactly 0)
/// is returned unchanged. The norm is accumulated in f64 reading each element as
/// f64; each scaled element is rounded back to f32 exactly as the typed-array
/// store does.
pub fn normalize_vector(v: &[f32]) -> Vec<f32> {
    let mut norm = 0.0_f64;
    for &x in v {
        norm += (x as f64) * (x as f64);
    }
    if norm == 0.0 {
        return v.to_vec();
    }
    let inv = 1.0 / norm.sqrt();
    v.iter().map(|&x| (x as f64 * inv) as f32).collect()
}

/// Apply an embedding profile's storage policy to a raw vector: optional
/// Matryoshka slice (keep the first `truncate_to_dimensions` components when the
/// vector is longer) followed by optional L2 normalisation. Never mutates the
/// input. `normalize_l2` defaults to true in v4 (`!== false`); the caller passes
/// the resolved boolean.
pub fn apply_embedding_profile(
    v: &[f32],
    truncate_to_dimensions: Option<usize>,
    normalize_l2: bool,
) -> Vec<f32> {
    let len = v.len();
    let slice_len = match truncate_to_dimensions {
        Some(t) if t < len => t,
        _ => len,
    };
    let out = v[..slice_len].to_vec();
    if normalize_l2 {
        normalize_vector(&out)
    } else {
        out
    }
}

/// Cosine similarity of two embeddings. Both are assumed unit-length, so the
/// result is the plain dot product (accumulated in f64). Returns
/// [`EmbeddingDimensionMismatch`] (with no context, matching v4) when the
/// lengths differ.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> Result<f64, EmbeddingDimensionMismatch> {
    if a.len() != b.len() {
        return Err(EmbeddingDimensionMismatch {
            query_length: a.len(),
            stored_length: b.len(),
            context: None,
        });
    }
    let mut dot = 0.0_f64;
    for i in 0..a.len() {
        dot += (a[i] as f64) * (b[i] as f64);
    }
    Ok(dot)
}

/// Fail-fast dimension guard used at search entry. Returns the mismatch error
/// (carrying the diagnostic `context`) when the lengths differ.
pub fn assert_embedding_dimensions_match(
    query_length: usize,
    stored_length: usize,
    context: Option<&str>,
) -> Result<(), EmbeddingDimensionMismatch> {
    if query_length != stored_length {
        return Err(EmbeddingDimensionMismatch {
            query_length,
            stored_length,
            context: context.map(|c| c.to_string()),
        });
    }
    Ok(())
}

/// Fallback keyword/phrase text-similarity score in `[0, 1]`, used when no
/// embedding is available. Exact phrase hits are worth 3 points each; keyword
/// hits 1 point each, capped at the keyword count. Returns 0 when there is
/// nothing to score. Matching is case-insensitive substring containment;
/// keywords are matched verbatim (v4 lowercases them upstream), phrases are
/// lowercased here.
pub fn text_similarity(keywords: &[String], exact_phrases: &[String], target_text: &str) -> f64 {
    let lower_target = target_text.to_lowercase();
    let mut score = 0_i64;
    let mut max_score = 0_i64;

    for phrase in exact_phrases {
        max_score += 3;
        if lower_target.contains(&phrase.to_lowercase()) {
            score += 3;
        }
    }

    let keyword_matches = keywords
        .iter()
        .filter(|kw| lower_target.contains(*kw))
        .count() as i64;
    let keyword_score = keyword_matches.min(keywords.len() as i64);
    score += keyword_score;
    max_score += keywords.len() as i64;

    if max_score == 0 {
        return 0.0;
    }
    score as f64 / max_score as f64
}
