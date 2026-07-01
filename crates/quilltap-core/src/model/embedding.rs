//! The embedding half of the model boundary — the tier-3 seam for the memory
//! subsystem. Mirrors v4's `generateEmbeddingForUser` /
//! `EmbeddingResult` / `EmbeddingError` (lib/embedding/embedding-service.ts).

use std::collections::{HashMap, HashSet};
use std::future::Future;

/// Result of an embedding operation. Mirrors v4's `EmbeddingResult`: the vector
/// is unit-length (L2-normalised) so downstream cosine similarity reduces to a
/// dot product; `dimensions == embedding.len()`.
#[derive(Clone, Debug, PartialEq)]
pub struct EmbeddingResult {
    /// The embedding vector — unit length.
    pub embedding: Vec<f32>,
    /// The model that produced it.
    pub model: String,
    /// Number of dimensions.
    pub dimensions: usize,
    /// The provider that produced it (v4's `EmbeddingProfileProvider` string).
    pub provider: String,
}

/// Error from an embedding call. Mirrors v4's `EmbeddingError` (a message plus an
/// optional provider). The memory gate treats any error as a failed attempt and
/// retries once before giving up (`SKIP_EMBEDDING_FAILED`).
#[derive(Clone, Debug)]
pub struct EmbeddingError {
    pub message: String,
    pub provider: Option<String>,
}

impl EmbeddingError {
    /// An error with just a message (no provider attribution).
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            provider: None,
        }
    }
}

impl std::fmt::Display for EmbeddingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for EmbeddingError {}

/// Priority lane for an embedding request (v4's `EmbeddingPriority`). It affects
/// only provider-side queueing (interactive vs background), never the returned
/// vector — carried here for signature fidelity and future scheduling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmbeddingPriority {
    Interactive,
    Background,
}

/// The embedding boundary — every embedding call goes through this trait.
///
/// The method is async (`impl Future + Send`) because the real provider is a
/// network call. Consumers hold a generic `P: EmbeddingProvider` rather than a
/// trait object, so the async-fn-in-trait return type needs no boxing and the
/// future stays `Send` for use across the writer/read threads.
pub trait EmbeddingProvider {
    /// Generate an embedding for `text` on behalf of `user_id`, optionally with an
    /// explicit `profile_id` (else the user's default profile). Mirrors v4's
    /// `generateEmbeddingForUser(text, userId, profileId?, { priority })`.
    fn generate_embedding_for_user(
        &self,
        text: &str,
        user_id: &str,
        profile_id: Option<&str>,
        priority: EmbeddingPriority,
    ) -> impl Future<Output = Result<EmbeddingResult, EmbeddingError>> + Send;
}

/// A deterministic [`EmbeddingProvider`] for the tier-3 differential. It returns a
/// fixed [`EmbeddingResult`] keyed by the **exact** input text, or a failure for
/// texts explicitly marked as failing. The Rust test and the v4 oracle build the
/// same input→vector map, so the model call is pinned identically on both sides.
///
/// An unregistered input is an error (a corpus omission — surfaced, never silently
/// answered). A failure input fails on **every** call, so the gate's
/// one-retry-then-give-up path lands on `SKIP_EMBEDDING_FAILED`.
#[derive(Clone, Default)]
pub struct CannedEmbeddingProvider {
    responses: HashMap<String, EmbeddingResult>,
    failures: HashSet<String>,
    failure_message: String,
}

impl CannedEmbeddingProvider {
    /// A fresh provider with no canned responses.
    pub fn new() -> Self {
        Self {
            responses: HashMap::new(),
            failures: HashSet::new(),
            failure_message: "canned embedding failure".to_string(),
        }
    }

    /// Register a unit-length vector for `text` (model/provider default to
    /// `"canned"`, `dimensions` from the vector length).
    pub fn with_vector(mut self, text: impl Into<String>, embedding: Vec<f32>) -> Self {
        let dimensions = embedding.len();
        self.responses.insert(
            text.into(),
            EmbeddingResult {
                embedding,
                model: "canned".to_string(),
                dimensions,
                provider: "canned".to_string(),
            },
        );
        self
    }

    /// Register a full [`EmbeddingResult`] for `text` (when model/provider/
    /// dimensions matter).
    pub fn with_result(mut self, text: impl Into<String>, result: EmbeddingResult) -> Self {
        self.responses.insert(text.into(), result);
        self
    }

    /// Mark `text` as always failing — drives the gate's retry then
    /// `SKIP_EMBEDDING_FAILED`.
    pub fn with_failure(mut self, text: impl Into<String>) -> Self {
        self.failures.insert(text.into());
        self
    }

    /// Override the message returned for failing inputs.
    pub fn with_failure_message(mut self, message: impl Into<String>) -> Self {
        self.failure_message = message.into();
        self
    }
}

impl EmbeddingProvider for CannedEmbeddingProvider {
    fn generate_embedding_for_user(
        &self,
        text: &str,
        _user_id: &str,
        _profile_id: Option<&str>,
        _priority: EmbeddingPriority,
    ) -> impl Future<Output = Result<EmbeddingResult, EmbeddingError>> + Send {
        // Resolve synchronously so the returned future owns its result and is
        // `'static` + `Send` (no borrow of `&self` escapes into the future).
        let result = if self.failures.contains(text) {
            Err(EmbeddingError::new(self.failure_message.clone()))
        } else {
            match self.responses.get(text) {
                Some(r) => Ok(r.clone()),
                None => Err(EmbeddingError::new(format!(
                    "no canned embedding registered for input ({} chars)",
                    text.len()
                ))),
            }
        };
        async move { result }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn returns_injected_vector_for_known_input() {
        let provider = CannedEmbeddingProvider::new().with_vector("hello", vec![1.0, 0.0, 0.0]);
        let result = provider
            .generate_embedding_for_user("hello", "user-1", None, EmbeddingPriority::Background)
            .await
            .unwrap();
        assert_eq!(result.embedding, vec![1.0, 0.0, 0.0]);
        assert_eq!(result.dimensions, 3);
        assert_eq!(result.provider, "canned");
    }

    #[tokio::test]
    async fn failing_input_errors_every_call() {
        let provider = CannedEmbeddingProvider::new()
            .with_vector("ok", vec![0.0, 1.0])
            .with_failure("bad")
            .with_failure_message("provider down");
        // Fails on both a first and a "retry" call (stateless → deterministic).
        for _ in 0..2 {
            let err = provider
                .generate_embedding_for_user("bad", "u", None, EmbeddingPriority::Background)
                .await
                .unwrap_err();
            assert_eq!(err.message, "provider down");
        }
    }

    #[tokio::test]
    async fn unregistered_input_is_an_error_not_a_silent_answer() {
        let provider = CannedEmbeddingProvider::new().with_vector("known", vec![1.0]);
        assert!(provider
            .generate_embedding_for_user("unknown", "u", None, EmbeddingPriority::Interactive)
            .await
            .is_err());
    }
}
