/**
 * Scenario picker — shared option shapes and dropdown tokens (v4
 * `components/scenario/types.ts`, NEW at `44a8137e`).
 *
 * Two surfaces offer the same four tiers of scenario — plus anything plucked
 * from elsewhere through the scenario browser (v4 `545e299bb`): the New Chat
 * dialog and the in-chat picker in the Salon sidebar. Both render {@link ScenarioSelect},
 * so the option shapes, the `<option value>` tokens, and the selection type
 * live here rather than in either surface's own module.
 *
 * v4 moved these out of `components/new-chat/types.ts` and re-exported them
 * from there so the New Chat dialog stays a single import; v5 mirrors that —
 * `screens/new-chat/new-chat.types.ts` re-exports this module.
 */

import type { ScenarioRef } from '../core/core-contract';

/** A scenario stored on the character record itself, addressed by UUID. */
export interface CharacterScenario {
  id: string;
  title: string;
  content: string;
  description?: string;
  /** Hidden from the picker unless the surface asked for archived entries. */
  archived?: boolean;
}

/**
 * Project-scoped scenario sourced from the project's `Scenarios/` folder
 * (`/api/v1/projects/[id]/scenarios`). Identified by relativePath rather
 * than UUID, since the file system is the source of truth.
 */
export interface ProjectScenarioOption {
  path: string;
  filename: string;
  name: string;
  description?: string;
  isDefault: boolean;
  /** Hidden from the picker unless the surface asked for archived entries. */
  archived?: boolean;
  body: string;
}

/**
 * Instance-wide scenario sourced from the "Quilltap General" mount's
 * `Scenarios/` folder (`/api/v1/scenarios`). Same shape as a project
 * scenario, but applies to every non-help chat regardless of project.
 */
export interface GeneralScenarioOption {
  path: string;
  filename: string;
  name: string;
  description?: string;
  isDefault: boolean;
  /** Hidden from the picker unless the surface asked for archived entries. */
  archived?: boolean;
  body: string;
}

/**
 * Group-scoped scenario sourced from a group's `Scenarios/` folder
 * (`/api/v1/groups/scenarios`). Same shape as project and general
 * scenarios, but applies when specific participant character IDs
 * belong to the group.
 */
export interface GroupScenarioOption {
  groupId: string;
  groupName: string;
  path: string;
  filename: string;
  name: string;
  description?: string;
  isDefault: boolean;
  /** Hidden from the picker unless the surface asked for archived entries. */
  archived?: boolean;
  body: string;
}

/**
 * Suffix appended to a dropdown option for an archived entry (v4 `d25dacc1`). A
 * native `<option>` cannot hold a badge element, so the marker has to be text.
 */
export const ARCHIVED_OPTION_SUFFIX = ' (archived)';

export const CUSTOM_SCENARIO_VALUE = '__custom__';
/**
 * Stable token used in the dropdown's `<option value>` to identify a project
 * scenario. Format: `project:<relativePath>`. Character scenarios continue
 * to use their UUID; "Custom" uses CUSTOM_SCENARIO_VALUE.
 */
export const PROJECT_SCENARIO_PREFIX = 'project:';
/**
 * Stable token for general scenarios in the dropdown. Format:
 * `general:<relativePath>`.
 */
export const GENERAL_SCENARIO_PREFIX = 'general:';
/**
 * Stable token for group scenarios in the dropdown. Format:
 * `group:<groupId>:<relativePath>`.
 */
export const GROUP_SCENARIO_PREFIX = 'group:';
/**
 * Stable token for a scenario plucked from outside the chat's hierarchy
 * through the scenario browser. Format: `store:<mountPointId>:<relativePath>`.
 */
export const STORE_SCENARIO_PREFIX = 'store:';
/**
 * The dropdown's "Browse all scenarios…" entry. Never a selection: choosing it
 * opens the scenario browser and leaves the current selection standing.
 */
export const BROWSE_SCENARIO_VALUE = '__browse__';

/** The universal pointer to one scenario file, wherever it lives (schema of record in the contract). */
export type { ScenarioRef };

/**
 * Which tier a picker is currently pointed at. `custom` means no preset —
 * whatever free text the surface collects is the whole scenario.
 */
