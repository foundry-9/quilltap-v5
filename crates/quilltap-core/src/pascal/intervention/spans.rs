//! Operator intervention — what a roll or value could have been (v4
//! `lib/pascal/intervention/spans.ts`, `8ae8a127c`).
//!
//! The engine reports a [`RunSpan`] for the raw draw and for the value after
//! modifiers; the hold dialog prints them beside the fields, and recompute
//! warns when a pin lands outside one. One phrasing for both, here.

use crate::pascal::custom_tools::RunSpan;
use crate::pascal::js_value::{number_to_string, to_number, to_precision};

/// Integers plain, floats to 4 significant digits — the way Pascal prints
/// numbers (v4 `Number.isInteger(n) ? String(n) : String(Number(n.toPrecision(4)))`).
fn format_span_number(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 {
        number_to_string(n)
    } else {
        number_to_string(to_number(&serde_json::Value::String(to_precision(n, 4))))
    }
}

/// Whether a number falls inside a span, honouring its open ends.
pub fn within_span(n: f64, span: &RunSpan) -> bool {
    let above_low = if span.min_exclusive == Some(true) {
        n > span.min
    } else {
        n >= span.min
    };
    let below_high = if span.max_exclusive == Some(true) {
        n < span.max
    } else {
        n <= span.max
    };
    above_low && below_high
}

/// A span in words: "exactly 5", "1 to 20", "1 up to (not including) 21",
/// "more than -42, up to -2".
pub fn describe_span(span: &RunSpan) -> String {
    let min = format_span_number(span.min);
    let max = format_span_number(span.max);
    let min_open = span.min_exclusive == Some(true);
    let max_open = span.max_exclusive == Some(true);
    if span.min == span.max && !min_open && !max_open {
        return format!("exactly {min}");
    }
    if min_open {
        return if max_open {
            format!("more than {min}, up to (not including) {max}")
        } else {
            format!("more than {min}, up to {max}")
        };
    }
    if max_open {
        return format!("{min} up to (not including) {max}");
    }
    format!("{min} to {max}")
}
