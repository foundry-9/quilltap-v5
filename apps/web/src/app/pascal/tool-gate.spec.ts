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

import type { ToolGate } from './custom-tool-types';
import { evaluateToolGate, hasToolGate } from './tool-gate';

const gate = (metadata: ToolGate['metadata']): ToolGate => ({ metadata });

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
