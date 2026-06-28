//! Tier-1 differential test #20 (Wave 4 / B11): embedding vector-math hot paths.
//! normalizeVector / applyEmbeddingProfile / blob round-trip are checked
//! bit-exact on f32; cosineSimilarity / textSimilarity / applyLiteralBoost
//! within 1e-12; the dimension-mismatch messages and literal-phrase/contains
//! results exactly; the literal-boost constants for drift.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/embedding-vector.ts \
//!     > /tmp/oracle-embedding-vector.ndjson
//! Run:
//!   QT_ORACLE_EMBEDDING_VECTOR=/tmp/oracle-embedding-vector.ndjson \
//!     cargo test -p quilltap-harness

use quilltap_core::embedding_blob::{blob_to_float32, float32_to_blob};
use quilltap_core::embedding_vector::{
    apply_embedding_profile, assert_embedding_dimensions_match, cosine_similarity,
    normalize_vector, text_similarity,
};
use quilltap_core::literal_boost::{
    apply_literal_boost, contains_literal_phrase, get_literal_phrase, LITERAL_BOOST_CHARACTER,
    LITERAL_BOOST_GLOBAL, LITERAL_BOOST_GROUP, LITERAL_BOOST_MIN_PHRASE_LENGTH,
    LITERAL_BOOST_PROJECT,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "normalize")]
    Normalize {
        id: String,
        input: Vec<f64>,
        out: Vec<f64>,
    },
    #[serde(rename = "profile")]
    Profile {
        id: String,
        input: Vec<f64>,
        truncate: Option<usize>,
        #[serde(rename = "normalizeL2")]
        normalize_l2: Option<bool>,
        out: Vec<f64>,
    },
    #[serde(rename = "cosine")]
    Cosine {
        id: String,
        a: Vec<f64>,
        b: Vec<f64>,
        out: f64,
    },
    #[serde(rename = "cosineErr")]
    CosineErr {
        id: String,
        #[serde(rename = "aLen")]
        a_len: usize,
        #[serde(rename = "bLen")]
        b_len: usize,
        message: String,
    },
    #[serde(rename = "assertErr")]
    AssertErr {
        id: String,
        #[serde(rename = "queryLen")]
        query_len: usize,
        #[serde(rename = "storedLen")]
        stored_len: usize,
        context: Option<String>,
        message: String,
    },
    #[serde(rename = "textSim")]
    TextSim {
        id: String,
        keywords: Vec<String>,
        #[serde(rename = "exactPhrases")]
        exact_phrases: Vec<String>,
        target: String,
        out: f64,
    },
    #[serde(rename = "literalPhrase")]
    LiteralPhrase {
        id: String,
        query: Option<String>,
        out: Option<String>,
    },
    #[serde(rename = "containsPhrase")]
    ContainsPhrase {
        id: String,
        text: Option<String>,
        #[serde(rename = "lowerPhrase")]
        lower_phrase: String,
        out: bool,
    },
    #[serde(rename = "literalBoost")]
    LiteralBoost {
        id: String,
        score: f64,
        fraction: Option<f64>,
        out: f64,
    },
    #[serde(rename = "consts")]
    Consts {
        #[serde(rename = "minPhraseLength")]
        min_phrase_length: usize,
        character: f64,
        group: f64,
        project: f64,
        global: f64,
    },
    #[serde(rename = "blob")]
    Blob {
        id: String,
        vec: Vec<f64>,
        bytes: Vec<u8>,
        #[serde(rename = "roundTrip")]
        round_trip: Vec<f64>,
    },
}

fn to_f32(v: &[f64]) -> Vec<f32> {
    v.iter().map(|&x| x as f32).collect()
}

/// Bit-exact comparison of a computed f32 vector against the oracle's f64-widened
/// values (each is an exact f32 value, so `as f32` round-trips losslessly).
fn assert_f32_vec_eq(got: &[f32], oracle: &[f64], label: &str) {
    assert_eq!(got.len(), oracle.len(), "{label}: length");
    for (i, (&g, &o)) in got.iter().zip(oracle.iter()).enumerate() {
        assert_eq!(
            g.to_bits(),
            (o as f32).to_bits(),
            "{label}: element {i} (rust={g} oracle={o})"
        );
    }
}

