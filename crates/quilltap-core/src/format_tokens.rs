//! Port of v4's lib/utils/format-tokens.ts — the client-safe cost / token-count
//! display helpers. Both round through JS `toFixed` (see
//! [`crate::jsnum::to_fixed`]).
//!
//! NB: a second `formatTokenCount` lives in lib/tokens/token-counter.ts (ported
//! in [`crate::token_estimation`]); it differs only in using a lowercase `k`
//! suffix. This module is the `K`/`M` variant.

use crate::jsnum::to_fixed;

/// Format a cost for display ("$0.0023", "$0.500", "$1.00", "Free", "N/A").
/// `None` maps to v4's `null` → "N/A". The sub-cent / sub-dollar / dollar-plus
/// thresholds use strict `<`, so exactly `0.01` renders with three decimals and
/// exactly `1` with two.
pub fn format_cost_for_display(cost_usd: Option<f64>) -> String {
    let cost = match cost_usd {
        None => return "N/A".to_string(),
        Some(c) => c,
    };
    if cost == 0.0 {
        return "Free".to_string();
    }
    if cost < 0.01 {
        return format!("${}", to_fixed(cost, 4));
    }
    if cost < 1.0 {
        return format!("${}", to_fixed(cost, 3));
    }
    format!("${}", to_fixed(cost, 2))
}

/// Format a token count for display ("1.5K", "2.3M"). Below 1000 the count is
/// stringified directly (JS `tokens.toString()`); Rust's shortest-float Display
/// matches it across this non-negative integer domain.
pub fn format_token_count(tokens: f64) -> String {
    if tokens >= 1_000_000.0 {
        return format!("{}M", to_fixed(tokens / 1_000_000.0, 1));
    }
    if tokens >= 1_000.0 {
        return format!("{}K", to_fixed(tokens / 1_000.0, 1));
    }
    format!("{tokens}")
}
