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

/// The `en-US` / **primary**-strength collator — JS
/// `a.localeCompare(b, undefined, { sensitivity: 'base' })`. Base sensitivity
/// ignores BOTH case and accents (`a`/`A`/`ä` all compare equal), so ties are
/// common and the caller's sort stability decides their order (V8's sort is
/// stable; Rust's `sort_by` is too). First consumer: the homepage character
/// sort (`services::home`, v4 `home-data.service.ts:197`).
static COLLATOR_BASE: LazyLock<CollatorBorrowed<'static>> = LazyLock::new(|| {
    let prefs = CollatorPreferences::from(&locale!("en-US"));
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Primary);
    Collator::try_new(prefs, options).expect("ICU en-US base collator (compiled data)")
});

/// Compare two strings the way JS `a.localeCompare(b)` does (en-US, tertiary).
pub fn locale_compare(a: &str, b: &str) -> Ordering {
    COLLATOR.compare(a, b)
}

/// Compare like JS `a.localeCompare(b, undefined, { sensitivity: 'base' })`
/// (en-US, primary strength — case- and accent-insensitive).
pub fn locale_compare_base(a: &str, b: &str) -> Ordering {
    COLLATOR_BASE.compare(a, b)
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
    fn base_sensitivity_matches_node() {
        // Probed against Node 24 (full ICU) 2026-07-16:
        //   ['b','A','a','B','z','Z','ä','é','e'].sort((x,y) =>
        //     x.localeCompare(y, undefined, {sensitivity:'base'}))
        //   → ['A','a','ä','b','B','é','e','z','Z']
        // — case AND accent compare EQUAL at base strength, so every tie keeps
        // the input order (V8's stable sort); only the letter identity sorts.
        let mut v = vec!["b", "A", "a", "B", "z", "Z", "ä", "é", "e"];
        v.sort_by(|x, y| locale_compare_base(x, y));
        assert_eq!(v, vec!["A", "a", "ä", "b", "B", "é", "e", "z", "Z"]);

        // The pairwise signs, same probe: ties where tertiary would order.
        assert_eq!(locale_compare_base("a", "A"), Ordering::Equal);
        assert_eq!(locale_compare_base("a", "ä"), Ordering::Equal);
        assert_eq!(locale_compare_base("e", "é"), Ordering::Equal);
        assert_eq!(locale_compare_base("Aria", "aria"), Ordering::Equal);
        assert_eq!(locale_compare_base("Ötz", "otz"), Ordering::Equal);
        assert_eq!(locale_compare_base("a", "b"), Ordering::Less);
        assert_eq!(locale_compare_base("ä", "b"), Ordering::Less);
        assert_eq!(locale_compare_base("b", "ä"), Ordering::Greater);
        // numeric:false — "10" sorts before "9" lexicographically.
        assert_eq!(locale_compare_base("10", "9"), Ordering::Less);
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
