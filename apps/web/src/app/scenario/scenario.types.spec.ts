import { describe, expect, it } from 'vitest';

import {
  BROWSE_SCENARIO_VALUE,
  CUSTOM_SCENARIO_VALUE,
  GENERAL_SCENARIO_PREFIX,
  GROUP_SCENARIO_PREFIX,
  PROJECT_SCENARIO_PREFIX,
  STORE_SCENARIO_PREFIX,
  scenarioSelectionToPayload,
  scenarioSelectionToValue,
  scenarioValueToSelection,
  type ScenarioSelection,
} from './scenario.types';

/**
 * Transcription parity for v4 `components/scenario/types.ts` at `44a8137e`
 * (the v4-client-oracle pattern: the spec IS the diff against v4's source).
 * Every arm below is read straight off v4's three exported functions.
 */
describe('scenario tokens (v4 components/scenario/types.ts @ 44a8137e)', () => {
  it('pins the four token literals', () => {
    expect(CUSTOM_SCENARIO_VALUE).toBe('__custom__');
    expect(PROJECT_SCENARIO_PREFIX).toBe('project:');
    expect(GENERAL_SCENARIO_PREFIX).toBe('general:');
    expect(GROUP_SCENARIO_PREFIX).toBe('group:');
  });

  describe('scenarioSelectionToValue', () => {
    it('renders each tier the way v4 does', () => {
      expect(scenarioSelectionToValue({ kind: 'custom' })).toBe('__custom__');
      expect(scenarioSelectionToValue({ kind: 'character', scenarioId: 'uuid-1' })).toBe('uuid-1');
      expect(scenarioSelectionToValue({ kind: 'project', path: 'Scenarios/a.md' })).toBe(
        'project:Scenarios/a.md',
      );
      expect(scenarioSelectionToValue({ kind: 'general', path: 'Scenarios/b.md' })).toBe(
        'general:Scenarios/b.md',
      );
      expect(
        scenarioSelectionToValue({ kind: 'group', groupId: 'g1', path: 'Scenarios/c.md' }),
      ).toBe('group:g1:Scenarios/c.md');
    });
  });

  describe('scenarioValueToSelection', () => {
    it('round-trips every tier', () => {
      const cases: ScenarioSelection[] = [
        { kind: 'custom' },
        { kind: 'character', scenarioId: 'uuid-1' },
        { kind: 'project', path: 'Scenarios/a.md' },
        { kind: 'general', path: 'Scenarios/b.md' },
        { kind: 'group', groupId: 'g1', path: 'Scenarios/c.md' },
      ];
      for (const selection of cases) {
        expect(scenarioValueToSelection(scenarioSelectionToValue(selection))).toEqual(selection);
      }
    });

    it('reads the empty string as custom (v4 `if (!value …)`)', () => {
      expect(scenarioValueToSelection('')).toEqual({ kind: 'custom' });
    });

    it('reads a MALFORMED group token as custom, not as a character UUID', () => {
      // v4 returns `{ kind: 'custom' }` from inside the group branch when the
      // token carries no second colon — it never falls through to the
      // character arm. This is the one arm a naive port gets wrong.
      expect(scenarioValueToSelection('group:no-colon-after-this')).toEqual({ kind: 'custom' });
    });

    it('keeps a group path that itself contains colons whole', () => {
      // v4 splits on the FIRST colon only (`indexOf`), so the remainder — colons
      // and all — is the path.
      expect(scenarioValueToSelection('group:g1:Scenarios/a:b.md')).toEqual({
        kind: 'group',
        groupId: 'g1',
        path: 'Scenarios/a:b.md',
      });
    });

    it('reads anything else as a character scenario id', () => {
      expect(scenarioValueToSelection('9f0c-not-a-prefix')).toEqual({
        kind: 'character',
        scenarioId: '9f0c-not-a-prefix',
      });
    });
  });

  describe('scenarioSelectionToPayload', () => {
    it('maps each tier to v4’s API field names, custom to nothing', () => {
      expect(scenarioSelectionToPayload({ kind: 'custom' })).toEqual({});
      expect(scenarioSelectionToPayload({ kind: 'character', scenarioId: 'uuid-1' })).toEqual({
        scenarioId: 'uuid-1',
      });
      expect(scenarioSelectionToPayload({ kind: 'project', path: 'p.md' })).toEqual({
        projectScenarioPath: 'p.md',
      });
      expect(scenarioSelectionToPayload({ kind: 'general', path: 'g.md' })).toEqual({
        generalScenarioPath: 'g.md',
      });
      expect(scenarioSelectionToPayload({ kind: 'group', groupId: 'g1', path: 'c.md' })).toEqual({
        groupScenarioPath: 'c.md',
        groupScenarioGroupId: 'g1',
      });
    });
  });
});

