//! Help-document slugs — port of v4 `lib/help/help-doc-slug.ts` (`helpDocSlug`),
//! promoted there by `d6e74145` out of `help-doc-sync.ts`'s private
//! `generateDocumentId`.
//!
//! **Why it exists (v4's rationale, carried forward):** the database primary key
//! for a help doc is a UUID that changes whenever the doc is re-created, so the
//! slug derived from its file path is the identifier everything OUTSIDE the
//! database uses — the Guide's category lists, `Related Pages` links between
//! docs, and the help-docs API.
//!
//! ## The consumers are NOT ported (a named deferral, not an oversight)
//!
//! v4 populates the slug in `HelpSearch`'s two mapping sites and matches it in
//! `getDocument(idOrSlug)` / `listDocuments()`. v5 has **no `HelpSearch` class
//! and no help-docs API** — `db::help_search` exposes only the two search
//! primitives, and the flat [`crate::db::help_search::HelpSearchResult`] is v4's
//! **tool-handler output** shape, which drops `slug`
//! (`lib/tools/handlers/help-search-handler.ts:64-71` maps `id`/`title`/`path`/
//! `url`/`score`/`content` only). Adding `slug` to that shape would DIVERGE from
//! v4's tool wire, so this module deliberately lands ahead of its consumers: it
//! is the promotion `d6e74145` performed, ready for the `HelpSearch` + help-docs
//! API surface when that lands.

/// v4 `helpDocSlug` — derive a help document's stable slug from its file path.
///
/// `help/character-creation.md` → `character-creation`.
///
/// v4's chain is
/// `.replace(/^help\//, '').replace(/\.md$/, '').replace(/[^a-zA-Z0-9]/g, '-').toLowerCase()`
/// — the two anchored strips are single, non-global replacements; the character
/// map is global.
///
/// **UTF-16 fidelity:** v4's `[^a-zA-Z0-9]` carries no `/u` flag, so it matches
/// per UTF-16 **code unit** — a non-BMP char (a surrogate PAIR) becomes **two**
/// dashes, not one. This port maps `encode_utf16()` for that reason rather than
/// `chars()`, which would emit a single dash and silently diverge. Every unit
/// that survives the map is ASCII-alphanumeric, so the result is pure ASCII and
/// the trailing `toLowerCase()` reduces to an ASCII lowercase (no ICU/locale
/// path can be reached from here).
pub fn help_doc_slug(rel_path: &str) -> String {
    // `.replace(/^help\//, '')` then `.replace(/\.md$/, '')` — anchored, once each.
    let stripped = rel_path.strip_prefix("help/").unwrap_or(rel_path);
    let stripped = stripped.strip_suffix(".md").unwrap_or(stripped);

    // `.replace(/[^a-zA-Z0-9]/g, '-')` per UTF-16 code unit, then `.toLowerCase()`.
    stripped
        .encode_utf16()
        .map(|unit| match u8::try_from(unit) {
            Ok(b) if b.is_ascii_alphanumeric() => (b as char).to_ascii_lowercase(),
            _ => '-',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_help_prefix_and_md_suffix() {
        assert_eq!(
            help_doc_slug("help/character-creation.md"),
            "character-creation"
        );
        assert_eq!(help_doc_slug("help/aurora.md"), "aurora");
    }

    #[test]
    fn lowercases_and_maps_non_alphanumerics() {
        assert_eq!(help_doc_slug("help/Brahma Console.md"), "brahma-console");
        assert_eq!(help_doc_slug("help/weird-CASE-Name.md"), "weird-case-name");
        // Underscores and dots are non-alphanumeric → dashes.
        assert_eq!(help_doc_slug("help/a_b.c.md"), "a-b-c");
    }

    #[test]
    fn nested_paths_keep_their_separators_as_dashes() {
        // Only the FIRST `help/` is stripped (the regex is anchored, non-global);
        // the remaining slash falls to the character map.
        assert_eq!(help_doc_slug("help/nested/deep-doc.md"), "nested-deep-doc");
        assert_eq!(help_doc_slug("help/help/x.md"), "help-x");
    }

    #[test]
    fn anchors_are_not_substring_matches() {
        // `help/` only strips at the START; `.md` only at the END.
        assert_eq!(help_doc_slug("docs/help/x.md"), "docs-help-x");
        assert_eq!(help_doc_slug("help/a.mdx"), "a-mdx");
        // `.md` mid-path survives; the trailing one goes.
        assert_eq!(help_doc_slug("help/a.md.md"), "a-md");
    }

    #[test]
    fn non_bmp_char_yields_two_dashes_per_v4_utf16_semantics() {
        // U+1F600 is a surrogate PAIR in UTF-16; v4's non-/u regex replaces each
        // unit, so ONE emoji becomes TWO dashes.
        assert_eq!(help_doc_slug("help/a\u{1F600}b.md"), "a--b");
        // A BMP non-ASCII char is a single unit → a single dash.
        assert_eq!(help_doc_slug("help/a\u{00E9}b.md"), "a-b");
    }

    #[test]
    fn empty_and_degenerate_paths() {
        assert_eq!(help_doc_slug(""), "");
        assert_eq!(help_doc_slug("help/"), "");
        assert_eq!(help_doc_slug("help/.md"), "");
    }
}
