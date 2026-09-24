/**
 * The Scenario Builder's data layer — the three reads its dialogs make, and the
 * query keys they cache under (v4 `lib/query/keys.ts` at `d1c06cd9d`:
 * `groups.byCharacters`, `scenarioBuilder.capabilities`), plus v4's
 * `useConnectionProfiles` mapping of the three profile flags the dialog reads.
 *
 * A RUN is not here: it is a stream, not a query (v4's own note — "TanStack
 * stays out of it"); see `scenario-builder-run.state.ts`.
 *
 * @module scenario-builder/scenario-builder.api
 */

import type { CoreClient } from '../core/core-client';
import type { ConnectionProfileDto, ScenarioBuilderCapabilities } from '../core/core-contract';

/** v4 `queryKeys.scenarioBuilder` — only the capability probe is cached. */
export const scenarioBuilderKeys = {
  all: ['scenario-builder'] as const,
  capabilities: ['scenario-builder', 'capabilities'] as const,
};

/**
 * v4 `queryKeys.groups.byCharacters(<sorted, comma-joined ids>)` — the groups
 * any of the cast belongs to. The `['groups', …]` prefix is v4's, so a sweep of
 * the groups family reaches it.
 */
export const groupsByCharactersKey = (characterIdsKey: string) =>
  ['groups', 'by-characters', characterIdsKey] as const;

/**
 * The connection-profile list's query key — UNDER the `['connectionProfiles']`
 * prefix, so every Settings-card invalidation (prefix match) and the tab
 * refetch reach it and the dialog never shows a profile list staler than the
 * one the user just edited (v4's dialog shares `queryKeys.connectionProfiles`).
 * NOT the bare key itself: the home page and the Brahma console cache MAPPED
 * shapes under the bare key (`{id, name, provider, modelName[, isDefault]}`),
 * which lack `allowToolUse`/`allowWebSearch` — read here through a shared
 * cache entry, every tool-less profile would turn pickable and every Real-mode
 * profile would read as web-search-less (the d1c06cd9d unification review).
 */
export const connectionProfilesKey = ['connectionProfiles', 'scenario-builder'] as const;

/** The profile as the Scenario Builder reads it (v4 `ConnectionProfileInfo` at `d1c06cd9d`). */
export interface ScenarioBuilderProfile {
  id: string;
  name: string;
  provider: string;
  modelName: string;
  /** The user's default chat profile. */
  isDefault: boolean;
  /** False when the profile has tool use switched off. */
  allowToolUse: boolean;
  /** Whether the profile may offer the `search_web` tool. */
  allowWebSearch: boolean;
}

/**
 * v4 `mapProfiles` — note the two flags default in OPPOSITE directions:
 * `allowToolUse` is opt-OUT (absent → true), `allowWebSearch` is opt-IN
 * (absent → false), and `isDefault` is a strict `=== true`.
 */
export function mapScenarioBuilderProfiles(
  profiles: readonly Partial<ConnectionProfileDto>[],
): ScenarioBuilderProfile[] {
  return profiles.map((p) => ({
    id: p.id as string,
    name: p.name || '',
    provider: p.provider || '',
    modelName: p.modelName || '',
    isDefault: p.isDefault === true,
    allowToolUse: p.allowToolUse !== false,
    allowWebSearch: p.allowWebSearch === true,
  }));
}

/** The raw profile list (the Settings cards' `queryFn` shape, so the cache agrees). */
export async function fetchConnectionProfiles(core: CoreClient): Promise<ConnectionProfileDto[]> {
  const resp = await core.dispatchExpect({ type: 'connectionProfileList' }, 'connectionProfiles');
  return resp.data.profiles;
}

/** v4 `GET /api/v1/scenario-builder?action=capabilities`. */
export async function fetchScenarioBuilderCapabilities(
  core: CoreClient,
): Promise<ScenarioBuilderCapabilities> {
  const data = await core.dispatchData({ type: 'scenarioBuilderCapabilities' });
  return data as unknown as ScenarioBuilderCapabilities;
}

/** One group the save dialog offers (v4 `GroupRow`). */
export interface ScenarioBuilderGroupRow {
  id: string;
  name: string;
}

/**
 * v4 `GET /api/v1/groups?characterIds=` → `groupList { characterIds }` (the
 * round's §S.2 — v5 has no REST groups edge; the SPA dispatches). The caller
 * gates on a NON-EMPTY cast: a present-but-empty list is zero groups on the
 * server, which is also what v4's `enabled: castKey.length > 0` never asks.
 */
export async function fetchGroupsByCharacters(
  core: CoreClient,
  characterIds: string[],
): Promise<ScenarioBuilderGroupRow[]> {
  const data = await core.dispatchData({ type: 'groupList', characterIds });
  return (data['groups'] as ScenarioBuilderGroupRow[] | undefined) ?? [];
}
