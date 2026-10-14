//! The BUILTIN embedding host glue — a port of v4's `generateBuiltinEmbedding`
//! (`lib/embedding/embedding-service.ts:184`). Resolves the fitted vocabulary for
//! a BUILTIN profile, loads it into a [`BuiltinEmbeddingProvider`], transforms the
//! text, and routes the raw vector through the ported `applyEmbeddingProfile`
//! storage policy (Float32 narrow → optional Matryoshka slice → optional L2).
//!
//! The API-based embedding path (`generateApiEmbedding`) is the provider-wire
//! work of W4.7e; this unit ports only the BUILTIN branch.

use crate::db::embedding_profiles::EmbeddingProfileRow;
use crate::db::runtime::Db;
use crate::embedding_vector::apply_embedding_profile;
use crate::model::embedding::{EmbeddingError, EmbeddingResult};
use crate::tfidf::{BuiltinEmbeddingProvider, LocalEmbeddingProviderState};

/// Generate an embedding for `text` using the BUILTIN TF-IDF provider bound to
/// `profile` (v4 `generateBuiltinEmbedding`). Reads the persisted vocabulary via
/// `tfidf_vocabulary.findByProfileId`; if the profile has not been fitted yet,
/// returns the exact v4 not-fitted [`EmbeddingError`].
///
/// P4.D288 (C1-10, v4 `4ae0c4ca6`, `embedding-service.ts:236-250`): a memory
/// writer hands over its `lexical_text` — the memory without its anchor line —
/// "and that is what a bag of words embeds; any other text is embedded as
/// given". v4 logs DEBUG `[Embedding] BUILTIN embed: using the caller's lexical
/// text {context, profileId, textChars, lexicalChars}` (UTF-16 lengths) once
/// the vocabulary is loaded; its bytes are P4.D292's pin (§R.5).
pub async fn generate_builtin_embedding(
    db: &Db,
    text: &str,
    profile: &EmbeddingProfileRow,
    lexical_text: Option<&str>,
) -> Result<EmbeddingResult, EmbeddingError> {
    // Get the stored vocabulary for this profile.
    let pid = profile.id.clone();
    let vocabulary = db
        .read_main(move |conn| crate::db::tfidf_vocabulary::find_by_profile_id(conn, &pid))
        .map_err(|e| EmbeddingError {
            message: format!("Built-in embedding failed: {e}"),
            provider: Some("BUILTIN".to_string()),
        })?;

    let Some(vocabulary) = vocabulary else {
        return Err(EmbeddingError {
            message: "Built-in embedding profile not yet fitted. \
                      Please wait for the vocabulary to be built from your memories."
                .to_string(),
            provider: Some("BUILTIN".to_string()),
        });
    };

    // Parse the JSON-text columns and load the state (v4's `JSON.parse` of the
    // `vocabulary`/`idf` strings).
    let vocab_entries: Vec<(String, usize)> = serde_json::from_str(&vocabulary.vocabulary)
        .map_err(|e| EmbeddingError {
            message: format!("Built-in embedding failed: {e}"),
            provider: Some("BUILTIN".to_string()),
        })?;
    let idf: Vec<f64> = serde_json::from_str(&vocabulary.idf).map_err(|e| EmbeddingError {
        message: format!("Built-in embedding failed: {e}"),
        provider: Some("BUILTIN".to_string()),
    })?;

    let state = LocalEmbeddingProviderState {
        vocabulary: vocab_entries,
        idf,
        avg_doc_length: vocabulary.avg_doc_length,
        vocabulary_size: vocabulary.vocabulary_size as usize,
        include_bigrams: vocabulary.include_bigrams,
        fitted_at: vocabulary.fitted_at,
    };

    let mut provider = BuiltinEmbeddingProvider::new(true);
    provider.load_state(state);

    if let Some(lexical) = lexical_text {
        tracing::debug!(
            context = "generateBuiltinEmbedding",
            profileId = %profile.id,
            textChars = crate::jsstr::utf16_len(text),
            lexicalChars = crate::jsstr::utf16_len(lexical),
            "[Embedding] BUILTIN embed: using the caller's lexical text"
        );
    }
    let result = provider
        .generate_embedding(lexical_text.unwrap_or(text))
        .map_err(|e| EmbeddingError {
            message: format!("Built-in embedding failed: {}", e.message()),
            provider: Some("BUILTIN".to_string()),
        })?;

    // applyEmbeddingProfile: v4 wraps the raw `number[]` in a Float32Array (each
    // element narrowed to f32) before the slice + renormalize.
    let raw: Vec<f32> = result.embedding.iter().map(|&x| x as f32).collect();
    let embedding = apply_embedding_profile(
        &raw,
        profile.truncate_to_dimensions.map(|t| t as usize),
        profile.normalize_l2,
    );
    let dimensions = embedding.len();

    Ok(EmbeddingResult {
        embedding,
        model: result.model,
        dimensions,
        provider: "BUILTIN".to_string(),
    })
}