/**
 * v4 `components/scenario/__tests__/scenario-selection.test.tsx` at the pin
 * `83334b9a5` (NEW at `545e299bb`) — "scenario selection tokens" (5 cases,
 * the `it.each` native round trip already pinned above) and
 * "scenarioSelectionToPayload" (2 cases), transcribed case for case.
 */
describe('scenario selection tokens — the store kind (v4 545e299bb)', () => {
  const STORE: ScenarioSelection = {
    kind: 'store',
    mountPointId: 'mp-9',
    path: 'Scenarios/the-attic.md',
    label: 'Farhouse › The Attic',
  };

  it('pins the two new token literals', () => {
    expect(STORE_SCENARIO_PREFIX).toBe('store:');
    expect(BROWSE_SCENARIO_VALUE).toBe('__browse__');
  });

  it('round-trips a store selection, keeping its label through `current`', () => {
    const value = scenarioSelectionToValue(STORE);
    expect(value).toBe('store:mp-9:Scenarios/the-attic.md');
    expect(scenarioValueToSelection(value, STORE)).toEqual(STORE);
  });

  it('keeps colons in a store path (only the first colon splits)', () => {
    const odd: ScenarioSelection = { ...STORE, path: 'Scenarios/a:b.md', label: 'X › a:b' };
    expect(scenarioValueToSelection(scenarioSelectionToValue(odd), odd)).toEqual(odd);
  });

  it('derives a label from the path when `current` names a different selection', () => {
    expect(scenarioValueToSelection('store:mp-9:Scenarios/the-attic.md')).toEqual({
      kind: 'store',
      mountPointId: 'mp-9',
      path: 'Scenarios/the-attic.md',
      label: 'the-attic',
    });
    expect(
      scenarioValueToSelection('store:mp-9:Scenarios/the-attic.md', {
        ...STORE,
        mountPointId: 'mp-other',
      }),
    ).toMatchObject({ label: 'the-attic' });
  });

  it('reads a malformed store token as custom', () => {
    expect(scenarioValueToSelection('store:')).toEqual({ kind: 'custom' });
    expect(scenarioValueToSelection('store::Scenarios/x.md')).toEqual({ kind: 'custom' });
  });

  it('reads the browse entry as custom — it is never a selection', () => {
    expect(scenarioValueToSelection(BROWSE_SCENARIO_VALUE)).toEqual({ kind: 'custom' });
    expect(scenarioValueToSelection(BROWSE_SCENARIO_VALUE, STORE)).toEqual({ kind: 'custom' });
  });

  describe('scenarioSelectionToPayload', () => {
    it('maps a store selection to a scenarioRef and nothing else', () => {
      expect(scenarioSelectionToPayload(STORE)).toEqual({
        scenarioRef: { mountPointId: 'mp-9', path: 'Scenarios/the-attic.md' },
      });
    });

    it('maps each native kind to exactly its own field', () => {
      expect(scenarioSelectionToPayload({ kind: 'custom' })).toEqual({});
      expect(scenarioSelectionToPayload({ kind: 'character', scenarioId: 'sc-1' })).toEqual({
        scenarioId: 'sc-1',
      });
      expect(scenarioSelectionToPayload({ kind: 'project', path: 'p.md' })).toEqual({
        projectScenarioPath: 'p.md',
      });
      expect(scenarioSelectionToPayload({ kind: 'general', path: 'g.md' })).toEqual({
        generalScenarioPath: 'g.md',
      });
      expect(scenarioSelectionToPayload({ kind: 'group', groupId: 'g-1', path: 'c.md' })).toEqual({
        groupScenarioPath: 'c.md',
        groupScenarioGroupId: 'g-1',
      });
    });
  });
});
