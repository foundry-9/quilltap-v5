//! Fallback engine — trigger classification and chain building (v4
//! `lib/llm/fallback/engine.ts`).
//!
//! Every connection profile gets an ordered chain of at most three attempts:
//!
//!   1. the profile itself,
//!   2. its configured understudy (`fallbackProfileId`),
//!   3. one auto-picked same-or-better-tier replacement, if `allowTierFallback`.
//!
//! Chains do **not** recurse. When profile A falls back to B, B's own
//! `fallbackProfileId` is not followed. That is what makes a cycle (A->B, B->A)
//! harmless config rather than an infinite loop, and it is why the worst case is
//! three calls no matter how the user has wired their profiles.
//!
//! There is no stickiness: a successful fallback applies to that call only. The
//! next message tries the primary again, so a transient outage heals itself
//! without the user having to notice it happened.
//!
//! ## The one shape change from v4
//!
//! v4's `buildFallbackChain` is `async` because its repo reads are; v5's are
//! synchronous reads over a borrowed `&Connection`, so the chain is built inside
//! one read and walked afterwards. The seam is [`FallbackRepos`] — narrow on
//! purpose (v4's comment: *the forked job child hands over a read-through proxy,
//! and everything here is a read*), which is also what lets the tier-1
//! differential drive it from an in-memory `Vec`.

use crate::files::image_transport::profile_can_receive_attachment;
use crate::services::llm_errors::LlmErrorKind;
use crate::services::primary_stream::{
    is_content_limit_error, is_token_limit_error, is_tool_unsupported_error,
};

use super::tier_picker::pick_tier_candidate;
use super::types::{
    FallbackAttempt, FallbackCandidate, FallbackCandidateKind, FallbackContext, FallbackError,
    FallbackProfile, FallbackTrigger,
};

/// Repository surface the engine needs. Narrow on purpose — everything here is
/// a read, which is what lets the chain be built inside a single `read_main`.
pub trait FallbackRepos {
    fn find_by_id(&self, id: &str) -> Option<FallbackProfile>;
    fn find_by_user_id(&self, user_id: &str) -> Vec<FallbackProfile>;
}

/// Message fragments that mean "the upstream is having a bad day" without
/// arriving as one of our typed errors. Providers reach us through plugins, and
/// a plugin that rethrows a bare `Error` with the HTTP status in the text is
/// common enough that matching on it is worth more than the tidiness of
/// insisting on the taxonomy.
const PROVIDER_ERROR_PATTERNS: &[&str] = &[
    r"\b5\d\d\b",
    r"(?i)internal server error",
    r"(?i)bad gateway",
    r"(?i)service unavailable",
    r"(?i)gateway timeout",
    r"(?i)overloaded",
    r"(?i)server had an error",
];

const NETWORK_ERROR_PATTERNS: &[&str] = &[
    r"(?i)timed? ?out",
    r"(?i)timeout",
    r"(?i)econnreset",
    r"(?i)econnrefused",
    r"(?i)enotfound",
    r"(?i)socket hang up",
    r"(?i)fetch failed",
    r"(?i)network",
    r"(?i)aborted",
];

/// The three message probes v4 runs after the typed ladder, in order.
const AUTH_PATTERN: &str = r"(?i)\b401\b|unauthoriz|invalid api key|authentication";
const RATE_LIMIT_PATTERN: &str = r"(?i)\b429\b|rate limit|too many requests";
const MODEL_MISSING_PATTERN: &str = r"(?i)model.*(not found|does not exist|unknown)";
/// A 4xx that is none of the above is a malformed request — ours to fix, and it
/// would be malformed at the next provider too.
const UNATTRIBUTED_4XX_PATTERN: &str = r"\b4\d\d\b";

fn compiled(patterns: &'static [&'static str]) -> Vec<regex::Regex> {
    patterns
        .iter()
        .map(|p| regex::Regex::new(p).expect("fallback pattern compiles"))
        .collect()
}

static PROVIDER_ERROR_RE: std::sync::LazyLock<Vec<regex::Regex>> =
    std::sync::LazyLock::new(|| compiled(PROVIDER_ERROR_PATTERNS));
static NETWORK_ERROR_RE: std::sync::LazyLock<Vec<regex::Regex>> =
    std::sync::LazyLock::new(|| compiled(NETWORK_ERROR_PATTERNS));
static AUTH_RE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(AUTH_PATTERN).unwrap());
static RATE_LIMIT_RE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(RATE_LIMIT_PATTERN).unwrap());
static MODEL_MISSING_RE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(MODEL_MISSING_PATTERN).unwrap());
static UNATTRIBUTED_4XX_RE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(UNATTRIBUTED_4XX_PATTERN).unwrap());

