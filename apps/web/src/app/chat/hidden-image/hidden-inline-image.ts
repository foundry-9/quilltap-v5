/**
 * The inline stand-in for an image embedded in message prose while the
 * Salon's images are hidden — v4 `HiddenInlineImage`
 * (`components/quick-hide/images-hidden-context.tsx:48-58`, `e3937d7aa`).
 *
 * v4 returns it from react-markdown's `img` renderer; v5's renderer emits an
 * HTML STRING (`chat/render/markdown-renderer.ts`, bound through
 * `[innerHTML]`), so the stand-in is that string rather than a component — a
 * component could never be mounted inside the rendered HTML. The markup is
 * v4's element for element: the span's classes, `role="img"`, the
 * `aria-label`, the `eye-off` icon exactly as `qt-icon` renders it
 * (`ui/icon.ts` — a mask-painted `span.qt-icon[data-icon]`, decorative), and
 * the VISIBLE text.
 *
 * Its neighbours: `qt-hidden-image-tile` (`hidden-image-tile.ts`) stands in
 * for a thumbnail; `qt-hidden-placeholder` (`quick-hide/hidden-placeholder.ts`)
 * is the whole-screen "Hidden" card for a quick-hidden tag's detail page.
 */
export function hiddenInlineImageHtml(alt?: string): string {
  const text = escapeHtml(alt ? `Image hidden: ${alt}` : 'Image hidden');
  return (
    '<span class="inline-flex items-center gap-1 px-2 py-0.5 rounded qt-bg-muted qt-text-secondary text-xs align-middle" ' +
    `role="img" aria-label="${text}">` +
    '<span class="qt-icon w-3 h-3" data-icon="eye-off" aria-hidden="true"></span>' +
    text +
    '</span>'
  );
}

/**
 * Replace every `<img>` in rendered message HTML with the inline stand-in —
 * the string-level form of v4's `img` renderer returning
 * `<HiddenInlineImage alt={alt || undefined} />` (`MessageContent.tsx:520-
 * 521`). The alt is read back out of the emitted tag and entity-decoded, so the
 * placeholder speaks the author's words, not their escaped form; an absent or
 * empty alt is v4's `alt || undefined` → "Image hidden".
 */
export function hideInlineImages(html: string): string {
  return html.replace(/<img\b[^>]*>/gi, (tag) => {
    const match = /\salt="([^"]*)"/i.exec(tag);
    const alt = match ? decodeHtmlEntities(match[1]) : '';
    return hiddenInlineImageHtml(alt || undefined);
  });
}

function decodeHtmlEntities(value: string): string {
  return value.replace(/&(#x[0-9a-f]+|#[0-9]+|amp|lt|gt|quot|apos);/gi, (whole, body: string) => {
    const lower = body.toLowerCase();
    if (lower.startsWith('#x')) return String.fromCodePoint(parseInt(lower.slice(2), 16));
    if (lower.startsWith('#')) return String.fromCodePoint(parseInt(lower.slice(1), 10));
    switch (lower) {
      case 'amp':
        return '&';
      case 'lt':
        return '<';
      case 'gt':
        return '>';
      case 'quot':
        return '"';
      case 'apos':
        return "'";
      default:
        return whole;
    }
  });
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}
