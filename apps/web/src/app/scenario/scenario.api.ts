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
 */
export const scenarioKeys = {
  all: ['scenarios'] as const,
  general: ['scenarios', 'general'] as const,
  project: (projectId: string) => ['scenarios', 'project', projectId] as const,
  group: (characterIdsKey: string) => ['scenarios', 'group', characterIdsKey] as const,
  character: (characterId: string) => ['scenarios', 'character', characterId] as const,
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
    body: s.body,
  };
}

/** v4 `GET /api/v1/scenarios` — the instance-wide "Quilltap General" tier. */
export async function fetchGeneralScenarios(core: CoreClient): Promise<GeneralScenarioOption[]> {
  const data = await core.dispatchData({ type: 'scenarioList' });
  return ((data as unknown as ScenarioListDto).scenarios ?? []).map(toScenarioOption);
}

/** v4 `GET /api/v1/projects/:id/scenarios`. */
export async function fetchProjectScenarios(
  core: CoreClient,
  projectId: string,
): Promise<ProjectScenarioOption[]> {
  const data = await core.dispatchData({ type: 'projectScenarioList', projectId });
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
): Promise<GroupScenarioOption[]> {
  const data = await core.dispatchData({ type: 'groupScenariosUnion', characterIds });
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
): Promise<CharacterScenario[]> {
  const data = await core.dispatchData({ type: 'characterScenarioList', characterId });
  return (data['scenarios'] as CharacterScenario[] | undefined) ?? [];
}
