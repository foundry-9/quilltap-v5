/**
 * Pure logic for the search UI (P4.9P): the server-URL → v5-route mapping and
 * the highlight splitter (v4 `search-results.tsx` `HighlightedText`).
 */

import type { SearchResult, SearchType } from './search.types';

/**
 * Map a result URL the server hands back — v4's URL strings VERBATIM, they are
 * data — onto the v5 route space. Each mapping is a documented divergence:
 *
 * - `/aurora/{id}[?tab=…]` → `/characters/{id}[?tab=…]` (v5's character
 *   detail; the route guard preserves `?tab=`, so `?tab=memories` lands on
 *   the Memories tab).
 * - `/gallery?tag={name}` → `/photos?tag={name}` (v5's photo gallery lives at
 *   `/photos`; the tag FILTER deep-link is a named deferral — the photos
 *   screen doesn't read `?tag=` yet, so the row lands on the unfiltered
 *   gallery. The param is carried so the affordance lights up when the screen
 *   learns it).
 * - `/salon/{id}?msg={messageId}` → unchanged (a named deferral: v5's Salon
 *   has no `?msg=` message-anchor affordance, and the workspace redirect
 *   guard carries only the chat id — the row lands in the right conversation,
 *   unanchored).
 * - everything else (`/salon/{id}`) → unchanged.
 */
export function mapResultUrl(url: string): string {
  if (url.startsWith('/aurora/')) {
    return '/characters/' + url.slice('/aurora/'.length);
  }
  if (url.startsWith('/gallery')) {
    return '/photos' + url.slice('/gallery'.length);
  }
  return url;
}

export interface HighlightSegment {
  text: string;
  match: boolean;
}

/**
 * v4 `HighlightedText` (`search-results.tsx`): split on the regex-escaped
 * query (case-insensitive, global) and mark the case-insensitive-equal parts.
 * An empty query or text yields one unmarked segment.
 */
export function highlightSegments(text: string, query: string): HighlightSegment[] {
  if (!query || !text) return [{ text, match: false }];
  const escaped = query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const parts = text.split(new RegExp(`(${escaped})`, 'gi'));
  return parts
    .filter((part) => part !== '')
    .map((part) => ({ text: part, match: part.toLowerCase() === query.toLowerCase() }));
}

/**
 * v4 `SearchResults` grouping: results bucketed by type in FIRST-ENCOUNTER
 * order (JS object insertion semantics — the server's sort decides which type
 * leads).
 */
export function groupResultsByType(
  results: SearchResult[],
): Array<{ type: SearchType; results: SearchResult[] }> {
  const order: SearchType[] = [];
  const buckets = new Map<SearchType, SearchResult[]>();
  for (const result of results) {
    if (!buckets.has(result.type)) {
      buckets.set(result.type, []);
      order.push(result.type);
    }
    buckets.get(result.type)!.push(result);
  }
  return order.map((type) => ({ type, results: buckets.get(type)! }));
}

/**
 * The dialog's append-page dedupe (v4 `search-dialog.tsx:83-89`) — `type-id`
 * keys, existing rows win.
 */
export function appendDeduped(prev: SearchResult[], next: SearchResult[]): SearchResult[] {
  const existing = new Set(prev.map((r) => `${r.type}-${r.id}`));
  return [...prev, ...next.filter((r) => !existing.has(`${r.type}-${r.id}`))];
}
