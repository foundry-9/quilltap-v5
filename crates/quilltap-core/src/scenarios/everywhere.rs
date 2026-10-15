//! Scenarios everywhere — v4 `lib/scenarios/everywhere.ts` (`545e299bb`,
//! pinned at `83334b9a5`): every scenario in the instance, grouped by owner, as
//! excerpts.

use crate::jsstr::{is_js_ws, js_trim, js_trim_end, utf16_truncate};

/// v4 `SCENARIO_EXCERPT_LENGTH` (`everywhere.ts:36`): characters of a body an
/// "everywhere" row carries in lieu of the body itself — UTF-16 code units, as
/// JavaScript counts them.
pub const SCENARIO_EXCERPT_LENGTH: usize = 240;

/// v4 `scenarioExcerpt(body)` (`everywhere.ts:70-75`): collapse every run of
/// JavaScript `\s` (the Unicode whitespace class — NBSP, U+FEFF and U+2028
/// included) to one space, trim, and cut past 240 code units to
/// `slice(0, 240).trimEnd() + '…'`.
///
/// ⚠ Ruled divergence `EXCERPT_LONE_SURROGATE` (R-AA): v4's `slice` counts
/// UTF-16 units and can cut a surrogate pair in half, leaving a lone high
/// surrogate at unit 239 that a Rust `String` cannot hold. v5 emits U+FFFD in
/// its place (one unit, the same position), so every excerpt is unit-for-unit
/// v4's except that one unit — pinned both ways by `scenario_pure_equivalence`
/// so a v4 convergence trips it.
pub fn scenario_excerpt(body: &str) -> String {
    let mut collapsed = String::with_capacity(body.len());
    let mut in_ws = false;
    for c in body.chars() {
        if is_js_ws(c) {
            if !in_ws {
                collapsed.push(' ');
                in_ws = true;
            }
        } else {
            collapsed.push(c);
            in_ws = false;
        }
    }
    let collapsed = js_trim(&collapsed);
    if collapsed.encode_utf16().count() > SCENARIO_EXCERPT_LENGTH {
        let cut = utf16_truncate(collapsed, SCENARIO_EXCERPT_LENGTH);
        format!("{}…", js_trim_end(&cut))
    } else {
        collapsed.to_string()
    }
}
