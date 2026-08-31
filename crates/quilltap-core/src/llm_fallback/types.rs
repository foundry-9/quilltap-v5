//! Fallback engine — shared types (v4 `lib/llm/fallback/types.ts`).
//!
//! The engine answers two questions for every LLM call in Quilltap: *is this
//! failure worth trying someone else for?* and *who is next?* Both answers are
//! the same regardless of which currency the caller holds (a connection profile
//! in the Salon, a [`crate::cheap_llm::CheapLlmSelection`] in the job runner),
//! which is the whole reason the machinery lives at the provider layer rather
//! than inside the chat-message services.

use serde_json::Value;

use crate::services::llm_errors::LlmErrorKind;

/// Why a call failed, in the only granularity the chain cares about.
///
/// Everything here means "the provider could not answer" — an availability
/// problem, which a different provider might not have. The two content-shaped
/// entries (`empty-response`, `moderation-refusal`) are included because a
/// refusal is still a call that produced nothing, and Quilltap has always
/// rerouted those; they simply arrive as an *outcome* rather than a throw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FallbackTrigger {
    Auth,
    RateLimit,
    Network,
    ModelMissing,
    ProviderError,
    EmptyResponse,
    ModerationRefusal,
}

impl FallbackTrigger {
    /// The wire/prose spelling — v4's union members verbatim. These reach the
    /// user through `summarize_fallback_attempts` and the empty-response
    /// reason, so they are bytes, not labels.
    pub fn as_str(self) -> &'static str {
        match self {
            FallbackTrigger::Auth => "auth",
            FallbackTrigger::RateLimit => "rate-limit",
            FallbackTrigger::Network => "network",
            FallbackTrigger::ModelMissing => "model-missing",
            FallbackTrigger::ProviderError => "provider-error",
            FallbackTrigger::EmptyResponse => "empty-response",
            FallbackTrigger::ModerationRefusal => "moderation-refusal",
        }
    }
}

impl std::fmt::Display for FallbackTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which call site is asking (v4's `FallbackContext.purpose`). Logged, and used
/// by the failure summary; it does not change candidate selection, which is
/// driven by the capability flags on [`FallbackContext`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FallbackPurpose {
    Chat,
    Cheap,
    Vision,
    Carina,
    Console,
    Help,
}

impl FallbackPurpose {
    pub fn as_str(self) -> &'static str {
        match self {
            FallbackPurpose::Chat => "chat",
            FallbackPurpose::Cheap => "cheap",
            FallbackPurpose::Vision => "vision",
            FallbackPurpose::Carina => "carina",
            FallbackPurpose::Console => "console",
            FallbackPurpose::Help => "help",
        }
    }
}

impl std::fmt::Display for FallbackPurpose {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a call needs from any profile that stands in for the failed one.
///
/// `already_tried` is the loop guard. It is carried rather than derived because
/// a chain can be entered more than once in a single turn — the empty-response
/// path may run a same-profile retry and an uncensored reroute before the chain
/// proper — and every one of those attempts has to count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FallbackContext {
    pub user_id: String,
    pub purpose: FallbackPurpose,
    /// Whether this call is running in dangerous-routed territory. When true,
    /// an auto-picked tier candidate MUST be `isDangerousCompatible` — the
    /// whole point of the reroute is that the content needs a provider the user
    /// has explicitly cleared for it, and quietly drafting a mainstream model
    /// would hand the content straight back to the moderation that refused it.
    pub dangerous: bool,
    /// The call carries image attachments; a stand-in must be able to see them.
    pub needs_vision: bool,
    /// The call sends tools; a stand-in must be able to receive them.
    pub needs_tools: bool,
    /// Profile ids already attempted on this call. Never re-offered.
    pub already_tried: Vec<String>,
}

/// One attempt in a chain walk, recorded for logging and the failure summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FallbackAttempt {
    pub profile_id: String,
    pub profile_name: String,
    pub provider: String,
    pub model_name: String,
    pub trigger: FallbackTrigger,
    /// Human-readable reason, taken from the underlying error where there was
    /// one.
    pub error: String,
}

/// How a candidate came to be in the chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FallbackCandidateKind {
    Primary,
    Configured,
    TierPick,
}

impl FallbackCandidateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FallbackCandidateKind::Primary => "primary",
            FallbackCandidateKind::Configured => "configured",
            FallbackCandidateKind::TierPick => "tier-pick",
        }
    }
}

impl std::fmt::Display for FallbackCandidateKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FallbackCandidate {
    pub profile: FallbackProfile,
    pub kind: FallbackCandidateKind,
}