export type ScenarioSelection =
  | { kind: 'custom' }
  | { kind: 'character'; scenarioId: string }
  | { kind: 'project'; path: string }
  | { kind: 'general'; path: string }
  | { kind: 'group'; groupId: string; path: string }
  /**
   * A scenario from outside the hierarchy, picked in the browser. The label
   * (`Owner › Name`) travels in the selection so the dropdown can show it
   * without a second fetch.
   */
  | { kind: 'store'; mountPointId: string; path: string; label: string };

/** The `<option value>` token for a selection. */
export function scenarioSelectionToValue(selection: ScenarioSelection): string {
  switch (selection.kind) {
    case 'project':
      return `${PROJECT_SCENARIO_PREFIX}${selection.path}`;
    case 'general':
      return `${GENERAL_SCENARIO_PREFIX}${selection.path}`;
    case 'group':
      return `${GROUP_SCENARIO_PREFIX}${selection.groupId}:${selection.path}`;
    case 'store':
      return `${STORE_SCENARIO_PREFIX}${selection.mountPointId}:${selection.path}`;
    case 'character':
      return selection.scenarioId;
    case 'custom':
    default:
      return CUSTOM_SCENARIO_VALUE;
  }
}

/**
 * Parse a `<option value>` token back into a selection. Anything unrecognised
 * — including the empty string, a malformed group or store token, and the
 * browse entry (which is not a selection at all) — reads as custom, which is
 * the one tier that can never be wrong. A store token carries no label, so
 * `current` supplies it when the token names the selection already held.
 */
export function scenarioValueToSelection(
  value: string,
  current?: ScenarioSelection,
): ScenarioSelection {
  if (!value || value === CUSTOM_SCENARIO_VALUE || value === BROWSE_SCENARIO_VALUE) {
    return { kind: 'custom' };
  }
  if (value.startsWith(STORE_SCENARIO_PREFIX)) {
    const rest = value.slice(STORE_SCENARIO_PREFIX.length);
    const colonIdx = rest.indexOf(':');
    if (colonIdx < 1) return { kind: 'custom' };
    const mountPointId = rest.slice(0, colonIdx);
    const path = rest.slice(colonIdx + 1);
    const label =
      current?.kind === 'store' && current.mountPointId === mountPointId && current.path === path
        ? current.label
        : path.replace(/^Scenarios\//, '').replace(/\.md$/i, '');
    return { kind: 'store', mountPointId, path, label };
  }
  if (value.startsWith(PROJECT_SCENARIO_PREFIX)) {
    return { kind: 'project', path: value.slice(PROJECT_SCENARIO_PREFIX.length) };
  }
  if (value.startsWith(GENERAL_SCENARIO_PREFIX)) {
    return { kind: 'general', path: value.slice(GENERAL_SCENARIO_PREFIX.length) };
  }
  if (value.startsWith(GROUP_SCENARIO_PREFIX)) {
    const rest = value.slice(GROUP_SCENARIO_PREFIX.length);
    const colonIdx = rest.indexOf(':');
    if (colonIdx > -1) {
      return {
        kind: 'group',
        groupId: rest.slice(0, colonIdx),
        path: rest.slice(colonIdx + 1),
      };
    }
    return { kind: 'custom' };
  }
  return { kind: 'character', scenarioId: value };
}

/** The scenario-source fields a selection maps to (v4 `scenarioSelectionToPayload`). */
export interface ScenarioSelectionPayload {
  scenarioRef?: ScenarioRef;
  scenarioId?: string;
  projectScenarioPath?: string;
  generalScenarioPath?: string;
  groupScenarioPath?: string;
  groupScenarioGroupId?: string;
}

/**
 * The API payload fields a selection maps to. Both `chatCreate` and
 * `chatSetScenario` accept these names (v4: `POST /api/v1/chats` and
 * `POST /api/v1/chats/[id]?action=scenario`), and the server resolves them
 * through the same precedence chain.
 */
export function scenarioSelectionToPayload(
  selection: ScenarioSelection,
): ScenarioSelectionPayload {
  switch (selection.kind) {
    case 'character':
      return { scenarioId: selection.scenarioId };
    case 'project':
      return { projectScenarioPath: selection.path };
    case 'general':
      return { generalScenarioPath: selection.path };
    case 'group':
      return { groupScenarioPath: selection.path, groupScenarioGroupId: selection.groupId };
    case 'store':
      return { scenarioRef: { mountPointId: selection.mountPointId, path: selection.path } };
    case 'custom':
    default:
      return {};
  }
}
