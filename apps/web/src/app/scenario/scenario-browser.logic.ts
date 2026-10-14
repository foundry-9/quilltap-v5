/**
 * The scenario browser's pure helpers (v4 `components/scenario/
 * ScenarioBrowserDialog.tsx` `:65-149`, NEW at `545e299bb`) — split out of the
 * dialog so the dropdown surfaces and the specs can read them without mounting
 * a component. Recorded against v4's REAL helpers by
 * `apps/web/oracle/scenario-selection.recorder.ts` (`QT_RECORD_BROWSER=1`).
 *
 * A pick that lies inside the chat's own hierarchy snaps to its native kind, so
 * the dropdown shows it under its usual heading; anything else becomes a
 * `store` selection labelled `Owner › Name`, which the dropdown shows under
 * "Elsewhere" and the server resolves by ref.
 */

import type { EverywhereScenario, EverywhereSource } from '../core/core-contract';
import type { ScenarioSelection } from './scenario.types';

export type ScenarioOwnerScope = EverywhereSource['scope'];

/** The owners the chat's picker already offers without browsing (v4 `ScenarioHierarchy`). */
export interface ScenarioHierarchy {
  projectId?: string | null;
  groupIds?: readonly string[];
  /** The single LLM character, when the room holds exactly one. */
  characterId?: string | null;
}

/** The owner sections of the browser's nav, in v4's order. */
export const SCOPE_SECTIONS: ReadonlyArray<{ scope: ScenarioOwnerScope; label: string }> = [
  { scope: 'general', label: 'General' },
  { scope: 'project', label: 'Projects' },
  { scope: 'group', label: 'Groups' },
  { scope: 'character', label: 'Characters' },
];

/** One owner's identity in the nav (v4 `sourceKey`). */
export const sourceKey = (s: Pick<EverywhereSource, 'scope' | 'id'>): string =>
  `${s.scope}:${s.id ?? ''}`;

/** One row's identity in the results list (v4 `rowKey`). */
export const rowKey = (mountPointId: string, path: string): string => `${mountPointId}|${path}`;

/** True when the chat's dropdown already offers this owner's shelf. */
export function isInHierarchy(
  source: Pick<EverywhereSource, 'scope' | 'id'>,
  hierarchy: ScenarioHierarchy,
): boolean {
  switch (source.scope) {
    case 'general':
      return true;
    case 'project':
      return !!source.id && source.id === hierarchy.projectId;
    case 'group':
      return !!source.id && (hierarchy.groupIds ?? []).includes(source.id);
    case 'character':
      return !!source.id && source.id === hierarchy.characterId;
    default:
      return false;
  }
}

/**
 * Turn a browser pick into the selection the dropdown holds: the native kind
 * when the pick is in the chat's hierarchy, a `store` selection otherwise.
 */
export function pickToSelection(
  source: EverywhereSource,
  scenario: EverywhereScenario,
  hierarchy: ScenarioHierarchy,
): ScenarioSelection {
  if (isInHierarchy(source, hierarchy)) {
    if (source.scope === 'general') return { kind: 'general', path: scenario.path };
    if (source.scope === 'project') return { kind: 'project', path: scenario.path };
    if (source.scope === 'group' && source.id) {
      return { kind: 'group', groupId: source.id, path: scenario.path };
    }
    if (source.scope === 'character' && scenario.scenarioId) {
      return { kind: 'character', scenarioId: scenario.scenarioId };
    }
  }
  return {
    kind: 'store',
    mountPointId: source.mountPointId,
    path: scenario.path,
    label: `${source.name} › ${scenario.name}`,
  };
}

/** Case-insensitive match on name, description, excerpt and owner name. */
export function scenarioMatchesSearch(
  source: EverywhereSource,
  scenario: EverywhereScenario,
  query: string,
): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return [scenario.name, scenario.description ?? '', scenario.excerpt, source.name].some((field) =>
    field.toLowerCase().includes(q),
  );
}
