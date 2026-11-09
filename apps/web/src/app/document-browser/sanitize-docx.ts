/**
 * The `.docx` reading copy's sanitizer — v4
 * `components/files/FilePreview/FilePreviewDocx.tsx` `sanitizeDocxHtml`
 * (`c34986421`, pin `624d0d25c`), with v4's config verbatim (SPA ruling R9).
 *
 * v4's why, carried: the server renders the document to HTML with mammoth
 * (`?action=render-html`, which already neutralises link targets); the HTML is
 * sanitised again here with DOMPurify before it reaches the page, since there
 * is no DOM on the server to do it there. Formatting beyond what mammoth maps
 * (headings, emphasis, lists, tables, links, images) is not reproduced — this
 * is a reading copy, not an editor.
 *
 * v5 runs it on a PRIVATE `DOMPurify(window)` instance (the
 * `chat/diagrams/sanitize-svg.ts` precedent), so nothing configured here ever
 * reaches another DOMPurify user. Today v5's server answers `.docx`
 * `render-html` with the named 422 refusal `DOCX_RENDER_HTML_REFUSED` (R-E,
 * P4.D381) and the viewer shows the download fallback, so this runs only once
 * a converter lands; it is ported and proven now so the viewer arm is whole.
 * `sanitize-docx.spec.ts` compares it byte for byte with v4's real function
 * under v4's jest jsdom.
 */

import DOMPurify, { type DOMPurify as DOMPurifyInstance } from 'dompurify';

let purifier: DOMPurifyInstance | null = null;

function getPurifier(): DOMPurifyInstance {
  purifier ??= DOMPurify(window);
  return purifier;
}

/** Sanitise mammoth's HTML for display: no scripts, no handlers, only safe link targets. */
export function sanitizeDocxHtml(html: string): string {
  return getPurifier().sanitize(html, {
    USE_PROFILES: { html: true },
    FORBID_TAGS: ['style', 'form', 'input', 'button', 'iframe', 'object', 'embed'],
    ALLOWED_URI_REGEXP: /^(?:https?:|mailto:|#|data:image\/)/i,
  });
}