/// Classify a failure into the trigger class the chain acts on, or `None` when
/// the chain should stay out of it.
///
/// The non-triggers are as important as the triggers:
///
/// - **Token / content limits** already have their own in-character recovery
///   ([`crate::services::recovery::attempt_request_limit_recovery`]), and a
///   prompt too long for one model is very likely too long for its stand-in —
///   burning the chain on it would turn one clear error into three slow ones.
/// - **Tool-unsupported** is already retried on the same profile with the tools
///   stripped, which is a better answer than changing model.
/// - **Zod validation errors** are our bug, not the provider's. Failing over
///   would hide it behind a second provider producing the same crash.
///
/// v4 takes an `unknown` and walks `instanceof`, then `error.name`, then
/// `error.message`; [`FallbackError`] names those three inputs so a caller can
/// supply whichever it holds. The ORDER below is v4's exactly — several
/// non-triggers arrive *as* `LLMProviderError` subclasses, which is why they are
/// checked before the typed ladder.
pub fn classify_fallback_trigger(error: FallbackError<'_>) -> Option<FallbackTrigger> {
    // Non-triggers first.
    if matches!(
        error.kind,
        Some(LlmErrorKind::TokenLimit) | Some(LlmErrorKind::ContentLimit)
    ) {
        return None;
    }
    if is_token_limit_error(error.message) || is_content_limit_error(error.message) {
        return None;
    }
    if is_tool_unsupported_error(error.message) {
        return None;
    }
    if error.name == Some("ZodError") {
        return None;
    }

    match error.kind {
        Some(LlmErrorKind::ApiKey) => return Some(FallbackTrigger::Auth),
        Some(LlmErrorKind::RateLimit) => return Some(FallbackTrigger::RateLimit),
        Some(LlmErrorKind::Network) => return Some(FallbackTrigger::Network),
        Some(LlmErrorKind::ModelNotFound) => return Some(FallbackTrigger::ModelMissing),
        _ => {}
    }

    // The cheap path's own deadline. Not an LLMProviderError — it is raised by
    // Quilltap, not the provider — but it means exactly the same thing to the
    // chain: this route did not answer in time, try another.
    if error.name == Some("CheapLLMTimeoutError") {
        return Some(FallbackTrigger::Network);
    }

    let message = error.message;
    if NETWORK_ERROR_RE.iter().any(|p| p.is_match(message)) {
        return Some(FallbackTrigger::Network);
    }
    if PROVIDER_ERROR_RE.iter().any(|p| p.is_match(message)) {
        return Some(FallbackTrigger::ProviderError);
    }

    if AUTH_RE.is_match(message) {
        return Some(FallbackTrigger::Auth);
    }
    if RATE_LIMIT_RE.is_match(message) {
        return Some(FallbackTrigger::RateLimit);
    }
    if MODEL_MISSING_RE.is_match(message) {
        return Some(FallbackTrigger::ModelMissing);
    }

    if UNATTRIBUTED_4XX_RE.is_match(message) {
        return None;
    }

    // Anything left is an unattributed failure from a provider call. Treat it as
    // the provider's: the alternative is that the most common shape of plugin
    // error — a bare `Error` with a vendor message — never fails over at all.
    Some(FallbackTrigger::ProviderError)
}

/// Whether a profile can actually receive the images this turn is carrying.
///
/// Both halves matter, and for different reasons: `supportsImageUpload` is the
/// operator's assertion that the *model* can see, and the transport check is
/// whether the *plugin* will put the bytes on the wire. A profile failing
/// either would answer from the prompt alone, or be refused outright by the
/// gateway.
///
/// Both halves are asked by
/// [`crate::files::image_transport::profile_can_receive_attachment`] since v4
/// `a1d88aa3a` (bug 106) — `image/jpeg` is v4's own probe MIME here, and it
/// stands for every image type because the profile flag is per-profile, not
/// per-format.
fn can_receive_this_turns_images(profile: &FallbackProfile) -> bool {
    profile_can_receive_attachment(profile.attachment_view(), "image/jpeg")
}

