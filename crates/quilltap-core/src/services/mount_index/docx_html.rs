//! `DOCX_RENDER_HTML_REFUSED` — the ONE home of v5's refusal of v4's
//! `?action=render-html` for a `.docx` (P4.D381 item 11).
//!
//! **The ruling (R-E, the human, 2026-11-04):** `.docx` `render-html` is a
//! NAMED refusal. v4 (`aec626eff`, `lib/mount-index/converters/docx-html.ts`)
//! renders the document through `mammoth.convertToHtml({buffer})`, then
//! `neutraliseUnsafeHrefs` replaces every `href` that is not `http(s):`,
//! `mailto:` or `#…` with `#`, and logs DEBUG `Rendered DOCX to HTML` (service
//! `MountIndex:DocxHtml`). v5 has no mammoth twin, so the route answers a 422
//! `not_available: docx render-html` (the storyteller idiom,
//! `quilltap-host/src/storyteller_spine.rs`) where v4 answers `200 {html}` —
//! and where v4 answers a 500 for a `.docx` that is not a zip, because v5
//! never parses. Every non-DOCX arm (the missing-file read refusal, the
//! `Only .docx files can be rendered to HTML` 400) stays v4's, byte-equal.
//!
//! **Not ported (R-P3, Tier 3 item 21):** `neutraliseUnsafeHrefs`, the
//! mammoth pipeline and its DEBUG line — their only caller is the refused
//! path, and a pure function with no caller is dead code. v4's vectors for the
//! re-open live in `__tests__/unit/lib/mount-index/converters/docx-html.test.ts`
//! at `624d0d25c` (`SAFE_HREF = /^(https?:|mailto:|#)/i`, `&amp;`-decoded and
//! trimmed before the test).
//!
//! **Re-open trigger:** a DOCX converter ruling (the prior documents survey's
//! R-2) — once v5 can turn DOCX bytes into HTML, this module becomes the
//! renderer and the both-ways table in `mount_ops_equivalence` retires.

/// The typed refusal [`render_docx_to_html`] answers for every input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocxRenderRefusal;

impl DocxRenderRefusal {
    /// The 422 body text the route answers (`Response::error(Unprocessable, …)`).
    pub const MESSAGE: &'static str = "not_available: docx render-html";
}

/// v4 `renderDocxToHtml(buffer)` — refused by name in v5 (R-E). The bytes are
/// accepted for the signature's sake (the route reads them FIRST, as v4 does,
/// so a missing file stays the read's refusal) and never parsed.
pub fn render_docx_to_html(_bytes: &[u8]) -> Result<String, DocxRenderRefusal> {
    Err(DocxRenderRefusal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_docx_is_refused_by_name() {
        assert_eq!(render_docx_to_html(b"PK\x03\x04"), Err(DocxRenderRefusal));
        assert_eq!(render_docx_to_html(b"not a zip"), Err(DocxRenderRefusal));
        assert_eq!(
            DocxRenderRefusal::MESSAGE,
            "not_available: docx render-html"
        );
    }
}
