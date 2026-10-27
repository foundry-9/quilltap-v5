/**
 * Pascal's Workbench — draft model round-trips.
 *
 * A CASE-FOR-CASE port of v4's own `__tests__/unit/lib/pascal/tool-draft.test.ts`
 * (408 lines): the same documents, the same assertions, the same normalizer. It
 * is the equivalence test for `tool-draft.ts` — a pure TS→TS port whose oracle is
 * the behavior v4's suite pins.
 *
 * The §6.2 invariant: load any valid definition into form mode, change nothing,
 * save — the only permissible diffs are key order, whitespace, and
 * default-elision; semantics identical (deep-equal after the schema parse). Plus
 * the `when` chip ⇄ JSON bijection across every subject and comparator, and the
 * unknown-key passthrough that keeps v2 files intact.
 */

import { describe, expect, it } from 'vitest';

import { safeParse, type WhenObject } from './custom-tool-types';
import {
  emptyDraftInventoryGesture,
  conditionSlotKey,
  conditionsFromWhen,
  defaultWardrobeVantage,
  definitionFromDraft,
  draftFromDefinition,
  emptyDraftWardrobeGesture,
  findParameterReferences,
  groupsOf,
  newDraft,
  newDraftEffect,
  renameParameterEverywhere,
  serializeDraft,
  gateConditionsFromGate,
  gateFromConditions,
  reachableOutcomesMissingBrief,
  slugFromTitle,
  validateDraft,
  whenFromConditions,
  type DraftCondition,
  type DraftGateCondition,
  type ToolDraft,
} from './tool-draft';

/**
 * Collapse the permissible diffs — `$schema` handling and default-elision — so
 * the remaining comparison is pure semantics. Deletes any optional key whose
 * value equals its documented default, at both the top level and inside a
 * range-form `roll`.
 */
function normalize(parsed: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = { ...parsed };
  delete out['$schema'];
  if (out['disabled'] === false) delete out['disabled'];
  if (out['revealOdds'] === true) delete out['revealOdds'];
  if (out['defaultVisibility'] === 'public') delete out['defaultVisibility'];
  if (out['announceAsCharacter'] === false) delete out['announceAsCharacter'];
  if (out['parameters'] && Object.keys(out['parameters'] as object).length === 0) {
    delete out['parameters'];
  }
  if (typeof out['roll'] === 'object' && out['roll'] !== null) {
    const roll: Record<string, unknown> = { ...(out['roll'] as Record<string, unknown>) };
    if (roll['min'] === 0) delete roll['min'];
    if (roll['max'] === 1) delete roll['max'];
    if (roll['multiplier'] === 1) delete roll['multiplier'];
    if (roll['offset'] === 0) delete roll['offset'];
    if (roll['round'] === false) delete roll['round'];
    if (Object.keys(roll).length === 0) delete out['roll'];
    else out['roll'] = roll;
  }
  return out;
}

/** Load → serialize → parse both sides and demand identical semantics. */
function expectRoundTrip(document: Record<string, unknown>): Record<string, unknown> {
  const draft = draftFromDefinition(document);
  expect(draft).not.toBeNull();

  const emitted = definitionFromDraft(draft!);

  const before = safeParse(document);
  const after = safeParse(emitted);
  expect(before.success).toBe(true);
  expect(after.success).toBe(true);
  expect(
    normalize((after as { success: true; data: unknown }).data as Record<string, unknown>),
  ).toEqual(normalize((before as { success: true; data: unknown }).data as Record<string, unknown>));
  return emitted;
}

describe('draft round-trip (§6.2 invariant)', () => {
  it('round-trips a minimal definition', () => {
    const emitted = expectRoundTrip({
      name: 'unlock',
      description: 'Attempt to pick the lock.',
      outcomes: [{ when: true, message: 'The lock resists.', state: 'info' }],
    });

    // Canonical emission: $schema first, no bloat added.
    expect(Object.keys(emitted)).toEqual(['$schema', 'name', 'description', 'outcomes']);
  });

  it('round-trips the full surface: params, transforms, every subject', () => {
    expectRoundTrip({
      $schema: '/schemas/qtap-custom-tool.schema.json',
      name: 'measure',
      title: 'Measure the Field',
      description: 'Take a reading.',
      disabled: true,
      revealOdds: false,
      defaultVisibility: 'whisper',
      announceAsCharacter: true,
      parameters: {
        baseline: { type: 'number', default: 0, description: 'Floor.', min: -10, max: 10 },
        scale: { type: 'integer', default: 2 },
        material: { type: 'string', default: 'brass' },
        blessed: { type: 'boolean', default: false },
      },
      roll: {
        min: { $param: 'baseline' },
        max: 100,
        multiplier: 2,
        offset: { $param: 'scale' },
        round: true,
      },
      outcomes: [
        {
          when: {
            gte: 10,
            lte: 90,
            roll: { lt: 0.9 },
            params: {
              material: { eq: 'brass', ncontains: 'iron' },
              scale: { gt: { $param: 'baseline' } },
            },
            metadata: {
              'has ansible access': { eq: true },
              clearanceLevel: { gte: { $param: 'scale' } },
            },
          },
          message: 'Reading: {{value}} ({{params.material}})',
          state: 'success',
        },
        { when: { neq: 0 }, message: 'Odd reading {{roll}}.', state: 'partial' },
        { when: true, message: 'Nothing.', state: 'failure' },
      ],
    });
  });

  it('round-trips a dice definition', () => {
    expectRoundTrip({
      name: 'saving_throw',
      description: 'Roll a d20 saving throw.',
      roll: '1d20',
      outcomes: [
        { when: { gte: 12 }, message: 'Saved! ({{dice}})', state: 'success' },
        { when: true, message: 'Failed. ({{dice}})', state: 'failure' },
      ],
    });
  });

  it('elides defaults a hand-written file left implicit', () => {
    const emitted = expectRoundTrip({
      name: 'plain',
      description: 'A plain tool.',
      disabled: false,
      revealOdds: true,
      defaultVisibility: 'public',
      announceAsCharacter: false,
      roll: { min: 0, max: 1, multiplier: 1, offset: 0, round: false },
      outcomes: [{ when: true, message: 'Done.', state: 'info' }],
    });

    expect(emitted['disabled']).toBeUndefined();
    expect(emitted['revealOdds']).toBeUndefined();
    expect(emitted['defaultVisibility']).toBeUndefined();
    expect(emitted['announceAsCharacter']).toBeUndefined();
    expect(emitted['roll']).toBeUndefined();
  });

  it('passes unknown top-level keys through untouched, after known keys', () => {
    const emitted = expectRoundTrip({
      name: 'ratchet',
      persist: { baseline: '{{value}}' },
      futureKey: [1, 2, 3],
      description: 'A v2 file.',
      outcomes: [{ when: true, message: 'Done.', state: 'info' }],
    });

    expect(emitted['persist']).toEqual({ baseline: '{{value}}' });
    expect(emitted['futureKey']).toEqual([1, 2, 3]);
    const keys = Object.keys(emitted);
    expect(keys.indexOf('persist')).toBeGreaterThan(keys.indexOf('outcomes'));
    // Original relative order of unknown keys is preserved.
    expect(keys.indexOf('persist')).toBeLessThan(keys.indexOf('futureKey'));
  });

  it('preserves a non-default $schema value and inserts one when absent', () => {
    const custom = expectRoundTrip({
      $schema: 'https://example.com/custom.schema.json',
      name: 'remote',
      description: 'Points elsewhere.',
      outcomes: [{ when: true, message: 'Done.', state: 'info' }],
    });
    expect(custom['$schema']).toBe('https://example.com/custom.schema.json');

    const inserted = expectRoundTrip({
      name: 'bare',
      description: 'No $schema.',
      outcomes: [{ when: true, message: 'Done.', state: 'info' }],
    });
    expect(inserted['$schema']).toBe('/schemas/qtap-custom-tool.schema.json');
  });

  it('emits 2-space indent with a trailing newline', () => {
    const draft = draftFromDefinition({
      name: 'plain',
      description: 'A plain tool.',
      outcomes: [{ when: true, message: 'Done.', state: 'info' }],
    });
    const text = serializeDraft(draft!);
    expect(text.endsWith('}\n')).toBe(true);
    expect(text).toContain('\n  "name": "plain"');
  });

  it('refuses an invalid document', () => {
    expect(draftFromDefinition({ name: 'broken' })).toBeNull();
    expect(draftFromDefinition('not even an object')).toBeNull();
  });
});

describe('when chips ⇄ JSON bijection', () => {
  function expectWhenRoundTrip(when: WhenObject): DraftCondition[] {
    const chips = conditionsFromWhen(when);
    expect(whenFromConditions(chips)).toEqual(when);
    return chips;
  }

  it('covers every comparator on the value subject', () => {
    const chips = expectWhenRoundTrip({ gt: 1, gte: 2, lt: 9, lte: 8, eq: 5, neq: 4 });
    expect(chips).toHaveLength(6);
    expect(chips.map((c) => c.comparator)).toEqual(['gt', 'gte', 'lt', 'lte', 'eq', 'neq']);
    expect(chips.every((c) => c.subject.kind === 'value')).toBe(true);
  });

  it('covers the raw-roll subject and $param operands', () => {
    const chips = expectWhenRoundTrip({ roll: { lte: { $param: 'fumble_under' }, gt: 0 } });
    const paramOperand = chips.find((c) => c.operand.kind === 'param');
    expect(paramOperand?.subject).toEqual({ kind: 'roll' });
    expect(paramOperand?.operand).toEqual({ kind: 'param', name: 'fumble_under' });
  });

  it('covers param subjects with string, boolean, and number literals', () => {
    expectWhenRoundTrip({
      params: {
        material: { eq: 'brass', neq: 'iron' },
        blessed: { eq: true },
        scale: { gte: 3 },
      },
    });
  });

  it('covers metadata subjects with exotic keys and every operand shape', () => {
    expectWhenRoundTrip({
      metadata: {
        'keys with spaces': { eq: 'yes' },
        'dotted.key.name': { neq: false },
        'ünïcode✓': { gte: { $param: 'threshold' } },
        plain: { lt: 12 },
      },
    });
  });

  it('covers containment on param, metadata, and llm subjects', () => {
    const chips = expectWhenRoundTrip({
      params: { material: { contains: 'ras', ncontains: 'iron' } },
      metadata: { faction: { contains: 'Aurum' } },
      llm: { ncontains: { $param: 'forbidden' } },
    } as WhenObject);
    expect(chips.map((c) => c.comparator)).toEqual([
      'contains',
      'ncontains',
      'contains',
      'ncontains',
    ]);
    expect(chips[3].operand).toEqual({ kind: 'param', name: 'forbidden' });
  });

  it('merges a band on one subject into one comparator object', () => {
    const chips: DraftCondition[] = [
      {
        id: 'a',
        subject: { kind: 'value' },
        comparator: 'gte',
        operand: { kind: 'number', text: '0.3' },
      },
      {
        id: 'b',
        subject: { kind: 'value' },
        comparator: 'lte',
        operand: { kind: 'number', text: '0.6' },
      },
    ];
    expect(whenFromConditions(chips)).toEqual({ gte: 0.3, lte: 0.6 });
  });

  it('keeps metadata identity per key: two keys with the same comparator coexist', () => {
    const when: WhenObject = {
      metadata: { strength: { gte: 10 }, cunning: { gte: 4 } },
    };
    const chips = conditionsFromWhen(when);
    expect(chips).toHaveLength(2);
    expect(whenFromConditions(chips)).toEqual(when);
  });
});

