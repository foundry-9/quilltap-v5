//! The LLM provider error taxonomy + normalizer/formatter (W4.7d).
//!
//! Ports the **unported half** of v4's `lib/llm/errors.ts`: the 8 error classes,
//! `handleProviderError` (the string-based normalizer), and `getUserFriendlyError`
//! (the per-class user-facing formatter). The predicate/parse half
//! (`isTokenLimitError` / `isContentLimitError` / `isToolUnsupportedError` /
//! `isRecoverableRequestError` / `parseTokenLimitError` / `parseContentLimitError`,
//! plus the `toLocaleString` grouper and [`ContentLimitType`]) was already ported
//! in [`crate::services::primary_stream`] and is reused here.
//!
//! v4's `Error` subclass hierarchy becomes one Rust struct
//! ([`LlmProviderError`]) tagged by [`LlmErrorKind`] (the class `name`), carrying
//! the per-class extra data (`retryAfter`; token `requested`/`max`; content
//! `limitType`/`limitValue`/`maxValue`). The class `name` selects the
//! `getUserFriendlyError` branch, so it is preserved exactly.
//!
//! ## JS truthiness (preserved)
//!
//! v4's default-message + user-friendly builders gate on `a && b`, where a
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

/// v4 `handleProviderError(provider, error)` — the string-based normalizer over an
/// `Error`'s message. **Match ORDER is precedence-bearing** — API key → rate limit
/// → network → model-not-found → token-limit → invalid-request → generic.
///
/// v4's leading `error instanceof LLMProviderError` passthrough and the trailing
/// `!(error instanceof Error)` "An unknown error occurred" fall to the caller: the
/// input here is always a raw provider message string (an `Error`), matching how
/// the differential drives v4 (`new Error(message)`).
pub fn handle_provider_error(provider: &str, message: &str) -> LlmProviderError {
    let lower = message.to_lowercase();

    // API key errors.
    if lower.contains("unauthorized")
        || lower.contains("invalid api key")
        || lower.contains("authentication")
        || lower.contains("401")
    {
        return LlmProviderError::api_key(provider);
    }

    // Rate limit errors.
    if lower.contains("rate limit") || lower.contains("too many requests") || lower.contains("429")
    {
        return LlmProviderError::rate_limit(provider, None);
    }

    // Network errors (message preserved).
    if lower.contains("network")
        || lower.contains("econnrefused")
        || lower.contains("timeout")
        || lower.contains("enotfound")
    {
        return LlmProviderError::network(provider, Some(message.to_string()));
    }

    // Model not found.
    if lower.contains("model") && (lower.contains("not found") || lower.contains("does not exist"))
    {
        return LlmProviderError::model_not_found(provider, "unknown");
    }

    // Token limit errors (parsed from the ORIGINAL message; message preserved).
    if super::primary_stream::is_token_limit_error(message) {
        let (requested, max) = super::primary_stream::parse_token_limit_error(message);
        return LlmProviderError::token_limit(provider, requested, max, Some(message.to_string()));
    }

    // Invalid request (message preserved).
    if lower.contains("invalid") || lower.contains("400") {
        return LlmProviderError::invalid_request(provider, message.to_string());
    }

    // Generic error with the original message.
    LlmProviderError::base(provider, message.to_string())
}

/// v4 `getUserFriendlyError(error)` for a normalized [`LlmProviderError`] — the
/// per-class byte-exact user-facing string. (v4's plain-`Error` and unknown
/// fallbacks apply above this seam, where the input isn't a provider error.)
pub fn user_friendly_error(error: &LlmProviderError) -> String {
    match error.kind {
        LlmErrorKind::ApiKey => format!(
            "Invalid or expired API key for {}. Please check your API key in settings.",
            error.provider
        ),
        LlmErrorKind::RateLimit => {
            let retry = if truthy(error.retry_after) {
                format!(
                    " Please try again in {} seconds.",
                    error.retry_after.unwrap()
                )
            } else {
                " Please try again later.".to_string()
            };
            format!("Rate limit exceeded for {}.{retry}", error.provider)
        }
        LlmErrorKind::Network => format!(
            "Unable to connect to {}. Please check your internet connection and provider settings.",
            error.provider
        ),
        LlmErrorKind::ModelNotFound => format!(
            "{}. Please select a different model in your connection profile.",
            error.message
        ),
        LlmErrorKind::InvalidRequest => {
            format!("Invalid request to {}: {}", error.provider, error.message)
        }
        LlmErrorKind::TokenLimit => {
            let token_info = if truthy(error.requested_tokens) && truthy(error.max_tokens) {
                format!(
                    " ({} tokens requested, {} maximum)",
                    to_locale_string(error.requested_tokens.unwrap()),
                    to_locale_string(error.max_tokens.unwrap())
                )
            } else {
                String::new()
            };
            format!(
                "Your message exceeds {}'s token limit{token_info}. Try removing attachments or shortening the conversation.",
                error.provider
            )
        }
        // ContentLimitError extends LLMProviderError with no dedicated branch, so
        // it (and the Base kind) fall to the generic `${provider} error:
        // ${message}` form.
        LlmErrorKind::ContentLimit | LlmErrorKind::Base => {
            format!("{} error: {}", error.provider, error.message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence_api_key_beats_rate_limit() {
        // A message matching BOTH 401 and 429 → API key (checked first).
        let e = handle_provider_error("OPENAI", "HTTP 401 and 429 too many requests");
        assert_eq!(e.kind, LlmErrorKind::ApiKey);
    }

    #[test]
    fn rate_limit_beats_invalid() {
        let e = handle_provider_error("OPENAI", "429 invalid something");
        assert_eq!(e.kind, LlmErrorKind::RateLimit);
    }

    #[test]
    fn token_limit_message_preserved_and_parsed() {
        // Needs a token-limit PATTERN (`prompt is too long`); the parse then pulls
        // the counts out of the `> maximum` clause. A bare `> maximum` message does
        // NOT match `isTokenLimitError`, so v4 (and this port) fall to generic.
        let e = handle_provider_error(
            "ANTHROPIC",
            "prompt is too long: 210311 tokens > 200000 maximum",
        );
        assert_eq!(e.kind, LlmErrorKind::TokenLimit);
        assert_eq!(e.requested_tokens, Some(210311));
        assert_eq!(e.max_tokens, Some(200000));
        assert_eq!(
            user_friendly_error(&e),
            "Your message exceeds ANTHROPIC's token limit (210,311 tokens requested, 200,000 maximum). Try removing attachments or shortening the conversation."
        );
    }

    #[test]
    fn zero_tokens_are_falsy() {
        // requested=0 → treated as absent → default message, no tokenInfo.
        let e = LlmProviderError::token_limit("X", Some(0), Some(200000), None);
        assert_eq!(e.message, "Prompt exceeds maximum token limit");
        assert_eq!(
            user_friendly_error(&e),
            "Your message exceeds X's token limit. Try removing attachments or shortening the conversation."
        );
    }

    #[test]
    fn generic_fallthrough() {
        let e = handle_provider_error("OLLAMA", "some weird failure");
        assert_eq!(e.kind, LlmErrorKind::Base);
        assert_eq!(user_friendly_error(&e), "OLLAMA error: some weird failure");
    }
}
