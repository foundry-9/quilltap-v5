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
  CharacterPhotoRemoveRequest,
  CoreRequest,
  PhotoAlbumKind,
  PhotoAlbumOption,
  PhotoGalleryEntryRemoveRequest,
  PhotoGalleryListRequest,
} from '../../core/core-contract';

// === P4.D330: the album selector (v4 `680b71044`) ===
// STAND-IN for P4.D338's core-contract fence — the unifier deletes these two
// types and imports them (§S.9). C9: `photoGalleryAlbums {}` → `{ albums }`,
// and `photoGalleryList` gains `album` (a mount point id).
/** STAND-IN — P4.D338's `photoGalleryAlbums` request. */
export interface PhotoGalleryAlbumsRequestStandIn {
  type: 'photoGalleryAlbums';
}
/** STAND-IN — P4.D338's `PhotoGalleryListRequest.album`. */
export type PhotoGalleryListRequestStandIn = PhotoGalleryListRequest & { album?: string };

/** v4 `PhotosView.tsx:76` — the selector value for the operator's own roll-up. */
export const MY_PHOTOS = '';

/** v4 `PhotosView.tsx:78-83` — insertion order = the server's sort. */
export const ALBUM_GROUP_LABELS: Record<PhotoAlbumKind, string> = {
  character: 'Characters',
  project: 'Projects',
  'document-store': 'Document stores',
  general: 'Quilltap General',
};

/** v4 `GET /api/v1/photos?action=albums` — the albums the selector offers. */
export async function fetchAlbums(core: CoreClient): Promise<{ albums: PhotoAlbumOption[] }> {
  const request: PhotoGalleryAlbumsRequestStandIn = { type: 'photoGalleryAlbums' };
  const data = await core.dispatchData(request as unknown as CoreRequest);
  return { albums: (data['albums'] as PhotoAlbumOption[] | undefined) ?? [] };
}

/** v4 `queryKeys.photos.albums()` — the one TanStack key this screen holds. */
export const photoAlbumsKey = () => ['photos', 'albums'] as const;

/**
 * v4 `PhotosView.tsx:112-131` — the albums grouped by kind, in the order the
 * kinds first appear (a `Map`'s insertion order).
 */
export function albumGroupsOf(albums: readonly PhotoAlbumOption[]): Array<[PhotoAlbumKind, PhotoAlbumOption[]]> {
  const groups = new Map<PhotoAlbumKind, PhotoAlbumOption[]>();
  for (const option of albums) {
    const list = groups.get(option.kind) ?? [];
    list.push(option);
    groups.set(option.kind, list);
  }
  return Array.from(groups.entries());
}
// === end P4.D330 ===

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
  opts: { query?: string; offset: number; album?: string },
): Promise<ListResponse> {
  // v4 `buildListParams` (`680b71044`, `:92-99`): `album` first and only when
  // set, then `q` only when the TRIMMED query is non-empty, `limit`, `offset`.
  const request: PhotoGalleryListRequestStandIn = { type: 'photoGalleryList' };
  if (opts.album) {
    request.album = opts.album;
  }
  const trimmed = opts.query?.trim();
  if (trimmed) {
    request.q = trimmed;
  }
  request.limit = PAGE_SIZE;
  request.offset = opts.offset;
  const data = await core.dispatchData(request as CoreRequest);
  return data as unknown as ListResponse;
}

/**
 * v4 `PhotosView.tsx:172`, `680b71044` `photoDeleteUrl` (`:86-90`) — link-only
 * removal. A character's album is removed through the character's own door,
 * which scrubs portrait pointers; the SELECTED album decides it, so in My
 * Photos every delete takes the generic door (v4 behaviour, survey trap 17).
 */
export async function removeGalleryEntry(
  core: CoreClient,
  linkId: string,
  album: PhotoAlbumOption | null = null,
): Promise<void> {
  if (album?.kind === 'character' && album.characterId) {
    const request: CharacterPhotoRemoveRequest = {
      type: 'characterPhotoRemove',
      characterId: album.characterId,
      linkId,
    };
    await core.dispatchData(request);
    return;
  }
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