describe('$state references round-trip (v4 f48f34dc)', () => {
  it('round-trips a $state roll field and transform, carrying it verbatim', () => {
    const draft = draftFromDefinition({
      name: 'draw',
      description: 'x',
      roll: {
        min: { $state: 'game.low', fallback: 0 },
        max: { $state: 'game.high', fallback: 6 },
        multiplier: { $state: 'game.mult', fallback: 2 },
      },
      outcomes: [{ when: true, message: 'fall', state: 'info' }],
    })!;
    // The builder does not author `$state`, but the draft carries it as a
    // `state` kind (fallback as text), and re-emits it byte-identically.
    expect(draft.rollRange.min).toEqual({ kind: 'state', path: 'game.low', fallback: '0' });
    expect(draft.rollRange.multiplier).toEqual({ kind: 'state', path: 'game.mult', fallback: '2' });
    expectRoundTrip({
      name: 'draw',
      description: 'x',
      roll: {
        min: { $state: 'game.low', fallback: 0 },
        max: { $state: 'game.high', fallback: 6 },
        multiplier: { $state: 'game.mult', fallback: 2 },
      },
      outcomes: [{ when: true, message: 'fall', state: 'info' }],
    });
  });

  it('round-trips $state comparator operands (ordering, eq, containment)', () => {
    const draft = draftFromDefinition({
      name: 'gate',
      description: 'x',
      parameters: { material: { type: 'string', default: 'brass' } },
      outcomes: [
        { when: { gte: { $state: 'game.difficulty', fallback: 3 } }, message: 'a', state: 'success' },
        {
          when: { params: { material: { contains: { $state: 'clue.needle', fallback: 'ras' } } } },
          message: 'b',
          state: 'partial',
        },
        { when: true, message: 'fall', state: 'info' },
      ],
    })!;
    const stateChips = draft.outcomes.flatMap((o) => o.conditions).filter((c) => c.operand.kind === 'state');
    expect(stateChips).toHaveLength(2);
    expect(stateChips[0].operand).toEqual({ kind: 'state', path: 'game.difficulty', fallback: 3 });
    expect(stateChips[1].operand).toEqual({ kind: 'state', path: 'clue.needle', fallback: 'ras' });
    expectRoundTrip({
      name: 'gate',
      description: 'x',
      parameters: { material: { type: 'string', default: 'brass' } },
      outcomes: [
        { when: { gte: { $state: 'game.difficulty', fallback: 3 } }, message: 'a', state: 'success' },
        {
          when: { params: { material: { contains: { $state: 'clue.needle', fallback: 'ras' } } } },
          message: 'b',
          state: 'partial',
        },
        { when: true, message: 'fall', state: 'info' },
      ],
    });
  });

  it('round-trips a $state parameter default verbatim', () => {
    const draft = draftFromDefinition({
      name: 'draw',
      description: 'x',
      parameters: { bonus: { type: 'number', default: { $state: 'player.bonus', fallback: 1 } } },
      outcomes: [{ when: true, message: 'fall', state: 'info' }],
    })!;
    expect(draft.parameters[0].defaultValue).toEqual({ $state: 'player.bonus', fallback: 1 });
    expectRoundTrip({
      name: 'draw',
      description: 'x',
      parameters: { bonus: { type: 'number', default: { $state: 'player.bonus', fallback: 1 } } },
      outcomes: [{ when: true, message: 'fall', state: 'info' }],
    });
  });

  it('validateDraft rejects a $state roll field whose fallback is not a number', () => {
    // The visual builder never authors this — construct it directly to exercise
    // the load-time rule (`validateRollRefs`' `$state` arm, mirrored in the draft).
    const draft = newDraft();
    draft.rollForm = 'range';
    draft.rollRange = {
      ...draft.rollRange,
      min: { kind: 'state', path: 'game.low', fallback: 'nope' },
    };
    const issues = validateDraft(draft);
    expect(issues.some((i) => i.message === 'min $state fallback must be a number')).toBe(true);
  });

  it('validateDraft accepts a $state roll field whose fallback IS a number', () => {
    const draft = newDraft();
    draft.rollForm = 'range';
    draft.rollRange = {
      ...draft.rollRange,
      min: { kind: 'state', path: 'game.low', fallback: '2' },
    };
    const issues = validateDraft(draft);
    expect(issues.some((i) => i.message === 'min $state fallback must be a number')).toBe(false);
  });
});

describe('validateDraft', () => {
  it('accepts a well-formed loaded draft', () => {
    const draft = draftFromDefinition({
      name: 'fine',
      description: 'A fine tool.',
      outcomes: [
        { when: { gte: 0.5 }, message: 'High.', state: 'success' },
        { when: true, message: 'Low.', state: 'failure' },
      ],
    });
    expect(validateDraft(draft!).filter((i) => i.severity === 'error')).toEqual([]);
  });

  it('starts a new draft with the empty row in error state', () => {
    const issues = validateDraft(newDraft());
    expect(issues.some((i) => i.message.includes('must test something'))).toBe(true);
    expect(issues.some((i) => i.where.section === 'identity')).toBe(true);
  });

  /**
   * v4's `a progress condition key` block (`tool-draft.test.ts` at
   * `25f534c0b`). The Workbench is where the shared `parseProgressKey`'s
   * per-mistake messages actually reach a person: through `when.progress` they
   * are lost to Zod's own "Invalid key in record". So this is the surface worth
   * asserting the wording on.
   */
  describe('a progress condition key', () => {
    function draftWithProgressKey(key: string) {
      const draft = newDraft();
      draft.name = 'probe';
      draft.description = 'x';
      draft.outcomes[0].message = 'm';
      draft.outcomes[0].conditions = [
        {
          id: 'a',
          subject: { kind: 'progress', key },
          comparator: 'eq',
          operand: { kind: 'boolean', value: true },
        },
      ];
      return validateDraft(draft).filter((i) => i.severity === 'error');
    }

    it('accepts a well-formed one', () => {
      expect(draftWithProgressKey('cannon.complete')).toEqual([]);
    });

    it('says specifically that no field was named', () => {
      expect(draftWithProgressKey('cannon').some((i) => i.message.includes('names no field'))).toBe(
        true,
      );
    });

    it('says specifically that the id is not an id', () => {
      expect(
        draftWithProgressKey('Cannon.complete').some((i) =>
          i.message.includes('not a progression id'),
        ),
      ).toBe(true);
    });

    it('rejects a blank key', () => {
      expect(draftWithProgressKey('').length).toBeGreaterThan(0);
    });
  });

  it('blocks duplicate subject+comparator pairs but not distinct metadata keys', () => {
    const draft = newDraft();
    draft.name = 'dup';
    draft.description = 'x';
    draft.outcomes[0].message = 'm';
    draft.outcomes[0].conditions = [
      {
        id: 'a',
        subject: { kind: 'metadata', key: 'strength' },
        comparator: 'gte',
        operand: { kind: 'number', text: '1' },
      },
      {
        id: 'b',
        subject: { kind: 'metadata', key: 'cunning' },
        comparator: 'gte',
        operand: { kind: 'number', text: '1' },
      },
    ];
    expect(validateDraft(draft).filter((i) => i.severity === 'error')).toEqual([]);

    draft.outcomes[0].conditions.push({
      id: 'c',
      subject: { kind: 'metadata', key: 'strength' },
      comparator: 'gte',
      operand: { kind: 'number', text: '5' },
    });
    const issues = validateDraft(draft);
    expect(issues.some((i) => i.message.includes('only once'))).toBe(true);
  });

  it('warns on {{dice}} in range form without blocking', () => {
    const draft = draftFromDefinition({
      name: 'ranged',
      description: 'x',
      outcomes: [{ when: true, message: 'Rolled {{dice}}.', state: 'info' }],
    });
    const issues = validateDraft(draft!);
    const diceIssue = issues.find((i) => i.message.includes('{{dice}}'));
    expect(diceIssue?.severity).toBe('warning');
  });

  it('never flags {{metadata.*}} placeholders as unknown', () => {
    const draft = draftFromDefinition({
      name: 'meta',
      description: 'x',
      outcomes: [
        { when: true, message: 'Sheet says {{metadata.some key nobody declared}}.', state: 'info' },
      ],
    });
    expect(validateDraft(draft!)).toEqual([]);
  });

  it('reports a bare family prefix as an unknown placeholder, not a missing name', () => {
    // v4 `0506517d3` correction (e). Before the three audits collapsed onto
    // `classifyPlaceholder`, `{{params.}}` reached the params arm with an empty
    // name and was reported as "names no declared parameter" — a sentence that
    // sends the author looking for a parameter list they cannot fix. It names
    // nothing at all, so it is unknown.
    const draft = draftFromDefinition({
      name: 'bare',
      description: 'x',
      parameters: { bonus: { type: 'number', default: 0 } },
      outcomes: [{ when: true, message: 'bare {{params.}} suffix.', state: 'info' }],
    });
    const issues = validateDraft(draft!);
    expect(issues).toHaveLength(1);
    expect(issues[0].severity).toBe('warning');
    expect(issues[0].message).toBe(
      '{{params.}} is not a placeholder this build knows — it will render as written',
    );
  });

  it('reports a bare {{metadata.}}, which used to pass in silence', () => {
    // The metadata arm skipped on the PREFIX, so a bare `{{metadata.}}` — which
    // names no key and renders verbatim — was waved through. Classification
    // sends it to `unknown`, where the audit does speak.
    const draft = draftFromDefinition({
      name: 'bare-meta',
      description: 'x',
      outcomes: [{ when: true, message: 'bare {{metadata.}} suffix.', state: 'info' }],
    });
    const issues = validateDraft(draft!);
    expect(issues).toHaveLength(1);
    expect(issues[0].message).toBe(
      '{{metadata.}} is not a placeholder this build knows — it will render as written',
    );
  });

  it('warns on a typo placeholder but does not block', () => {
    const draft = draftFromDefinition({
      name: 'typo',
      description: 'x',
      parameters: { bonus: { type: 'number', default: 0 } },
      outcomes: [{ when: true, message: '{{params.bonsu}} and {{vlaue}}.', state: 'info' }],
    });
    const issues = validateDraft(draft!);
    expect(issues).toHaveLength(2);
    expect(issues.every((i) => i.severity === 'warning')).toBe(true);
  });

  it('audits containment chips: text subjects, text operands, nothing empty', () => {
    const draft = newDraft();
    draft.name = 'contain';
    draft.description = 'x';
    draft.parameters = [
      {
        id: 'p1',
        name: 'scale',
        type: 'number',
        defaultValue: '1',
        description: '',
        min: '',
        max: '',
      },
      {
        id: 'p2',
        name: 'material',
        type: 'string',
        defaultValue: 'brass',
        description: '',
        min: '',
        max: '',
      },
    ];
    draft.outcomes[0].message = 'm';

    const chip = (partial: Partial<DraftCondition>): DraftCondition[] => [
      {
        id: 'a',
        subject: { kind: 'param', name: 'material' },
        comparator: 'contains',
        operand: { kind: 'string', text: 'ras' },
        ...partial,
      },
    ];

    draft.outcomes[0].conditions = chip({});
    expect(validateDraft(draft).filter((i) => i.severity === 'error')).toEqual([]);

    draft.outcomes[0].conditions = chip({ subject: { kind: 'param', name: 'scale' } });
    expect(
      validateDraft(draft).some((i) => i.message.includes('only text can contain text')),
    ).toBe(true);

    draft.outcomes[0].conditions = chip({ operand: { kind: 'param', name: 'scale' } });
    expect(validateDraft(draft).some((i) => i.message.includes('needs text to look for'))).toBe(
      true,
    );

    draft.outcomes[0].conditions = chip({ operand: { kind: 'string', text: '' } });
    expect(validateDraft(draft).some((i) => i.message.includes('must not be empty'))).toBe(true);

    draft.outcomes[0].conditions = chip({ operand: { kind: 'number', text: '5' } });
    expect(validateDraft(draft).some((i) => i.message.includes('needs text to look for'))).toBe(
      true,
    );
  });

  it('rejects containment on whether the consult succeeded', () => {
    const draft = newDraft();
    draft.name = 'okc';
    draft.description = 'x';
    draft.llmEnabled = true;
    draft.llmPrompt = 'Speak.';
    draft.llmErrorMessage = 'Silence.';
    draft.outcomes[0].message = 'm';
    draft.outcomes[0].conditions = [
      {
        id: 'a',
        subject: { kind: 'llm-ok' },
        comparator: 'contains',
        operand: { kind: 'string', text: 'x' },
      },
    ];
    expect(validateDraft(draft).some((i) => i.message.includes('= or ≠'))).toBe(true);
  });

  it('flags a literal min above a literal max, but not a $param bound', () => {
    const draft = newDraft();
    draft.name = 'bounds';
    draft.description = 'x';
    draft.outcomes[0].conditions = [
      {
        id: 'a',
        subject: { kind: 'value' },
        comparator: 'gte',
        operand: { kind: 'number', text: '0' },
      },
    ];
    draft.outcomes[0].message = 'm';

    draft.rollRange.min = { kind: 'literal', text: '5' };
    draft.rollRange.max = { kind: 'literal', text: '2' };
    expect(validateDraft(draft).some((i) => i.message.includes('low bound'))).toBe(true);

    draft.parameters = [
      { id: 'p1', name: 'top', type: 'number', defaultValue: '9', description: '', min: '', max: '' },
    ];
    draft.rollRange.max = { kind: 'param', name: 'top' };
    expect(validateDraft(draft).some((i) => i.message.includes('low bound'))).toBe(false);
  });
});

