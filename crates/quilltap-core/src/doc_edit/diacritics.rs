//! Diacritics-aware text matching — v4 `lib/doc-edit/diacritics.ts`.
//!
//! NFD-decompose text and strip combining marks so a base character matches its
//! accented variant ("Nimue" matches "Nimuë") — essential for fiction vaults with
//! accented names. `find_all_matches` / `find_unique_match` are the core of the
//! `str_replace` uniqueness constraint.
//!
//! ## UTF-16 fidelity
//!
//! v4 operates on **UTF-16 code units** throughout: `indexOf` returns UTF-16
//! indices, and `buildNormalizationMap` walks the original string one UTF-16 unit
//! at a time (per JS `original[i]`), mapping each resulting normalized unit back.
//! The port reproduces that on `Vec<u16>` / the `jsstr` UTF-16 primitives.
//!
//! **Documented seam** (kept out of the corpus): v4's map is built **per-UTF-16-
//! unit** while the searched `normalizedHaystack` is built **whole-string** (and
//! `buildNormalizationMap` ignores its `normalized` argument entirely, rebuilding
//! from `original`). For the corpus space — BMP Latin (precomposed + decomposed),
//! Hangul (one syllable → two jamo units), and ASCII-length-preserving
//! `toLowerCase` — the per-unit and whole-string decompositions are identical, so
//! the map aligns with the search string. Decomposable astral characters and
//! length-changing case folds (İ / final-sigma) can desync the two the same way
//! v4 does; they are excluded.

use crate::jsstr::js_index_of;
use unicode_normalization::UnicodeNormalization;

/// Options for diacritics-aware matching (v4 `DiacriticsMatchOptions`). Both
/// default true.
#[derive(Debug, Clone, Copy)]
pub struct DiacriticsMatchOptions {
    pub normalize_diacritics: bool,
    pub case_sensitive: bool,
}

impl Default for DiacriticsMatchOptions {
    fn default() -> Self {
        DiacriticsMatchOptions {
            normalize_diacritics: true,
            case_sensitive: true,
        }
    }
}

/// The combining-mark ranges v4 strips (category-M subset): Combining Diacritical
/// Marks (+ Extended, Supplement, for-Symbols) and Combining Half Marks.
fn is_combining_mark(cp: u32) -> bool {
    matches!(cp,
        0x0300..=0x036f
        | 0x1AB0..=0x1AFF
        | 0x1DC0..=0x1DFF
        | 0x20D0..=0x20FF
        | 0xFE20..=0xFE2F
    )
}

/// NFD-decompose then strip combining marks (v4 `normalizeDiacritics`). Whole-
/// string canonical decomposition, matching JS `text.normalize('NFD')`.
pub fn normalize_diacritics(text: &str) -> String {
    text.nfd()
        .filter(|c| !is_combining_mark(*c as u32))
        .collect()
}

/// The NFD-then-strip UTF-16 units a SINGLE original UTF-16 unit contributes (v4's
/// per-unit `original[i].normalize('NFD').replace(combining, '')`). A lone
/// surrogate normalizes to itself; a BMP scalar decomposes.
fn per_unit_normalized(unit: u16) -> Vec<u16> {
    if (0xD800..=0xDFFF).contains(&unit) {
        // Lone surrogate: `String.fromCharCode(unit).normalize('NFD')` is the unit
        // itself, and it is not a combining mark.
        return vec![unit];
    }
    let c = char::from_u32(unit as u32).expect("BMP non-surrogate is a scalar");
    c.to_string()
        .nfd()
        .filter(|d| !is_combining_mark(*d as u32))
        .flat_map(|d| {
            let mut buf = [0u16; 2];
            d.encode_utf16(&mut buf).to_vec()
        })
        .collect()
}

