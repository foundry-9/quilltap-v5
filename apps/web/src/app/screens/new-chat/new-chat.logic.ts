/**
 * New-Chat pure logic (v4 `useNewChat` / `NewChatForm` decision helpers). These
 * are the load-bearing transforms — the title generator, the in-place Play-As
 * transition, the scenario-dropdown token → state patch, the roster sort, the
 * initial connection-profile/system-prompt seed, and the submit payload — pulled
 * out as pure functions so they carry v4's exact strings/decisions under unit
 * test (tier-4's one differential class).
 */

import { buildAutonomousCreatePatch } from '../../autonomous/autonomous.logic';
import type { CharacterListItem, ChatCreateRequest } from '../../core/core-contract';
import {
  CUSTOM_SCENARIO_VALUE,
  GENERAL_SCENARIO_PREFIX,
  GROUP_SCENARIO_PREFIX,
  PROJECT_SCENARIO_PREFIX,
  USER_CONTROLLED_PROFILE,
  type NewChatFormState,
  type NewChatSelectedCharacter,
} from './new-chat.types';

/**
 * The auto-generated chat title (v4 `generateTitle`). Never user-editable:
 * counts LLM-controlled cast members and picks the copy verbatim.
 */
export function generateTitle(selected: NewChatSelectedCharacter[]): string {
  const llm = selected.filter((sc) => sc.controlledBy === 'llm');
  if (llm.length === 0) return 'New Chat';
  if (llm.length === 1) return `Chat with ${llm[0].character.name}`;
  if (llm.length === 2) return `Chat with ${llm[0].character.name} and ${llm[1].character.name}`;
  if (llm.length === 3) {
    return `Chat with ${llm[0].character.name}, ${llm[1].character.name}, and ${llm[2].character.name}`;
  }
  return `Group Chat (${llm.length} characters)`;
}

/**
 * The initial connection-profile + system-prompt seed for a freshly-selected
 * character (v4 `CharacterPickerPanel.handleSelect` / the seed branches): the
 * character's default profile → the first available; the default prompt
 * (`defaultSystemPromptId` → `isDefault` → first).
 */
export function seedSelectedCharacter(
  character: CharacterListItem,
  profiles: { id: string }[],
): NewChatSelectedCharacter {
  const connectionProfileId = character.defaultConnectionProfileId || profiles[0]?.id || '';
  const prompts = character.systemPrompts ?? [];
  const defaultPrompt = character.defaultSystemPromptId
    ? prompts.find((p) => p.id === character.defaultSystemPromptId)
    : (prompts.find((p) => p.isDefault) ?? prompts[0]);
  return {
    character,
    connectionProfileId,
    selectedSystemPromptId: defaultPrompt?.id ?? null,
    controlledBy: 'llm',
  };
}

/**
 * The picker roster sort (v4 `CharacterPickerPanel.filtered`): favorites >
 * user-controlled > chat count desc > lowercased name localeCompare > title.
 */
export function sortRoster(characters: CharacterListItem[]): CharacterListItem[] {
  return [...characters].sort((a, b) => {
    const aFav = a.isFavorite ? 1 : 0;
    const bFav = b.isFavorite ? 1 : 0;
    if (bFav !== aFav) return bFav - aFav;
    const aUser = a.controlledBy === 'user' ? 1 : 0;
    const bUser = b.controlledBy === 'user' ? 1 : 0;
    if (bUser !== aUser) return bUser - aUser;
    const aCount = a._count?.chats ?? 0;
    const bCount = b._count?.chats ?? 0;
    if (bCount !== aCount) return bCount - aCount;
    const nameCmp = a.name.toLowerCase().localeCompare(b.name.toLowerCase());
    if (nameCmp !== 0) return nameCmp;
    const aTitle = a.title?.toLowerCase() ?? '';
    const bTitle = b.title?.toLowerCase() ?? '';
    return aTitle.localeCompare(bTitle);
  });
}

/**
 * The in-place Play-As transition (v4 `e2eb3d21` `NewChatForm.handlePlayAsChange`).
 * Marks one cast member as the user's persona. Every option now comes from the
 * cast (default-user personas enter the cast via the picker on the left), so
 * there is nothing to pull in or remove: any prior user entry is handed back to
 * the LLM in place with its profile CLEARED (matching the per-character profile
 * select, so the submit guard asks again), and the chosen id — if any — is
 * flipped to `user`. `nextId === ''` = "Chat as yourself".
 */
export function applyPlayAs(
  prev: NewChatSelectedCharacter[],
  nextId: string,
): NewChatSelectedCharacter[] {
  const reverted = prev.map((sc) =>
    sc.controlledBy === 'user'
      ? { ...sc, controlledBy: 'llm' as const, connectionProfileId: '' }
      : sc,
  );
  if (nextId === '') return reverted; // "Chat as yourself"
  return reverted.map((sc) =>
    sc.character.id === nextId
      ? { ...sc, controlledBy: 'user' as const, connectionProfileId: '' }
      : sc,
  );
}

