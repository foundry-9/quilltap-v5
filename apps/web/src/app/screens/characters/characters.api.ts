/**
 * The characters-vertical data layer: TanStack query keys + thin `CoreClient`
 * dispatch helpers shared by the list / view / edit / new screens.
 *
 * Every op reads through {@link CoreClient.dispatchData} (raw `data`, throwing a
 * `CoreDispatchError` on the error envelope) because the p4.6f Shared contract
 * pins the response BODIES via its differentials, not a narrowed Rust `Response`
 * type — the response `type` string is not load-bearing here (the settings-lane
 * precedent for unpinned response types).
 */

import { apiUrl } from '../../core/api-url';
import type { CoreClient } from '../../core/core-client';
import { normalizeAvatarSrc } from '../../ui/avatar-stack';
import type {
  CascadePreview,
  CharacterChatsResult,
  CharacterConnectionProfile,
  CharacterDetail,
  CharacterListItem,
  CharacterPhoto,
  CharacterStats,
  CharacterGroupBadge,
  CharacterTagDetail,
  ConnectionProfileDto,
  TagDto,
} from '../../core/core-contract';

/**
 * Query keys for the characters vertical (v4 `queryKeys.characters`).
 *
 * P4.D64: the roster's "Show Archived" toggle keys its two fetches DISTINCTLY —
 * `list({archived:'include'})` vs `list()` — so the filter states never
 * cross-contaminate a cache entry (v4 `AuroraView.tsx:47-49`). Mutations
 * therefore invalidate the `all` PREFIX rather than one list key, "so the other
 * archived-filter variant refreshes too" (v4 `:72`). v5 keeps its own
 * omit-the-object-when-absent convention for the unfiltered key (v4 keys `{}`);
 * only the DISTINCTNESS is contractual.
 */
export const characterKeys = {
  all: ['characters'] as const,
  list: (filter?: { npc?: string; controlledBy?: string; archived?: string }) =>
    filter ? (['characters', 'list', filter] as const) : (['characters', 'list'] as const),
  detail: (id: string) => ['characters', 'detail', id] as const,
  stats: (id: string) => ['characters', 'stats', id] as const,
  tags: (id: string) => ['characters', 'tags', id] as const,
  chats: (id: string, search?: string) =>
    search
      ? (['characters', 'chats', id, search] as const)
      : (['characters', 'chats', id] as const),
  photos: (id: string) => ['characters', 'photos', id] as const,
  depiction: (id: string) => ['characters', 'depiction', id] as const,
  prompts: (id: string) => ['characters', 'prompts', id] as const,
  scenarios: (id: string) => ['characters', 'scenarios', id] as const,
  defaultPartner: (id: string) => ['characters', 'default-partner', id] as const,
};

export const tagKeys = {
  all: ['tags'] as const,
  list: (search?: string) =>
    search ? (['tags', 'list', search] as const) : (['tags', 'list'] as const),
};

/**
 * The card avatar src: v4 resolves `defaultImage.filepath` (ensuring a leading
 * slash) and the SPA adds a `?v={defaultImageId}` cache-bust so a changed avatar
 * invalidates the browser cache. Returns `null` → the initial-letter fallback.
 */