describe('parameter rename and reference finding', () => {
  const document = {
    name: 'opposed',
    description: 'x',
    parameters: { difficulty: { type: 'number' as const, default: 10 } },
    roll: { offset: { $param: 'difficulty' } },
    outcomes: [
      {
        when: { gte: { $param: 'difficulty' }, params: { difficulty: { gt: 0 } } },
        message: 'Beat {{params.difficulty}}.',
        state: 'success' as const,
      },
      { when: true, message: 'Failed.', state: 'failure' as const },
    ],
  };

  it('finds every reference site for the deletion confirm', () => {
    const draft = draftFromDefinition(document)!;
    const sites = findParameterReferences(draft, 'difficulty');
    expect(sites.sort()).toEqual(
      [
        'roll offset',
        'outcome 1: a condition tests it',
        'outcome 1: a condition compares against it',
        'outcome 1: the message renders it',
      ].sort(),
    );
  });

  it('renames everywhere atomically and stays valid', () => {
    const draft = draftFromDefinition(document)!;
    const renamed = renameParameterEverywhere(draft, 'difficulty', 'target');

    const emitted = definitionFromDraft(renamed);
    const parsed = safeParse(emitted);
    expect(parsed.success).toBe(true);
    expect(JSON.stringify(emitted)).not.toContain('difficulty');
    expect((emitted['roll'] as { offset: unknown }).offset).toEqual({ $param: 'target' });
    expect((emitted['outcomes'] as Array<{ message: string }>)[0].message).toBe(
      'Beat {{params.target}}.',
    );
  });
});

describe('slugFromTitle', () => {
  it.each([
    ['Force the Lock', 'force_the_lock'],
    ["The Baron's Gambit", 'the_barons_gambit'],
    ['  3 Wishes!  ', 'wishes'],
    ['Scan — Hawking Radiation', 'scan_hawking_radiation'],
  ])('%s → %s', (title, expected) => {
    expect(slugFromTitle(title)).toBe(expected);
  });
});

/**
 * v4 `__tests__/unit/lib/pascal/tool-draft.test.ts:469-583` — the consult block
 * in the draft, ported case-for-case.
 */
describe('the llm block in the draft', () => {
  const ORACLE_TOOL = {
    name: 'augury',
    description: 'Consult the oracle.',
    llm: { prompt: 'YES or NO about {{value}}?', errorMessage: 'The wire went dead.' },
    outcomes: [
      { when: { llm: { ok: false } }, message: 'Silence: {{llm}}', state: 'failure' },
      { when: { llm: { eq: 'YES', ok: true } }, message: 'Assent.', state: 'success' },
      { when: true, message: 'Demurral: {{llm}}', state: 'info' },
    ],
  };

  it('round-trips a definition with an llm block and llm tests', () => {
    const emitted = expectRoundTrip(ORACLE_TOOL);
    expect(Object.keys(emitted)).toEqual(['$schema', 'name', 'description', 'llm', 'outcomes']);
  });

  it('loads the block into the draft fields', () => {
    const draft = draftFromDefinition(ORACLE_TOOL)!;
    expect(draft.llmEnabled).toBe(true);
    expect(draft.llmPrompt).toBe('YES or NO about {{value}}?');
    expect(draft.llmErrorMessage).toBe('The wire went dead.');
  });

  it('emits no llm key while the consult is disabled', () => {
    const draft = newDraft();
    draft.llmPrompt = 'kept but dormant';
    expect(definitionFromDraft(draft)['llm']).toBeUndefined();
  });

  it('round-trips maxOutput, and omits it while blank', () => {
    const capped = { ...ORACLE_TOOL, llm: { ...ORACLE_TOOL.llm, maxOutput: 50000 } };
    expectRoundTrip(capped);

    const draft = draftFromDefinition(capped)!;
    expect(draft.llmMaxOutput).toBe('50000');

    draft.llmMaxOutput = '';
    expect((definitionFromDraft(draft)['llm'] as { maxOutput?: number }).maxOutput).toBeUndefined();
  });

  it('rejects a nonsense answer cap while enabled', () => {
    const draft = draftFromDefinition(ORACLE_TOOL)!;
    draft.llmMaxOutput = '0';
    let messages = validateDraft(draft)
      .filter((i) => i.severity === 'error')
      .map((i) => i.message);
    expect(messages.some((m) => m.includes('whole number'))).toBe(true);

    draft.llmMaxOutput = '999999';
    messages = validateDraft(draft)
      .filter((i) => i.severity === 'error')
      .map((i) => i.message);
    expect(messages.some((m) => m.includes('tops out'))).toBe(true);
  });

  it('bijects ok and answer comparators through chips', () => {
    const when = { llm: { gte: 5, eq: 'YES', ok: false } } as WhenObject;
    const chips = conditionsFromWhen(when);
    // ok rides its own chip kind; the rest are ordinary comparator chips.
    expect(chips.map((c) => c.subject.kind)).toEqual(['llm-ok', 'llm', 'llm']);
    expect(whenFromConditions(chips)).toEqual(when);
  });

  it('serializes a "succeeded ≠ true" chip as ok: false', () => {
    const chips: DraftCondition[] = [
      {
        id: 'a',
        subject: { kind: 'llm-ok' },
        comparator: 'neq',
        operand: { kind: 'boolean', value: true },
      },
    ];
    expect(whenFromConditions(chips)).toEqual({ llm: { ok: false } });
  });

  describe('validation', () => {
    it('requires a prompt and an error message while enabled', () => {
      const draft = draftFromDefinition(ORACLE_TOOL)!;
      draft.llmPrompt = '  ';
      draft.llmErrorMessage = '';
      const messages = validateDraft(draft)
        .filter((i) => i.severity === 'error')
        .map((i) => i.message);
      expect(messages).toEqual(
        expect.arrayContaining([
          expect.stringContaining('needs a prompt'),
          expect.stringContaining('error message'),
        ]),
      );
    });

    it('flags llm chips when the consult is disabled', () => {
      const draft = draftFromDefinition(ORACLE_TOOL)!;
      draft.llmEnabled = false;
      const messages = validateDraft(draft)
        .filter((i) => i.severity === 'error')
        .map((i) => i.message);
      expect(messages.some((m) => m.includes('consult is not enabled'))).toBe(true);
    });

    it('warns on {{llm}} inside the prompt itself', () => {
      const draft = draftFromDefinition(ORACLE_TOOL)!;
      draft.llmPrompt = 'Echo {{llm}} back?';
      const warnings = validateDraft(draft)
        .filter((i) => i.severity === 'warning')
        .map((i) => i.message);
      expect(warnings.some((m) => m.includes('cannot quote its own answer'))).toBe(true);
    });

    it('warns on {{llm}} in a message while the consult is off', () => {
      const draft = newDraft();
      draft.name = 'plain';
      draft.description = 'd';
      draft.outcomes = [draft.outcomes[1]];
      draft.outcomes[0].message = 'Says {{llm}}.';
      const warnings = validateDraft(draft)
        .filter((i) => i.severity === 'warning')
        .map((i) => i.message);
      expect(warnings.some((m) => m.includes('unless the LLM consult is enabled'))).toBe(true);
    });
  });

  it('renames a parameter inside the consult prompt, and finds it as a reference', () => {
    const withParam = draftFromDefinition({
      ...ORACLE_TOOL,
      parameters: { omen: { type: 'string', default: 'sparrows' } },
      llm: { prompt: 'What of the {{params.omen}}?', errorMessage: 'Silence.' },
    })!;
    expect(findParameterReferences(withParam, 'omen')).toEqual(['the consult prompt renders it']);
    const renamed = renameParameterEverywhere(withParam, 'omen', 'portent');
    expect(renamed.llmPrompt).toBe('What of the {{params.portent}}?');
  });
});

/**
 * The availability gate in the draft (v4 `6864bf0e`, the +140 lines its
 * `tool-draft.test.ts` gained). Ported case for case, same documents, same
 * assertions.
 */
describe('the availability gate in the draft', () => {
  const GATED_TOOL = {
    name: 'reprogram',
    description: 'Rewrite the thing’s instructions.',
    availableWhen: {
      metadata: {
        toolAbilities: { contains: 'programmable' },
        rank: { gte: 3 },
        cleared: { eq: true },
      },
    },
    outcomes: [{ when: true, message: 'It obliges.', state: 'success' }],
  };

  it('round-trips an availableWhen gate', () => {
    expectRoundTrip(GATED_TOOL);
  });

  it('round-trips a withheldWhen gate', () => {
    const { availableWhen, ...rest } = GATED_TOOL;
    expectRoundTrip({ ...rest, withheldWhen: availableWhen });
  });

  it('loads the clause into the mode, and the tests into chips', () => {
    const draft = draftFromDefinition(GATED_TOOL)!;
    expect(draft.gateMode).toBe('available');
    expect(draft.gateConditions.map((c) => [c.key, c.comparator, c.operand])).toEqual([
      ['toolAbilities', 'contains', { kind: 'string', text: 'programmable' }],
      ['rank', 'gte', { kind: 'number', text: '3' }],
      ['cleared', 'eq', { kind: 'boolean', value: true }],
    ]);
  });

  it('emits neither key while the mode is none, whatever chips linger', () => {
    const draft = draftFromDefinition(GATED_TOOL)!;
    draft.gateMode = 'none';
    const emitted = definitionFromDraft(draft);
    expect(emitted['availableWhen']).toBeUndefined();
    expect(emitted['withheldWhen']).toBeUndefined();
    // The chips survive the flip, so switching back costs nothing.
    expect(draft.gateConditions).toHaveLength(3);
  });

  it('flips the clause without disturbing the tests', () => {
    const draft = draftFromDefinition(GATED_TOOL)!;
    draft.gateMode = 'withheld';
    const emitted = definitionFromDraft(draft);
    expect(emitted['availableWhen']).toBeUndefined();
    expect(emitted['withheldWhen']).toEqual(GATED_TOOL.availableWhen);
  });

  it('emits an ungated tool with no gate keys at all', () => {
    // v4 writes `{ ...GATED_TOOL, availableWhen: undefined }`. That relies on a
    // Zod property this hand port does not have and CANNOT be given without
    // diverging from the Rust half: Zod's `.optional()` reads a present-but-
    // `undefined` key as absent, while both v5 parsers key off own-property
    // presence. Unreachable from a file — `JSON.parse` never yields `undefined`
    // — so the case is expressed as what it means, an ungated document, and the
    // divergence is pinned below rather than papered over.
    const { availableWhen: _gate, ...ungated } = GATED_TOOL;
    const emitted = definitionFromDraft(draftFromDefinition(ungated)!);
    expect('availableWhen' in emitted).toBe(false);
    expect('withheldWhen' in emitted).toBe(false);
  });

  it('rejects a present-but-undefined clause, where Zod would read it as absent', () => {
    // The pinned divergence. Module-wide (every optional key behaves this way,
    // long before the gate existed) and unreachable from JSON bytes.
    expect(draftFromDefinition({ ...GATED_TOOL, availableWhen: undefined })).toBeNull();
  });

  it('refuses a definition carrying both clauses — the form models one', () => {
    expect(
      draftFromDefinition({
        ...GATED_TOOL,
        withheldWhen: { metadata: { novice: { eq: true } } },
      }),
    ).toBeNull();
  });

  describe('validateDraft', () => {
    const gated = (): ToolDraft => {
      const draft = newDraft();
      draft.name = 'reprogram';
      draft.description = 'd';
      draft.outcomes = [draft.outcomes[1]];
      draft.gateMode = 'available';
      return draft;
    };

    const gateErrors = (draft: ToolDraft): string[] =>
      validateDraft(draft)
        .filter((i) => i.severity === 'error' && i.where.section === 'gate')
        .map((i) => i.message);

    it('blocks a gate that tests nothing', () => {
      expect(gateErrors(gated()).some((m) => m.includes('must test something'))).toBe(true);
    });

    it('blocks a condition with no metadata key', () => {
      const draft = gated();
      draft.gateConditions = [
        { id: 'g1', subject: 'metadata', key: '  ', comparator: 'eq', operand: { kind: 'boolean', value: true } },
      ];
      expect(gateErrors(draft).some((m) => m.includes('needs a metadata key'))).toBe(true);
    });

    it('blocks ordering against text, and containment against a number', () => {
      const draft = gated();
      draft.gateConditions = [
        { id: 'g1', subject: 'metadata', key: 'rank', comparator: 'gte', operand: { kind: 'string', text: 'senior' } },
        { id: 'g2', subject: 'metadata', key: 'abilities', comparator: 'contains', operand: { kind: 'number', text: '7' } },
      ];
      const errors = gateErrors(draft);
      expect(errors.some((m) => m.includes('can only order numbers'))).toBe(true);
      expect(errors.some((m) => m.includes('needs text to look for'))).toBe(true);
    });

    it('blocks an empty substring and an unparseable number', () => {
      const draft = gated();
      draft.gateConditions = [
        { id: 'g1', subject: 'metadata', key: 'abilities', comparator: 'contains', operand: { kind: 'string', text: '' } },
        { id: 'g2', subject: 'metadata', key: 'rank', comparator: 'eq', operand: { kind: 'number', text: '' } },
      ];
      const errors = gateErrors(draft);
      expect(errors.some((m) => m.includes('must not be empty'))).toBe(true);
      expect(errors.some((m) => m.includes('needs a number'))).toBe(true);
    });

    it('blocks the same key tested twice with the same comparator', () => {
      const draft = gated();
      draft.gateConditions = [
        { id: 'g1', subject: 'metadata', key: 'rank', comparator: 'gte', operand: { kind: 'number', text: '3' } },
        { id: 'g2', subject: 'metadata', key: 'rank', comparator: 'gte', operand: { kind: 'number', text: '5' } },
      ];
      expect(gateErrors(draft).some((m) => m.includes('only once'))).toBe(true);
    });

    it('says nothing about a gate the author has switched off', () => {
      const draft = gated();
      draft.gateMode = 'none';
      draft.gateConditions = [
        { id: 'g1', subject: 'metadata', key: '', comparator: 'eq', operand: { kind: 'boolean', value: true } },
      ];
      expect(gateErrors(draft)).toEqual([]);
    });

    it('passes a complete gate', () => {
      const draft = gated();
      draft.gateConditions = [
        {
          id: 'g1',
          subject: 'metadata',
          key: 'toolAbilities',
          comparator: 'contains',
          operand: { kind: 'string', text: 'programmable' },
        },
      ];
      expect(gateErrors(draft)).toEqual([]);
      expect(safeParse(definitionFromDraft(draft)).success).toBe(true);
    });
  });
});

