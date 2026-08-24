//! Port of v4's `lib/llm/moderation-finish-reason.ts` (a14a1811, bug 93) —
//! recognising a provider's "I refused this on content grounds" stop reason.
//!
//! Providers report a moderation refusal in the finish-reason field and then
//! return empty content. Quilltap had no branch for any of these strings, so a
//! hard refusal reached the Salon as a blank message under the generic "the
//! model returned an empty response, this is a known issue with some
//! providers, try resending" copy — advice that cannot work, for a cause the
//! provider had stated plainly.
//!
//! The list is deliberately literal. A finish reason we don't recognise stays
//! unrecognised rather than being guessed at from a substring, because a false
//! positive tells the user their content was refused when it wasn't.

/// Finish reasons that mean "the provider declined to answer on content
/// grounds", lower-cased for comparison.
///
/// - `sensitive` — Z.AI (GLM), observed on glm-5v-turbo
/// - `content_filter` — OpenAI Chat Completions and Azure OpenAI
/// - `refusal` — OpenAI Responses API
/// - `safety`, `prohibited_content`, `blocklist`, `spii`, `image_safety` —
///   Google Gemini `finishReason`
/// - `recitation` — Google, a copyright rather than a safety stop, but the same
///   shape of dead end from the user's point of view
///
/// (`content-filter`, the hyphenated spelling, is a tenth literal in v4's set
/// its docblock doesn't call out — carried verbatim.)
const MODERATION_FINISH_REASONS: &[&str] = &[
    "sensitive",
    "content_filter",
    "content-filter",
    "refusal",
    "safety",
    "prohibited_content",
    "blocklist",
    "spii",
    "image_safety",
    "recitation",
];

/// True when this finish reason is a provider-side content refusal.
///
/// v4 `isModerationFinishReason(reason)` — `reason.trim().toLowerCase()`
/// set-membership; false for null/empty (JS `if (!reason) return false`
/// catches `null`/`undefined`/`''` — all spelled `None`/empty here).
pub fn is_moderation_finish_reason(reason: Option<&str>) -> bool {
    let Some(reason) = reason else { return false };
    if reason.is_empty() {
        return false;
    }
    // JS `.trim()` trims Unicode whitespace (incl. NBSP and the line
    // terminators); `js_trim` is the ported twin. `.toLowerCase()` on these
    // ASCII literals is byte-equivalent to `str::to_lowercase` (proven
    // byte-identical to JS in Phase 1).
    let normalized = crate::jsstr::js_trim(reason).to_lowercase();
    MODERATION_FINISH_REASONS.contains(&normalized.as_str())
}

/// A sentence naming what happened, for the empty-response explanation.
/// Returns `None` when the reason isn't a moderation refusal, so callers can
/// fall through to their generic copy.
///
/// v4 `describeModerationRefusal(reason, provider, modelName)` — the template
/// is byte-exact, interpolating the RAW (untrimmed, case-preserved) reason.
pub fn describe_moderation_refusal(
    reason: Option<&str>,
    provider: &str,
    model_name: &str,
) -> Option<String> {
    if !is_moderation_finish_reason(reason) {
        return None;
    }
    let reason = reason.expect("is_moderation_finish_reason is false for None");
    Some(format!(
        "{provider} {model_name} refused this turn on content grounds — it reported \
         `finish_reason: {reason}` and returned nothing. This is the provider's own \
         moderation layer, not a Quilltap error and not a transient fault: resending \
         the same content will be refused again. Route the chat to an uncensored \
         provider (Concierge settings), or change what is being asked for."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mirrors v4 `__tests__/unit/lib/llm/moderation-finish-reason.test.ts`.

    #[test]
    fn recognises_each_provider_dialect() {
        // The one observed in the wild: Z.AI glm-5v-turbo on an image it
        // declined.
        assert!(is_moderation_finish_reason(Some("sensitive")));
        assert!(is_moderation_finish_reason(Some("content_filter")));
        assert!(is_moderation_finish_reason(Some("refusal")));
        assert!(is_moderation_finish_reason(Some("SAFETY")));
        assert!(is_moderation_finish_reason(Some("PROHIBITED_CONTENT")));
        assert!(is_moderation_finish_reason(Some("RECITATION")));
    }

    #[test]
    fn case_and_whitespace_insensitive() {
        assert!(is_moderation_finish_reason(Some("  Sensitive  ")));
    }

    #[test]
    fn leaves_ordinary_stops_alone() {
        assert!(!is_moderation_finish_reason(Some("stop")));
        assert!(!is_moderation_finish_reason(Some("length")));
        assert!(!is_moderation_finish_reason(Some("tool_calls")));
        assert!(!is_moderation_finish_reason(None));
        assert!(!is_moderation_finish_reason(Some("")));
    }

    #[test]
    fn does_not_guess_from_substrings() {
        // A false positive tells the user their content was refused when it
        // wasn't, so matching is literal rather than fuzzy.
        assert!(!is_moderation_finish_reason(Some("insensitive_stop")));
        assert!(!is_moderation_finish_reason(Some(
            "no_content_filter_applied"
        )));
    }

    #[test]
    fn names_the_provider_the_model_and_the_reason() {
        let text = describe_moderation_refusal(Some("sensitive"), "Z_AI", "glm-5v-turbo").unwrap();
        assert!(text.contains("Z_AI"));
        assert!(text.contains("glm-5v-turbo"));
        assert!(text.contains("sensitive"));
    }

    #[test]
    fn contradicts_the_generic_retry_advice() {
        let text = describe_moderation_refusal(Some("content_filter"), "OPENAI", "gpt-5").unwrap();
        assert!(text.contains("refused"));
        assert!(text.contains("will be refused again"));
    }

    #[test]
    fn returns_none_for_a_non_moderation_stop_so_callers_fall_through() {
        assert!(describe_moderation_refusal(Some("stop"), "OPENAI", "gpt-5").is_none());
        assert!(describe_moderation_refusal(None, "OPENAI", "gpt-5").is_none());
    }
}
