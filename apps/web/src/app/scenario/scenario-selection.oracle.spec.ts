import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/scenario-selection.ndjson';
import type { EverywhereScenario, EverywhereSource } from '../core/core-contract';
import {
  defaultScenarioSelection,
  seedScenarioSelection,
  type ScenarioDefaults,
} from '../screens/new-chat/scenario-seed';
import {
  isInHierarchy,
  pickToSelection,
  scenarioMatchesSearch,
  type ScenarioHierarchy,
} from './scenario-browser.logic';
import {
  scenarioSelectionToPayload,
  scenarioSelectionToValue,
  scenarioValueToSelection,
  type ScenarioSelection,
} from './scenario.types';

/**
 * The corpus differential for v4 `545e299bb`'s pure scenario rules at the pin
 * `83334b9a5` (P4.D302): the selection tokens and payloads of
 * `components/scenario/types.ts`, New Chat's seed in
 * `components/new-chat/scenario-seed.ts`, and the browser's three helpers in
 * `components/scenario/ScenarioBrowserDialog.tsx`. Recorded by
 * `apps/web/oracle/scenario-selection.recorder.ts` against v4's REAL modules —
 * see that file's header for the pinned-worktree invocation.
 */

type Fn =
  | 'scenarioSelectionToValue'
  | 'scenarioSelectionToPayload'
  | 'scenarioValueToSelection'
  | 'roundTrip'
  | 'defaultScenarioSelection'
  | 'seedScenarioSelection'
  | 'isInHierarchy'
  | 'pickToSelection'
  | 'scenarioMatchesSearch';

interface Row {
  fn: Fn;
  id: string;
  args: unknown[];
  out: unknown;
}

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

/** The recorder's tag for an `undefined` value (JSON cannot carry one). */
function isUndefinedTag(v: unknown): boolean {
  return v !== null && typeof v === 'object' && '$undefined' in v;
}
function revive(v: unknown): unknown {
  if (isUndefinedTag(v)) return undefined;
  if (Array.isArray(v)) return v.map(revive);
  if (v !== null && typeof v === 'object') {
    const out: Record<string, unknown> = {};
    for (const [k, val] of Object.entries(v)) {
      // An absent default is recorded by OMITTING the key, so nothing to revive
      // inside objects beyond nested tags.
      out[k] = revive(val);
    }
    return out;
  }
  return v;
}

function replay(row: Row): unknown {
  const a = row.args.map(revive);
  switch (row.fn) {
    case 'scenarioSelectionToValue':
      return scenarioSelectionToValue(a[0] as ScenarioSelection);
    case 'scenarioSelectionToPayload':
      return scenarioSelectionToPayload(a[0] as ScenarioSelection);
    case 'scenarioValueToSelection':
      return a[1] === undefined
        ? scenarioValueToSelection(a[0] as string)
        : scenarioValueToSelection(a[0] as string, a[1] as ScenarioSelection);
    case 'roundTrip': {
      const selection = a[0] as ScenarioSelection;
      return scenarioValueToSelection(scenarioSelectionToValue(selection), selection);
    }
    case 'defaultScenarioSelection':
      return defaultScenarioSelection(a[0] as ScenarioDefaults);
    case 'seedScenarioSelection':
      return seedScenarioSelection(
        a[0] as { scenarioSelection: ScenarioSelection; scenarioTouched: boolean },
        a[1] as ScenarioDefaults,
      );
    case 'isInHierarchy':
      return isInHierarchy(a[0] as EverywhereSource, a[1] as ScenarioHierarchy);
    case 'pickToSelection':
      return pickToSelection(
        a[0] as EverywhereSource,
        a[1] as EverywhereScenario,
        a[2] as ScenarioHierarchy,
      );
    case 'scenarioMatchesSearch':
      return scenarioMatchesSearch(
        a[0] as EverywhereSource,
        a[1] as EverywhereScenario,
        a[2] as string,
      );
  }
}

describe("the scenario selection rules agree with v4's real modules row for row", () => {
  it('carries the whole recorded corpus', () => {
    // A truncated fixture would make the it.each below vacuously green.
    const count = (fn: Fn) => ROWS.filter((r) => r.fn === fn).length;
    expect(ROWS).toHaveLength(607);
    expect(count('scenarioSelectionToValue')).toBe(10);
    expect(count('scenarioSelectionToPayload')).toBe(10);
    expect(count('scenarioValueToSelection')).toBe(144);
    expect(count('roundTrip')).toBe(10);
    expect(count('defaultScenarioSelection')).toBe(64);
    expect(count('seedScenarioSelection')).toBe(256);
    expect(count('isInHierarchy')).toBe(27);
    expect(count('pickToSelection')).toBe(54);
    expect(count('scenarioMatchesSearch')).toBe(32);
  });

  it.each(ROWS)('$fn $id', (row) => {
    expect(replay(row)).toEqual(revive(row.out));
  });

  it('compares the store selections byte for byte, key order included', () => {
    // `toEqual` is key-order blind; the selection objects travel into option
    // values and payloads, so pin the serialisation too.
    for (const row of ROWS.filter((r) => r.fn === 'scenarioValueToSelection')) {
      expect(JSON.stringify(replay(row))).toBe(JSON.stringify(row.out));
    }
  });
});