/**
 * v4 `c4d4b0de`'s two new suites, ported case-for-case: the chip label and the
 * side effects in the draft layer. Same documents, same assertions.
 */

describe('the chip label in the draft', () => {
  const BASE = {
    name: 'agent_lambda',
    description: 'Dispatch.',
    outcomes: [{ when: true, message: '-', state: 'info' }],
  };

  it('round-trips a chipLabel', () => {
    expectRoundTrip({
      ...BASE,
      chipLabel: 'Agent lambda — {{params.label}}',
      parameters: { label: { type: 'string', default: '' } },
    });
  });

  it('emits no chipLabel key while blank', () => {
    const draft = newDraft();
    draft.name = 'probe';
    draft.description = 'x';
    expect('chipLabel' in definitionFromDraft(draft)).toBe(false);
  });

  it('errors on an over-long chip label', () => {
    const draft = newDraft();
    draft.chipLabel = 'x'.repeat(161);
    expect(
      validateDraft(draft).some(
        (i) =>
          i.severity === 'error' && i.where.section === 'identity' && i.where.field === 'chipLabel',
      ),
    ).toBe(true);
  });

  it('warns — never errors — on {{llm}} without the oracle, and accepts it with one', () => {
    const draft = newDraft();
    draft.chipLabel = 'Consulted: {{llm}}';
    const withoutOracle = validateDraft(draft).filter(
      (i) => i.where.section === 'identity' && i.where.field === 'chipLabel',
    );
    expect(withoutOracle).toHaveLength(1);
    expect(withoutOracle[0].severity).toBe('warning');

    draft.llmEnabled = true;
    draft.llmPrompt = 'Speak.';
    draft.llmErrorMessage = 'Silence.';
    expect(
      validateDraft(draft).filter(
        (i) => i.where.section === 'identity' && i.where.field === 'chipLabel',
      ),
    ).toEqual([]);
  });

  it('warns on a placeholder naming no declared parameter', () => {
    const draft = newDraft();
    draft.chipLabel = '{{params.ghost}}';
    const issues = validateDraft(draft).filter(
      (i) => i.where.section === 'identity' && i.where.field === 'chipLabel',
    );
    expect(issues).toHaveLength(1);
    expect(issues[0].severity).toBe('warning');
  });

  it('still admits a real {{state.path}} but reports a bare {{state.}}', () => {
    // The chip label is the ONE audit that admits `{{state.…}}` (`allowState`),
    // and pre-collapse it skipped on the prefix — so a bare `{{state.}}` was
    // silently allowed alongside a real path. v4 `0506517d3` correction (e)
    // splits them: a path is still fine, a bare prefix is unknown.
    const draft = newDraft();
    draft.chipLabel = '{{state.encounter.count}}';
    expect(
      validateDraft(draft).filter(
        (i) => i.where.section === 'identity' && i.where.field === 'chipLabel',
      ),
    ).toEqual([]);

    draft.chipLabel = 'bare {{state.}}';
    const bare = validateDraft(draft).filter(
      (i) => i.where.section === 'identity' && i.where.field === 'chipLabel',
    );
    expect(bare).toHaveLength(1);
    expect(bare[0].message).toBe(
      '{{state.}} is not a placeholder this build knows — it will render as written',
    );
  });

  it('flags a {{state.path}} in an outcome message and in the consult prompt', () => {
    // The other side of `allowState`: only the chip label admits state. The
    // message and prompt audits report it as unknown — the shape the collapse
    // made explicit rather than implicit in three separate skip lists.
    const message = newDraft();
    message.name = 'st';
    message.description = 'x';
    message.outcomes[0].message = 'stakes {{state.difficulty}}';
    expect(
      validateDraft(message)
        .filter((i) => i.where.section === 'message')
        .map((i) => i.message),
    ).toEqual([
      '{{state.difficulty}} is not a placeholder this build knows — it will render as written',
    ]);

    const prompt = newDraft();
    prompt.name = 'st2';
    prompt.description = 'x';
    prompt.llmEnabled = true;
    prompt.llmPrompt = 'at {{state.difficulty}}, wise?';
    prompt.llmErrorMessage = 'Silence.';
    expect(
      validateDraft(prompt)
        .filter((i) => i.where.section === 'llm')
        .map((i) => i.message),
    ).toEqual([
      '{{state.difficulty}} is not a placeholder this build knows — it will render as written',
    ]);
  });
});

describe('effects in the draft', () => {
  const BASE = {
    name: 'unlock',
    description: 'Pick the lock.',
    outcomes: [{ when: true, message: '-', state: 'info' }],
  };

  /** Issues anchored to any effect row. */
  function effectIssues(draft: ToolDraft) {
    return validateDraft(draft).filter((i) => i.where.section === 'effect');
  }

  it('round-trips a literal, an expression, and every when shape', () => {
    expectRoundTrip({
      ...BASE,
      parameters: { by: { type: 'number', default: 1 } },
      effects: [
        { target: 'state.tally', value: 3 },
        { target: 'metadata.lockBroken', value: true },
        {
          when: { outcome: { eq: 'failure' } },
          target: 'metadata.lockpick',
          value: "'broken pick'",
        },
        {
          when: { gte: 0.5, metadata: { hasKey: { eq: true } } },
          target: 'state.opened',
          value: '{{value}} + {{params.by}}',
        },
      ],
    });
  });

  it('maps the when shapes onto their form kinds', () => {
    const draft = draftFromDefinition({
      ...BASE,
      effects: [
        { target: 'state.a', value: 1 },
        { when: { outcome: { eq: 'success' } }, target: 'state.b', value: 1 },
        { when: { outcome: { neq: 'failure' } }, target: 'state.c', value: 1 },
        { when: { gt: 0.5 }, target: 'state.d', value: 1 },
      ],
    });
    expect(draft).not.toBeNull();
    expect(draft!.effects.map((e) => e.when.kind)).toEqual([
      'always',
      'outcome-state',
      'verbatim',
      'verbatim',
    ]);
  });

  it('emits no effects key while the list is empty', () => {
    const draft = newDraft();
    draft.name = 'probe';
    draft.description = 'x';
    expect('effects' in definitionFromDraft(draft)).toBe(false);
  });

  it('errors on a target with the wrong prefix, an underscore-guarded path, and a blank', () => {
    const draft = newDraft();
    draft.effects = [
      {
        id: 'e1',
        when: { kind: 'always' },
        target: 'nowhere.x',
        kind: 'write',
        wardrobe: emptyDraftWardrobeGesture(),
 inventory: emptyDraftInventoryGesture(),
 log: '',
        valueKind: 'literal-number',
        value: '1',
      },
      {
        id: 'e2',
        when: { kind: 'always' },
        target: 'state._secret',
        kind: 'write',
        wardrobe: emptyDraftWardrobeGesture(),
 inventory: emptyDraftInventoryGesture(),
 log: '',
        valueKind: 'literal-number',
        value: '1',
      },
      { id: 'e3', when: { kind: 'always' }, kind: 'write', target: '', valueKind: 'literal-number', value: '1', wardrobe: emptyDraftWardrobeGesture(), inventory: emptyDraftInventoryGesture(), log: '' },
    ];
    expect(effectIssues(draft).filter((i) => i.severity === 'error')).toHaveLength(3);
  });

  it('errors on the bare-prose quoting trap', () => {
    const draft = newDraft();
    draft.effects = [
      {
        id: 'e1',
        when: { kind: 'always' },
        target: 'metadata.lockpick',
        kind: 'write',
        wardrobe: emptyDraftWardrobeGesture(),
 inventory: emptyDraftInventoryGesture(),
 log: '',
        valueKind: 'expression',
        value: 'broken pick',
      },
    ];
    const issues = effectIssues(draft);
    expect(issues.some((i) => i.severity === 'error' && i.message.includes('does not parse'))).toBe(
      true,
    );
  });

  it('errors on an expression naming an undeclared parameter', () => {
    const draft = newDraft();
    draft.effects = [
      {
        id: 'e1',
        when: { kind: 'always' },
        target: 'state.x',
        kind: 'write',
        wardrobe: emptyDraftWardrobeGesture(),
 inventory: emptyDraftInventoryGesture(),
 log: '',
        valueKind: 'expression',
        value: '{{params.ghost}} + 1',
      },
    ];
    expect(effectIssues(draft).some((i) => i.message.includes('ghost'))).toBe(true);
  });

  it('errors on {{llm}} (and a verbatim llm condition) without the oracle enabled', () => {
    const draft = newDraft();
    draft.effects = [
      {
        id: 'e1',
        when: { kind: 'always' },
        target: 'state.x',
        kind: 'write',
        wardrobe: emptyDraftWardrobeGesture(),
 inventory: emptyDraftInventoryGesture(),
 log: '',
        valueKind: 'expression',
        value: '{{llm}}',
      },
      {
        id: 'e2',
        when: { kind: 'verbatim', when: { llm: { ok: true } } },
        target: 'state.y',
        kind: 'write',
        wardrobe: emptyDraftWardrobeGesture(),
 inventory: emptyDraftInventoryGesture(),
 log: '',
        valueKind: 'literal-number',
        value: '1',
      },
    ];
    expect(effectIssues(draft).filter((i) => i.severity === 'error')).toHaveLength(2);

    draft.llmEnabled = true;
    draft.llmPrompt = 'Speak.';
    draft.llmErrorMessage = 'Silence.';
    expect(effectIssues(draft)).toEqual([]);
  });

  it('errors past the effects cap', () => {
    const draft = newDraft();
    draft.effects = Array.from({ length: 17 }, (_, i) => ({
      id: `e${i}`,
      when: { kind: 'always' as const },
      target: `state.k${i}`,
      kind: 'write' as const,
      wardrobe: emptyDraftWardrobeGesture(),
 inventory: emptyDraftInventoryGesture(),
 log: '',
      valueKind: 'literal-number' as const,
      value: '1',
    }));
    expect(validateDraft(draft).some((i) => i.message.includes('at most 16 effects'))).toBe(true);
  });
});

/**
 * The gate's two subjects (v4 `0587d1e96`).
 *
 * `gateFromConditions`'s comment — "a chip saved before the progress subject
 * existed carries no `subject`; metadata is what it always meant" — is a claim
 * about a draft PERSISTED before the field existed, so the type cannot express
 * it and no chip a current build authors can reach it. It is pinned here by
 * construction: a mutation reading a subject-less chip as `progress` survives
 * every other spec in this file and in the corpus.
 */
describe('gate chips carry their subject', () => {
  const chip = (over: Partial<DraftGateCondition>): DraftGateCondition => ({
    id: 'g1',
    subject: 'metadata',
    key: 'rank',
    comparator: 'eq',
    operand: { kind: 'string', text: 'captain' },
    ...over,
  });

  it('sorts a chip with NO subject into metadata — what it always meant', () => {
    const legacy = chip({});
    delete (legacy as Partial<DraftGateCondition>).subject;
    expect(JSON.stringify(gateFromConditions([legacy]))).toBe(
      JSON.stringify({ metadata: { rank: { eq: 'captain' } } }),
    );
  });

  it('sorts a progress chip into progress, and keeps metadata first', () => {
    const gate = gateFromConditions([
      chip({
        id: 'g1',
        subject: 'progress',
        key: 'cannon.complete',
        operand: { kind: 'boolean', value: true },
      }),
      chip({ id: 'g2' }),
    ]);
    // `metadata` is written first whatever order the chips arrive in, which is
    // what makes `gateConditionsFromGate` → `gateFromConditions` stable.
    expect(JSON.stringify(gate)).toBe(
      JSON.stringify({
        metadata: { rank: { eq: 'captain' } },
        progress: { 'cannon.complete': { eq: true } },
      }),
    );
  });

  it('emits neither subject when nothing survives, rather than an empty one', () => {
    expect(gateFromConditions([chip({ key: '   ' })])).toBeUndefined();
    expect(
      JSON.stringify(
        gateFromConditions([chip({}), chip({ id: 'g2', subject: 'progress', key: ' ' })]),
      ),
    ).toBe(JSON.stringify({ metadata: { rank: { eq: 'captain' } } }));
  });

  it('flattens a two-subject gate metadata-first', () => {
    const chips = gateConditionsFromGate({
      progress: { 'cannon.complete': { eq: true } },
      metadata: { rank: { eq: 'captain' } },
    });
    expect(chips.map((c) => c.subject)).toEqual(['metadata', 'progress']);
  });
});

