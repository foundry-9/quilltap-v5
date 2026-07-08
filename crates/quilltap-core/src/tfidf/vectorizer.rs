//! `TfIdfVectorizer` with BM25 enhancement — a transcription of v4's
//! `plugins/dist/qtap-plugin-builtin-embeddings/tfidf-vectorizer.ts`.
//!
//! Float math is f64 throughout (JS numbers are f64). The one non-bit-exact
//! operation is the IDF's `Math.log`: macOS's system `ln` (and the `libm` crate)
//! diverge from V8's `Math.log` by up to 1 ULP on many inputs — a documented
//! libm seam (the `memory_injector` `Math.pow` precedent). The tier-1 differential
//! compares `idf` and the transformed vectors NUMERICALLY at 1e-12 (a 1-ULP
//! divergence is ≈1e-16 ≪ 1e-12); the tier-3 refit differential normalizes the
//! persisted `idf` JSON column at the same tolerance. Everything else
//! (`vocabulary`, `avgDocLength`, `vocabularySize`) is bit-exact.

use std::collections::{HashMap, HashSet};

use super::porter::{generate_bigrams, tokenize};

/// BM25 term-frequency saturation parameter (v4 `BM25_K1`).
pub const BM25_K1: f64 = 1.5;
/// BM25 document-length normalization parameter (v4 `BM25_B`).
pub const BM25_B: f64 = 0.75;

/// The errors the vectorizer / provider raise, each carrying v4's exact message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TfidfError {
    /// `fitCorpus([])` — v4 `'Cannot fit on empty corpus'`.
    EmptyCorpus,
    /// `transform` before fit — v4 `'Vectorizer must be fitted before transform'`.
    NotFittedTransform,
    /// The provider's own pre-transform guard (v4 `BuiltinEmbeddingProvider`).
    NotFittedProvider,
}

impl TfidfError {
    /// The exact v4 error string.
    pub fn message(&self) -> &'static str {
        match self {
            TfidfError::EmptyCorpus => "Cannot fit on empty corpus",
            TfidfError::NotFittedTransform => "Vectorizer must be fitted before transform",
            TfidfError::NotFittedProvider => {
                "Built-in embedding provider must be fitted before generating embeddings. \
                 Call fitCorpus() with your documents first."
            }
        }
    }
}

impl std::fmt::Display for TfidfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for TfidfError {}

/// The serializable vectorizer state (v4 `LocalEmbeddingProviderState`). The
/// `vocabulary` is the ordered `[[term, index], …]` pair list (insertion order —
/// sorted-term order after a fit); `idf` is the per-index weight array.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalEmbeddingProviderState {
    /// `[[term, index], …]` in insertion (sorted) order.
    pub vocabulary: Vec<(String, usize)>,
    /// Per-index BM25 IDF weights.
    pub idf: Vec<f64>,
    /// Average tokenized-document length (BM25 length normalization).
    pub avg_doc_length: f64,
    /// `vocabulary.len()` — carried in the state for the persisted column.
    pub vocabulary_size: usize,
    /// Whether bigrams are part of the vocabulary.
    pub include_bigrams: bool,
    /// The fit timestamp (v4 `new Date().toISOString()`, injected here).
    pub fitted_at: String,
}

/// TF-IDF vectorizer with BM25 enhancement (v4 `TfIdfVectorizer`).
pub struct TfIdfVectorizer {
    /// `[[term, index], …]` — the ordered entries (`Array.from(map.entries())`).
    vocab_entries: Vec<(String, usize)>,
    /// term → index lookup (v4's `Map.get`).
    vocab_index: HashMap<String, usize>,
    idf: Vec<f64>,
    avg_doc_length: f64,
    include_bigrams: bool,
    fitted_at: Option<String>,
}

