/**
 * New-Chat vertical types (v4 `components/new-chat/types.ts`). The cast entry
 * ({@link NewChatSelectedCharacter}) is the single source of truth for who
 * plays whom — the human's persona is the entry whose `controlledBy` is
 * `'user'`, never a separate slot (v4's in-place Play-As).
 */

import {
  INITIAL_AUTONOMOUS_STATE,
  type NewChatAutonomousState,
} from '../../autonomous/autonomous.logic';
import type {
  CharacterListItem,
  ChatCreateOutfitSelectionInput,
  TimestampConfig,
} from '../../core/core-contract';

export type { NewChatAutonomousState };

/** The special connection-profile option that flips a cast entry to the user (v4 `USER_CONTROLLED_PROFILE`). */
export const USER_CONTROLLED_PROFILE = '__USER_CONTROLLED__';
/** The dropdown "Custom…" free-text sentinel (v4 `CUSTOM_SCENARIO_VALUE`). */
export const CUSTOM_SCENARIO_VALUE = '__custom__';
/** `<option value>` prefix for a project scenario: `project:<relativePath>`. */
export const PROJECT_SCENARIO_PREFIX = 'project:';
/** `<option value>` prefix for a general scenario: `general:<relativePath>`. */
export const GENERAL_SCENARIO_PREFIX = 'general:';
/** `<option value>` prefix for a group scenario: `group:<groupId>:<relativePath>`. */
export const GROUP_SCENARIO_PREFIX = 'group:';

/** A scenario file option in the dropdown (project / general / group share this shape). */
export interface ScenarioOption {
  path: string;
  filename: string;
  name: string;
  description?: string;
  isDefault: boolean;
  body: string;
}

/** A group-scoped scenario option (adds the owning group's id/name). */
export interface GroupScenarioOption extends ScenarioOption {
  groupId: string;
  groupName: string;
}

/** A character's own scenario (v4 `CharacterScenario` — the list projection). */
export interface CharacterScenarioOption {
  id: string;
  title: string;
  content: string;
  description?: string;
}

/** A slim project row for the in-form project picker (v4 `ProjectListEntry`). */
export interface ProjectListEntry {
  id: string;
  name: string;
  color?: string | null;
}

/** The full project record (defaults ride the detail GET, not the list). */
export interface NewChatProject {
  id: string;
  name: string;
  color?: string | null;
  defaultAvatarGenerationEnabled?: boolean | null;
  defaultImageProfileId?: string | null;
}

/**
 * One cast member (v4 `SelectedCharacter`). Carries the full
 * {@link CharacterListItem} so the picker/form read its defaults, prompts, and
 * scenarios without a second fetch.
 */
export interface NewChatSelectedCharacter {
  character: CharacterListItem;
  connectionProfileId: string;
  selectedSystemPromptId?: string | null;
  controlledBy: 'llm' | 'user';
}

/** The mutable form state (v4 `NewChatFormState`). */
export interface NewChatFormState {
  imageProfileId: string;
  /** Free-text scenario notes (layered beneath any resolved preset). */
  scenario: string;
  scenarioId: string | null;
  projectScenarioPath: string | null;
  generalScenarioPath: string | null;
  groupScenarioPath: string | null;
  groupScenarioGroupId: string | null;
  timestampConfig: TimestampConfig | null;
  avatarGenerationEnabled: boolean;
  outfitSelections: ChatCreateOutfitSelectionInput[];
  /** The autonomous-room slice (v4 `state.autonomous`; consulted only when `enabled`). */
  autonomous: NewChatAutonomousState;
}

/** The pristine form state (v4 `INITIAL_STATE`). */
export const INITIAL_FORM_STATE: NewChatFormState = {
  imageProfileId: '',
  scenario: '',
  scenarioId: null,
  projectScenarioPath: null,
  generalScenarioPath: null,
  groupScenarioPath: null,
  groupScenarioGroupId: null,
  timestampConfig: null,
  avatarGenerationEnabled: false,
  outfitSelections: [],
  autonomous: { ...INITIAL_AUTONOMOUS_STATE },
};
