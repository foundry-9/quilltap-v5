/**
 * The Home dashboard data layer (v4 `components/homepage/types.ts` +
 * `HomeViewContainer.tsx`'s fetch): the §1 `systemHome` payload types, the
 * TanStack query key (v4 `queryKeys.home.all`), and the one dispatch helper.
 *
 * §1 (the P4.6au ∥ P4.6av ∥ P4.6/P4.7c Shared contract): the payload field
 * names and omit-vs-null are v4's `HomeData` verbatim; lane A's differential
 * pins the body. `defaultImage.filepath` / `storyBackgroundUrl` arrive as
 * server-relative paths (dogfood finding #12) — components resolve them only
 * through the shared §2 helpers, never inline.
 */

import type { CoreClient } from '../../core/core-client';

/** Lightweight chat data for homepage display (v4 `RecentChat`). */
export interface RecentChat {
  id: string;
  title: string;
  updatedAt: string;
  lastMessageAt: string | null;
  /** Whether this chat has been classified as dangerous */
  isDangerousChat?: boolean;
  /** Story background image URL - displayed instead of avatars when present */
  storyBackgroundUrl?: string | null;
  participants: Array<{
    id: string;
    type: 'CHARACTER';
    isActive: boolean;
    displayOrder: number;
    character?: {
      id: string;
      name: string;
      defaultImageId?: string;
      defaultImage?: {
        id: string;
        filepath: string;
        url?: string;
      } | null;
      tags?: string[];
    } | null;
  }>;
  _count: {
    messages: number;
  };
}

/** Lightweight project data for homepage display (v4 `HomepageProject`). */
export interface HomepageProject {
  id: string;
  name: string;
  description?: string | null;
  color?: string | null;
  icon?: string | null;
  chatCount: number;
  /** Most recent activity (file, chat message, or metadata change) */
  lastActivity: string;
}

/** Character data for homepage grid (v4 `HomepageCharacter`). */
export interface HomepageCharacter {
  id: string;
  name: string;
  title?: string | null;
  defaultImageId: string | null;
  defaultImage: {
    id: string;
    filepath: string;
    url?: string | null;
  } | null;
  tags?: string[];
  // Sorting fields (same as /characters page)
  isFavorite: boolean;
  npc: boolean;
  chatCount: number;
  /** Default connection profile ID for showing provider badge */
  defaultConnectionProfileId?: string | null;
}

/** The whole §1 payload (v4 `HomeData` = `HomeViewProps`). */
export interface HomeData {
  displayName: string;
  lastChatId: string | null;
  recentChats: RecentChat[];
  projects: HomepageProject[];
  characters: HomepageCharacter[];
}

/** Query keys for the home dashboard (v4 `queryKeys.home`). */
export const homeKeys = {
  all: ['home'] as const,
};

/** Fetch the §1 payload — the data bag IS the `HomeData` object. */
export async function fetchHome(core: CoreClient): Promise<HomeData> {
  const data = await core.dispatchData({ type: 'systemHome' });
  return data as unknown as HomeData;
}

/**
 * v4 `components/homepage/CharacterCard.tsx` `profileDisplayName`: for compact
 * card display, strip the provider prefix (e.g. "GROK/grok-4.20…" →
 * "grok-4.20…"). Faithful to v4 including the empty-provider edge (an empty
 * provider makes the prefix "/", stripping a leading slash from the name).
 */
export function profileDisplayName(
  profile: { name?: string; provider?: string; modelName?: string } | null,
): string {
  const name = profile?.name || profile?.modelName || '';
  if (!name) {
    return '';
  }
  const provider = profile?.provider || '';
  const prefixes = [`${provider}/`, `${provider.toLowerCase()}/`];
  for (const prefix of prefixes) {
    if (name.startsWith(prefix)) {
      return name.slice(prefix.length);
    }
  }
  return name;
}