/// v4 `buildNormalizationMap`: `map[i]` = position in the ORIGINAL string
/// (UTF-16) of the character at position `i` in the normalized string. Built
/// per-original-UTF-16-unit.
fn build_normalization_map(original_u16: &[u16]) -> Vec<usize> {
    let mut map: Vec<usize> = Vec::new();
    for (original_pos, unit) in original_u16.iter().enumerate() {
        let stripped = per_unit_normalized(*unit);
        for _ in 0..stripped.len() {
            map.push(original_pos);
        }
    }
    map
}

/// Find ALL occurrences of `needle` in `haystack` (v4 `findAllMatches`). Returns
/// `(index, length)` pairs in the ORIGINAL haystack's UTF-16 code units.
pub fn find_all_matches(
    haystack: &str,
    needle: &str,
    options: DiacriticsMatchOptions,
) -> Vec<(usize, usize)> {
    let should_normalize = options.normalize_diacritics;
    let case_sensitive = options.case_sensitive;

    if needle.is_empty() {
        return Vec::new();
    }

    // Simple case: no normalization needed.
    if !should_normalize && case_sensitive {
        let mut matches = Vec::new();
        let needle_len = crate::jsstr::utf16_len(needle);
        let mut search_index = 0usize;
        while let Some(idx) = js_index_of(haystack, needle, search_index) {
            matches.push((idx, needle_len));
            search_index = idx + 1;
        }
        return matches;
    }

    // Build normalized versions.
    let (mut normalized_haystack, mut normalized_needle, haystack_map) = if should_normalize {
        let orig_u16: Vec<u16> = haystack.encode_utf16().collect();
        let map = build_normalization_map(&orig_u16);
        (
            normalize_diacritics(haystack),
            normalize_diacritics(needle),
            Some(map),
        )
    } else {
        (haystack.to_string(), needle.to_string(), None)
    };

    if !case_sensitive {
        normalized_haystack = normalized_haystack.to_lowercase();
        normalized_needle = normalized_needle.to_lowercase();
    }

    let mut matches = Vec::new();
    if normalized_needle.is_empty() {
        return matches;
    }

    let needle_len = crate::jsstr::utf16_len(&normalized_needle);
    let mut search_index = 0usize;
    while let Some(idx) = js_index_of(&normalized_haystack, &normalized_needle, search_index) {
        let (original_index, original_length) = if let Some(map) = &haystack_map {
            let original_index = map[idx];
            let normalized_end_pos = idx + needle_len - 1;
            let original_end_pos = map[normalized_end_pos];
            (original_index, original_end_pos - original_index + 1)
        } else {
            (idx, needle_len)
        };
        matches.push((original_index, original_length));
        search_index = idx + 1;
    }
    matches
}

/// The result of [`find_unique_match`] (v4's discriminated return).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UniqueMatch {
    Found { index: usize, length: usize },
    NotFound { count: usize },
}

/// Find a UNIQUE match of `needle` in `haystack` (v4 `findUniqueMatch`). Returns
/// the match iff exactly one exists, else the count.
pub fn find_unique_match(
    haystack: &str,
    needle: &str,
    options: DiacriticsMatchOptions,
) -> UniqueMatch {
    let matches = find_all_matches(haystack, needle, options);
    if matches.len() == 1 {
        UniqueMatch::Found {
            index: matches[0].0,
            length: matches[0].1,
        }
    } else {
        UniqueMatch::NotFound {
            count: matches.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_accents() {
        assert_eq!(normalize_diacritics("Nimuë"), "Nimue");
        assert_eq!(normalize_diacritics("café"), "cafe");
    }

    #[test]
    fn accented_needle_matches_base() {
        let m = find_all_matches("Nimue speaks", "Nimuë", DiacriticsMatchOptions::default());
        assert_eq!(m, vec![(0, 5)]);
    }

    #[test]
    fn base_needle_matches_accented_with_length_adjust() {
        // "Nimuë" is 5 UTF-16 units (precomposed ë); the base "Nimue" needle
        // matches it, and the ORIGINAL length is 5.
        let m = find_all_matches("say Nimuë now", "Nimue", DiacriticsMatchOptions::default());
        assert_eq!(m, vec![(4, 5)]);
    }
}
