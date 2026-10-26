/**
 * The availability gate's evaluator — a CASE-FOR-CASE port of the
 * `evaluateToolGate` describe in v4's own
 * `__tests__/unit/lib/pascal/custom-tools-gate.test.ts` (`6864bf0e`).
 *
 * v4's file also covers the schema (ported at greater depth in
 * `custom-tool-types.gate.spec.ts`, against captured v4 bytes) and roster
 * resolution (server-side; P4.d19's). What is left here is the part that runs in
 * the browser, and the two things about it that are easy to get subtly wrong:
 * the fail-soft asymmetry between the two clauses on a character with no sheet,
 * and `withheldBy` being ABSENT rather than null when a tool is available.
 */

import { describe, expect, it } from 'vitest';

import type { ToolGate, ToolGateGroup } from './custom-tool-types';
import { evaluateToolGate, gateNamesProgress, hasToolGate } from './tool-gate';

const gate = (metadata: ToolGateGroup['metadata']): ToolGate => ({ metadata });

describe('evaluateToolGate', () => {
  it('reports an ungated definition as available', () => {
    expect(hasToolGate({})).toBe(false);
    expect(evaluateToolGate({}, { anything: true })).toEqual({ available: true });
  });

  it('reports a definition carrying either clause as gated', () => {
    expect(hasToolGate({ availableWhen: gate({ rank: { gte: 3 } }) })).toBe(true);
    expect(hasToolGate({ withheldWhen: gate({ rank: { gte: 3 } }) })).toBe(true);
  });

  it('offers a tool whose availableWhen holds', () => {
    const definition = { availableWhen: gate({ toolAbilities: { contains: 'programmable' } }) };
    expect(evaluateToolGate(definition, { toolAbilities: 'programmable, networked' })).toEqual({
      available: true,
    });
  });

  it('withholds a tool whose availableWhen fails', () => {
    const definition = { availableWhen: gate({ toolAbilities: { contains: 'programmable' } }) };
    expect(evaluateToolGate(definition, { toolAbilities: 'mechanical' })).toEqual({
      available: false,
      withheldBy: 'availableWhen',
    });
  });

  it('ANDs every test in a gate', () => {
    const definition = { availableWhen: gate({ rank: { gte: 3 }, cleared: { eq: true } }) };
    expect(evaluateToolGate(definition, { rank: 4, cleared: true }).available).toBe(true);
    expect(evaluateToolGate(definition, { rank: 4, cleared: false }).available).toBe(false);
    expect(evaluateToolGate(definition, { rank: 1, cleared: true }).available).toBe(false);
  });

  it('withholds a tool whose withheldWhen holds', () => {
    const definition = { withheldWhen: gate({ novice: { eq: true } }) };
    expect(evaluateToolGate(definition, { novice: true })).toEqual({
      available: false,
      withheldBy: 'withheldWhen',
    });
    expect(evaluateToolGate(definition, { novice: false })).toEqual({ available: true });
  });

  it('treats an absent key fail-soft, which cuts opposite ways for the two clauses', () => {
    // The asymmetry that makes both clauses worth having: a character with no
    // sheet qualifies for nothing and is disqualified by nothing.
    expect(evaluateToolGate({ availableWhen: gate({ rank: { gte: 3 } }) }, {}).available).toBe(
      false,
    );
    expect(evaluateToolGate({ withheldWhen: gate({ rank: { gte: 3 } }) }, {}).available).toBe(true);
    expect(evaluateToolGate({ availableWhen: gate({ rank: { gte: 3 } }) }, null).available).toBe(
      false,
    );
  });

  it('takes an undefined sheet as an empty one', () => {
    expect(evaluateToolGate({ withheldWhen: gate({ rank: { gte: 3 } }) }, undefined)).toEqual({
      available: true,
    });
  });

  it('declines a test the stored type cannot sustain, rather than throwing', () => {
    const ordering = { availableWhen: gate({ rank: { gte: 3 } }) };
    expect(evaluateToolGate(ordering, { rank: 'senior' }).available).toBe(false);
    expect(evaluateToolGate(ordering, { rank: ['a'] }).available).toBe(false);

    const containment = { availableWhen: gate({ abilities: { contains: 'programmable' } }) };
    expect(evaluateToolGate(containment, { abilities: 7 }).available).toBe(false);
    expect(evaluateToolGate(containment, { abilities: { list: [] } }).available).toBe(false);
  });

  it('does not treat absence as inequality under neq/ncontains', () => {
    expect(evaluateToolGate({ availableWhen: gate({ rank: { neq: 3 } }) }, {}).available).toBe(
      false,
    );
    expect(evaluateToolGate({ availableWhen: gate({ a: { ncontains: 'x' } }) }, {}).available).toBe(
      false,
    );
  });

  it('omits withheldBy entirely when the tool is available — absent, not null', () => {
    const verdict = evaluateToolGate({ withheldWhen: gate({ novice: { eq: true } }) }, {});
    expect(verdict.available).toBe(true);
    expect('withheldBy' in verdict).toBe(false);
  });

  it('reports availableWhen first when a file somehow declares both', () => {
    // The schema rejects such a file at load, so this can only arrive by hand.
    // The evaluator is still total, and reads the clauses in written order.
    const both = {
      availableWhen: gate({ rank: { gte: 3 } }),
      withheldWhen: gate({ novice: { eq: true } }),
    };
    expect(evaluateToolGate(both, { rank: 1, novice: true })).toEqual({
      available: false,
      withheldBy: 'availableWhen',
    });
    expect(evaluateToolGate(both, { rank: 4, novice: true })).toEqual({
      available: false,
      withheldBy: 'withheldWhen',
    });
  });
});

