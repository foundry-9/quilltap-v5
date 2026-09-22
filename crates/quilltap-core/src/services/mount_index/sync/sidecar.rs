//! `<file>.description.md` — a binary's caption, on disk.
//!
//! Port of v4 `lib/mount-index/sync/sidecar.ts` (`23da0b322`).
//!
//! A database store keeps a per-location `description` on every link row: the
//! operator's caption, `doc_set_blob_description`, or whatever the image
//! auto-captioner wrote. Nothing in the tree has ever had a disk representation
//! for it, so a sync that only copied bytes would drop every caption on the
//! first pass out and blank them on the way back.
//!
//! The sidecar is the plainest thing that survives a text editor: the
//! description verbatim, no frontmatter, named after the whole file (extension
//! and all) so `harbour.png` and `harbour.webp` cannot collide. Only binaries
//! get one — a text document's description is not synced at all.

use super::types::{sha256_hex, SIDECAR_SUFFIX};

/// v4 `sidecarPathFor`: `lore/harbour.png` → `lore/harbour.png.description.md`.
pub fn sidecar_path_for(relative_path: &str) -> String {
    format!("{relative_path}{SIDECAR_SUFFIX}")
}

/// v4 `isSidecarPath` — true when this path IS a sidecar rather than a file
/// that has one. v4 lower-cases with `toLowerCase()`, which `str::to_lowercase`
/// reproduces byte for byte (the ICU cluster, Phase 1).
pub fn is_sidecar_path(relative_path: &str) -> bool {
    relative_path.to_lowercase().ends_with(SIDECAR_SUFFIX)
}

/// v4 `partnerPathFor`: `lore/harbour.png.description.md` → `lore/harbour.png`,
/// or `None` when the path is not a sidecar.
///
/// ⚠ v4 slices by the suffix's LENGTH off the original string after matching on
/// the lower-cased one, so a `…DESCRIPTION.MD` sidecar's partner is the original
/// casing minus the same number of characters. The suffix is ASCII, so a byte
/// slice is the same cut JS's UTF-16 slice makes here only when the suffix
/// region is ASCII — which the `is_sidecar_path` test has just established.
pub fn partner_path_for(relative_path: &str) -> Option<String> {
    if !is_sidecar_path(relative_path) {
        return None;
    }
    let partner = &relative_path[..relative_path.len() - SIDECAR_SUFFIX.len()];
    if partner.is_empty() {
        None
    } else {
        Some(partner.to_string())
    }
}

/// v4 `renderSidecar` — the bytes a sidecar holds for `description`.
///
/// Trailing-newline hygiene only: editors add one, and a round trip through a
/// text editor must not look like an edit. [`parse_sidecar`] strips it back off.
/// An empty description renders to nothing, which the writer reads as "remove
/// the file".
pub fn render_sidecar(description: &str) -> String {
    let body = trim_trailing_js_whitespace(description);
    if body.is_empty() {
        String::new()
    } else {
        format!("{body}\n")
    }
}

/// v4 `parseSidecar` — the description a sidecar's bytes mean.
pub fn parse_sidecar(body: &str) -> String {
    trim_trailing_js_whitespace(body).to_string()
}

/// v4 `descriptionSha256` — the comparison currency for descriptions, so the
/// manifest can tell "the sidecar was edited" from "the store's caption was
/// edited" without storing the caption itself. Computed on the PARSED text, so
/// whitespace the editor added is not a change.
pub fn description_sha256(description: &str) -> String {
    sha256_hex(parse_sidecar(description).as_bytes())
}

/// v4 `descriptionsEqual` — true when two descriptions mean the same thing. An
/// absent description and an empty one are the same thing.
pub fn descriptions_equal(a: Option<&str>, b: Option<&str>) -> bool {
    parse_sidecar(a.unwrap_or("")) == parse_sidecar(b.unwrap_or(""))
}

/// `String.prototype.replace(/\s+$/, '')`.
///
/// JS `\s` is not Rust's `char::is_whitespace`: it is the Unicode space
/// separators plus `\t\n\v\f\r`, U+00A0, U+FEFF and U+1680/U+2000-200A/U+2028/
/// U+2029/U+202F/U+205F/U+3000 — the difference from Rust being U+FEFF (JS
/// counts it, Rust does not) and U+0085 (Rust counts it, JS does not).
/// The memory note `js-regex-to-rust-regex-fidelity` names this as one of the
/// three axes that actually diverge, so the predicate is spelled out rather
/// than delegated.
fn trim_trailing_js_whitespace(text: &str) -> &str {
    text.trim_end_matches(is_js_whitespace)
}

/// The exact set ECMAScript's `\s` matches: `WhiteSpace` ∪ `LineTerminator`.
pub(crate) fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'      // tab
            | '\u{000A}' // line feed
            | '\u{000B}' // vertical tab
            | '\u{000C}' // form feed
            | '\u{000D}' // carriage return
            | '\u{0020}' // space
            | '\u{00A0}' // no-break space
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200A}'
            | '\u{2028}' // line separator
            | '\u{2029}' // paragraph separator
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}' // zero-width no-break space
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_whitespace_differs_from_rust_in_exactly_two_places() {
        // U+FEFF: JS `\s` matches it, `char::is_whitespace` does not.
        assert!(is_js_whitespace('\u{FEFF}'));
        assert!(!'\u{FEFF}'.is_whitespace());
        // U+0085 (NEL): `char::is_whitespace` matches it, JS `\s` does not.
        assert!(!is_js_whitespace('\u{0085}'));
        assert!('\u{0085}'.is_whitespace());
        // …so a caption ending in NEL keeps it, and one ending in BOM loses it.
        assert_eq!(parse_sidecar("a\u{0085}"), "a\u{0085}");
        assert_eq!(parse_sidecar("a\u{FEFF}"), "a");
    }

    #[test]
    fn an_empty_description_renders_no_file() {
        assert_eq!(render_sidecar(""), "");
        assert_eq!(render_sidecar("   \n\t "), "");
        assert_eq!(render_sidecar("a caption"), "a caption\n");
        assert_eq!(render_sidecar("a caption\n\n\n"), "a caption\n");
    }

    #[test]
    fn a_sidecar_is_named_after_the_whole_file() {
        assert_eq!(
            sidecar_path_for("lore/harbour.png"),
            "lore/harbour.png.description.md"
        );
        assert!(is_sidecar_path("lore/harbour.png.DESCRIPTION.MD"));
        assert_eq!(
            partner_path_for("lore/harbour.png.description.md").as_deref(),
            Some("lore/harbour.png")
        );
        assert_eq!(partner_path_for("lore/harbour.png"), None);
        // A file called exactly `.description.md` has no partner.
        assert_eq!(partner_path_for(".description.md"), None);
    }
}
