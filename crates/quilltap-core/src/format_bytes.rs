//! Port of v4's lib/utils/format-bytes.ts — a human-readable byte-count string
//! ("123 B", "1.5 KB", …) on the 1024-based scale with single-letter units.
//!
//! Sub-KB values render as whole bytes; everything else rounds to one decimal
//! via JS `toFixed(1)` (see [`crate::jsnum::to_fixed`]). The unit index comes
//! from `floor(log(bytes) / log(1024))` clamped to the unit table, exactly as
//! v4 computes it — the boundary lands on exact powers of 1024.

use crate::jsnum::to_fixed;

const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

/// Format a byte count as a human-readable string. Matches v4 byte-for-byte for
/// the realistic domain (a finite, non-negative byte *count*); non-finite input
/// yields the empty string, as in v4.
pub fn format_bytes(bytes: f64) -> String {
    if !bytes.is_finite() {
        return String::new();
    }
    if bytes == 0.0 {
        return "0 B".to_string();
    }
    if bytes < 0.0 {
        return format!("-{}", format_bytes(-bytes));
    }
    let k = 1024.0_f64;
    // Math.min(Math.floor(log(bytes)/log(k)), UNITS.len()-1). For bytes >= 1 the
    // floor is >= 0, so the index is always a valid table slot.
    let i = (bytes.ln() / k.ln()).floor().min((UNITS.len() - 1) as f64) as usize;
    if i == 0 {
        // v4's `${bytes} B`: JS stringifies the (integer) byte count. Rust's
        // shortest-float Display coincides with `String(bytes)` across this
        // sub-1024 integer domain.
        return format!("{bytes} B");
    }
    format!("{} {}", to_fixed(bytes / k.powi(i as i32), 1), UNITS[i])
}