impl TfIdfVectorizer {
    /// New vectorizer (v4 `new TfIdfVectorizer(includeBigrams = true)`).
    pub fn new(include_bigrams: bool) -> Self {
        Self {
            vocab_entries: Vec::new(),
            vocab_index: HashMap::new(),
            idf: Vec::new(),
            avg_doc_length: 0.0,
            include_bigrams,
            fitted_at: None,
        }
    }

    /// Has the vectorizer been fitted? (v4: `vocabulary.size > 0 && fittedAt !==
    /// null`.)
    pub fn is_fitted(&self) -> bool {
        !self.vocab_entries.is_empty() && self.fitted_at.is_some()
    }

    /// Vocabulary size (v4 `getVocabularySize`).
    pub fn vocabulary_size(&self) -> usize {
        self.vocab_entries.len()
    }

    /// Embedding dimensions (== vocabulary size; v4 `getDimensions`).
    pub fn dimensions(&self) -> usize {
        self.vocab_entries.len()
    }

    /// Tokenize a document into terms (unigrams and optionally bigrams; v4
    /// `tokenizeDocument`).
    fn tokenize_document(&self, document: &str) -> Vec<String> {
        let unigrams = tokenize(document, true);
        if !self.include_bigrams {
            return unigrams;
        }
        let bigrams = generate_bigrams(&unigrams);
        let mut out = unigrams;
        out.extend(bigrams);
        out
    }

    /// Fit the vectorizer on a corpus (v4 `fitCorpus`). `fitted_at` is the injected
    /// clock (v4's internal `new Date().toISOString()`). Throws
    /// [`TfidfError::EmptyCorpus`] on an empty corpus.
    pub fn fit_corpus(
        &mut self,
        documents: &[String],
        fitted_at: String,
    ) -> Result<(), TfidfError> {
        if documents.is_empty() {
            return Err(TfidfError::EmptyCorpus);
        }

        let tokenized: Vec<Vec<String>> = documents
            .iter()
            .map(|doc| self.tokenize_document(doc))
            .collect();

        // Build the vocabulary from unique terms, sorted for a stable index map.
        // v4: `Array.from(new Set(...)).sort()` — JS default sort is by UTF-16 code
        // unit; for ASCII tokens (incl. `_`) that equals Rust's byte/scalar `sort`.
        let mut term_set: HashSet<String> = HashSet::new();
        for tokens in &tokenized {
            for token in tokens {
                term_set.insert(token.clone());
            }
        }
        let mut sorted_terms: Vec<String> = term_set.into_iter().collect();
        sorted_terms.sort();

        self.vocab_entries = sorted_terms
            .iter()
            .enumerate()
            .map(|(i, t)| (t.clone(), i))
            .collect();
        self.vocab_index = sorted_terms
            .iter()
            .enumerate()
            .map(|(i, t)| (t.clone(), i))
            .collect();
        let vocab_size = sorted_terms.len();

        // Document frequencies (per distinct term per doc).
        let mut doc_frequencies = vec![0i64; vocab_size];
        for tokens in &tokenized {
            let unique: HashSet<&String> = tokens.iter().collect();
            for token in unique {
                if let Some(&idx) = self.vocab_index.get(token) {
                    doc_frequencies[idx] += 1;
                }
            }
        }

        // BM25 IDF: log((N - df + 0.5) / (df + 0.5) + 1). `ln` is the libm seam.
        let n = documents.len() as f64;
        self.idf = doc_frequencies
            .iter()
            .map(|&df| {
                let df = df as f64;
                ((n - df + 0.5) / (df + 0.5) + 1.0).ln()
            })
            .collect();

        // Average document length (tokenized, including bigrams).
        let total_length: usize = tokenized.iter().map(|t| t.len()).sum();
        self.avg_doc_length = total_length as f64 / documents.len() as f64;

        self.fitted_at = Some(fitted_at);
        Ok(())
    }