#[test]
fn embedding_vector_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_EMBEDDING_VECTOR") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_EMBEDDING_VECTOR to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).unwrap() {
            Row::Normalize { id, input, out } => {
                let got = normalize_vector(&to_f32(&input));
                assert_f32_vec_eq(&got, &out, &format!("normalize '{id}'"));
            }
            Row::Profile {
                id,
                input,
                truncate,
                normalize_l2,
                out,
            } => {
                // v4 normalizes unless normalizeL2 is explicitly false.
                let norm = normalize_l2 != Some(false);
                let got = apply_embedding_profile(&to_f32(&input), truncate, norm);
                assert_f32_vec_eq(&got, &out, &format!("profile '{id}'"));
            }
            Row::Cosine { id, a, b, out } => {
                let got = cosine_similarity(&to_f32(&a), &to_f32(&b)).expect("dims match");
                assert!(
                    (got - out).abs() < 1e-12,
                    "cosine '{id}': rust={got} oracle={out}"
                );
            }
            Row::CosineErr {
                id,
                a_len,
                b_len,
                message,
            } => {
                let a = vec![0.0_f32; a_len];
                let b = vec![0.0_f32; b_len];
                let err = cosine_similarity(&a, &b).expect_err("dims mismatch");
                assert_eq!(err.message(), message, "cosineErr '{id}'");
            }
            Row::AssertErr {
                id,
                query_len,
                stored_len,
                context,
                message,
            } => {
                let err =
                    assert_embedding_dimensions_match(query_len, stored_len, context.as_deref())
                        .expect_err("dims mismatch");
                assert_eq!(err.message(), message, "assertErr '{id}'");
            }
            Row::TextSim {
                id,
                keywords,
                exact_phrases,
                target,
                out,
            } => {
                let got = text_similarity(&keywords, &exact_phrases, &target);
                assert!(
                    (got - out).abs() < 1e-12,
                    "textSim '{id}': rust={got} oracle={out}"
                );
            }
            Row::LiteralPhrase { id, query, out } => {
                let got = get_literal_phrase(query.as_deref());
                assert_eq!(got, out, "literalPhrase '{id}'");
            }
            Row::ContainsPhrase {
                id,
                text,
                lower_phrase,
                out,
            } => {
                let got = contains_literal_phrase(text.as_deref(), &lower_phrase);
                assert_eq!(got, out, "containsPhrase '{id}'");
            }
            Row::LiteralBoost {
                id,
                score,
                fraction,
                out,
            } => {
                let got = apply_literal_boost(score, fraction.unwrap_or(0.5));
                assert!(
                    (got - out).abs() < 1e-12,
                    "literalBoost '{id}': rust={got} oracle={out}"
                );
            }
            Row::Consts {
                min_phrase_length,
                character,
                group,
                project,
                global,
            } => {
                assert_eq!(
                    LITERAL_BOOST_MIN_PHRASE_LENGTH, min_phrase_length,
                    "const minPhraseLength"
                );
                assert_eq!(LITERAL_BOOST_CHARACTER, character, "const character");
                assert_eq!(LITERAL_BOOST_GROUP, group, "const group");
                assert_eq!(LITERAL_BOOST_PROJECT, project, "const project");
                assert_eq!(LITERAL_BOOST_GLOBAL, global, "const global");
            }
            Row::Blob {
                id,
                vec,
                bytes,
                round_trip,
            } => {
                let v = to_f32(&vec);
                assert_eq!(float32_to_blob(&v), bytes, "blob '{id}': float32_to_blob");
                assert_f32_vec_eq(
                    &blob_to_float32(&bytes),
                    &round_trip,
                    &format!("blob '{id}': round-trip"),
                );
            }
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: embedding-vector matched oracle ({count} rows).");
}