export function characterAvatarSrc(
  defaultImage: { filepath: string; url?: string | null } | null | undefined,
  defaultImageId: string | null | undefined,
): string | null {
  if (!defaultImage) {
    return null;
  }
  const base = defaultImage.url || normalizeAvatarSrc(defaultImage.filepath);
  if (!base) {
    return null;
  }
  return defaultImageId ? `${base}?v=${defaultImageId}` : base;
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

export async function fetchCharacterList(
  core: CoreClient,
  filter?: {
    npc?: 'true' | 'false';
    controlledBy?: 'user' | 'llm';
    archived?: 'include' | 'only';
  },
): Promise<CharacterListItem[]> {
  const data = await core.dispatchData({ type: 'characterList', ...filter });
  return (data['characters'] as CharacterListItem[]) ?? [];
}

export async function fetchCharacter(
  core: CoreClient,
  characterId: string,
): Promise<CharacterDetail> {
  const data = await core.dispatchData({ type: 'characterGet', characterId });
  return data['character'] as CharacterDetail;
}

export async function fetchCharacterStats(
  core: CoreClient,
  characterId: string,
): Promise<{ stats: CharacterStats; groups: CharacterGroupBadge[] }> {
  const data = await core.dispatchData({ type: 'characterStats', characterId });
  return {
    stats: data['stats'] as CharacterStats,
    groups: (data['groups'] as CharacterGroupBadge[]) ?? [],
  };
}

export async function fetchCharacterTags(
  core: CoreClient,
  characterId: string,
): Promise<CharacterTagDetail[]> {
  const data = await core.dispatchData({ type: 'characterGetTags', characterId });
  return (data['tags'] as CharacterTagDetail[]) ?? [];
}

export async function fetchCharacterPhotos(
  core: CoreClient,
  characterId: string,
): Promise<CharacterPhoto[]> {
  const data = await core.dispatchData({ type: 'characterPhotoList', characterId });
  // The pinned P4.6i envelope: `{ entries, total, hasMore }`.
  return (data['entries'] as CharacterPhoto[]) ?? [];
}

export async function fetchCharacterChats(
  core: CoreClient,
  characterId: string,
  opts?: { search?: string; limit?: number; offset?: number },
): Promise<CharacterChatsResult> {
  const data = await core.dispatchData({
    type: 'characterChats',
    characterId,
    ...(opts?.search ? { search: opts.search } : {}),
    ...(opts?.limit !== undefined ? { limit: opts.limit } : {}),
    ...(opts?.offset !== undefined ? { offset: opts.offset } : {}),
  });
  return {
    chats: (data['chats'] as CharacterChatsResult['chats']) ?? [],
    total: (data['total'] as number) ?? 0,
  };
}

export async function fetchDepictionGuidelines(
  core: CoreClient,
  characterId: string,
): Promise<string> {
  const data = await core.dispatchData({ type: 'characterDepictionGuidelines', characterId });
  return (data['content'] as string) ?? '';
}

export async function fetchCascadePreview(
  core: CoreClient,
  characterId: string,
): Promise<CascadePreview> {
  const data = await core.dispatchData({ type: 'characterCascadePreview', characterId });
  return data as unknown as CascadePreview;
}

export async function fetchConnectionProfiles(
  core: CoreClient,
): Promise<CharacterConnectionProfile[]> {
  const data = await core.dispatchData({ type: 'connectionProfileList' });
  const profiles = (data['profiles'] as ConnectionProfileDto[]) ?? [];
  return profiles.map((p) => ({
    id: p.id,
    name: p.name,
    provider: p.provider,
    modelName: p.modelName,
  }));
}

export async function fetchDefaultPartner(
  core: CoreClient,
  characterId: string,
): Promise<string | null> {
  const data = await core.dispatchData({ type: 'characterDefaultPartner', characterId });
  return (data['partnerId'] as string | null) ?? null;
}

/**
 * Fetch the SillyTavern JSON export card via dispatch (v4 `?action=export&
 * format=json`, which returns the ST card object). Lane A pins the exact
 * envelope; unwrap a `card`/`character` wrapper defensively, else the data IS
 * the card. PNG export stays the deferred binary web route.
 */
export async function fetchCharacterExport(
  core: CoreClient,
  characterId: string,
): Promise<unknown> {
  const data = await core.dispatchData({ type: 'characterExport', characterId, format: 'json' });
  return data['card'] ?? data['character'] ?? data;
}

/**
 * Trigger a client-side JSON download (v4's export route sets a
 * `Content-Disposition: attachment; filename="<name>.json"`; over dispatch the
 * SPA builds the Blob itself). No-op in non-DOM contexts.
 */
export function triggerJsonDownload(filename: string, value: unknown): void {
  if (typeof document === 'undefined' || typeof URL === 'undefined' || !URL.createObjectURL) {
    return;
  }
  const blob = new Blob([JSON.stringify(value, null, 2)], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  URL.revokeObjectURL(url);
}

// ---------------------------------------------------------------------------
// The archive family (P4.D64 §2). The two verbs are DEFINED by P4.D63 answering
// the loud not-yet-available refusal; ROUND 2 fills them, so in this round both
// calls surface that refusal as the thrown message and the action e2e beats stay
// gated. The success shapes below are the §2-pinned bodies.
// ---------------------------------------------------------------------------

/** v4's archive success body (`?action=archive`). */
export interface ArchiveCharacterResult {
  archived: boolean;
  archiveFileId: string | null;
  /**
   * FALSE when the bundle sealed but some effects resisted the prune — v4 says
   * so in the toast and invites a second archive to finish the sweep.
   */
  pruneComplete: boolean;
}

/** v4's rehydrate success body (`?action=rehydrate`). */
export interface RehydrateCharacterResult {
  rehydrated: boolean;
  archived: boolean;
  /** The bundle left on the shelf — non-empty ⇒ offer its disposal. */
  archiveBundleFileId: string | null;
  restored?: { memories: number; documents: number; blobs: number };
  warnings: string[];
}

/**
 * Pack a character into a sealed bundle and set them resting in the archive
 * (v4 `POST /api/v1/characters/{id}?action=archive`). Read defensively: only
 * `pruneComplete === false` takes the "some effects resisted packing" arm, so a
 * body that omits the key reads as a clean sweep (v4 `:286`).
 */
export async function archiveCharacter(
  core: CoreClient,
  characterId: string,
): Promise<ArchiveCharacterResult> {
  const data = await core.dispatchData({ type: 'characterArchive', characterId });
  return data as unknown as ArchiveCharacterResult;
}

/**
 * Unpack an archived character and wake them (v4 `?action=rehydrate`). v4 only
 * offers the leftover-bundle dialog when `archiveBundleFileId` is a NON-EMPTY
 * string (`:314`) — an absent or empty id means there is nothing on the shelf.
 */
export async function rehydrateCharacter(
  core: CoreClient,
  characterId: string,
): Promise<RehydrateCharacterResult> {
  const data = await core.dispatchData({ type: 'characterRehydrate', characterId });
  return data as unknown as RehydrateCharacterResult;
}

/**
 * Discard the archive bundle a rehydration left behind (v4 `DELETE
 * /api/v1/files/{id}`, `RehydrateBundleDialog.tsx:31`). v4 sends no options, so
 * neither do we — the bundle has no associations to dissociate.
 */
export async function deleteArchiveBundle(core: CoreClient, fileId: string): Promise<void> {
  await core.dispatchData({ type: 'fileDelete', fileId });
}

/**
 * How many archive bundles are sealed under the current passphrase (v4
 * `ChangePassphraseCard.tsx:24-31` — `GET /api/v1/files?category=ARCHIVE`, whose
 * `files.length` IS the count). A COURTESY: v4 swallows every failure, because
 * the passphrase change itself reports the real numbers.
 */
export async function countArchiveBundles(core: CoreClient): Promise<number | null> {
  try {
    const data = await core.dispatchData({ type: 'filesList', category: 'ARCHIVE' });
    const files = data['files'];
    return Array.isArray(files) ? files.length : null;
  } catch {
    return null;
  }
}

export async function fetchTags(core: CoreClient, search?: string): Promise<TagDto[]> {
  const data = await core.dispatchData({ type: 'tagList', ...(search ? { search } : {}) });
  return (data['tags'] as TagDto[]) ?? [];
}

// ---------------------------------------------------------------------------
// Multipart / binary WEB ROUTES (v4-shaped, NOT dispatch variants — the byte
// legs dispatch can't carry). Lane C (P4.6m) ships these; the SPA calls them via
// `fetch`, live at unification. Contract B↔C is pinned in the work orders.
// ---------------------------------------------------------------------------

/** The parsed 201 body of a photo upload (contract B↔C). */
export interface UploadedPhoto {
  linkId: string;
  mountPointId: string;
  relativePath: string;
  keptAt: string;
  sha256: string;
}

/**
 * Upload a photo to a character's gallery (`POST /api/v1/characters/{id}/photos`,
 * multipart). `file` is required; `caption` + repeated `tags` are optional. The
 * v4 400 keyword list surfaces as the thrown message.
 */
export async function uploadCharacterPhoto(
  characterId: string,
  file: File,
  opts?: { caption?: string; tags?: string[] },
): Promise<UploadedPhoto> {
  const form = new FormData();
  form.append('file', file);
  if (opts?.caption) {
    form.append('caption', opts.caption);
  }
  for (const tag of opts?.tags ?? []) {
    form.append('tags', tag);
  }
  const res = await fetch(apiUrl(`/api/v1/characters/${encodeURIComponent(characterId)}/photos`), {
    method: 'POST',
    body: form,
  });
  if (!res.ok) {
    let message = `Upload failed (HTTP ${res.status})`;
    try {
      const body = (await res.json()) as { error?: string };
      if (body?.error) {
        message = body.error;
      }
    } catch {
      // non-JSON error body — keep the status message
    }
    throw new Error(message);
  }
  return (await res.json()) as UploadedPhoto;
}

/** The v4 `handleResetBuiltins` success body (counts shape). */
export interface ResetBuiltinsResult {
  success: boolean;
  deletedCharacterIds: string[];
  preservedIds?: { Lorian: string | null; Riya: string | null };
  postResetIds?: { Lorian: string | null; Riya: string | null };
  remappedIdCount?: number;
  importResult?: { imported?: { characters?: number } };
}

/**
 * Reset the built-in characters (`POST /api/v1/characters?action=reset-builtins`,
 * the WEB-EDGE route that went live in P4.4u4 — dispatched at the edge like the
 * upload/PNG riders). Deletes any existing Lorian/Riya then re-imports their
 * first-run versions; the success body carries v4's counts shape.
 */
export async function resetBuiltinCharacters(): Promise<ResetBuiltinsResult> {
  const res = await fetch(apiUrl('/api/v1/characters?action=reset-builtins'), { method: 'POST' });
  if (!res.ok) {
    let message = `Failed to reset built-in characters (HTTP ${res.status})`;
    try {
      const body = (await res.json()) as { error?: string };
      if (body?.error) {
        message = body.error;
      }
    } catch {
      // non-JSON error body — keep the status message
    }
    throw new Error(message);
  }
  return (await res.json()) as ResetBuiltinsResult;
}

/**
 * Export a character as a SillyTavern PNG card (`GET /api/v1/characters/{id}?
 * action=export&format=png`) and trigger a client-side download. The route sets
 * `Content-Disposition: attachment; filename="<name>.png"`; over `fetch` the SPA
 * rebuilds the blob download itself. No-op in non-DOM contexts.
 */
export async function downloadCharacterPng(characterId: string, name: string): Promise<void> {
  const res = await fetch(
    apiUrl(`/api/v1/characters/${encodeURIComponent(characterId)}?action=export&format=png`),
  );
  if (!res.ok) {
    throw new Error(`PNG export failed (HTTP ${res.status})`);
  }
  const blob = await res.blob();
  if (typeof document === 'undefined' || typeof URL === 'undefined' || !URL.createObjectURL) {
    return;
  }
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = `${name}.png`;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  URL.revokeObjectURL(url);
}
