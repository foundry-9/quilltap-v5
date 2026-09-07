import { describe, expect, it } from 'vitest';

import type { OptimizerSuggestion, SuggestionDecision } from '../detail-generators.api';
import {
  buildOptimizerApplyPlan,
  getAcceptedChanges,
  mergeOptimizerApplyPlan,
  type MergeableCharacter,
} from './apply-plan-builder';

/**
 * Parity spec — v4 `useCharacterOptimizer.ts:285-436` (`getAcceptedChanges` +
 * `applyChanges`'s field-routing/merge logic), driven against a hand-built
 * suggestion set covering every `field` branch v4's routing switch handles.
 */
function suggestion(overrides: Partial<OptimizerSuggestion> & { id: string; field: string }): OptimizerSuggestion {
  return {
    currentValue: '',
    proposedValue: '',
    rationale: '',
    significance: 0.5,
    memoryExcerpts: [],
    ...overrides,
  };
}

describe('getAcceptedChanges', () => {
  const suggestions: OptimizerSuggestion[] = [
    suggestion({ id: 's1', field: 'identity', proposedValue: 'A' }),
    suggestion({ id: 's2', field: 'description', proposedValue: 'B' }),
    suggestion({ id: 's3', field: 'manifesto', proposedValue: 'C' }),
  ];

  it('includes only accepted and edited suggestions, in original order', () => {
    const decisions = new Map<string, SuggestionDecision>([
      ['s1', 'accepted'],
      ['s2', 'rejected'],
      ['s3', 'edited'],
    ]);
    const edited = new Map([['s3', 'C-amended']]);
    const accepted = getAcceptedChanges(suggestions, decisions, edited);
    expect(accepted.map((a) => a.suggestion.id)).toEqual(['s1', 's3']);
    expect(accepted.find((a) => a.suggestion.id === 's1')?.finalValue).toBe('A');
    expect(accepted.find((a) => a.suggestion.id === 's3')?.finalValue).toBe('C-amended');
  });

  it('an edited decision with no recorded edit value falls back to proposedValue', () => {
    const decisions = new Map<string, SuggestionDecision>([['s1', 'edited']]);
    const accepted = getAcceptedChanges(suggestions.slice(0, 1), decisions, new Map());
    expect(accepted[0].finalValue).toBe('A');
  });

  it('no decisions ⇒ nothing accepted', () => {
    expect(getAcceptedChanges(suggestions, new Map(), new Map())).toHaveLength(0);
  });
});

describe('buildOptimizerApplyPlan — field routing', () => {
  it('simple fields map 1:1 onto updatePayload', () => {
    const plan = buildOptimizerApplyPlan([
      { suggestion: suggestion({ id: 's1', field: 'identity' }), finalValue: 'new identity' },
      { suggestion: suggestion({ id: 's2', field: 'manifesto' }), finalValue: 'new manifesto' },
    ]);
    expect(plan.updatePayload).toEqual({ identity: 'new identity', manifesto: 'new manifesto' });
  });

  it('talkativeness parses and clamps to [0.1, 1]', () => {
    const over = buildOptimizerApplyPlan([
      { suggestion: suggestion({ id: 's1', field: 'talkativeness' }), finalValue: '2.5' },
    ]);
    expect(over.updatePayload['talkativeness']).toBe(1);

    const under = buildOptimizerApplyPlan([
      { suggestion: suggestion({ id: 's1', field: 'talkativeness' }), finalValue: '0.01' },
    ]);
    expect(under.updatePayload['talkativeness']).toBe(0.1);

    const nonNumeric = buildOptimizerApplyPlan([
      { suggestion: suggestion({ id: 's1', field: 'talkativeness' }), finalValue: 'not a number' },
    ]);
    expect(nonNumeric.updatePayload['talkativeness']).toBeUndefined();
  });

  it('a scenario suggestion with a subId becomes a scenarioUpdate; without one it is DROPPED (v4: "new scenarios are no longer proposed")', () => {
    const withSub = buildOptimizerApplyPlan([
      { suggestion: suggestion({ id: 's1', field: 'scenarios', subId: 'scn-1' }), finalValue: 'new scene' },
    ]);
    expect(withSub.scenarioUpdates).toEqual([{ subId: 'scn-1', finalValue: 'new scene' }]);

    const withoutSub = buildOptimizerApplyPlan([
      { suggestion: suggestion({ id: 's1', field: 'scenarios' }), finalValue: 'new scene' },
    ]);
    expect(withoutSub.scenarioUpdates).toHaveLength(0);
  });

  it('a physicalDescription suggestion routes by its subId key, dropping an unrecognised key', () => {
    const valid = buildOptimizerApplyPlan([
      {
        suggestion: suggestion({ id: 's1', field: 'physicalDescription', subId: 'shortPrompt' }),
        finalValue: 'short desc',
      },
    ]);
    expect(valid.physicalUpdates).toEqual([{ key: 'shortPrompt', finalValue: 'short desc' }]);

    const invalidKey = buildOptimizerApplyPlan([
      {
        suggestion: suggestion({ id: 's1', field: 'physicalDescription', subId: 'notAKey' }),
        finalValue: 'x',
      },
    ]);
    expect(invalidKey.physicalUpdates).toHaveLength(0);
  });

  it('systemPrompt with a subId is a promptUpdate; without one it is a promptCreate named from name/title, else "Refined Prompt"', () => {
    const update = buildOptimizerApplyPlan([
      { suggestion: suggestion({ id: 's1', field: 'systemPrompt', subId: 'p1' }), finalValue: 'refined' },
    ]);
    expect(update.promptUpdates).toEqual([{ subId: 'p1', finalValue: 'refined' }]);

    const createNamed = buildOptimizerApplyPlan([
      { suggestion: suggestion({ id: 's1', field: 'systemPrompt', name: 'Combat Voice' }), finalValue: 'new prompt' },
    ]);
    expect(createNamed.promptCreates).toEqual([{ name: 'Combat Voice', finalValue: 'new prompt' }]);

    const createFallback = buildOptimizerApplyPlan([
      { suggestion: suggestion({ id: 's1', field: 'systemPrompt' }), finalValue: 'new prompt' },
    ]);
    expect(createFallback.promptCreates).toEqual([{ name: 'Refined Prompt', finalValue: 'new prompt' }]);
  });

  it('wardrobeItems with a subId is a wardrobeUpdate; a wardrobeItem payload with no subId is a wardrobeCreate; neither is dropped', () => {
    const update = buildOptimizerApplyPlan([
      { suggestion: suggestion({ id: 's1', field: 'wardrobeItems', subId: 'w1' }), finalValue: 'new desc' },
    ]);
    expect(update.wardrobeUpdates).toEqual([{ subId: 'w1', finalValue: 'new desc' }]);

    const wardrobeItem = {
      title: 'Cloak',
      description: 'A weathered travelling cloak',
      types: ['top'],
    };
    const create = buildOptimizerApplyPlan([
      { suggestion: suggestion({ id: 's1', field: 'wardrobeItems', wardrobeItem }), finalValue: 'x' },
    ]);
    expect(create.wardrobeCreates).toHaveLength(1);
    expect(create.wardrobeCreates[0].suggestion.wardrobeItem).toBe(wardrobeItem);

    const dropped = buildOptimizerApplyPlan([
      { suggestion: suggestion({ id: 's1', field: 'wardrobeItems' }), finalValue: 'x' },
    ]);
    expect(dropped.wardrobeUpdates).toHaveLength(0);
    expect(dropped.wardrobeCreates).toHaveLength(0);
  });

  it('aliases trims and collects; a blank alias is dropped', () => {
    const plan = buildOptimizerApplyPlan([
      { suggestion: suggestion({ id: 's1', field: 'aliases' }), finalValue: '  Nightshade  ' },
      { suggestion: suggestion({ id: 's2', field: 'aliases' }), finalValue: '   ' },
    ]);
    expect(plan.aliasAdditions).toEqual(['Nightshade']);
  });
});

