//! Match normalization — diacritics and typographic spelling. v4
//! `lib/doc-edit/diacritics.ts`.
//!
//! NFD-decompose text and strip combining marks so a base character matches its
//! accented variant ("Nimue" matches "Nimuë") — essential for fiction vaults with
//! accented names — and, behind an opt-in flag, let a straight quote match the
//! curly one a model wrote into the file (see [`super::typographic_folding`],
//! v4 bug 109 / `487ae16b1`).
//!
//! Both are the same kind of concession and share one mechanism: a
//! **per-character** rewrite, plus a map from positions in the rewritten string
//! back to positions in the original, so a match found in normalized space can be
//! applied to the original bytes. Every normalization here must therefore be
//! expressible one source character at a time; a rule that needed to see two
//! characters at once could not be mapped back and does not belong in this file.
//!
//! `find_all_matches` / `find_unique_match` are the core of the `str_replace`
//! uniqueness constraint.
//!
//! ## UTF-16 fidelity
//!
//! v4 operates on **UTF-16 code units** throughout: `indexOf` returns UTF-16
//! indices, and both `normalizeForMatching` and `buildNormalizationMap` walk the
//! original string one UTF-16 unit at a time (per JS `text[i]`). The port
//! reproduces that on `Vec<u16>` / the `jsstr` UTF-16 primitives.
//!
//! **The former whole-string/per-unit seam is closed.** Before `487ae16b1` v4
//! built the searched string whole-string and the map per-unit, so a length-
//! changing rewrite could desync the two; the commit rebuilt both from ONE
//! per-character function precisely so a length-changing fold (`…` → `...`) maps
//! back correctly, and this port follows. Decomposable astral characters and
//! length-changing case folds (İ / final-sigma) are still excluded from the
//! corpus: `to_lowercase` is applied whole-string on both sides, after the map is
//! built, exactly as v4 does.

use crate::jsstr::js_index_of;
use unicode_normalization::UnicodeNormalization;

use super::typographic_folding::fold_typographic_char;

/// Options for normalization-aware matching (v4 `DiacriticsMatchOptions`).
/// `normalize_diacritics` and `case_sensitive` default true; `fold_typography`
/// defaults **false**, so every pre-existing caller keeps byte-exact semantics.
#[derive(Debug, Clone, Copy)]
pub struct DiacriticsMatchOptions {
    pub normalize_diacritics: bool,
    pub case_sensitive: bool,
    /// Whether to treat a curly quote, a dash-family character, `…` or a
    /// non-breaking space as equal to its ASCII spelling.
    ///
    /// [`find_all_matches`] and [`find_unique_match`] read this flag differently,
    /// and deliberately: `find_all_matches` folds and reports whatever that
    /// finds, while `find_unique_match` — which owes its caller a *unique*
    /// answer — tries the exact reading first and only folds when the exact one
    /// found nothing at all.
    pub fold_typography: bool,
}