// ---------------------------------------------------------------------------
// v4 `f0db8cbb8` + `362426c8f` (pinned at `25b8fab6a`): the two describes v4's
// `tool-draft.test.ts` grew, transcribed CASE FOR CASE — "wardrobe effects in
// the draft" (`:918-995`) and "trigger phrases in the draft" (`:996-1060`).
// The §6.2 round-trip normaliser above needs no change: `triggers` is emitted
// only when non-empty, and `wardrobe` only on a wardrobe-kind effect.
// ---------------------------------------------------------------------------

describe('wardrobe effects in the draft', () => {
  const BASE = {
    name: 'regalia',
    description: 'Be knighted.',
    parameters: { garment: { type: 'string', default: 'Boots' } },
    roll: { min: 1, max: 1 },
    outcomes: [{ when: true, message: 'Arise.', state: 'success' }],
  };

  const effectIssues = (draft: ToolDraft) =>
    validateDraft(draft).filter((i) => i.where.section === 'effect');

  function wardrobeDraft(wardrobe: Partial<ReturnType<typeof newDraftEffect>['wardrobe']>): ToolDraft {
    const draft = draftFromDefinition(BASE)!;
    const effect = newDraftEffect('wardrobe');
    effect.wardrobe = { ...effect.wardrobe, ...wardrobe };
    draft.effects = [effect];
    return draft;
  }

  it('round-trips each action, beside a write', () => {
    expectRoundTrip({
      ...BASE,
      effects: [
        { wardrobe: { action: 'wear', items: ['Cloak of Night', '{{params.garment}}'] } },
        { wardrobe: { action: 'wear', items: ['Ribbon'], mode: 'add_to_slot', slot: 'hair' } },
        { wardrobe: { action: 'wear', items: ['Regalia'], mode: 'replace' } },
        {
          when: { outcome: { eq: 'failure' } },
          wardrobe: { action: 'take_off', items: ['Sundress'], slot: 'top' },
        },
        { wardrobe: { action: 'clear_slot', slot: 'hair' } },
        { target: 'state.knighted', value: true },
      ],
    });
  });

  it('keeps both halves on the draft, and emits only the active one', () => {
    const draft = draftFromDefinition({ ...BASE, effects: [{ target: 'state.x', value: 1 }] })!;
    const effect = draft.effects[0];
    expect(effect.kind).toBe('write');

    effect.kind = 'wardrobe';
    effect.wardrobe = { action: 'wear', items: ['Hat', '  '], mode: 'wear', slot: 'hair' };
    expect(definitionFromDraft(draft)['effects']).toEqual([{ wardrobe: { action: 'wear', items: ['Hat'] } }]);
    // The write half survives the switch.
    expect(effect.target).toBe('state.x');

    effect.kind = 'write';
    expect(definitionFromDraft(draft)['effects']).toEqual([{ target: 'state.x', value: 1 }]);
  });

  it("says the per-action rules in the loader's own words", () => {
    expect(effectIssues(wardrobeDraft({ action: 'wear', items: [''] })).map((i) => i.message)).toEqual([
      'wear needs at least one item',
    ]);
    expect(
      effectIssues(wardrobeDraft({ action: 'wear', items: ['Hat'], mode: 'add_to_slot' })).map((i) => i.message),
    ).toEqual(['mode add_to_slot needs a slot']);
    expect(effectIssues(wardrobeDraft({ action: 'clear_slot' })).map((i) => i.message)).toEqual([
      'clear_slot needs a slot',
    ]);
    expect(effectIssues(wardrobeDraft({ action: 'clear_slot', slot: 'hair', items: ['ignored'] }))).toEqual([]);
  });

  it('errors on an undeclared parameter, warns on an unknown placeholder family', () => {
    const issues = effectIssues(wardrobeDraft({ action: 'wear', items: ['{{params.ghost}}', '{{mystery}}'] }));
    expect(issues.filter((i) => i.severity === 'error').map((i) => i.message)).toEqual([
      '{{params.ghost}} names no declared parameter',
    ]);
    expect(issues.filter((i) => i.severity === 'warning')).toHaveLength(1);
  });

  it('renames a parameter inside wardrobe items, and lists it as a reference', () => {
    const draft = wardrobeDraft({ action: 'take_off', items: ['{{params.garment}}'] });
    expect(findParameterReferences(draft, 'garment')).toContain('effect 1: a wardrobe item names it');
    const renamed = renameParameterEverywhere(draft, 'garment', 'cloth');
    expect(renamed.effects[0].wardrobe.items).toEqual(['{{params.cloth}}']);
  });
});

describe('trigger phrases in the draft', () => {
  const BASE = {
    name: 'spell',
    description: 'Cast it.',
    outcomes: [{ when: true, message: 'The air shimmers.', state: 'info' }],
  };

  const triggerIssues = (draft: ToolDraft) =>
    validateDraft(draft).filter((i) => i.where.section === 'triggers');

  it('round-trips triggers, as written and in order', () => {
    expectRoundTrip({ ...BASE, triggers: ['Abracadabra', 'open sesame'] });
  });

  it('emits triggers right after chipLabel, and none while empty', () => {
    const draft = draftFromDefinition({ ...BASE, chipLabel: 'Spell', triggers: ['abracadabra'] })!;
    expect(Object.keys(definitionFromDraft(draft)).slice(0, 5)).toEqual([
      '$schema',
      'name',
      'chipLabel',
      'triggers',
      'description',
    ]);
    draft.triggers = [];
    expect('triggers' in definitionFromDraft(draft)).toBe(false);
  });

  it('errors on a phrase too short, padded, or repeated ignoring case', () => {
    const draft = draftFromDefinition(BASE)!;
    draft.triggers = ['ok', ' abracadabra', 'Hocus', 'HOCUS'];
    const issues = triggerIssues(draft);
    expect(issues.every((i) => i.severity === 'error')).toBe(true);
    expect(issues.map((i) => (i.where as { index?: number }).index)).toEqual([0, 1, 3]);
  });

  it('errors past the cap', () => {
    const draft = draftFromDefinition(BASE)!;
    draft.triggers = Array.from({ length: 9 }, (_, i) => `phrase ${i}`);
    expect(triggerIssues(draft).some((i) => i.severity === 'error')).toBe(true);
  });

  it('warns — never errors — when an outcome message echoes its own trigger', () => {
    const draft = draftFromDefinition({
      ...BASE,
      outcomes: [{ when: true, message: 'You cry ABRACADABRA and the door opens.', state: 'info' }],
    })!;
    draft.triggers = ['abracadabra'];
    const outcomeId = draft.outcomes[0].id;
    const echoes = validateDraft(draft).filter(
      (i) => i.where.section === 'message' && i.where.id === outcomeId,
    );
    expect(echoes).toHaveLength(1);
    expect(echoes[0].severity).toBe('warning');
    expect(echoes[0].message).toContain('"abracadabra"');
  });

  it('warns when the chip label or consult prompt echoes a trigger', () => {
    const draft = draftFromDefinition(BASE)!;
    draft.triggers = ['abracadabra'];
    draft.chipLabel = 'Abracadabra!';
    draft.llmEnabled = true;
    draft.llmPrompt = 'Say abracadabra back.';
    draft.llmErrorMessage = 'Silence.';
    const issues = validateDraft(draft);
    expect(
      issues.some(
        (i) =>
          i.severity === 'warning' &&
          i.where.section === 'identity' &&
          i.where.field === 'chipLabel' &&
          i.message.includes('trigger'),
      ),
    ).toBe(true);
    expect(
      issues.some((i) => i.severity === 'warning' && i.where.section === 'llm' && i.message.includes('trigger')),
    ).toBe(true);
  });
});

/**
 * `announceAsCharacter` (v4 `a05c6f755`): emitted only when true, in schema
 * position, and never carried as an unknown key (R-3 / T19).
 */
describe('announceAsCharacter in the draft', () => {
  const BASE = {
    name: 'probe',
    description: 'A probe.',
    outcomes: [{ when: true, message: '-', state: 'info' }],
  };

  it('is off, and absent from the file, for a new draft', () => {
    expect(newDraft().announceAsCharacter).toBe(false);
    expect('announceAsCharacter' in definitionFromDraft(newDraft())).toBe(false);
  });

  it('round-trips true in schema position, after defaultVisibility', () => {
    const draft = draftFromDefinition({
      ...BASE,
      announceAsCharacter: true,
      defaultVisibility: 'whisper',
      parameters: { n: { type: 'number', default: 1 } },
    });
    expect(draft!.announceAsCharacter).toBe(true);
    expect(draft!.unknownKeys).toEqual([]);
    const keys = Object.keys(definitionFromDraft(draft!));
    expect(keys.indexOf('announceAsCharacter')).toBe(keys.indexOf('defaultVisibility') + 1);
    expect(keys.indexOf('parameters')).toBe(keys.indexOf('announceAsCharacter') + 1);
  });

  it('drops a file\'s false on save rather than writing it back', () => {
    const draft = draftFromDefinition({ ...BASE, announceAsCharacter: false });
    expect(draft!.announceAsCharacter).toBe(false);
    expect(draft!.unknownKeys).toEqual([]);
    expect('announceAsCharacter' in definitionFromDraft(draft!)).toBe(false);
  });
});

/** `{{character}}` (v4 `a05c6f755`): the audit never flags it (R-4). */
describe('{{character}} in the draft', () => {
  it('audits to no placeholder issue anywhere it may appear', () => {
    const draft = draftFromDefinition({
      name: 'announced',
      description: 'A probe.',
      chipLabel: 'For {{character}}',
      llm: { prompt: 'Describe {{character}} at work.', errorMessage: 'The wire went dead.' },
      effects: [
        { target: 'state.winner', value: "{{character}} + ' wins'" },
        { wardrobe: { action: 'wear', items: ["{{character}}'s armour"] } },
      ],
      outcomes: [{ when: true, message: '{{character}} rolls {{value}}', state: 'info' }],
    });
    expect(draft).not.toBeNull();
    const issues = validateDraft(draft!);
    expect(issues.filter((issue) => issue.message.includes('character'))).toEqual([]);
  });

  it("loads a file whose effect value quotes {{character}}, and round-trips it", () => {
    // T19: before the port the twin's grammar refused this file outright, so a
    // v4-authored tool dropped off the Workbench.
    const document = {
      name: 'winner',
      description: 'A probe.',
      effects: [{ target: 'state.winner', value: "{{character}} + ' wins'" }],
      outcomes: [{ when: true, message: '-', state: 'info' }],
    };
    expect(draftFromDefinition(document)).not.toBeNull();
    expectRoundTrip(document);
  });
});

/**
 * `defaultWardrobeVantage` — the Workbench's opening wardrobe lens, derived
 * from the store a definition lives in (v4 `5cb0e0190`,
 * `__tests__/unit/lib/pascal/wardrobe-vantage.test.ts`, transcribed name for
 * name; the corpus is `wardrobe-vantage.oracle.spec.ts`).
 */
describe('defaultWardrobeVantage', () => {
  const store = (mountPointId: string) => ({
    mountPointId,
    mountName: mountPointId,
    existingToolNames: [],
  });
  const destinations = {
    general: store('mp-general'),
    characters: [
      { characterId: 'char-imogen', characterName: 'Imogen', ...store('mp-imogen') },
      // A vault that is also linked to a group: the character wins.
      { characterId: 'char-bertie', characterName: 'Bertie', ...store('mp-shared') },
    ],
    groups: [
      {
        groupId: 'group-regulars',
        groupName: 'The Regulars',
        stores: [
          { ...store('mp-regulars'), official: true },
          { ...store('mp-shared'), official: false },
        ],
      },
    ],
    projects: [
      { projectId: 'proj-thornfield', projectName: 'Thornfield', stores: [store('mp-thornfield')] },
    ],
    other: [store('mp-loose')],
  };

  it("opens a character vault's tool on that character", () => {
    expect(defaultWardrobeVantage('mp-imogen', destinations)).toEqual({
      kind: 'character',
      characterId: 'char-imogen',
    });
  });

  it("opens a group store's tool on the group wardrobe", () => {
    expect(defaultWardrobeVantage('mp-regulars', destinations)).toEqual({
      kind: 'container',
      container: { scope: 'group', id: 'group-regulars' },
    });
  });

  it("opens a project store's tool on the project wardrobe", () => {
    expect(defaultWardrobeVantage('mp-thornfield', destinations)).toEqual({
      kind: 'container',
      container: { scope: 'project', id: 'proj-thornfield' },
    });
  });

  it('opens a General-store tool on Quilltap General', () => {
    expect(defaultWardrobeVantage('mp-general', destinations)).toEqual({
      kind: 'container',
      container: { scope: 'general', id: null },
    });
  });

  it('opens an unattached store or an unsaved file on nobody', () => {
    expect(defaultWardrobeVantage('mp-loose', destinations)).toEqual({ kind: 'none' });
    expect(defaultWardrobeVantage(null, destinations)).toEqual({ kind: 'none' });
  });

  it('prefers the character when a vault is also attached to a group', () => {
    expect(defaultWardrobeVantage('mp-shared', destinations)).toEqual({
      kind: 'character',
      characterId: 'char-bertie',
    });
  });
});