/// The connection-profile fields the chain reads.
///
/// v4 hands the engine a whole `ConnectionProfile`; v5's callers hold a net-read
/// [`Value`] (the Salon), a `CheapLlmSelection` (the job runner), or a mount
/// profile row (the describer), so the engine takes the narrow projection all
/// three can produce. [`FallbackProfile::from_value`] is the one reader.
#[derive(Clone, Debug, PartialEq)]
pub struct FallbackProfile {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub provider: String,
    pub model_name: String,
    pub base_url: Option<String>,
    pub api_key_id: Option<String>,
    /// `'api' | 'courier'`.
    pub transport: String,
    pub is_cheap: bool,
    pub is_dangerous_compatible: bool,
    pub supports_image_upload: bool,
    /// v4's `allowToolUse` — the profile's own master override. Absent reads as
    /// `true` (v4's Zod `.default(true)`), and only an explicit `false` blocks a
    /// tool-carrying call.
    pub allow_tool_use: bool,
    pub model_class: Option<String>,
    pub sort_index: f64,
    /// The understudy this profile names, if any.
    pub fallback_profile_id: Option<String>,
    /// Whether a same-or-better-tier stand-in may be drafted after both named
    /// players fail.
    pub allow_tier_fallback: bool,
    /// The profile's `parameters` bag, carried so a stand-in's per-model
    /// settings survive the swap.
    pub parameters: Option<Value>,
}

impl FallbackProfile {
    /// Read a profile out of a net-read `connection_profiles` row.
    ///
    /// Every optional key follows v4's Zod: an absent boolean with a
    /// `.default()` takes that default (`allowToolUse` true, the rest false), a
    /// NULL nullable string is `None`, and a missing `sortIndex` is `0`.
    pub fn from_value(v: &Value) -> Option<FallbackProfile> {
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        let os = |k: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .filter(|x| !x.is_empty())
                .map(str::to_string)
        };
        let b = |k: &str, d: bool| v.get(k).and_then(Value::as_bool).unwrap_or(d);
        Some(FallbackProfile {
            id: s("id")?,
            user_id: s("userId").unwrap_or_default(),
            name: s("name").unwrap_or_default(),
            provider: s("provider").unwrap_or_default(),
            model_name: s("modelName").unwrap_or_default(),
            base_url: os("baseUrl"),
            api_key_id: os("apiKeyId"),
            transport: s("transport").unwrap_or_else(|| "api".to_string()),
            is_cheap: b("isCheap", false),
            is_dangerous_compatible: b("isDangerousCompatible", false),
            supports_image_upload: b("supportsImageUpload", false),
            allow_tool_use: b("allowToolUse", true),
            model_class: os("modelClass"),
            sort_index: v.get("sortIndex").and_then(Value::as_f64).unwrap_or(0.0),
            fallback_profile_id: os("fallbackProfileId"),
            allow_tier_fallback: b("allowTierFallback", false),
            parameters: v.get("parameters").cloned(),
        })
    }
}

/// The error a caller hands [`super::classify_fallback_trigger`].
///
/// v4 classifies an `unknown` by walking `instanceof` checks, then `error.name`,
/// then `error.message`. v5 has no error-class hierarchy at the stream seam —
/// [`crate::model::stream::StreamError`] carries a message and nothing else — so
/// the three inputs are named explicitly and a caller supplies whichever it has.
/// A bare message (`FallbackError::message("…")`) is the production shape on the
/// Salon path; `kind` is set where a caller genuinely holds a normalized
/// [`LlmErrorKind`], and `name` where it holds one of the two non-LLM classes v4
/// tests by name (`ZodError`, `CheapLLMTimeoutError`).
#[derive(Clone, Copy, Debug, Default)]
pub struct FallbackError<'a> {
    /// v4's `error instanceof <LLMProviderError subclass>`.
    pub kind: Option<LlmErrorKind>,
    /// v4's `error.name`, for the classes that are not ours.
    pub name: Option<&'a str>,
    /// v4's `error.message` (or `String(error ?? '')` for a non-Error throw).
    pub message: &'a str,
}

impl<'a> FallbackError<'a> {
    /// The production shape: a provider failure that reached us as text.
    pub fn message(message: &'a str) -> FallbackError<'a> {
        FallbackError {
            kind: None,
            name: None,
            message,
        }
    }

    /// A normalized [`LlmProviderError`](crate::services::llm_errors::LlmProviderError).
    pub fn typed(kind: LlmErrorKind, message: &'a str) -> FallbackError<'a> {
        FallbackError {
            kind: Some(kind),
            name: None,
            message,
        }
    }

    /// One of the two classes v4 tests by `name` rather than by `instanceof`.
    pub fn named(name: &'a str, message: &'a str) -> FallbackError<'a> {
        FallbackError {
            kind: None,
            name: Some(name),
            message,
        }
    }
}
