/**
 * The My Photos data layer (v4 `app/photos/PhotosView.tsx`'s inline types +
 * its two fetches): the §3 payload types and the dispatch helpers.
 *
 * §3 (the P4.9a Shared contract): `photoGalleryList`, `photoGallerySave`,
 * `photoGalleryEntryGet`, `photoGalleryEntryRemove`. The field names and the
 * omit-vs-null splits are v4's verbatim — `photos_routes_equivalence` pins the
 * bytes. The request types live in `core-contract.ts` (folded from this
 * file's locals at the round's unification, the `home.api.ts` pattern).
 *
 * `blobUrl` arrives as a server-RELATIVE path (dogfood finding #12), so every
 * consumer resolves it through the shared `apiUrl` helper and never inline.
 */

import { apiUrl } from '../../core/api-url';
import type { CoreClient } from '../../core/core-client';
import type {
  PhotoGalleryEntryRemoveRequest,
  PhotoGalleryListRequest,
} from '../../core/core-contract';

/** v4 `PhotosView.tsx:20-31`. */
export interface PhotoLinker {
  linkId: string;
  mountPointId: string;
  mountPointName: string;
  mountStoreType: 'character' | 'documents';
  relativePath: string;
  isPhotoAlbum: boolean;
  linkedAt: string;
  linkedBy: string | null;
  linkedById: string | null;
  caption: string | null;
  tags: string[];
}

/** v4 `PhotosView.tsx:33-36`. */
export interface PhotoLinkSummary {
  count: number;
  linkers: PhotoLinker[];
}

/** v4 `PhotosView.tsx:38-52` (the service's `UserGalleryEntry`). */
export interface GalleryEntry {
  linkId: string;
  mountPointId: string;
  relativePath: string;
  fileName: string;
  blobUrl: string;
  mimeType: string;
  sha256: string;
  fileSizeBytes: number;
  keptAt: string;
  caption: string | null;
  tags: string[];
  generationPromptExcerpt: string;
  /** ABSENT (not null) outside the semantic-query branch — see the service. */
  relevanceScore?: number;
  linkSummary: PhotoLinkSummary;
}

/** v4 `PhotosView.tsx:54-58`. */
export interface ListResponse {
  entries: GalleryEntry[];
  total: number;
  hasMore: boolean;
}

/** v4 `PAGE_SIZE` (`PhotosView.tsx:61`). NOTE: 60, not the service's own 24. */
export const PAGE_SIZE = 60;

/**
 * v4 `PhotosView.tsx:85-95` / `:113-119` — one list endpoint serves both the
 * initial load and every load-more; only `offset` moves.
 */
export async function fetchGallery(
  core: CoreClient,
  opts: { query?: string; offset: number },
): Promise<ListResponse> {
  const request: PhotoGalleryListRequest = {
    type: 'photoGalleryList',
    limit: PAGE_SIZE,
    offset: opts.offset,
  };
  // v4 sets `q` only when the TRIMMED query is non-empty.
  const trimmed = opts.query?.trim();
  if (trimmed) {
    request.q = trimmed;
  }
  const data = await core.dispatchData(request);
  return data as unknown as ListResponse;
}

/** v4 `PhotosView.tsx:172` — link-only removal. */
export async function removeGalleryEntry(core: CoreClient, linkId: string): Promise<void> {
  const request: PhotoGalleryEntryRemoveRequest = {
    type: 'photoGalleryEntryRemove',
    id: linkId,
  };
  await core.dispatchData(request);
}

/**
 * v4 `PhotoCard` (`:338`): caption, else the prompt excerpt, else the file
 * name. All three are `||` chains in v4, so an EMPTY STRING falls through —
 * `generationPromptExcerpt` is `''` (never null) when there is no prompt, and
 * relying on `??` here would show a blank label.
 */
export function cardCaption(entry: GalleryEntry): string {
  return entry.caption || entry.generationPromptExcerpt || entry.fileName;
}

/**
 * v4 `PhotoCard` (`:339-343`): the linker row for THIS entry's own link, else
 * the first linker; its `linkedBy` if set, else the mount name. `Unfiled` when
 * the summary is somehow empty.
 */
export function cardLinkerLabel(entry: GalleryEntry): string {
  const primary =
    entry.linkSummary.linkers.find((l) => l.linkId === entry.linkId) ??
    entry.linkSummary.linkers[0];
  return primary ? (primary.linkedBy ?? primary.mountPointName) : 'Unfiled';
}

/** v4 `PhotoDetailModal` (`:403`) — the modal title drops the prompt fallback. */
export function detailCaption(entry: GalleryEntry): string {
  return entry.caption || entry.fileName;
}

/** v4 `:471` — a character store reads "Vault", anything else "Album". */
export function linkerBadge(linker: PhotoLinker): string {
  return linker.mountStoreType === 'character' ? 'Vault' : 'Album';
}

/** The displayable image URL for an entry (the server path is relative). */
export function entryImageUrl(entry: GalleryEntry): string {
  return apiUrl(entry.blobUrl);
}