describe('mergeOptimizerApplyPlan', () => {
  const character: MergeableCharacter = {
    aliases: ['Nyx'],
    scenarios: [
      { id: 'scn-1', title: 'Scene One', content: 'old content' },
      { id: 'scn-2', title: 'Scene Two', content: 'untouched' },
    ] as MergeableCharacter['scenarios'],
    physicalDescription: { name: 'Appearance', shortPrompt: 'old short' },
  };

  it('merges alias additions, skipping a case-insensitive duplicate', () => {
    const plan = buildOptimizerApplyPlan([
      { suggestion: suggestion({ id: 's1', field: 'aliases' }), finalValue: 'nyx' },
      { suggestion: suggestion({ id: 's2', field: 'aliases' }), finalValue: 'Shade' },
    ]);
    const merged = mergeOptimizerApplyPlan(plan, character);
    expect(merged['aliases']).toEqual(['Nyx', 'Shade']);
  });

  it('merges a scenario update into the FULL scenarios array, stamping updatedAt on the touched one only', () => {
    const plan = buildOptimizerApplyPlan([
      {
        suggestion: suggestion({ id: 's1', field: 'scenarios', subId: 'scn-1' }),
        finalValue: 'new content',
      },
    ]);
    const merged = mergeOptimizerApplyPlan(plan, character, '2026-01-01T00:00:00.000Z');
    const scenarios = merged['scenarios'] as Array<{ id: string; content: string; updatedAt?: string }>;
    expect(scenarios).toHaveLength(2);
    expect(scenarios.find((s) => s.id === 'scn-1')).toMatchObject({
      content: 'new content',
      updatedAt: '2026-01-01T00:00:00.000Z',
    });
    expect(scenarios.find((s) => s.id === 'scn-2')).toMatchObject({ content: 'untouched' });
    expect(scenarios.find((s) => s.id === 'scn-2')?.updatedAt).toBeUndefined();
  });

  it('merges physicalDescription sub-field keys into the FULL existing object, defaulting name when blank', () => {
    const plan = buildOptimizerApplyPlan([
      {
        suggestion: suggestion({ id: 's1', field: 'physicalDescription', subId: 'shortPrompt' }),
        finalValue: 'new short',
      },
    ]);
    const merged = mergeOptimizerApplyPlan(plan, character);
    expect(merged['physicalDescription']).toEqual({ name: 'Appearance', shortPrompt: 'new short' });
  });

  it('starts a fresh physicalDescription object (name: "Appearance") when the character has none yet', () => {
    const plan = buildOptimizerApplyPlan([
      {
        suggestion: suggestion({ id: 's1', field: 'physicalDescription', subId: 'longPrompt' }),
        finalValue: 'new long',
      },
    ]);
    const merged = mergeOptimizerApplyPlan(plan, { ...character, physicalDescription: null });
    expect(merged['physicalDescription']).toEqual({ name: 'Appearance', longPrompt: 'new long' });
  });

  it('mutation guard: an empty plan leaves updatePayload untouched (no accidental key writes)', () => {
    const plan = buildOptimizerApplyPlan([]);
    const merged = mergeOptimizerApplyPlan(plan, character);
    expect(merged).toEqual({});
  });
});
