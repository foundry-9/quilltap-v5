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

/**
 * Scenario option shapes and dropdown tokens are shared with the Salon
 * sidebar's in-chat picker — they live in `app/scenario/scenario.types` and are
 * re-exported here so this module stays the New Chat dialog's single import
 * (v4 `44a8137e` does exactly this in `components/new-chat/types.ts`).
 */
export type {
  CharacterScenario,
  ProjectScenarioOption,
  GeneralScenarioOption,
  GroupScenarioOption,
  ScenarioSelection,
} from '../../scenario/scenario.types';
export {
  CUSTOM_SCENARIO_VALUE,
  PROJECT_SCENARIO_PREFIX,
  GENERAL_SCENARIO_PREFIX,
  GROUP_SCENARIO_PREFIX,
} from '../../scenario/scenario.types';
import type {
  CharacterScenario,
  ProjectScenarioOption,
} from '../../scenario/scenario.types';

/** The special connection-profile option that flips a cast entry to the user (v4 `USER_CONTROLLED_PROFILE`). */
export const USER_CONTROLLED_PROFILE = '__USER_CONTROLLED__';

/**
 * A scenario file option in the dropdown. v4 declares `ProjectScenarioOption`
 * and `GeneralScenarioOption` as two identical interfaces; v5 collapsed them
 * into one name before the shared module existed, so this alias keeps the v5
 * call sites reading naturally while the shape is the shared one.
 */
export type ScenarioOption = ProjectScenarioOption;

/** A character's own scenario (v4 `CharacterScenario` — the list projection). */
export type CharacterScenarioOption = CharacterScenario;

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
  defaultRoleplayTemplateId?: string | null;
}

/**
 * A roleplay template offered in the New Chat form's template dropdown (v4
 * `RoleplayTemplateOption`). Trimmed from the `roleplayTemplateList` record —
 * the form only needs enough to label an option.
 */
export interface RoleplayTemplateOption {
  id: string;
  name: string;
  description?: string | null;
  isBuiltIn: boolean;
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
  /**
   * Roleplay template for the new chat (v4 `4bbeab47`). Seeded with whatever
   * the chat would have defaulted to (project default > user/global default)
   * and sent verbatim at create time — `null` means "no template".
   */
  roleplayTemplateId: string | null;
  /**
   * Set once the user picks a template by hand. Reference-data reloads (adding
   * a character, switching projects) re-seed the default only while this is
   * false, so an explicit choice is never quietly overwritten.
   */
  roleplayTemplateTouched: boolean;
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
  roleplayTemplateId: null,
  roleplayTemplateTouched: false,
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
