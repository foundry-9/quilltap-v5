//! Tier-1 differential (W4.7e2): the BUILTIN TF-IDF/BM25 embedder — exact
//! equivalence against v4's REAL plugin (`stem` / `tokenize` / `generateBigrams`
//! / `TfIdfVectorizer`).
//!
//! Diffed:
//!   * `stem` — dozens of words per suffix family (a one-word divergence shifts
//!     every vocabulary index);
//!   * `tokenize` — stop-word on/off, punctuation, digits, non-ASCII, whitespace;
//!   * `generateBigrams`;
//!   * fit → getState (vocabulary/avgDocLength/vocabularySize/includeBigrams/
//!     fittedAt exact, idf at 1e-12) + transform (at 1e-12);
//!   * loadState-from-persisted-JSON → transform (the host read path);
//!   * the two throw messages.
//!
//! The `idf` and transformed vectors carry the `Math.log` libm seam (macOS `ln`
//! vs V8 `Math.log` differ by ≤1 ULP ≈ 1e-16), so numeric fields compare at
//! 1e-12; the stemmer / tokenizer / vocabulary are exact.
//!
//! Generate the oracle:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/tfidf-vectorizer.ts \
//!     > /tmp/oracle-tfidf-vectorizer.ndjson
//! Run:
//!   QT_ORACLE_TFIDF=/tmp/oracle-tfidf-vectorizer.ndjson \
//!     cargo test -p quilltap-harness --test tfidf_vectorizer_equivalence

use quilltap_core::tfidf::{
    generate_bigrams, stem, tokenize, LocalEmbeddingProviderState, TfIdfVectorizer, TfidfError,
};
use serde::Deserialize;

const TOL: f64 = 1e-12;

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "stem")]
    Stem { word: String, stemmed: String },
    #[serde(rename = "tokenize")]
    Tokenize {
        id: String,
        text: String,
        #[serde(rename = "removeStopWords")]
        remove_stop_words: bool,
        tokens: Vec<String>,
    },
    #[serde(rename = "bigrams")]
    Bigrams {
        id: String,
        tokens: Vec<String>,
        bigrams: Vec<String>,
    },
    #[serde(rename = "fit")]
    Fit {
        id: String,
        #[serde(rename = "includeBigrams")]
        include_bigrams: bool,
        documents: Vec<String>,
        state: OracleState,
        transforms: Vec<OracleTransform>,
    },
    #[serde(rename = "loadstate")]
    LoadState {
        id: String,
        #[serde(rename = "vocabularyJson")]
        vocabulary_json: String,
        #[serde(rename = "idfJson")]
        idf_json: String,
        #[serde(rename = "avgDocLength")]
        avg_doc_length: f64,
        #[serde(rename = "vocabularySize")]
        vocabulary_size: usize,
        #[serde(rename = "includeBigrams")]
        include_bigrams: bool,
        #[serde(rename = "fittedAt")]
        fitted_at: String,
        transforms: Vec<OracleTransform>,
    },
    #[serde(rename = "throw")]
    Throw {
        id: String,
        op: String,
        message: String,
    },
}

#[derive(Deserialize)]
struct OracleState {
    vocabulary: Vec<(String, usize)>,
    idf: Vec<f64>,
    #[serde(rename = "avgDocLength")]
    avg_doc_length: f64,
    #[serde(rename = "vocabularySize")]
    vocabulary_size: usize,
    #[serde(rename = "includeBigrams")]
    include_bigrams: bool,
    #[serde(rename = "fittedAt")]
    fitted_at: String,
}

#[derive(Deserialize)]
struct OracleTransform {
    query: String,
    vector: Vec<f64>,
}

fn approx_vec(got: &[f64], want: &[f64], ctx: &str) {
    assert_eq!(got.len(), want.len(), "vector length mismatch: {ctx}");
    for (i, (&g, &w)) in got.iter().zip(want).enumerate() {
        assert!(
            (g - w).abs() <= TOL,
            "vector[{i}] mismatch ({ctx}): got {g}, want {w} (|Δ|={})",
            (g - w).abs()
        );
    }
}