// ===========================================================================
// P4.D334 — the any-of groups, the brief, the Inventory and the Log
// (v4 a53024d3c, 322a5488e, e1872c94f, e4a60ad9e at the pin).
//
// v4's own cases are transcribed NAME FOR NAME. Cases that load a document
// through `draftFromDefinition` read P4.D333's twin of the schema; each has a
// twin that builds the same draft by hand.
// ===========================================================================

const messagesOf = (draft: ToolDraft) => validateDraft(draft).map((i) => i.message);
const errorsOf = (draft: ToolDraft) =>
  validateDraft(draft)
    .filter((i) => i.severity === 'error')
    .map((i) => i.message);
const warningsOf = (draft: ToolDraft) =>
  validateDraft(draft)
    .filter((i) => i.severity === 'warning')
    .map((i) => i.message);

/** A valid draft with only the catch-all (v4's `{ name: 'x', description: 'x', outcomes: [catch-all] }`). */
function bareDraft(): ToolDraft {
  const draft = newDraft();
  draft.name = 'x';
  draft.description = 'x';
  draft.outcomes = [{ ...draft.outcomes[1], message: '-' }];
  return draft;
}

let handId = 0;
const chip = (
  subject: DraftCondition['subject'],
  comparator: DraftCondition['comparator'],
  operand: DraftCondition['operand'],
  joiner?: 'and' | 'or',
): DraftCondition => ({
  id: `h-${++handId}`,
  subject,
  comparator,
  operand,
  ...(joiner ? { joiner } : {}),
});
const gateChip = (
  subject: DraftGateCondition['subject'],
  key: string,
  comparator: DraftGateCondition['comparator'],
  operand: DraftGateCondition['operand'],
  joiner?: 'and' | 'or',
): DraftGateCondition => ({ id: `g-${++handId}`, subject, key, comparator, operand, ...(joiner ? { joiner } : {}) });

const sortKeys = (value: unknown): unknown =>
  Array.isArray(value)
    ? value.map(sortKeys)
    : value && typeof value === 'object'
      ? Object.fromEntries(
          Object.entries(value as Record<string, unknown>)
            .sort(([a], [b]) => a.localeCompare(b))
            .map(([k, v]) => [k, sortKeys(v)]),
        )
      : value;

describe('groupsOf, joinGroups and collapseGroups (the one-level any-of)', () => {
  it('splits at each or; the first chip always opens a group and its joiner is ignored', () => {
    const chips = [{ n: 1, joiner: 'or' as const }, { n: 2 }, { n: 3, joiner: 'or' as const }, { n: 4, joiner: 'and' as const }];
    expect(groupsOf(chips).map((g) => g.map((c) => c.n))).toEqual([[1, 2], [3, 4]]);
    expect(groupsOf([])).toEqual([]);
  });

  it('round-trips a flat chip list through its groups: only the first chip of a later group is marked or', () => {
    const when: WhenObject[] = [{ gte: 1 }, { lt: 3 }];
    const chips = conditionsFromWhen(when);
    expect(chips.map((c) => c.joiner)).toEqual([undefined, 'or']);
    // A later chip that already carries `or` keeps it, so the round trip is stable.
    expect(conditionsFromWhen(whenFromConditions(chips)!).map((c) => c.joiner)).toEqual([undefined, 'or']);
  });

  it('a single line is a bare object, several an array, none undefined; a line of incomplete chips drops', () => {
    expect(whenFromConditions([])).toBeUndefined();
    const one = [chip({ kind: 'value' }, 'gte', { kind: 'number', text: '1' })];
    expect(whenFromConditions(one)).toEqual({ gte: 1 });
    const two = [...one, chip({ kind: 'value' }, 'lt', { kind: 'number', text: '3' }, 'or')];
    expect(whenFromConditions(two)).toEqual([{ gte: 1 }, { lt: 3 }]);
    const dropped = [chip({ kind: 'value' }, 'gte', { kind: 'number', text: '' }), ...two.slice(1)];
    expect(whenFromConditions(dropped)).toEqual({ lt: 3 });
  });

  it('an empty condition list is still a non-catch-all row with when {}', () => {
    const draft = bareDraft();
    draft.outcomes = [{ ...newDraft().outcomes[0], message: 'm' }, draft.outcomes[0]];
    const out = definitionFromDraft(draft) as { outcomes: Array<{ when: unknown }> };
    expect(out.outcomes[0].when).toEqual({});
  });

  it('carries an array effect when verbatim, never flattened', () => {
    const draft = bareDraft();
    draft.effects = [
      {
        ...newDraftEffect('write'),
        target: 'state.tuned',
        valueKind: 'literal-boolean',
        value: 'true',
        when: { kind: 'verbatim', when: [{ outcome: { eq: 'success' } }, { metadata: { lucky: { eq: true } } }] },
      },
    ];
    const out = definitionFromDraft(draft) as { effects: Array<{ when: unknown }> };
    expect(out.effects[0].when).toEqual([{ outcome: { eq: 'success' } }, { metadata: { lucky: { eq: true } } }]);
  });
});

describe('the any-of groups in the draft (pascal-gate-broadening)', () => {
  it('draws two lines, round-trips their joiners and emits an array (hand-built twin)', () => {
    const draft = bareDraft();
    draft.gateMode = 'available';
    draft.gateConditions = [
      gateChip('metadata', 'a', 'eq', { kind: 'boolean', value: true }),
      gateChip('inventoryAggregate', 'equipped.metadata.toolVersion.max', 'gte', { kind: 'number', text: '4' }),
      gateChip('metadata', 'a', 'eq', { kind: 'boolean', value: true }, 'or'),
      gateChip('inventoryAggregate', 'equipped.names', 'eq', { kind: 'string', text: 'ansible resonator' }),
    ];
    const out = definitionFromDraft(draft);
    expect(out['availableWhen']).toEqual([
      { metadata: { a: { eq: true } }, inventoryAggregate: { 'equipped.metadata.toolVersion.max': { gte: 4 } } },
      { metadata: { a: { eq: true } }, inventoryAggregate: { 'equipped.names': { eq: 'ansible resonator' } } },
    ]);
    expect(errorsOf(draft)).toEqual([]);
    expect(gateConditionsFromGate(out['availableWhen'] as never).map((c) => c.joiner ?? 'and')).toEqual(['and', 'and', 'or', 'and']);
  });

  const ANY_OF = {
    $schema: '/schemas/qtap-custom-tool.schema.json',
    name: 'tune',
    description: 'Tune.',
    parameters: { band: { type: 'string', default: 'low' } },
    availableWhen: [
      { metadata: { a: { eq: true } }, inventoryAggregate: { 'equipped.metadata.toolVersion.max': { gte: 4 } } },
      { metadata: { a: { eq: true } }, inventoryAggregate: { 'equipped.names': { eq: 'ansible resonator' } } },
    ],
    effects: [{ when: [{ outcome: { eq: 'success' } }, { metadata: { lucky: { eq: true } } }], target: 'state.tuned', value: true }],
    outcomes: [
      {
        when: [{ gte: 0.9 }, { inventory: { 'lucky charm': { eq: 'equipped' } }, inventoryAggregate: { 'owned.titles': { contains: { $param: 'band' } } } }],
        message: 'tuned',
        state: 'success',
      },
      { when: true, message: 'static', state: 'info' },
    ],
  };

  it('round-trips an any-of gate, outcome and effect byte-equal after key sort', () => {
    const draft = draftFromDefinition(ANY_OF)!;
    expect(draft.gateConditions.map((c) => c.joiner ?? 'and')).toEqual(['and', 'and', 'or', 'and']);
    expect(draft.outcomes[0].conditions.map((c) => c.joiner ?? 'and')).toEqual(['and', 'or', 'and']);
    expect(draft.effects[0].when.kind).toBe('verbatim');
    const out = definitionFromDraft(draft);
    expect(JSON.stringify(sortKeys(out))).toBe(JSON.stringify(sortKeys(ANY_OF)));
    expect(errorsOf(draft)).toEqual([]);
  });

  it('collapses a single line to a bare object, and drops a line of incomplete chips', () => {
    const draft = draftFromDefinition(ANY_OF)!;
    draft.gateConditions = draft.gateConditions.map((c) => ({ ...c, joiner: 'and' as const }));
    expect(Array.isArray(definitionFromDraft(draft)['availableWhen'])).toBe(false);
    draft.outcomes[0].conditions = draft.outcomes[0].conditions.slice(0, 1);
    expect((definitionFromDraft(draft)['outcomes'] as Array<{ when: unknown }>)[0].when).toEqual({ gte: 0.9 });
  });

  it('collapses a single line to a bare object, and drops a line of incomplete chips (hand-built twin)', () => {
    const draft = bareDraft();
    draft.gateMode = 'available';
    draft.gateConditions = [
      gateChip('metadata', 'a', 'eq', { kind: 'boolean', value: true }),
      gateChip('metadata', 'b', 'eq', { kind: 'boolean', value: true }, 'and'),
    ];
    expect(Array.isArray(definitionFromDraft(draft)['availableWhen'])).toBe(false);
    draft.outcomes = [
      { ...newDraft().outcomes[0], message: 'm', conditions: [chip({ kind: 'value' }, 'gte', { kind: 'number', text: '0.9' }), chip({ kind: 'value' }, 'lt', { kind: 'number', text: '' }, 'or')] },
      draft.outcomes[0],
    ];
    expect((definitionFromDraft(draft)['outcomes'] as Array<{ when: unknown }>)[0].when).toEqual({ gte: 0.9 });
  });

  it('lets two lines repeat a slot, but not one line', () => {
    const draft = draftFromDefinition({ ...ANY_OF, availableWhen: [{ metadata: { rank: { gt: 3 } } }, { metadata: { rank: { gt: 5 } } }] })!;
    expect(errorsOf(draft)).toEqual([]);
    draft.gateConditions = draft.gateConditions.map((c) => ({ ...c, joiner: 'and' as const }));
    expect(messagesOf(draft)).toContain('a line of the gate can test this key with this comparator only once');
  });

  it('lets two lines repeat a slot, but not one line (hand-built twin)', () => {
    const draft = bareDraft();
    draft.gateMode = 'available';
    draft.gateConditions = [
      gateChip('metadata', 'rank', 'gt', { kind: 'number', text: '3' }),
      gateChip('metadata', 'rank', 'gt', { kind: 'number', text: '5' }, 'or'),
    ];
    expect(errorsOf(draft)).toEqual([]);
    draft.gateConditions = draft.gateConditions.map((c) => ({ ...c, joiner: 'and' as const }));
    expect(messagesOf(draft)).toContain('a line of the gate can test this key with this comparator only once');
  });

  it('applies the same per-line rule to an outcome row, and counts the lines', () => {
    const draft = bareDraft();
    const row = { ...newDraft().outcomes[0], message: 'm' };
    row.conditions = [
      chip({ kind: 'value' }, 'gte', { kind: 'number', text: '1' }),
      chip({ kind: 'value' }, 'gte', { kind: 'number', text: '2' }, 'or'),
    ];
    draft.outcomes = [row, draft.outcomes[0]];
    expect(errorsOf(draft)).toEqual([]);
    row.conditions[1] = { ...row.conditions[1], joiner: 'and' };
    expect(messagesOf(draft)).toContain('a line can test this subject with this comparator only once');
    row.conditions = Array.from({ length: 9 }, (_, i) => chip({ kind: 'value' }, 'gte', { kind: 'number', text: String(i) }, i === 0 ? undefined : 'or'));
    expect(messagesOf(draft)).toContain('at most 8 lines joined by OR');
    row.conditions = row.conditions.slice(0, 8);
    expect(messagesOf(draft)).not.toContain('at most 8 lines joined by OR');
  });

  it('puts the gate line count after the both-clauses check and the outcome line count after its slot walk', () => {
    const draft = bareDraft();
    draft.gateMode = 'available';
    draft.gateConditions = Array.from({ length: 9 }, (_, i) => gateChip('metadata', `k${i}`, 'eq', { kind: 'number', text: '1' }, i === 0 ? undefined : 'or'));
    const issues = validateDraft(draft).filter((i) => i.where.section === 'gate');
    expect(issues.map((i) => i.message)).toEqual(['at most 8 lines joined by OR']);
  });

  it.each([
    ['equipped.bag', { kind: 'number', text: '1' }, 'eq', /not a field/],
    ['equipped.names', { kind: 'number', text: '1' }, 'gt', /cannot order a list; fold it/],
    ['owned.metadata.tags', { kind: 'number', text: '1' }, 'lte', /cannot order a list; fold it/],
    ['equipped.count', { kind: 'string', text: 'x' }, 'contains', /a number holds no text/],
    ['equipped.metadata.v.max', { kind: 'string', text: 'x' }, 'ncontains', /a number holds no text/],
  ] as const)('flags %s with %s', (key, operand, comparator, message) => {
    const draft = bareDraft();
    draft.gateMode = 'available';
    draft.gateConditions = [{ id: 'g1', subject: 'inventoryAggregate', key, comparator, operand }];
    expect(errorsOf(draft).join('\n')).toMatch(message);
  });

  it('warns that a metadata key ending in a fold word reads as the fold', () => {
    const draft = bareDraft();
    const row = { ...newDraft().outcomes[0], message: 'y' };
    row.conditions = [chip({ kind: 'inventoryAggregate', key: 'owned.metadata.stats.max' }, 'gte', { kind: 'number', text: '1' })];
    draft.outcomes = [row, draft.outcomes[0]];
    expect(warningsOf(draft).join('\n')).toMatch(/reads as the \.max of metadata key "stats"/);
  });

  it('refuses a $param of the wrong type and a templated aggregate key', () => {
    const draft = bareDraft();
    draft.parameters = [{ id: 'p1', name: 'n', type: 'number', defaultValue: '1', description: '', min: '', max: '' }];
    const row = { ...newDraft().outcomes[0], message: 'y' };
    row.conditions = [chip({ kind: 'inventoryAggregate', key: 'equipped.names' }, 'contains', { kind: 'param', name: 'n' })];
    draft.outcomes = [row, draft.outcomes[0]];
    expect(messagesOf(draft).join('\n')).toMatch(/needs text to look for; "n" is number/);
    row.conditions[0] = { ...row.conditions[0], subject: { kind: 'inventoryAggregate', key: 'equipped.metadata.{{params.n}}' } };
    expect(messagesOf(draft).join('\n')).toMatch(/aggregate key is literal/);
  });
});

