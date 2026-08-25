import { describe, expect, it } from 'vitest';

import {
  CUSTOM_SCENARIO_VALUE,
  GENERAL_SCENARIO_PREFIX,
  GROUP_SCENARIO_PREFIX,
  PROJECT_SCENARIO_PREFIX,
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