/**
 * The per-character connection-profile select transition (v4
 * `CharacterPickerPanel.handleProfileChange`): the {@link USER_CONTROLLED_PROFILE}
 * option flips the entry to the user and clears the profile; any other value
 * hands it back to the LLM.
 */
export function applyProfileChange(
  prev: NewChatSelectedCharacter[],
  characterId: string,
  profileId: string,
): NewChatSelectedCharacter[] {
  const isUser = profileId === USER_CONTROLLED_PROFILE;
  return prev.map((sc) =>
    sc.character.id === characterId
      ? {
          ...sc,
          connectionProfileId: isUser ? '' : profileId,
          controlledBy: isUser ? ('user' as const) : ('llm' as const),
        }
      : sc,
  );
}

/** The five scenario-source fields the dropdown owns (mutually exclusive). */
type ScenarioSourcePatch = Pick<
  NewChatFormState,
  | 'scenarioId'
  | 'projectScenarioPath'
  | 'generalScenarioPath'
  | 'groupScenarioPath'
  | 'groupScenarioGroupId'
>;

/**
 * The scenario dropdown token → source-field patch (v4
 * `NewChatForm.handleScenarioSelectChange`). Each choice nulls the other three
 * sources; it never touches free-text `scenario` (the layering invariant).
 */
export function scenarioSelectPatch(value: string): ScenarioSourcePatch {
  const cleared: ScenarioSourcePatch = {
    scenarioId: null,
    projectScenarioPath: null,
    generalScenarioPath: null,
    groupScenarioPath: null,
    groupScenarioGroupId: null,
  };
  if (value === CUSTOM_SCENARIO_VALUE || value === '') {
    return cleared;
  }
  if (value.startsWith(PROJECT_SCENARIO_PREFIX)) {
    return { ...cleared, projectScenarioPath: value.slice(PROJECT_SCENARIO_PREFIX.length) };
  }
  if (value.startsWith(GENERAL_SCENARIO_PREFIX)) {
    return { ...cleared, generalScenarioPath: value.slice(GENERAL_SCENARIO_PREFIX.length) };
  }
  if (value.startsWith(GROUP_SCENARIO_PREFIX)) {
    const rest = value.slice(GROUP_SCENARIO_PREFIX.length);
    const colonIdx = rest.indexOf(':');
    if (colonIdx > -1) {
      return {
        ...cleared,
        groupScenarioGroupId: rest.slice(0, colonIdx),
        groupScenarioPath: rest.slice(colonIdx + 1),
      };
    }
  }
  // A bare UUID — a character scenario.
  return { ...cleared, scenarioId: value };
}

/**
 * The submit payload (v4 `useNewChat.handleCreateChat`, non-autonomous). Sends
 * booleans only-when-true, optionals truly absent, and the mutually-exclusive
 * scenario source per the precedence chain (`scenarioId` > project > group >
 * general). `timestampConfig` is dropped when its mode is `NONE`.
 */
export function buildCreateRequest(
  selectedCharacters: NewChatSelectedCharacter[],
  form: NewChatFormState,
  selectedProjectId: string | null,
  progressId: string | undefined,
): ChatCreateRequest {
  const participants = selectedCharacters.map((sc) => ({
    type: 'CHARACTER' as const,
    characterId: sc.character.id,
    ...(sc.controlledBy === 'llm' ? { connectionProfileId: sc.connectionProfileId } : {}),
    ...(sc.selectedSystemPromptId ? { selectedSystemPromptId: sc.selectedSystemPromptId } : {}),
    controlledBy: sc.controlledBy,
  }));

  const body: ChatCreateRequest = {
    type: 'chatCreate',
    title: generateTitle(selectedCharacters),
    participants,
  };

  if (form.imageProfileId) body.imageProfileId = form.imageProfileId;
  // Free-text notes ride independently of any preset (the server layers them).
  if (form.scenario) body.scenario = form.scenario;

  // Preset (mutually exclusive): scenarioId > project > group > general.
  if (form.scenarioId) {
    body.scenarioId = form.scenarioId;
  } else if (form.projectScenarioPath) {
    body.projectScenarioPath = form.projectScenarioPath;
  } else if (form.groupScenarioPath) {
    body.groupScenarioPath = form.groupScenarioPath;
    body.groupScenarioGroupId = form.groupScenarioGroupId ?? undefined;
  } else if (form.generalScenarioPath) {
    body.generalScenarioPath = form.generalScenarioPath;
  }

  if (form.timestampConfig && form.timestampConfig['mode'] !== 'NONE') {
    body.timestampConfig = form.timestampConfig;
  }
  if (selectedProjectId) body.projectId = selectedProjectId;
  if (form.avatarGenerationEnabled) body.avatarGenerationEnabled = true;
  if (form.outfitSelections.length > 0) body.outfitSelections = form.outfitSelections;
  if (progressId) body.progressId = progressId;

  // Autonomous-room fields (v4 `useNewChat.ts:736-765`): chatType='autonomous', the
  // ms conversions, caps only when > 0, and budgetExcludeCacheHits ALWAYS sent.
  if (form.autonomous.enabled) {
    Object.assign(body, buildAutonomousCreatePatch(form.autonomous));
  }

  return body;
}
