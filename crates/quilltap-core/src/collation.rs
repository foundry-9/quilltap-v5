//! ICU collation matching JS `String.prototype.localeCompare(b)` (no locale/options).
//!
//! v4 sorts several lists with `a.localeCompare(b)` — true ICU collation, NOT the
//! code-unit order Rust's `str: Ord` gives. The no-arg form uses the runtime's
//! default `Intl.Collator`, which in the v4 oracle's Node (full ICU 78) resolves
//! to **locale `en-US`, usage `sort`, sensitivity `variant` (= tertiary strength),
//! `numeric: false`, `caseFirst: false`** (probed 2026-06-30). That ordering puts
//! lowercase before uppercase and interleaves accents (`a,A,ä,b,B,e,é,z,Z`),
//! which neither code-unit nor `to_lowercase` reproduces.
//!
//! This module reproduces it with ICU4X (`icu`) configured to the same locale +
//! strength. For common Latin + accents the CLDR root collation is version-stable,
//! so ICU4X's tables agree with Node's ICU 78; exotic scripts could differ by
//! CLDR/Unicode version — a bounded residual seam, kept out of the corpora.
//!
//! This is the locked "add ICU crate" decision for the ~30 `localeCompare` sites
//! (ported so far: [`crate::semver::compare_versions`] fallback,
//! [`crate::canonicalize`] tool-name sort).

use std::cmp::Ordering;
use std::sync::LazyLock;

use icu::collator::options::{CollatorOptions, Strength};
use icu::collator::{Collator, CollatorBorrowed, CollatorPreferences};
use icu::locale::locale;

/// The shared `en-US` / tertiary collator. `Collator::try_new` with compiled data
/// returns a [`CollatorBorrowed`] over the baked-in 'static tables — `Copy` +
/// `Sync`, so it lives in a `static` with no locking.
static COLLATOR: LazyLock<CollatorBorrowed<'static>> = LazyLock::new(|| {
    let prefs = CollatorPreferences::from(&locale!("en-US"));
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Tertiary);
    Collator::try_new(prefs, options).expect("ICU en-US collator (compiled data)")
});

/// Compare two strings the way JS `a.localeCompare(b)` does (en-US, tertiary).
pub fn locale_compare(a: &str, b: &str) -> Ordering {
    COLLATOR.compare(a, b)
}

/// `localeCompare` as the `-1 / 0 / 1` integer JS returns (sign-only).
pub fn locale_compare_i32(a: &str, b: &str) -> i32 {
    match locale_compare(a, b) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_node_en_us_order() {
        // The exact order Node's no-arg localeCompare produces (probed against
        // ICU 78): a,A,ä,b,B,e,é,z,Z — lowercase-before-uppercase, accents
        // interleaved. If ICU4X's tables diverge from Node's, this fails.
        let mut v = vec!["b", "A", "a", "B", "z", "Z", "ä", "é", "e"];
        v.sort_by(|x, y| locale_compare(x, y));
        assert_eq!(v, vec!["a", "A", "ä", "b", "B", "e", "é", "z", "Z"]);
    }

    #[test]
    fn pairwise_signs_match_node() {
        assert_eq!(locale_compare_i32("a", "A"), -1);
        assert_eq!(locale_compare_i32("A", "a"), 1);
        assert_eq!(locale_compare_i32("z", "ä"), 1);
        assert_eq!(locale_compare_i32("ä", "z"), -1);
        assert_eq!(locale_compare_i32("e", "é"), -1);
        assert_eq!(locale_compare_i32("A", "B"), -1);
        assert_eq!(locale_compare_i32("a", "a"), 0);
    }
}
