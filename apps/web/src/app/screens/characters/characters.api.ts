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

/** Query keys for the characters vertical (v4 `queryKeys.characters`). */
export const characterKeys = {
  all: ['characters'] as const,
  list: (filter?: { npc?: string; controlledBy?: string }) =>
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
  filter?: { npc?: 'true' | 'false'; controlledBy?: 'user' | 'llm' },
): Promise<CharacterListItem[]> {
  const data = await core.dispatchData({ type: 'characterList', ...filter });
  return (data['characters'] as CharacterListItem[]) ?? [];
}

export async function fetchCharacter(core: CoreClient, characterId: string): Promise<CharacterDetail> {
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
  // v4's `/photos` returns `{ entries }` (each entry's `id` is the vault
  // `doc_mount_file_links.id` = the linkId); fall back to the legacy
  // `photos`/`images` keys until lane A's envelope is pinned at unification.
  const photos = data['entries'] ?? data['photos'] ?? data['images'];
  return (photos as CharacterPhoto[]) ?? [];
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

export async function fetchTags(core: CoreClient, search?: string): Promise<TagDto[]> {
  const data = await core.dispatchData({ type: 'tagList', ...(search ? { search } : {}) });
  return (data['tags'] as TagDto[]) ?? [];
}
