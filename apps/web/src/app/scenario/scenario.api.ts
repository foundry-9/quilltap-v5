/**
 * The scenario option lists, per tier — the data layer both the New Chat dialog
 * and the Salon sidebar's in-chat picker read (v4 `lib/query/keys.ts`'s
 * `scenarios` key family, added by `44a8137e`).
 *
 * The files behind these lists live in document stores, so every key is scoped
 * to what the list actually depends on — the project, the character-id set, the
 * character — rather than to the chat. Two chats in the same project share one
 * cached project tier; the same chat's list does not go stale because a
 * different chat refetched.
 *
 * The SPA reuses the EXISTING list verbs (Shared contract §5) — no new list
 * endpoints were added for the in-chat picker.
 */

import type { CoreClient } from '../core/core-client';
import type { ScenarioDto, ScenarioListDto } from '../core/core-contract';
import type {
  CharacterScenario,
  GeneralScenarioOption,
  GroupScenarioOption,
  ProjectScenarioOption,
} from './scenario.types';

/**
 * v4 `queryKeys.scenarios`. `all` is the family root (invalidate everything);
 * the group key is the comma-joined, SORTED character ids the groups derive
 * from, so the same cast in a different order hits the same cache row.
 *
 * `includeArchived` is part of every key since v4 `d25dacc1`: the two lists are
 * different server ANSWERS, not one list filtered two ways, so they must not
 * share a cache entry. Prefix invalidation on `all` still sweeps both.
 */
export const scenarioKeys = {
  all: ['scenarios'] as const,
  general: (includeArchived = false) => ['scenarios', 'general', { includeArchived }] as const,
  project: (projectId: string, includeArchived = false) =>
    ['scenarios', 'project', projectId, { includeArchived }] as const,
  group: (characterIdsKey: string, includeArchived = false) =>
    ['scenarios', 'group', characterIdsKey, { includeArchived }] as const,
  character: (characterId: string, includeArchived = false) =>
    ['scenarios', 'character', characterId, { includeArchived }] as const,
};

/** The group-union envelope's per-group entry (v4 `GroupScenarioGroup`). */
interface GroupScenarioGroup {
  groupId: string;
  groupName: string;
  scenarios: ScenarioDto[];
}

/**
 * A `ScenarioDto` narrowed to what the dropdown renders. `description` is
 * omitted rather than nulled when absent, because the option label branches on
 * its truthiness and v4's own DTO leaves it undefined.
 */
export function toScenarioOption(s: ScenarioDto): ProjectScenarioOption {
  return {
    path: s.path,
    filename: s.filename,
    name: s.name,
    ...(s.description !== undefined && { description: s.description }),
    isDefault: s.isDefault,
    // v4 `useNewChat` narrows the wire value with `=== true`, so a server that
    // predates `d25dacc1` (and omits the key) reads as active rather than as
    // `undefined` leaking into the option label's truthiness test.
    archived: s.archived === true,
    body: s.body,
  };
}

/**
 * v4 `GET /api/v1/scenarios` — the instance-wide "Quilltap General" tier.
 *
 * `includeArchived` is v4's `?includeArchived=true`: the SERVER decides what is
 * hidden, so a surface that never asks is archived-free by construction and no
 * client-side pass exists to drift from the rule.
 */
export async function fetchGeneralScenarios(
  core: CoreClient,
  includeArchived = false,
): Promise<GeneralScenarioOption[]> {
  const data = await core.dispatchData({ type: 'scenarioList', includeArchived });
  return ((data as unknown as ScenarioListDto).scenarios ?? []).map(toScenarioOption);
}

/** v4 `GET /api/v1/projects/:id/scenarios`. */
export async function fetchProjectScenarios(
  core: CoreClient,
  projectId: string,
  includeArchived = false,
): Promise<ProjectScenarioOption[]> {
  const data = await core.dispatchData({
    type: 'projectScenarioList',
    projectId,
    includeArchived,
  });
  return ((data as unknown as ScenarioListDto).scenarios ?? []).map(toScenarioOption);
}

/**
 * v4 `GET /api/v1/groups/scenarios?characterIds=` — the union over every group
 * the given characters belong to, flattened from v4's grouped envelope into the
 * flat shape the dropdown renders.
 */
export async function fetchGroupScenarios(
  core: CoreClient,
  characterIds: string[],
  includeArchived = false,
): Promise<GroupScenarioOption[]> {
  const data = await core.dispatchData({
    type: 'groupScenariosUnion',
    characterIds,
    includeArchived,
  });
  const groups = (data['groupScenarios'] as GroupScenarioGroup[] | undefined) ?? [];
  const flat: GroupScenarioOption[] = [];
  for (const group of groups) {
    for (const scenario of group.scenarios ?? []) {
      flat.push({
        ...toScenarioOption(scenario),
        groupId: group.groupId,
        groupName: group.groupName,
      });
    }
  }
  return flat;
}

/** v4 `GET /api/v1/characters/:id/scenarios` — the lone LLM character's own tier. */
export async function fetchCharacterScenarios(
  core: CoreClient,
  characterId: string,
  includeArchived = false,
): Promise<CharacterScenario[]> {
  const data = await core.dispatchData({
    type: 'characterScenarioList',
    characterId,
    includeArchived,
  });
  return (data['scenarios'] as CharacterScenario[] | undefined) ?? [];
}