describe('the gate round-trips through the draft at the pin (the law P4.D333 carves)', () => {
  // `gateFromConditions(gateConditionsFromGate(gate))` is byte-equal over the
  // NON-EMPTY records, group by group — the same normalisation the shared
  // corpus spec applies, extended to arrays.
  const normGroup = (group: Record<string, unknown>) =>
    Object.fromEntries(Object.entries(group).filter(([, v]) => v && Object.keys(v as object).length > 0));
  const norm = (gate: unknown): unknown => (Array.isArray(gate) ? gate.map((g) => normGroup(g)) : normGroup(gate as never));
  const ROUNDTRIPS: Array<[string, unknown]> = [
    ['one group, metadata', { metadata: { a: { eq: 1 } } }],
    ['one group, progress', { progress: { 'cannon.percent': { gte: 50 } } }],
    ['one group, inventory', { inventory: { lantern: { eq: 'equipped' } } }],
    ['one group, inventoryCount', { inventoryCount: { ring: { gte: 1, lte: 3 } } }],
    ['one group, inventoryAggregate', { inventoryAggregate: { 'equipped.count': { gte: 1 } } }],
    [
      'one group, every record in schema order',
      {
        metadata: { a: { eq: 1 } },
        progress: { 'cannon.percent': { gte: 50 } },
        inventory: { lantern: { eq: 'equipped' } },
        inventoryCount: { ring: { gte: 1 } },
        inventoryAggregate: { 'equipped.names': { contains: 'x' } },
      },
    ],
    ['an array of one', [{ metadata: { a: { eq: 1 } } }]],
    ['an array of two', [{ metadata: { a: { eq: 1 } } }, { inventory: { lantern: { neq: 'absent' } } }]],
    [
      'an array of eight',
      Array.from({ length: 8 }, (_, i) => ({ metadata: { [`k${i}`]: { eq: i } } })),
    ],
    ['an empty record is dropped', { metadata: { a: { eq: 1 } }, inventory: {} }],
  ];
  it.each(ROUNDTRIPS)('%s', (_name, gate) => {
    const back = gateFromConditions(gateConditionsFromGate(gate as never));
    // An array of one collapses to the bare group, as the draft cannot tell them apart.
    const expected = Array.isArray(gate) && gate.length === 1 ? norm(gate[0]) : norm(gate);
    expect(JSON.stringify(back)).toBe(JSON.stringify(expected));
  });

  it('keeps an aggregate key as typed on the chip and trims it on the way out', () => {
    const draft = bareDraft();
    draft.gateMode = 'available';
    draft.gateConditions = [gateChip('inventoryAggregate', '  equipped.count  ', 'gte', { kind: 'number', text: '1' })];
    expect(gateFromConditions(draft.gateConditions)).toEqual({ inventoryAggregate: { 'equipped.count': { gte: 1 } } });
  });
});

describe('the outcome brief in the draft (bug 257)', () => {
  const scanner = (overrides: Record<string, unknown> = {}) => ({
    name: 'entanglement_scan',
    description: 'Scan a shard for entanglement.',
    parameters: { layers: { type: 'number', default: 95 } },
    roll: { min: 1, max: 100 },
    llm: {
      prompt: 'Write a report. State: {{outcome.state}}, row {{outcome.index}}. Conclusion to reach: {{outcome.brief}}',
      errorMessage: 'The link falters.',
    },
    outcomes: [
      { when: { roll: { gte: { $param: 'layers' } } }, message: '{{llm}}', state: 'success', brief: 'Confirmed at {{params.layers}} layers.' },
      { when: true, message: '{{llm}}', state: 'failure', brief: 'Not found.' },
    ],
    ...overrides,
  });
  const draftOf = (doc: unknown) => draftFromDefinition(doc)!;

  /** The scanner, built by hand — the same draft `draftFromDefinition(scanner())` yields. */
  function handScanner(prompt = scanner().llm.prompt): ToolDraft {
    const draft = newDraft();
    draft.name = 'entanglement_scan';
    draft.description = 'Scan a shard for entanglement.';
    draft.parameters = [{ id: 'p1', name: 'layers', type: 'number', defaultValue: '95', description: '', min: '', max: '' }];
    draft.rollRange.min = { kind: 'literal', text: '1' };
    draft.rollRange.max = { kind: 'literal', text: '100' };
    draft.llmEnabled = true;
    draft.llmPrompt = prompt;
    draft.llmErrorMessage = 'The link falters.';
    draft.outcomes[0] = {
      ...draft.outcomes[0],
      conditions: [chip({ kind: 'roll' }, 'gte', { kind: 'param', name: 'layers' })],
      message: '{{llm}}',
      state: 'success',
      brief: 'Confirmed at {{params.layers}} layers.',
    };
    draft.outcomes[1] = { ...draft.outcomes[1], message: '{{llm}}', state: 'failure', brief: 'Not found.' };
    return draft;
  }

  it('round-trips a brief, and omits a blank one (hand-built twin)', () => {
    const emitted = definitionFromDraft(handScanner()) as { outcomes: Array<{ brief?: string }> };
    expect(emitted.outcomes.map((o) => o.brief)).toEqual(['Confirmed at {{params.layers}} layers.', 'Not found.']);
    const blank = handScanner();
    blank.outcomes[0].brief = '  ';
    const reEmitted = definitionFromDraft(blank) as { outcomes: Array<{ brief?: string }> };
    expect(reEmitted.outcomes[0]).not.toHaveProperty('brief');
  });

  it('round-trips a brief, and omits a blank one', () => {
    const emitted = definitionFromDraft(draftOf(scanner())) as { outcomes: Array<{ brief?: string }> };
    expect(emitted.outcomes.map((o) => o.brief)).toEqual(['Confirmed at {{params.layers}} layers.', 'Not found.']);
    const blank = draftOf(scanner());
    blank.outcomes[0].brief = '  ';
    expect((definitionFromDraft(blank) as { outcomes: Array<{ brief?: string }> }).outcomes[0]).not.toHaveProperty('brief');
  });

  it('emits the brief LAST on the row and UNTRIMMED', () => {
    const draft = handScanner();
    draft.outcomes[0].brief = '  padded  ';
    const row = (definitionFromDraft(draft) as { outcomes: Array<Record<string, unknown>> }).outcomes[0];
    expect(Object.keys(row)).toEqual(['when', 'message', 'state', 'brief']);
    expect(row['brief']).toBe('  padded  ');
  });

  it('is valid as written', () => {
    expect(errorsOf(draftOf(scanner()))).toEqual([]);
  });

  it('is valid as written (hand-built twin: the brief rules that read no outcome placeholder)', () => {
    const draft = handScanner('Write a report.');
    expect(errorsOf(draft)).toEqual([]);
  });

  it('warns about a row that has no brief when the prompt reads one', () => {
    const draft = draftOf(scanner());
    draft.outcomes[1].brief = '';
    const warnings = validateDraft(draft).filter((i) => i.severity === 'warning' && i.where.section === 'brief');
    expect(warnings).toHaveLength(1);
    expect(warnings[0].where).toMatchObject({ id: draft.outcomes[1].id });
  });

  it('errors on {{outcome.*}} in the prompt when a row tests the consult', () => {
    const draft = draftOf(scanner());
    draft.outcomes[0].conditions.push({ id: 'c-llm', subject: { kind: 'llm' }, comparator: 'eq', operand: { kind: 'string', text: 'YES' } });
    const errors = validateDraft(draft).filter((i) => i.severity === 'error' && i.where.section === 'llm');
    expect(errors.length).toBeGreaterThan(0);
    expect(errors[0].message).toMatch(/table cannot be dealt before the consult/);
  });

  it('warns when the prompt asks the model to compare numbers', () => {
    for (const prompt of [
      'If {{roll}} >= {{params.layers}}, report success.',
      'Report entangled when {{value}} < 40.',
      'You must actually do the math here.',
    ]) {
      const draft = handScanner(prompt);
      draft.outcomes = [{ ...draft.outcomes[1], conditions: [], message: '{{llm}}', state: 'info', brief: '' }];
      const messages = validateDraft(draft)
        .filter((i) => i.severity === 'warning' && i.where.section === 'llm')
        .map((i) => i.message);
      expect(messages.join('\n')).toMatch(/compare numbers/);
      expect(messages).toHaveLength(1);
    }
  });

  it('does not warn for a prompt that merely quotes a figure', () => {
    const draft = handScanner('Write a report on a scan of depth {{params.layers}}. Tone: clinical.');
    draft.outcomes = [{ ...draft.outcomes[1], conditions: [], message: '{{llm}}', state: 'info', brief: '' }];
    expect(validateDraft(draft).filter((i) => i.where.section === 'llm')).toEqual([]);
  });

  it('flags, from the audit, reachable rows without a brief', () => {
    const draft = handScanner();
    draft.outcomes[0].brief = '';
    const hits = [
      { index: 0, hits: 12 },
      { index: 1, hits: 988 },
    ];
    expect(reachableOutcomesMissingBrief(draft, hits)).toEqual([0]);
    draft.llmPrompt = 'Write a report.';
    expect(reachableOutcomesMissingBrief(draft, hits)).toEqual([]);
  });

  it('puts a brief over 1000 characters (UNTRIMMED length) and a brief with the consult off in front of the author', () => {
    const draft = handScanner('Write a report.');
    draft.outcomes[0].brief = 'a'.repeat(1001);
    expect(messagesOf(draft)).toContain('a brief is over 1000 characters');
    draft.outcomes[0].brief = 'fine';
    draft.llmEnabled = false;
    expect(messagesOf(draft)).toContain('has a brief, but the LLM consult is not enabled to read it');
  });
});

