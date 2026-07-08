//! `BuiltinEmbeddingProvider` — the offline TF-IDF/BM25 embedder (v4's
//! `plugins/dist/qtap-plugin-builtin-embeddings/embedding-provider.ts`). A thin
//! wrapper over [`TfIdfVectorizer`]: fit / load state, then synchronously
//! `generate_embedding`.

use super::vectorizer::{LocalEmbeddingProviderState, TfIdfVectorizer, TfidfError};

/// Model name for the built-in TF-IDF provider (v4 `BUILTIN_MODEL_NAME`).
pub const BUILTIN_MODEL_NAME: &str = "tfidf-bm25-v1";

/// A single builtin embedding result (v4 `EmbeddingResult` from the plugin — the
/// LOCAL shape: an f64 `number[]` vector, before the host's
/// `applyEmbeddingProfile` narrows it to Float32 + slices + renormalizes).
#[derive(Debug, Clone, PartialEq)]
pub struct LocalEmbeddingResult {
    /// The raw (already L2-normalized) TF-IDF vector, f64 like v4's `number[]`.
    pub embedding: Vec<f64>,
    /// The model that produced it (`tfidf-bm25-v1`).
    pub model: String,
    /// `embedding.len()`.
    pub dimensions: usize,
}

/// The built-in TF-IDF embedding provider (v4 `BuiltinEmbeddingProvider`).
pub struct BuiltinEmbeddingProvider {
    vectorizer: TfIdfVectorizer,
}

impl BuiltinEmbeddingProvider {
    /// New provider (v4 `new BuiltinEmbeddingProvider(includeBigrams = true)`).
    pub fn new(include_bigrams: bool) -> Self {
        Self {
            vectorizer: TfIdfVectorizer::new(include_bigrams),
        }
    }

    /// Generate an embedding for a single text (v4 `generateEmbedding`). Fails with
    /// [`TfidfError::NotFittedProvider`] (the provider's own message) before a fit.
    pub fn generate_embedding(&self, text: &str) -> Result<LocalEmbeddingResult, TfidfError> {
        if !self.vectorizer.is_fitted() {
            return Err(TfidfError::NotFittedProvider);
        }
        let embedding = self.vectorizer.transform(text)?;
        let dimensions = embedding.len();
        Ok(LocalEmbeddingResult {
            embedding,
            model: BUILTIN_MODEL_NAME.to_string(),
            dimensions,
        })
    }

    /// Generate embeddings for a batch (v4 `generateBatchEmbeddings`). Propagates
    /// the first failure (v4's `.map` throws on the first not-fitted item).
    pub fn generate_batch_embeddings(
        &self,
        texts: &[String],
    ) -> Result<Vec<LocalEmbeddingResult>, TfidfError> {
        texts.iter().map(|t| self.generate_embedding(t)).collect()
    }

    /// Fit the vocabulary on a corpus (v4 `fitCorpus`). `fitted_at` is the injected
    /// clock.
    pub fn fit_corpus(
        &mut self,
        documents: &[String],
        fitted_at: String,
    ) -> Result<(), TfidfError> {
        self.vectorizer.fit_corpus(documents, fitted_at)
    }

    /// Whether the vocabulary has been fitted (v4 `isFitted`).
    pub fn is_fitted(&self) -> bool {
        self.vectorizer.is_fitted()
    }

    /// Load a serialized state (v4 `loadState`).
    pub fn load_state(&mut self, state: LocalEmbeddingProviderState) {
        self.vectorizer.load_state(state);
    }

    /// The serializable state, or `None` when not fitted (v4 `getState`).
    pub fn get_state(&self) -> Option<LocalEmbeddingProviderState> {
        self.vectorizer.get_state()
    }

    /// Vocabulary size (v4 `getVocabularySize`).
    pub fn vocabulary_size(&self) -> usize {
        self.vectorizer.vocabulary_size()
    }

    /// Embedding dimensions (v4 `getDimensions`).
    pub fn dimensions(&self) -> usize {
        self.vectorizer.dimensions()
    }
}
