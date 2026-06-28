//! Port of the character-based token estimation from v4's
//! lib/tokens/token-counter.ts — the conservative token counts used for context
//! budgeting (no tiktoken dependency, just chars-per-token with a safety buffer).
//!
//! The provider→chars-per-token resolution (`getProviderCharsPerToken`) reads
//! the plugin registry, so the rate is a parameter here (`chars_per_token`); the
//! no-provider path uses [`DEFAULT_CHARS_PER_TOKEN`] (3.5), which is what the
//! oracle exercises. The display formatter `formatTokenCount` is ported here via
//! [`crate::jsnum::to_fixed`], whose half-away-from-zero rounding matches JS
//! `toFixed` (Rust's `{:.1}` rounds half-to-even and would diverge).
//!
//! Lengths follow JS `String.length` (UTF-16 code units), via `encode_utf16`.

/// Default chars-per-token when no provider is given (v4's `CHARS_PER_TOKEN.default`).
pub const DEFAULT_CHARS_PER_TOKEN: f64 = 3.5;

const SAFETY_BUFFER_MULT: f64 = 1.05; // 1 + 5% buffer
const MESSAGE_OVERHEAD: i64 = 4;
const CONVERSATION_OVERHEAD: i64 = 3;

/// JS `String.length`: the number of UTF-16 code units.
fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Estimate the token count for `text`: `ceil(len/charsPerToken)` then a 5%
/// safety buffer (`ceil(base * 1.05)`). Empty text is 0.
pub fn estimate_tokens(text: &str, chars_per_token: f64) -> i64 {
    if text.is_empty() {
        return 0;
    }
    let base = (utf16_len(text) as f64 / chars_per_token).ceil();
    (base * SAFETY_BUFFER_MULT).ceil() as i64
}

/// Token count for a single message: content + role token estimates plus a
/// fixed per-message overhead (4).
pub fn count_message_tokens(role: &str, content: &str, chars_per_token: f64) -> i64 {
    estimate_tokens(content, chars_per_token)
        + estimate_tokens(role, chars_per_token)
        + MESSAGE_OVERHEAD
}

/// Total token count for a conversation: the sum of per-message counts plus a
/// fixed conversation overhead (3). An empty conversation is 0 (no overhead).
pub fn count_messages_tokens(messages: &[(String, String)], chars_per_token: f64) -> i64 {
    if messages.is_empty() {
        return 0;
    }
    let sum: i64 = messages
        .iter()
        .map(|(role, content)| count_message_tokens(role, content, chars_per_token))
        .sum();
    sum + CONVERSATION_OVERHEAD
}

/// Truncate `text` to fit within `max_tokens`, appending `suffix` when it had to
/// cut. Preserves as much as possible and prefers a trailing word boundary.
///
/// Slicing is on UTF-16 code units (JS `String.slice`/`lastIndexOf`), matching
/// the estimator's length basis. The corpus stays in the BMP so no surrogate
/// pair is split; non-BMP truncation fidelity is deferred with the formatters.
pub fn truncate_to_token_limit(
    text: &str,
    max_tokens: i64,
    chars_per_token: f64,
    suffix: &str,
) -> String {
    if text.is_empty() {
        return String::new();
    }
    if estimate_tokens(text, chars_per_token) <= max_tokens {
        return text.to_string();
    }

    let suffix_tokens = estimate_tokens(suffix, chars_per_token);
    let available_tokens = max_tokens - suffix_tokens;
    // 5% safety margin on the char budget (v4's `* 0.95`).
    let max_chars = (available_tokens as f64 * chars_per_token * 0.95).floor() as i64;
    if max_chars <= 0 {
        return suffix.to_string();
    }

    let units: Vec<u16> = text.encode_utf16().collect();
    let end = (max_chars as usize).min(units.len());
    let mut sliced = &units[..end];

    // Back up to the last space if it's past 80% of the char budget.
    if let Some(last_space) = sliced.iter().rposition(|&u| u == 0x20) {
        if (last_space as f64) > (max_chars as f64) * 0.8 {
            sliced = &sliced[..last_space];
        }
    }

    let mut out = String::from_utf16_lossy(sliced);
    out.push_str(suffix);
    out
}

/// Percentage of the context window used, rounded and capped at 100. A
/// non-positive limit yields 100 (treat as full).
///
/// Mirrors JS `Math.round` for non-negative input as `floor(x + 0.5)` (the
/// ECMAScript identity), so the .5 rounding matches exactly.
pub fn get_context_usage_percent(used_tokens: i64, context_limit: i64) -> i64 {
    if context_limit <= 0 {
        return 100;
    }
    let pct = (used_tokens as f64 / context_limit as f64) * 100.0;
    let rounded = (pct + 0.5).floor() as i64;
    rounded.min(100)
}

/// Format a token count for display ("1.5k", "125k", "2.3M"). This is the
/// token-counter.ts variant: a lowercase `k` thousands suffix (the
/// format-tokens.ts twin in [`crate::format_tokens`] uses uppercase `K`). Below
/// 1000 the count is stringified directly (`tokens.toString()`); Rust's
/// shortest-float Display matches it across the non-negative integer domain.
pub fn format_token_count(tokens: f64) -> String {
    if tokens >= 1_000_000.0 {
        return format!("{}M", crate::jsnum::to_fixed(tokens / 1_000_000.0, 1));
    }
    if tokens >= 1_000.0 {
        return format!("{}k", crate::jsnum::to_fixed(tokens / 1_000.0, 1));
    }
    format!("{tokens}")
}

/// Context-usage warning level: `critical` at ≥95%, `warning` at ≥80%, else `ok`.
pub fn get_context_warning_level(used_tokens: i64, context_limit: i64) -> &'static str {
    let percent = get_context_usage_percent(used_tokens, context_limit);
    if percent >= 95 {
        "critical"
    } else if percent >= 80 {
        "warning"
    } else {
        "ok"
    }
}
