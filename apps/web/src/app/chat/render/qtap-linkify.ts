/**
 * qtap:// bare-URL linkification — verbatim port of v4 `lib/chat/qtap-linkify.ts`.
 * Upgrades surfaced literal qtap:// URIs to markdown link form while skipping
 * inline/fenced code spans.
 */

const INLINE_CODE_OR_FENCE_RE = /(```[\s\S]*?```|`[^`\n]*`)/g;
const BARE_QTAP_URI_RE = /(?<!\]\()(?<!<)(qtap:\/\/[^\s<>()\]]+)/g;

export function linkifyBareQtapUris(text: string): string {
  return text
    .split(INLINE_CODE_OR_FENCE_RE)
    .map((segment) => {
      if (!segment) return segment;
      if (segment.startsWith('```') || segment.startsWith('`')) {
        return segment;
      }
      return segment.replace(BARE_QTAP_URI_RE, (uri) => `[${uri}](${uri})`);
    })
    .join('');
}

/** True when a URL uses the qtap:// scheme (used to preserve it through sanitizers). */
export function isQtapUri(url: string): boolean {
  return url.startsWith('qtap://');
}
