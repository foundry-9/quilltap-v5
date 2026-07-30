/**
 * The search fetch (P4.9P) — v4 calls `GET /api/v1/ui/search` directly from
 * both the bar (`search-bar.tsx:75`) and the dialog (`search-dialog.tsx:68`),
 * so v5 does the same over `apiUrl()` (the REST edge is the surface v4
 * exercises; the `uiSearch` dispatch verb exists for §1 parity).
 */

import { apiUrl } from '../core/api-url';
import type { SearchResponse, SearchType } from './search.types';

/** The bar's call: `?q=…&limit=20` (v4 `search-bar.tsx:75`). */
export async function searchInline(query: string, limit: number): Promise<SearchResponse> {
  const response = await fetch(
    apiUrl(`/api/v1/ui/search?q=${encodeURIComponent(query)}&limit=${limit}`),
  );
  if (!response.ok) {
    throw new Error(`Search failed: ${response.status}`);
  }
  return (await response.json()) as SearchResponse;
}

/** The dialog's call: `?q=…&types=…&limit=…&offset=…` (v4 `search-dialog.tsx:66-70`). */
export async function searchPaged(
  query: string,
  types: SearchType[],
  limit: number,
  offset: number,
): Promise<SearchResponse> {
  const typesParam = types.join(',');
  const response = await fetch(
    apiUrl(
      `/api/v1/ui/search?q=${encodeURIComponent(query)}&types=${typesParam}&limit=${limit}&offset=${offset}`,
    ),
  );
  if (!response.ok) {
    throw new Error(`Search failed: ${response.status}`);
  }
  return (await response.json()) as SearchResponse;
}