/// Build the ordered candidate list for a call: `[primary, understudy?, tierPick?]`.
///
/// Candidates already tried on this call (`context.already_tried`) are dropped,
/// so a chain re-entered mid-turn — the empty-response path does exactly that —
/// never re-offers a profile that has already had its chance.
pub fn build_fallback_chain<R: FallbackRepos + ?Sized>(
    primary: &FallbackProfile,
    repos: &R,
    context: &FallbackContext,
) -> Vec<FallbackCandidate> {
    let mut chain: Vec<FallbackCandidate> = Vec::new();
    let mut claimed: Vec<String> = context.already_tried.clone();
    let has = |claimed: &[String], id: &str| claimed.iter().any(|c| c == id);

    if !has(&claimed, &primary.id) {
        chain.push(FallbackCandidate {
            profile: primary.clone(),
            kind: FallbackCandidateKind::Primary,
        });
        claimed.push(primary.id.clone());
    }

    // 2. The configured understudy.
    if let Some(understudy_id) = primary
        .fallback_profile_id
        .as_deref()
        .filter(|id| *id != primary.id)
    {
        if has(&claimed, understudy_id) {
            tracing::debug!(
                target: "quilltap::fallback",
                primary_profile_id = %primary.id,
                understudy_id,
                purpose = %context.purpose,
                "Fallback chain skipped configured understudy: already tried"
            );
        } else {
            match repos.find_by_id(understudy_id) {
                None => tracing::debug!(
                    target: "quilltap::fallback",
                    primary_profile_id = %primary.id,
                    understudy_id,
                    purpose = %context.purpose,
                    "Fallback chain skipped configured understudy: profile not found"
                ),
                Some(understudy) if understudy.transport == "courier" => tracing::debug!(
                    target: "quilltap::fallback",
                    primary_profile_id = %primary.id,
                    understudy_id = %understudy.id,
                    purpose = %context.purpose,
                    "Fallback chain skipped configured understudy: courier transport"
                ),
                Some(understudy)
                    if context.needs_vision && !can_receive_this_turns_images(&understudy) =>
                {
                    // The one capability a *named* understudy is still filtered
                    // on.
                    //
                    // A chain swaps the model but reuses the message array the
                    // primary's call was built against — and when that turn
                    // carries an image, the raw bytes are already embedded in
                    // it. Handing that array to a text-only stand-in is not a
                    // risk, it is a guaranteed 400 (v4 bug 106, the same defect
                    // in the Concierge's uncensored reroute). Skipping is
                    // strictly better than spending the attempt.
                    //
                    // Everything else the user names is honoured,
                    // danger-compatibility included: that is their call to
                    // make. This is not a policy preference, it is an
                    // incompatibility.
                    tracing::warn!(
                        target: "quilltap::fallback",
                        primary_profile_id = %primary.id,
                        understudy_id = %understudy.id,
                        understudy_name = %understudy.name,
                        understudy_provider = %understudy.provider,
                        supports_image_upload = understudy.supports_image_upload,
                        purpose = %context.purpose,
                        "Fallback chain skipped configured understudy: cannot receive this turn's images"
                    );
                }
                Some(understudy) => {
                    claimed.push(understudy.id.clone());
                    chain.push(FallbackCandidate {
                        profile: understudy,
                        kind: FallbackCandidateKind::Configured,
                    });
                }
            }
        }
    }

    // 3. One auto-picked tier replacement — the last resort, and opt-in.
    if primary.allow_tier_fallback {
        let all_profiles = repos.find_by_user_id(&context.user_id);
        let scoped = FallbackContext {
            already_tried: claimed.clone(),
            ..context.clone()
        };
        if let Some(pick) = pick_tier_candidate(primary, &all_profiles, &scoped) {
            claimed.push(pick.id.clone());
            chain.push(FallbackCandidate {
                profile: pick.clone(),
                kind: FallbackCandidateKind::TierPick,
            });
        }
    } else {
        tracing::debug!(
            target: "quilltap::fallback",
            primary_profile_id = %primary.id,
            purpose = %context.purpose,
            "Fallback chain skipped tier pick: not enabled on the profile"
        );
    }

    tracing::debug!(
        target: "quilltap::fallback",
        primary_profile_id = %primary.id,
        purpose = %context.purpose,
        dangerous = context.dangerous,
        needs_vision = context.needs_vision,
        needs_tools = context.needs_tools,
        chain = ?chain
            .iter()
            .map(|c| (
                c.profile.id.as_str(),
                c.profile.name.as_str(),
                c.profile.provider.as_str(),
                c.profile.model_name.as_str(),
                c.kind.as_str(),
            ))
            .collect::<Vec<_>>(),
        "Fallback chain built"
    );

    chain
}

/// Record one failed attempt, for logging and for the message the user sees.
/// v4: `error instanceof Error ? error.message : String(error ?? 'unknown
/// error')`. Three shapes, and the difference between the first and the third
/// is not the string — an `Error` whose message is `''` keeps the empty string,
/// while a `throw null` becomes the literal `unknown error`.
///
/// `message: None` is that nullish arm. Every v5 production caller holds a
/// message and passes `Some`; `None` exists because v4 can reach it and the
/// corpus asks.
pub fn record_attempt(
    profile: &FallbackProfile,
    trigger: FallbackTrigger,
    message: Option<&str>,
) -> FallbackAttempt {
    FallbackAttempt {
        profile_id: profile.id.clone(),
        profile_name: profile.name.clone(),
        provider: profile.provider.clone(),
        model_name: profile.model_name.clone(),
        trigger,
        error: message.unwrap_or("unknown error").to_string(),
    }
}

/// Turn an exhausted chain into the sentence the user reads.
///
/// Names the profiles rather than the providers: a user with three OpenAI
/// profiles needs to know *which* understudy was called, and the profile name is
/// the thing they chose themselves.
pub fn summarize_fallback_attempts(
    attempts: &[FallbackAttempt],
    tier_pick_was_offered: bool,
) -> String {
    if attempts.is_empty() {
        return String::new();
    }

    let roll = attempts
        .iter()
        .map(|a| format!("{} failed ({})", a.profile_name, a.trigger))
        .collect::<Vec<_>>()
        .join(", ");

    if attempts.len() == 1 {
        return roll;
    }

    if tier_pick_was_offered {
        roll
    } else {
        format!("{roll}; no tier replacement qualified")
    }
}
