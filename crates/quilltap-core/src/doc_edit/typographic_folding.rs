//! Typographic folding — v4 `lib/doc-edit/typographic-folding.ts` (bug 109,
//! v4 `487ae16b1`).
//!
//! A per-character fold collapsing the typographic spellings of a handful of
//! ASCII characters onto the ASCII form: curly quotes onto `'` and `"`, the dash
//! family onto `-`, `…` onto `...`, and the non-breaking/wide spaces onto a plain
//! space. Used only for **matching** — nothing here ever reaches a file.
//!
//! ## Why the document tools need it
//!
//! Quilltap's typography rules are a rendering opinion and live entirely in the
//! two render pipelines; no tool, no export, and no LLM-facing string is ever
//! curled on its way past. **That did not change with this commit** and must not
//! drift on the strength of it: `487ae16b1` touched only the matcher.
//!
//! The problem comes from the other direction: **models write curly punctuation
//! of their own accord.** A character's prose, a custom tool's answer, a pasted
//! passage — any of them can put `’` into a file, and `doc_write_file` stores
//! exactly that, as it should. The failure arrives one turn later, when a model
//! that has just read the file retypes a sentence into `doc_str_replace`'s
//! `find` and spells the apostrophe `'`. The bytes differ, the exact match
//! fails, and the tool tells the model its text is stale.
//!
//! A fold is the right shape of answer because the difference is one of
//! *spelling*, not of meaning — the same argument the sibling
//! [`super::diacritics`] fold already makes for `Nimuë` and `Nimue`, whose
//! machinery this composes with.
//!
//! ## What it deliberately leaves alone
//!
//! Zero-width and invisible characters (U+200B, U+00AD, U+FEFF) are **not**
//! folded. They are an encoding problem rather than a typographic one, a model
//! cannot see them in a read to reproduce them either way, and stripping them
//! would let a needle match across a boundary no reader would agree was there.
//! Guillemets (`«` `»`) are left alone for the opposite reason: they are their
//! own punctuation, not a spelling of `"`.

/// The fold table, keyed by the single character being folded — v4's
/// `TYPOGRAPHIC_FOLDINGS`, transcribed in source order (the table is compared
/// entry-for-entry and in order by `doc_edit_leaves_equivalence`).
///
/// Values may be any length: callers map result positions back to source
/// positions per character, so `…` → `...` (one to three) is as legal as
/// `’` → `'` (one to one).
pub const TYPOGRAPHIC_FOLDINGS: &[(char, &str)] = &[
    // Single quotes and apostrophes
    ('\u{2018}', "'"), // ‘ left single quotation mark
    ('\u{2019}', "'"), // ’ right single quotation mark (the apostrophe models write)
    ('\u{201A}', "'"), // ‚ single low-9 quotation mark
    ('\u{201B}', "'"), // ‛ single high-reversed-9 quotation mark
    ('\u{2032}', "'"), // ′ prime
    ('\u{02BC}', "'"), // ʼ modifier letter apostrophe
    // Double quotes
    ('\u{201C}', "\""), // “ left double quotation mark
    ('\u{201D}', "\""), // ” right double quotation mark
    ('\u{201E}', "\""), // „ double low-9 quotation mark
    ('\u{201F}', "\""), // ‟ double high-reversed-9 quotation mark
    ('\u{2033}', "\""), // ″ double prime
    // The dash family
    ('\u{2010}', "-"), // ‐ hyphen
    ('\u{2011}', "-"), // ‑ non-breaking hyphen
    ('\u{2012}', "-"), // ‒ figure dash
    ('\u{2013}', "-"), // – en dash
    ('\u{2014}', "-"), // — em dash
    ('\u{2015}', "-"), // ― horizontal bar
    ('\u{2212}', "-"), // − minus sign
    // Ellipsis — the one fold that is not one-to-one
    ('\u{2026}', "..."), // …
    // Spaces that are not U+0020. Written as escapes deliberately: a literal
    // no-break space in source is indistinguishable from a plain one by eye.
    ('\u{00A0}', " "), // no-break space
    ('\u{2002}', " "), // en space
    ('\u{2003}', " "), // em space
    ('\u{2007}', " "), // figure space
    ('\u{2009}', " "), // thin space
    ('\u{202F}', " "), // narrow no-break space
];

/// Fold one character (v4 `foldTypographicChar`). Returns `None` when the
/// character is not in the table — the overwhelmingly common case, which must
/// stay cheap: this runs once per character of every haystack it is asked about.
pub fn fold_typographic_char(c: char) -> Option<&'static str> {
    TYPOGRAPHIC_FOLDINGS
        .iter()
        .find(|(k, _)| *k == c)
        .map(|(_, v)| *v)
}

/// Fold every character of `text` (v4 `foldTypography`).
///
/// Exported for tests and for callers that want the folded string itself; the
/// matcher in [`super::diacritics`] does not use it, because it must fold
/// character-by-character to keep its position map honest.
///
/// v4 iterates UTF-16 code units; every table key is BMP, so a lone surrogate is
/// never a key and concatenating the halves reproduces the pair — iterating
/// Rust `char`s is therefore byte-identical.
pub fn fold_typography(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match fold_typographic_char(c) {
            Some(folded) => out.push_str(folded),
            None => out.push(c),
        }
    }
    out
}

/// True if `text` contains at least one character the fold would change
/// (v4 `hasTypographicVariants`).
pub fn has_typographic_variants(text: &str) -> bool {
    text.chars().any(|c| fold_typographic_char(c).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_the_quote_family_onto_ascii() {
        assert_eq!(
            fold_typography("\u{2018}a\u{2019} \u{201C}b\u{201D}"),
            "'a' \"b\""
        );
    }

    #[test]
    fn folds_the_dash_family_onto_a_hyphen() {
        assert_eq!(fold_typography("a\u{2013}b\u{2014}c\u{2212}d"), "a-b-c-d");
    }

    #[test]
    fn expands_an_ellipsis_to_three_periods() {
        assert_eq!(fold_typography("wait\u{2026} now"), "wait... now");
    }

    #[test]
    fn folds_non_breaking_and_wide_spaces() {
        assert_eq!(fold_typography("a\u{00A0}b\u{202F}c\u{2003}d"), "a b c d");
    }

    #[test]
    fn leaves_guillemets_and_zero_width_alone() {
        let untouched = "\u{00AB}mot\u{00BB}\u{200B}\u{00AD}";
        assert_eq!(fold_typography(untouched), untouched);
    }

    #[test]
    fn has_variants_agrees_with_the_table() {
        assert!(!has_typographic_variants("plain ascii's fine"));
        assert!(has_typographic_variants("curly\u{2019}s not"));
        for (k, _) in TYPOGRAPHIC_FOLDINGS {
            assert!(has_typographic_variants(&k.to_string()), "table key {k:?}");
        }
    }
}
