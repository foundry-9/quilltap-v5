//! The LLM provider error taxonomy (W4.7d).
//!
//! Ports the **unported half** of v4's `lib/llm/errors.ts`: the 8 error classes
//! and their default-message builders. The predicate/parse half
//! (`isTokenLimitError` / `isContentLimitError` / `isToolUnsupportedError` /
//! `isRecoverableRequestError` / `parseTokenLimitError` / `parseContentLimitError`,
//! plus the `toLocaleString` grouper and [`ContentLimitType`]) was already ported
//! in [`crate::services::primary_stream`] and is reused here.
//!
//! v4's `Error` subclass hierarchy becomes one Rust struct
//! ([`LlmProviderError`]) tagged by [`LlmErrorKind`] (the class `name`), carrying
//! the per-class extra data (`retryAfter`; token `requested`/`max`; content
//! `limitType`/`limitValue`/`maxValue`). [`LlmErrorKind`] is the live half —
//! [`crate::llm_fallback`] classifies on it.
//!
//! P4.D157 (v4 `d4138b96b`, the 4.9 dead-code sweep): v4 deleted
//! `handleProviderError` (the string-based normalizer) and `getUserFriendlyError`
//! (the per-class user-facing formatter) as unreferenced. Measured here too —
//! every v5 caller of either lived in this file's own unit-test module — so both
//! twins were deleted with them. The classes, their default messages and the
//! JS-truthiness rules below all survive and stay differential-pinned.
//!
//! ## JS truthiness (preserved)
//!
//! v4's default-message builders gate on `a && b`, where a
//! numeric `0` is **falsy**. So `requestedTokens = 0` (or `retryAfter = 0`, or a
//! `limitValue = 0`) is treated as absent — reproduced with [`truthy`].

use super::primary_stream::{to_locale_string, ContentLimitType};

/// The error class (v4's subclass `name`). Selects the `getUserFriendlyError`
/// branch and carries the class identity byte-for-byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LlmErrorKind {
    /// `LLMProviderError` (the base / generic).
    Base,
    /// `APIKeyError`.
    ApiKey,
    /// `RateLimitError`.
    RateLimit,
    /// `NetworkError`.
    Network,
    /// `ModelNotFoundError`.
    ModelNotFound,
    /// `InvalidRequestError`.
    InvalidRequest,
    /// `TokenLimitError`.
    TokenLimit,
    /// `ContentLimitError`.
    ContentLimit,
}

impl LlmErrorKind {
    /// The v4 class `name` string.
    pub fn name(self) -> &'static str {
        match self {
            LlmErrorKind::Base => "LLMProviderError",
            LlmErrorKind::ApiKey => "APIKeyError",
            LlmErrorKind::RateLimit => "RateLimitError",
            LlmErrorKind::Network => "NetworkError",
            LlmErrorKind::ModelNotFound => "ModelNotFoundError",
            LlmErrorKind::InvalidRequest => "InvalidRequestError",
            LlmErrorKind::TokenLimit => "TokenLimitError",
            LlmErrorKind::ContentLimit => "ContentLimitError",
        }
    }
}

/// A normalized LLM provider error (v4's `LLMProviderError` + subclasses folded
/// into one struct tagged by [`LlmErrorKind`]). `message` is the `Error.message`;
/// the per-class extras are `Some` only for their owning kind.
#[derive(Clone, Debug, PartialEq)]
pub struct LlmProviderError {
    pub kind: LlmErrorKind,
    pub provider: String,
    pub message: String,
    /// `RateLimitError.retryAfter` (seconds).
    pub retry_after: Option<i64>,
    /// `TokenLimitError.requestedTokens`.
    pub requested_tokens: Option<i64>,
    /// `TokenLimitError.maxTokens`.
    pub max_tokens: Option<i64>,
    /// `ContentLimitError.limitType`.
    pub content_limit_type: Option<ContentLimitType>,
    /// `ContentLimitError.limitValue`.
    pub content_limit_value: Option<i64>,
    /// `ContentLimitError.maxValue`.
    pub content_max_value: Option<i64>,
}

/// JS truthiness of an optional integer (v4's `a && b` gates): `0` is falsy.
fn truthy(v: Option<i64>) -> bool {
    matches!(v, Some(n) if n != 0)
}

impl LlmProviderError {
    fn base_fields(kind: LlmErrorKind, provider: &str, message: String) -> LlmProviderError {
        LlmProviderError {
            kind,
            provider: provider.to_string(),
            message,
            retry_after: None,
            requested_tokens: None,
            max_tokens: None,
            content_limit_type: None,
            content_limit_value: None,
            content_max_value: None,
        }
    }

