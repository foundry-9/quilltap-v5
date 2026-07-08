//! The BUILTIN TF-IDF/BM25 embedding provider (W4.7e sub-unit 5, split off as
//! W4.7e2). A faithful transcription of v4's zero-network fallback embedder
//! (`plugins/dist/qtap-plugin-builtin-embeddings/`): the Porter stemmer +
//! tokenizer ([`porter`]), the BM25-enhanced TF-IDF vectorizer ([`vectorizer`]),
//! and the [`provider`] wrapper. Pure computation — no model seam, no HTTP.
//!
//! The host glue (`generate_builtin_embedding`) and the `EMBEDDING_REFIT` job
//! handler live in [`crate::services::builtin_embedding`] /
//! [`crate::services::embedding_refit_job`] (they need `Db` reads).

pub mod porter;
pub mod provider;
pub mod vectorizer;

pub use porter::{generate_bigrams, stem, tokenize, STOP_WORDS};
pub use provider::{BuiltinEmbeddingProvider, LocalEmbeddingResult, BUILTIN_MODEL_NAME};
pub use vectorizer::{
    state_idf_json, state_vocabulary_json, LocalEmbeddingProviderState, TfIdfVectorizer,
    TfidfError, BM25_B, BM25_K1,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_words_returned_verbatim() {
        // length <= 2 → unchanged and un-lowercased (v4 early return).
        assert_eq!(stem("AB"), "AB");
        assert_eq!(stem("Go"), "Go");
        assert_eq!(stem("a"), "a");
    }

    #[test]
    fn classic_porter_examples() {
        assert_eq!(stem("caresses"), "caress");
        assert_eq!(stem("ponies"), "poni");
        assert_eq!(stem("running"), "run");
        assert_eq!(stem("happy"), "happi");
        assert_eq!(stem("relational"), "relat");
        assert_eq!(stem("controll"), "control");
    }

    #[test]
    fn tokenize_strips_stop_and_short_words() {
        assert_eq!(
            tokenize("The quick brown foxes", true),
            vec!["quick", "brown", "fox"]
        );
        // Stop words kept when disabled; 1-char words still dropped.
        assert_eq!(tokenize("the a big", false), vec!["the", "big"]);
        // Non-ASCII replaced with a space (a delimiter).
        assert_eq!(tokenize("café", true), vec!["caf"]);
        assert!(tokenize("", true).is_empty());
    }

    #[test]
    fn bigrams_edges() {
        assert!(generate_bigrams(&[]).is_empty());
        assert!(generate_bigrams(&["only".to_string()]).is_empty());
        assert_eq!(
            generate_bigrams(&["a".to_string(), "b".to_string(), "c".to_string()]),
            vec!["a_b", "b_c"]
        );
    }

    #[test]
    fn fit_transform_roundtrip_and_state() {
        let docs = vec![
            "the dragon guards treasure".to_string(),
            "a knight slays the dragon".to_string(),
            "merchants trade along roads".to_string(),
        ];
        let mut v = TfIdfVectorizer::new(true);
        v.fit_corpus(&docs, "2020-01-01T00:00:00.000Z".to_string())
            .unwrap();
        assert!(v.is_fitted());
        let state = v.get_state().expect("state");
        assert_eq!(state.vocabulary_size, state.vocabulary.len());
        assert_eq!(state.idf.len(), state.vocabulary.len());

        // getState → JSON → loadState → transform equals fit → transform.
        let vocab_json = state_vocabulary_json(&state);
        let idf_json = state_idf_json(&state);
        let mut v2 = TfIdfVectorizer::new(true);
        v2.load_state(LocalEmbeddingProviderState {
            vocabulary: serde_json::from_str(&vocab_json).unwrap(),
            idf: serde_json::from_str(&idf_json).unwrap(),
            avg_doc_length: state.avg_doc_length,
            vocabulary_size: state.vocabulary_size,
            include_bigrams: state.include_bigrams,
            fitted_at: state.fitted_at.clone(),
        });
        let q = "the dragon";
        assert_eq!(v.transform(q).unwrap(), v2.transform(q).unwrap());
    }

    #[test]
    fn error_messages_are_exact() {
        let mut v = TfIdfVectorizer::new(true);
        assert_eq!(
            v.fit_corpus(&[], "x".to_string()).unwrap_err(),
            TfidfError::EmptyCorpus
        );
        assert_eq!(
            v.transform("x").unwrap_err().message(),
            "Vectorizer must be fitted before transform"
        );
        let p = BuiltinEmbeddingProvider::new(true);
        assert_eq!(
            p.generate_embedding("x").unwrap_err().message(),
            "Built-in embedding provider must be fitted before generating embeddings. \
             Call fitCorpus() with your documents first."
        );
    }

    #[test]
    fn provider_result_carries_model_and_dims() {
        let docs = vec![
            "alpha beta gamma".to_string(),
            "beta gamma delta".to_string(),
        ];
        let mut p = BuiltinEmbeddingProvider::new(true);
        p.fit_corpus(&docs, "2020-01-01T00:00:00.000Z".to_string())
            .unwrap();
        let r = p.generate_embedding("alpha beta").unwrap();
        assert_eq!(r.model, BUILTIN_MODEL_NAME);
        assert_eq!(r.dimensions, r.embedding.len());
        assert_eq!(r.dimensions, p.dimensions());
    }
}