#[test]
fn tfidf_vectorizer_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_TFIDF") else {
        eprintln!("SKIP: set QT_ORACLE_TFIDF to the oracle NDJSON (see test header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut stem_rows = 0usize;
    let mut fit_rows = 0usize;
    let mut count = 0usize;

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).expect("oracle line parses");
        match row {
            Row::Stem { word, stemmed } => {
                assert_eq!(stem(&word), stemmed, "stem mismatch for {word:?}");
                stem_rows += 1;
            }
            Row::Tokenize {
                id,
                text,
                remove_stop_words,
                tokens,
            } => {
                assert_eq!(
                    tokenize(&text, remove_stop_words),
                    tokens,
                    "tokenize mismatch for '{id}' ({text:?})"
                );
            }
            Row::Bigrams {
                id,
                tokens,
                bigrams,
            } => {
                assert_eq!(
                    generate_bigrams(&tokens),
                    bigrams,
                    "generateBigrams mismatch for '{id}'"
                );
            }
            Row::Fit {
                id,
                include_bigrams,
                documents,
                state,
                transforms,
            } => {
                let mut v = TfIdfVectorizer::new(include_bigrams);
                v.fit_corpus(&documents, state.fitted_at.clone())
                    .unwrap_or_else(|e| panic!("fit failed for '{id}': {}", e.message()));
                let got = v.get_state().expect("fitted state");

                assert_eq!(
                    got.vocabulary, state.vocabulary,
                    "vocabulary mismatch for fit '{id}'"
                );
                assert_eq!(
                    got.vocabulary_size, state.vocabulary_size,
                    "vocabularySize mismatch for fit '{id}'"
                );
                assert_eq!(
                    got.include_bigrams, state.include_bigrams,
                    "includeBigrams mismatch for fit '{id}'"
                );
                assert_eq!(got.fitted_at, state.fitted_at, "fittedAt mismatch '{id}'");
                assert!(
                    (got.avg_doc_length - state.avg_doc_length).abs() <= TOL,
                    "avgDocLength mismatch for fit '{id}': {} vs {}",
                    got.avg_doc_length,
                    state.avg_doc_length
                );
                approx_vec(&got.idf, &state.idf, &format!("idf fit '{id}'"));

                for t in &transforms {
                    let vec = v
                        .transform(&t.query)
                        .unwrap_or_else(|e| panic!("transform failed '{id}': {}", e.message()));
                    approx_vec(
                        &vec,
                        &t.vector,
                        &format!("transform fit '{id}' q={:?}", t.query),
                    );
                }
                fit_rows += 1;
            }
            Row::LoadState {
                id,
                vocabulary_json,
                idf_json,
                avg_doc_length,
                vocabulary_size,
                include_bigrams,
                fitted_at,
                transforms,
            } => {
                // Mirror the host read path: JSON.parse(vocabulary), JSON.parse(idf).
                let vocabulary: Vec<(String, usize)> =
                    serde_json::from_str(&vocabulary_json).expect("vocabulary JSON parses");
                let idf: Vec<f64> = serde_json::from_str(&idf_json).expect("idf JSON parses");
                let state = LocalEmbeddingProviderState {
                    vocabulary,
                    idf,
                    avg_doc_length,
                    vocabulary_size,
                    include_bigrams,
                    fitted_at,
                };
                let mut v = TfIdfVectorizer::new(true);
                v.load_state(state);
                for t in &transforms {
                    let vec = v
                        .transform(&t.query)
                        .unwrap_or_else(|e| panic!("loadstate transform '{id}': {}", e.message()));
                    approx_vec(
                        &vec,
                        &t.vector,
                        &format!("loadstate '{id}' q={:?}", t.query),
                    );
                }
            }
            Row::Throw { id, op, message } => {
                let got = match op.as_str() {
                    "fit" => {
                        let mut v = TfIdfVectorizer::new(true);
                        v.fit_corpus(&[], "2020-01-01T00:00:00.000Z".to_string())
                            .unwrap_err()
                    }
                    "transform" => TfIdfVectorizer::new(true)
                        .transform("anything")
                        .unwrap_err(),
                    other => panic!("unknown throw op {other}"),
                };
                // Sanity: the corpus throw is EmptyCorpus, not the provider message.
                if op == "fit" {
                    assert_eq!(got, TfidfError::EmptyCorpus);
                }
                assert_eq!(got.message(), message, "throw message mismatch for '{id}'");
            }
        }
        count += 1;
    }

    assert!(
        stem_rows >= 40,
        "stemmer corpus too small: {stem_rows} rows"
    );
    assert!(fit_rows >= 3, "fit corpus too small: {fit_rows} rows");
    eprintln!(
        "OK: tfidf-vectorizer matched oracle ({count} rows, {stem_rows} stem, {fit_rows} fit)."
    );
}
