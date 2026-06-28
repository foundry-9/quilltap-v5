//! Port of v4's lib/utils/char-count.ts — the qt-* text colour class for a
//! character-count indicator based on how close `current` is to `max`.

/// Pick the colour class: over the max → destructive, past 90% → warning, else
/// secondary. (The 90% threshold is a float compare, matching v4's `max * 0.9`.)
pub fn char_count_class(current: i64, max: i64) -> &'static str {
    if current > max {
        "qt-text-destructive"
    } else if (current as f64) > (max as f64) * 0.9 {
        "qt-text-warning"
    } else {
        "qt-text-secondary"
    }
}