/**
 * Any-of gates (v4 `a53024d3c`) — v4's `any-of gates` describe
 * (`__tests__/unit/lib/pascal/inventory-aggregate.test.ts`), case for case.
 */
describe('evaluateToolGate — any-of clauses', () => {
  const AB: ToolGateGroup = { metadata: { a: { eq: true }, b: { eq: true } } };
  const AC: ToolGateGroup = { metadata: { a: { eq: true }, c: { eq: true } } };
  const available = { availableWhen: [AB, AC] };
  const withheld = { withheldWhen: [AB, AC] };

  const cases: Array<[Record<string, unknown>, boolean, number | undefined]> = [
    [{ a: true, b: true }, true, 0],
    [{ a: true, c: true }, true, 1],
    [{ a: true, b: true, c: true }, true, 0],
    [{ a: true }, false, undefined],
    [{ b: true, c: true }, false, undefined],
  ];
  for (const [sheet, holdsGate, line] of cases) {
    it(`a∧b ∨ a∧c on ${JSON.stringify(sheet)}`, () => {
      expect(evaluateToolGate(available, sheet)).toEqual(
        holdsGate ? { available: true, heldGroup: line } : { available: false, withheldBy: 'availableWhen' },
      );
      expect(evaluateToolGate(withheld, sheet)).toEqual(
        holdsGate ? { available: false, withheldBy: 'withheldWhen', heldGroup: line } : { available: true },
      );
    });
  }

  it('reports heldGroup only when a group held — opening availableWhen, or closing withheldWhen', () => {
    // availableWhen held → available, WITH the group that opened it; failed →
    // withheld, WITHOUT a group (none held). withheldWhen held → withheld,
    // WITH the closing group; unsatisfied → available, WITHOUT.
    expect(JSON.stringify(evaluateToolGate(available, { a: true, c: true }))).toBe(
      '{"available":true,"heldGroup":1}',
    );
    expect(JSON.stringify(evaluateToolGate(available, {}))).toBe(
      '{"available":false,"withheldBy":"availableWhen"}',
    );
    expect(JSON.stringify(evaluateToolGate(withheld, { a: true, b: true }))).toBe(
      '{"available":false,"withheldBy":"withheldWhen","heldGroup":0}',
    );
    expect(JSON.stringify(evaluateToolGate(withheld, {}))).toBe('{"available":true}');
  });

  it('never reports heldGroup for a bare-object clause', () => {
    expect(evaluateToolGate({ availableWhen: AB }, { a: true, b: true })).toEqual({ available: true });
    expect(evaluateToolGate({ withheldWhen: AB }, { a: true, b: true })).toEqual({
      available: false,
      withheldBy: 'withheldWhen',
    });
  });

  it('a null inventory declines every Inventory group: availableWhen closed, withheldWhen unsatisfied', () => {
    const groups: ToolGateGroup[] = [
      { inventoryAggregate: { 'equipped.count': { gte: 0 } } },
      { inventory: { sword: { eq: 'absent' } } },
      { inventoryCount: { sword: { eq: 0 } } },
    ];
    expect(evaluateToolGate({ availableWhen: groups }, {})).toEqual({
      available: false,
      withheldBy: 'availableWhen',
    });
    expect(evaluateToolGate({ withheldWhen: groups }, {})).toEqual({ available: true });
    // A metadata group beside them still opens the clause, and names itself.
    expect(evaluateToolGate({ availableWhen: [...groups, { metadata: { a: { eq: 1 } } }] }, { a: 1 })).toEqual({
      available: true,
      heldGroup: 3,
    });
  });
});

describe('gateNamesProgress', () => {
  it('is false for no gate, and for a gate naming no progress key', () => {
    expect(gateNamesProgress(undefined)).toBe(false);
    expect(gateNamesProgress(gate({ rank: { gte: 1 } }))).toBe(false);
  });

  it('counts progress KEYS, so an empty progress record names nothing', () => {
    expect(gateNamesProgress({ metadata: { a: { eq: 1 } }, progress: {} })).toBe(false);
    expect(gateNamesProgress({ progress: { 'cannon.complete': { eq: true } } })).toBe(true);
  });

  it('reads every group of an any-of clause', () => {
    expect(gateNamesProgress([{ metadata: { a: { eq: 1 } } }, { progress: { 'cannon.percent': { gte: 50 } } }])).toBe(
      true,
    );
    expect(gateNamesProgress([{ metadata: { a: { eq: 1 } } }, { progress: {} }])).toBe(false);
  });
});
