import {
  renderMarkdownToHtml,
  type MarkdownRenderOptions,
} from './markdown-renderer';

/**
 * A keyed memo over {@link renderMarkdownToHtml}, the Angular equivalent of v4's
 * `LazyMessageContent` / `MessageRow` memoization (`LazyMessageContent.tsx:72-81`,
 * which skips a re-render unless `content` / `renderingPatterns` / `dialogueDetection`
 * change by reference).
 *
 * Why it matters under virtualization: the windowed list mounts and unmounts row
 * components as the reader scrolls, so the SAME message content is rendered again
 * every time a row re-enters the viewport. Without a cache each re-entry re-runs
 * the full unified markdown + roleplay pipeline; with it, a re-mount is a Map hit.
 * This is the fix's second half — windowing bounds *how many* rows render, the
 * cache bounds *how often* a given row's markdown is computed.
 *
 * The cache key is `content` plus reference ids for the two options objects (v4
 * compares them by reference too — they are stable module singletons or per-chat
 * template refs, so identity is the right grain). It is bounded with simple
 * insertion-order (oldest-first) eviction so streaming partials — each a unique
 * string — can't grow it without limit.
 */

const MAX_ENTRIES = 2000;
const cache = new Map<string, string>();

/** Field separator for the composite key — a control char no message contains. */
const SEP = '\u0000';

let refCounter = 0;
const refIds = new WeakMap<object, number>();

/** A stable per-reference id (`0` for null/undefined) used in the cache key. */
function refId(value: object | null | undefined): string {
  if (value == null) return '0';
  let id = refIds.get(value);
  if (id === undefined) {
    id = ++refCounter;
    refIds.set(value, id);
  }
  return String(id);
}

/**
 * Render markdown through the v4-parity pipeline, memoized by
 * `(content, renderingPatterns, dialogueDetection)`. Byte-identical to calling
 * {@link renderMarkdownToHtml} directly — only the second call for a given key is
 * cheap.
 */
export function renderMarkdownCached(
  content: string,
  options?: MarkdownRenderOptions,
): string {
  const key =
    content +
    SEP +
    refId(options?.renderingPatterns) +
    SEP +
    refId(options?.dialogueDetection);

  const hit = cache.get(key);
  if (hit !== undefined) return hit;

  const html = renderMarkdownToHtml(content, options ?? {});
  if (cache.size >= MAX_ENTRIES) {
    // Evict the oldest inserted entry (Map preserves insertion order).
    const oldest = cache.keys().next().value;
    if (oldest !== undefined) cache.delete(oldest);
  }
  cache.set(key, html);
  return html;
}

/** Test-only: clear the memo between cases. */
export function clearRenderCache(): void {
  cache.clear();
}