impl Default for DiacriticsMatchOptions {
    fn default() -> Self {
        DiacriticsMatchOptions {
            normalize_diacritics: true,
            case_sensitive: true,
            fold_typography: false,
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

/// The per-character rewrites in force for one match (v4 `NormalizationFlags`).
#[derive(Debug, Clone, Copy)]
struct NormalizationFlags {
    diacritics: bool,
    typography: bool,
}

/// Rewrite ONE source UTF-16 unit (v4 `normalizeChar`), returning the units it
/// contributes.
///
/// The typographic fold runs *before* the diacritics strip because it is defined
/// over composed characters (`’`, `—`, `…`), and its output is plain ASCII that
/// NFD leaves alone. May return more than one unit (`…` → `...`); both the
/// searched string and the position map are built from this one function, so they
/// cannot drift.
fn normalize_unit(unit: u16, flags: NormalizationFlags) -> Vec<u16> {
    if (0xD800..=0xDFFF).contains(&unit) {
        // Lone surrogate (one half of an astral pair): it is not a fold-table key
        // and `String.fromCharCode(unit).normalize('NFD')` is the unit itself.
        // Each half passes through in order, so the pair is reconstituted.
        return vec![unit];
    }
    let c = char::from_u32(unit as u32).expect("BMP non-surrogate is a scalar");
    let mut out: String = if flags.typography {
        match fold_typographic_char(c) {
            Some(folded) => folded.to_string(),
            None => c.to_string(),
        }
    } else {
        c.to_string()
    };
    if flags.diacritics {
        out = out
            .nfd()
            .filter(|d| !is_combining_mark(*d as u32))
            .collect();
    }
    out.encode_utf16().collect()
}

/// Rewrite a whole string, unit by unit (v4 `normalizeForMatching`). Built from
/// [`normalize_unit`] rather than from whole-string operations so that it cannot
/// drift from [`build_normalization_map`]: the string being searched and the map
/// used to translate the hit back must be produced by the same rule, or an index
/// found in one is meaningless in the other.
fn normalize_for_matching(text: &str, flags: NormalizationFlags) -> String {
    let mut units: Vec<u16> = Vec::with_capacity(text.len());
    for unit in text.encode_utf16() {
        units.extend(normalize_unit(unit, flags));
    }
    // Surrogate halves pass through in order, so the result is always well-formed.
    String::from_utf16(&units).expect("per-unit rewrite preserves surrogate pairs")
}

/// v4 `buildNormalizationMap`: `map[i]` = position in the ORIGINAL string
/// (UTF-16) of the character at position `i` in the normalized string.
fn build_normalization_map(original_u16: &[u16], flags: NormalizationFlags) -> Vec<usize> {
    let mut map: Vec<usize> = Vec::new();
    for (original_pos, unit) in original_u16.iter().enumerate() {
        let rewritten = normalize_unit(*unit, flags);
        for _ in 0..rewritten.len() {
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
    let should_fold = options.fold_typography;
    let case_sensitive = options.case_sensitive;

    if needle.is_empty() {
        return Vec::new();
    }

    // Simple case: no rewriting needed.
    if !should_normalize && !should_fold && case_sensitive {
        let mut matches = Vec::new();
        let needle_len = crate::jsstr::utf16_len(needle);
        let mut search_index = 0usize;
        while let Some(idx) = js_index_of(haystack, needle, search_index) {
            matches.push((idx, needle_len));
            search_index = idx + 1;
        }
        return matches;
    }

    let flags = NormalizationFlags {
        diacritics: should_normalize,
        typography: should_fold,
    };
    let rewriting = should_normalize || should_fold;

    // Build normalized versions.
    let (mut normalized_haystack, mut normalized_needle, haystack_map) = if rewriting {
        let orig_u16: Vec<u16> = haystack.encode_utf16().collect();
        let map = build_normalization_map(&orig_u16, flags);
        (
            normalize_for_matching(haystack, flags),
            normalize_for_matching(needle, flags),
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
            // Map normalized positions back to the original string.
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

/// Which reading of the text produced the answer (v4 `MatchTier`).
///
/// `Exact` means the needle was found as written (modulo the diacritics and case
/// options the caller had already asked for); `Typographic` means it was found
/// only once curly quotes, dashes, `…` and non-breaking spaces were folded onto
/// their ASCII spellings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchTier {
    Exact,
    Typographic,
}

impl MatchTier {
    /// v4's wire token (`'exact'` / `'typographic'`).
    pub fn as_str(self) -> &'static str {
        match self {
            MatchTier::Exact => "exact",
            MatchTier::Typographic => "typographic",
        }
    }
}

/// The result of [`find_unique_match`] (v4's discriminated `UniqueMatchResult`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UniqueMatch {
    Found {
        index: usize,
        length: usize,
        tier: MatchTier,
    },
    NotFound {
        count: usize,
        tier: MatchTier,
    },
}

/// Find a UNIQUE match of `needle` in `haystack` (v4 `findUniqueMatch`). Returns
/// the match iff exactly one exists, else the count. This is the core matching
/// function for `str_replace`'s uniqueness constraint.
///
/// With `fold_typography` on, this runs **exact first and folded only on a total
/// miss**. The order is the whole design: a file holding both `Veyra-5's` and
/// `Veyra-5’s` has one exact answer and two folded ones, and the caller asked for
/// the text it typed — so folding unconditionally would turn a good edit into an
/// ambiguity error. Folding is a rescue, not a policy.
///
/// On failure, `tier` says which reading produced `count`, so a caller can tell
/// "three exact matches, be more specific" from "nothing matched even when the
/// punctuation was ignored".
pub fn find_unique_match(
    haystack: &str,
    needle: &str,
    options: DiacriticsMatchOptions,
) -> UniqueMatch {
    let exact = find_all_matches(
        haystack,
        needle,
        DiacriticsMatchOptions {
            fold_typography: false,
            ..options
        },
    );

    if exact.len() == 1 {
        return UniqueMatch::Found {
            index: exact[0].0,
            length: exact[0].1,
            tier: MatchTier::Exact,
        };
    }
    // More than one exact match is an answer, not a miss: folding could only add
    // candidates to an ambiguity the caller must resolve anyway.
    if exact.len() > 1 || !options.fold_typography {
        return UniqueMatch::NotFound {
            count: exact.len(),
            tier: MatchTier::Exact,
        };
    }

    let folded = find_all_matches(
        haystack,
        needle,
        DiacriticsMatchOptions {
            fold_typography: true,
            ..options
        },
    );

    if folded.len() == 1 {
        tracing::debug!(
            target: "quilltap::doc_edit",
            needle_length = crate::jsstr::utf16_len(needle),
            haystack_length = crate::jsstr::utf16_len(haystack),
            "Match found only after folding typographic variants",
        );
        return UniqueMatch::Found {
            index: folded[0].0,
            length: folded[0].1,
            tier: MatchTier::Typographic,
        };
    }

    UniqueMatch::NotFound {
        count: folded.len(),
        tier: MatchTier::Typographic,
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