describe('the Inventory in the draft (e1872c94f)', () => {
  const TOOL = {
    name: 'strike',
    description: 'Strike.',
    parameters: { weapon: { type: 'string', default: 'Excalibur' } },
    availableWhen: { inventory: { lantern: { eq: 'equipped' } }, inventoryCount: { ring: { gte: 1 } } },
    roll: { $inventory: 'sword.metadata.damage', fallback: '1d4' },
    effects: [
      { inventory: { action: 'equip', items: ['{{params.weapon}}'] } },
      { when: { outcome: { eq: 'failure' } }, inventory: { action: 'pick_up', items: ['magic bow'], equip: false } },
    ],
    outcomes: [
      { when: { inventory: { '{{params.weapon}}': { eq: 'equipped' } }, inventoryCount: { ring: { gte: 2 } } }, message: 'hit', state: 'success' },
      { when: true, message: 'miss', state: 'failure' },
    ],
  };
  const load = (doc: unknown) => {
    const draft = draftFromDefinition(doc);
    if (!draft) throw new Error('fixture failed to load');
    return draft;
  };

  /** The strike tool, built by hand — what `load(TOOL)` yields. */
  function handTool(): ToolDraft {
    const draft = newDraft();
    draft.name = 'strike';
    draft.description = 'Strike.';
    draft.parameters = [{ id: 'p1', name: 'weapon', type: 'string', defaultValue: 'Excalibur', description: '', min: '', max: '' }];
    draft.gateMode = 'available';
    draft.gateConditions = [
      gateChip('inventory', 'lantern', 'eq', { kind: 'string', text: 'equipped' }),
      gateChip('inventoryCount', 'ring', 'gte', { kind: 'number', text: '1' }),
    ];
    draft.rollForm = 'inventory';
    draft.rollInventory = { path: 'sword.metadata.damage', fallback: '1d4' };
    draft.effects = [
      { ...newDraftEffect('inventory'), inventory: { action: 'equip', items: ['{{params.weapon}}'], equip: true } },
      {
        ...newDraftEffect('inventory'),
        when: { kind: 'outcome-state', state: 'failure' },
        inventory: { action: 'pick_up', items: ['magic bow'], equip: false },
      },
    ];
    draft.outcomes = [
      {
        ...draft.outcomes[0],
        message: 'hit',
        conditions: [
          chip({ kind: 'inventory', key: '{{params.weapon}}' }, 'eq', { kind: 'string', text: 'equipped' }),
          chip({ kind: 'inventoryCount', key: 'ring' }, 'gte', { kind: 'number', text: '2' }),
        ],
      },
      { ...draft.outcomes[1], message: 'miss', state: 'failure' },
    ];
    return draft;
  }

  describe('round trip', () => {
    it('serialises back to the same definition', () => {
      const draft = load(TOOL);
      expect(draft.rollForm).toBe('inventory');
      expect(draft.effects.map((e) => e.kind)).toEqual(['inventory', 'inventory']);
      const out = definitionFromDraft(draft) as Record<string, unknown>;
      expect(out['roll']).toEqual(TOOL.roll);
      expect(out['availableWhen']).toEqual(TOOL.availableWhen);
      expect(out['effects']).toEqual(TOOL.effects);
      expect((out['outcomes'] as Array<{ when: unknown }>)[0].when).toEqual(TOOL.outcomes[0].when);
      expect(safeParse(out).success).toBe(true);
      expect(errorsOf(draft)).toEqual([]);
    });

    it('serialises back to the same definition (hand-built twin)', () => {
      const draft = handTool();
      const out = definitionFromDraft(draft) as Record<string, unknown>;
      expect(out['roll']).toEqual(TOOL.roll);
      expect(out['availableWhen']).toEqual(TOOL.availableWhen);
      expect(out['effects']).toEqual(TOOL.effects);
      expect((out['outcomes'] as Array<{ when: unknown }>)[0].when).toEqual(TOOL.outcomes[0].when);
    });

    it('a fresh inventory effect emits only the active half', () => {
      const draft = bareDraft();
      draft.effects = [{ ...newDraftEffect('inventory'), inventory: { action: 'pick_up', items: ['bow', ''], equip: true } }];
      expect(definitionFromDraft(draft)['effects']).toEqual([{ inventory: { action: 'pick_up', items: ['bow'] } }]);
    });

    it('emits equip only on pick_up and only when false, in the order action, items, equip', () => {
      const draft = bareDraft();
      draft.effects = [
        { ...newDraftEffect('inventory'), inventory: { action: 'pick_up', items: ['bow'], equip: false } },
        { ...newDraftEffect('inventory'), inventory: { action: 'equip', items: ['bow'], equip: false } },
      ];
      const effects = definitionFromDraft(draft)['effects'] as Array<{ inventory: Record<string, unknown> }>;
      expect(Object.keys(effects[0].inventory)).toEqual(['action', 'items', 'equip']);
      expect(effects[1].inventory).toEqual({ action: 'equip', items: ['bow'] });
    });

    it('emits inventory and inventoryCount keys UNTRIMMED and aggregate keys TRIMMED', () => {
      const row = { ...newDraft().outcomes[0], message: 'm' };
      row.conditions = [
        chip({ kind: 'inventory', key: ' sword ' }, 'eq', { kind: 'string', text: 'equipped' }),
        chip({ kind: 'inventoryCount', key: ' sword ' }, 'gte', { kind: 'number', text: '1' }),
        chip({ kind: 'inventoryAggregate', key: ' equipped.count ' }, 'gte', { kind: 'number', text: '1' }),
      ];
      expect(whenFromConditions(row.conditions)).toEqual({
        inventory: { ' sword ': { eq: 'equipped' } },
        inventoryCount: { ' sword ': { gte: 1 } },
        inventoryAggregate: { 'equipped.count': { gte: 1 } },
      });
    });
  });

  describe('validation', () => {
    it('a state chip needs one of the four states, and only = or ≠', () => {
      const draft = load(TOOL);
      const chipOf = draft.outcomes[0].conditions.find((c) => c.subject.kind === 'inventory')!;
      chipOf.operand = { kind: 'string', text: 'worn' };
      expect(messagesOf(draft)).toContain('pick a state: equipped, carried, reachable or absent');
      chipOf.comparator = 'gt';
      expect(messagesOf(draft)).toContain('an inventory state can only be tested with = or ≠');
    });

    it('a state chip needs one of the four states, and only = or ≠ (hand-built twin)', () => {
      const draft = handTool();
      const chipOf = draft.outcomes[0].conditions.find((c) => c.subject.kind === 'inventory')!;
      chipOf.operand = { kind: 'string', text: 'worn' };
      expect(messagesOf(draft)).toContain('pick a state: equipped, carried, reachable or absent');
      chipOf.comparator = 'gt';
      expect(messagesOf(draft)).toContain('an inventory state can only be tested with = or ≠');
    });

    it('a gate key may not carry a placeholder', () => {
      const draft = load(TOOL);
      draft.gateConditions[0].key = '{{params.weapon}}';
      expect(messagesOf(draft)).toContain('a gate is answered before any parameter exists — name the object literally');
    });

    it('a gate key may not carry a placeholder (hand-built twin)', () => {
      const draft = handTool();
      draft.gateConditions[0].key = '{{params.weapon}}';
      expect(messagesOf(draft)).toContain('a gate is answered before any parameter exists — name the object literally');
    });

    it('an inventory effect states the per-action rule and names undeclared parameters', () => {
      const draft = load(TOOL);
      draft.effects[0].inventory = { action: 'equip', items: ['{{params.ghost}}'], equip: true };
      expect(messagesOf(draft)).toContain('{{params.ghost}} names no declared parameter');
      draft.effects[0].inventory = { action: 'equip', items: [''], equip: true };
      expect(messagesOf(draft)).toContain('equip needs at least one object');
    });

    it('an inventory effect names undeclared parameters (hand-built twin: the half that reads no grammar twin)', () => {
      const draft = handTool();
      draft.effects[0].inventory = { action: 'equip', items: ['{{params.ghost}}'], equip: true };
      expect(messagesOf(draft)).toContain('{{params.ghost}} names no declared parameter');
    });

    it('a $inventory roll needs an object path and dice to fall back on', () => {
      const draft = load(TOOL);
      draft.rollInventory = { path: 'sword.title', fallback: 'four' };
      const messages = messagesOf(draft);
      expect(messages).toContain('name "<object>.metadata.<key>", e.g. "sword.metadata.damage"');
      expect(messages).toContain('the fallback must be dice notation like "1d4"');
    });

    it('a $inventory roll needs dice to fall back on (hand-built twin: the half that reads no grammar twin)', () => {
      const draft = handTool();
      draft.rollInventory = { path: 'sword.metadata.damage', fallback: 'four' };
      expect(messagesOf(draft)).toContain('the fallback must be dice notation like "1d4"');
      expect(validateDraft(draft).find((i) => i.message.startsWith('the fallback'))!.where).toEqual({ section: 'roll', field: 'inventory' });
    });

    it('narrows the {{dice}} warning to the range form', () => {
      const draft = bareDraft();
      draft.outcomes[0].message = 'rolled {{dice}}';
      draft.rollForm = 'range';
      expect(warningsOf(draft)).toContain('{{dice}} renders as an empty string outside the dice form');
      draft.rollForm = 'inventory';
      expect(warningsOf(draft)).not.toContain('{{dice}} renders as an empty string outside the dice form');
    });
  });

  describe('R-8: three measured v4 Workbench defects ported as v4 does them (filing candidates)', () => {
    const draftWithParam = () => {
      const draft = handTool();
      draft.effects[0].inventory = { action: 'equip', items: ['{{params.weapon}} blade'], equip: true };
      return draft;
    };

    it('renameParameterEverywhere does not rewrite an inventory effect, a chip key or the roll path', () => {
      const renamed = renameParameterEverywhere(draftWithParam(), 'weapon', 'arm');
      expect(renamed.effects[0].inventory.items).toEqual(['{{params.weapon}} blade']);
      expect(renamed.outcomes[0].conditions[0].subject).toEqual({ kind: 'inventory', key: '{{params.weapon}}' });
      expect(messagesOf(renamed)).toContain('{{params.weapon}} names no declared parameter');
    });

    it('findParameterReferences reports none of those three sites', () => {
      expect(findParameterReferences(draftWithParam(), 'weapon')).toEqual([]);
    });
  });
});

describe('log effects (e4a60ad9e)', () => {
  const logDraft = (log: string, extra?: Partial<ToolDraft>): ToolDraft => {
    const draft = bareDraft();
    draft.effects = [{ ...newDraftEffect('log'), log }];
    return Object.assign(draft, extra);
  };

  it('round-trips a log line with and without a when', () => {
    const doc = {
      name: 'x',
      description: 'x',
      effects: [{ log: 'Opened.' }, { when: { outcome: { eq: 'success' } }, log: 'It worked.' }],
      outcomes: [{ when: true, message: '-', state: 'info' }],
    };
    const draft = draftFromDefinition(doc)!;
    expect(draft.effects.map((e) => e.kind)).toEqual(['log', 'log']);
    expect(definitionFromDraft(draft)['effects']).toEqual(doc.effects);
  });

  it('round-trips a log line with and without a when (hand-built twin)', () => {
    const draft = bareDraft();
    draft.effects = [
      { ...newDraftEffect('log'), log: 'Opened.' },
      { ...newDraftEffect('log'), log: 'It worked.', when: { kind: 'outcome-state', state: 'success' } },
    ];
    expect(definitionFromDraft(draft)['effects']).toEqual([{ log: 'Opened.' }, { when: { outcome: { eq: 'success' } }, log: 'It worked.' }]);
  });

  it('trims the line on save and flags an empty one', () => {
    expect(definitionFromDraft(logDraft('  spaced  '))['effects']).toEqual([{ log: 'spaced' }]);
    const blank = validateDraft(logDraft('   ')).filter((i) => i.severity === 'error');
    expect(blank.map((i) => i.message)).toEqual(['a log line needs some prose to write down']);
    expect(blank[0].where).toMatchObject({ section: 'effect', field: 'log' });
  });

  it('warns about an undeclared parameter in the line', () => {
    const messages = validateDraft(logDraft('Opened by {{params.ghost}}.')).map((i) => i.message);
    expect(messages.some((m) => m.includes('params.ghost'))).toBe(true);
  });

  it('measures the length on the TRIMMED line and still audits placeholders past the limit', () => {
    expect(errorsOf(logDraft(' ' + 'x'.repeat(1000) + ' '))).toEqual([]);
    const long = messagesOf(logDraft('x'.repeat(1001) + '{{params.ghost}}'));
    expect(long[0]).toBe('a log line is at most 1000 characters');
    expect(long.some((m) => m.includes('params.ghost'))).toBe(true);
  });

  it('renames a parameter in a log line, and its find arm returns before the wardrobe check', () => {
    const draft = logDraft('Log {{params.a}} {{params.a}}.');
    draft.parameters = [{ id: 'p1', name: 'a', type: 'string', defaultValue: 'x', description: '', min: '', max: '' }];
    expect(renameParameterEverywhere(draft, 'a', 'b').effects[0].log).toBe('Log {{params.b}} {{params.b}}.');
    expect(findParameterReferences(draft, 'a')).toEqual(['effect 1: the log line renders it']);
  });
});

describe('conditionSlotKey and the three inventory subjects', () => {
  it('keys the inventory subjects by subject, key and comparator', () => {
    expect(conditionSlotKey({ subject: { kind: 'inventory', key: 'sword' }, comparator: 'eq' })).toBe('inventory:sword:eq');
    expect(conditionSlotKey({ subject: { kind: 'inventoryCount', key: 'sword' }, comparator: 'gte' })).toBe('inventoryCount:sword:gte');
    expect(conditionSlotKey({ subject: { kind: 'inventoryAggregate', key: 'equipped.count' }, comparator: 'gt' })).toBe('inventoryAggregate:equipped.count:gt');
  });
});