    /// Transform a document into a BM25 TF-IDF vector (v4 `transform`). The
    /// returned `Vec<f64>` mirrors v4's `number[]` — already L2-normalized. Throws
    /// [`TfidfError::NotFittedTransform`] before a fit.
    pub fn transform(&self, document: &str) -> Result<Vec<f64>, TfidfError> {
        if !self.is_fitted() {
            return Err(TfidfError::NotFittedTransform);
        }

        let tokens = self.tokenize_document(document);
        let doc_length = tokens.len() as f64;

        // Term frequencies.
        let mut term_counts: HashMap<&str, f64> = HashMap::new();
        for token in &tokens {
            *term_counts.entry(token.as_str()).or_insert(0.0) += 1.0;
        }

        // BM25 TF-IDF per present term (writes to distinct indices — iteration
        // order is irrelevant).
        let mut vector = vec![0.0_f64; self.vocab_entries.len()];
        for (term, &count) in &term_counts {
            let Some(&idx) = self.vocab_index.get(*term) else {
                continue;
            };
            let tf = count;
            let length_norm = 1.0 - BM25_B + BM25_B * (doc_length / self.avg_doc_length);
            let bm25_tf = (tf * (BM25_K1 + 1.0)) / (tf + BM25_K1 * length_norm);
            vector[idx] = bm25_tf * self.idf[idx];
        }

        // L2 normalize (sum accumulated in index order, matching v4's reduce).
        let magnitude = vector.iter().map(|v| v * v).sum::<f64>().sqrt();
        if magnitude > 0.0 {
            for v in &mut vector {
                *v /= magnitude;
            }
        }

        Ok(vector)
    }

    /// The serializable state, or `None` when not fitted (v4 `getState`).
    pub fn get_state(&self) -> Option<LocalEmbeddingProviderState> {
        if !self.is_fitted() {
            return None;
        }
        Some(LocalEmbeddingProviderState {
            vocabulary: self.vocab_entries.clone(),
            idf: self.idf.clone(),
            avg_doc_length: self.avg_doc_length,
            vocabulary_size: self.vocab_entries.len(),
            include_bigrams: self.include_bigrams,
            fitted_at: self
                .fitted_at
                .clone()
                .expect("is_fitted() guarantees fitted_at"),
        })
    }

    /// Load a serialized state (v4 `loadState`). Rebuilds both the ordered entries
    /// and the lookup map from `state.vocabulary`.
    pub fn load_state(&mut self, state: LocalEmbeddingProviderState) {
        self.vocab_index = state.vocabulary.iter().cloned().collect();
        self.vocab_entries = state.vocabulary;
        self.idf = state.idf;
        self.avg_doc_length = state.avg_doc_length;
        self.include_bigrams = state.include_bigrams;
        self.fitted_at = Some(state.fitted_at);
    }
}

/// `JSON.stringify(state.vocabulary)` — the persisted `vocabulary` column bytes.
/// `[["term",0],["term2",1],…]`; serde tuples become JSON arrays, indices are
/// integers, matching v4 byte-for-byte.
pub fn state_vocabulary_json(state: &LocalEmbeddingProviderState) -> String {
    serde_json::to_string(&state.vocabulary).expect("vocabulary serializes")
}

/// `JSON.stringify(state.idf)` — the persisted `idf` column bytes. Each weight is
/// rendered through [`crate::db::js_number_to_json`] so an integer-valued weight
/// collapses to a JS integer (matching `JSON.stringify`), and finite floats use
/// serde_json's shortest round-trip (== V8's `Number.prototype.toString`). NB: the
/// weights themselves carry the `ln` libm seam — byte-exactness holds only when
/// Rust's `ln` and V8's `Math.log` agree (usually; the tier-3 differential
/// normalizes this column numerically).
pub fn state_idf_json(state: &LocalEmbeddingProviderState) -> String {
    let arr = serde_json::Value::Array(
        state
            .idf
            .iter()
            .map(|&x| crate::db::js_number_to_json(x))
            .collect(),
    );
    serde_json::to_string(&arr).expect("idf serializes")
}