    /// v4 `new LLMProviderError(provider, message)`.
    pub fn base(provider: &str, message: impl Into<String>) -> LlmProviderError {
        Self::base_fields(LlmErrorKind::Base, provider, message.into())
    }

    /// v4 `new APIKeyError(provider)` — default message `Invalid or missing API
    /// key`.
    pub fn api_key(provider: &str) -> LlmProviderError {
        Self::base_fields(
            LlmErrorKind::ApiKey,
            provider,
            "Invalid or missing API key".to_string(),
        )
    }

    /// v4 `new RateLimitError(provider, retryAfter?)` — default message `Rate limit
    /// exceeded`.
    pub fn rate_limit(provider: &str, retry_after: Option<i64>) -> LlmProviderError {
        let mut e = Self::base_fields(
            LlmErrorKind::RateLimit,
            provider,
            "Rate limit exceeded".to_string(),
        );
        e.retry_after = retry_after;
        e
    }

    /// v4 `new NetworkError(provider, message?)` — default message `Network error
    /// occurred`.
    pub fn network(provider: &str, message: Option<String>) -> LlmProviderError {
        Self::base_fields(
            LlmErrorKind::Network,
            provider,
            message.unwrap_or_else(|| "Network error occurred".to_string()),
        )
    }

    /// v4 `new ModelNotFoundError(provider, model)` — message `Model "{model}" not
    /// found or not available`.
    pub fn model_not_found(provider: &str, model: &str) -> LlmProviderError {
        Self::base_fields(
            LlmErrorKind::ModelNotFound,
            provider,
            format!("Model \"{model}\" not found or not available"),
        )
    }

    /// v4 `new InvalidRequestError(provider, message)`.
    pub fn invalid_request(provider: &str, message: impl Into<String>) -> LlmProviderError {
        Self::base_fields(LlmErrorKind::InvalidRequest, provider, message.into())
    }

    /// v4 `new TokenLimitError(provider, requestedTokens?, maxTokens?, message?)` —
    /// the default message uses `toLocaleString` grouping and the `req && max`
    /// truthiness gate.
    pub fn token_limit(
        provider: &str,
        requested_tokens: Option<i64>,
        max_tokens: Option<i64>,
        message: Option<String>,
    ) -> LlmProviderError {
        let default = if truthy(requested_tokens) && truthy(max_tokens) {
            format!(
                "Prompt too long: {} tokens exceeds {} maximum",
                to_locale_string(requested_tokens.unwrap()),
                to_locale_string(max_tokens.unwrap())
            )
        } else {
            "Prompt exceeds maximum token limit".to_string()
        };
        let mut e = Self::base_fields(
            LlmErrorKind::TokenLimit,
            provider,
            message.unwrap_or(default),
        );
        e.requested_tokens = requested_tokens;
        e.max_tokens = max_tokens;
        e
    }

    /// v4 `new ContentLimitError(provider, limitType, limitValue?, maxValue?,
    /// message?)` — the default message from `buildDefaultMessage`.
    pub fn content_limit(
        provider: &str,
        limit_type: ContentLimitType,
        limit_value: Option<i64>,
        max_value: Option<i64>,
        message: Option<String>,
    ) -> LlmProviderError {
        let desc = content_limit_description(limit_type);
        let default = if truthy(limit_value) && truthy(max_value) {
            format!(
                "Content exceeds {desc}: {} > {} maximum",
                to_locale_string(limit_value.unwrap()),
                to_locale_string(max_value.unwrap())
            )
        } else if truthy(max_value) {
            format!(
                "Content exceeds {desc}: maximum is {}",
                to_locale_string(max_value.unwrap())
            )
        } else {
            format!("Content exceeds {desc}")
        };
        let mut e = Self::base_fields(
            LlmErrorKind::ContentLimit,
            provider,
            message.unwrap_or(default),
        );
        e.content_limit_type = Some(limit_type);
        e.content_limit_value = limit_value;
        e.content_max_value = max_value;
        e
    }
}

/// v4's `limitDescriptions[type]` (the `ContentLimitError` default-message noun).
/// [`ContentLimitType::description`] is module-private in `primary_stream`; this
/// re-derives the same mapping (the `Token` variant maps to `"token limit"`).
fn content_limit_description(kind: ContentLimitType) -> &'static str {
    match kind {
        ContentLimitType::Token => "token limit",
        ContentLimitType::PdfPages => "PDF page limit",
        ContentLimitType::ImageSize => "image size limit",
        ContentLimitType::FileSize => "file size limit",
        ContentLimitType::Unknown => "content limit",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_tokens_are_falsy() {
        // requested=0 → treated as absent → default message, no tokenInfo.
        let e = LlmProviderError::token_limit("X", Some(0), Some(200000), None);
        assert_eq!(e.message, "Prompt exceeds maximum token limit");
    }
}
