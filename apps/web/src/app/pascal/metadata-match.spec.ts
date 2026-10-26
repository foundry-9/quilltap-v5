/**
 * The fail-soft comparison table (v4 `lib/pascal/metadata-match.ts`).
 *
 * v4 has no dedicated suite for this module — it is exercised through its two
 * callers, and the gate half of that is ported in `tool-gate.spec.ts`. What is
 * pinned HERE is the thing a caller can observe and the gate cannot: the decline
 * REASONS. They are strings a caller logs, so a paraphrase would be a silent
 * divergence in a debug log rather than a failing test; every branch that can
 * produce one is exercised, with the wording copied from v4.
 */

import { describe, expect, it } from 'vitest';

import type { MetadataComparator } from './custom-tool-types';
import { isPrimitive, metadataComparatorHolds, type Primitive } from './metadata-match';

/** Run one comparator with the gate's identity resolver, collecting declines. */
function run(
  comparator: MetadataComparator,
  key: string,
  metadata: Record<string, unknown>,
): { held: boolean; declines: string[] } {
  const declines: string[] = [];
  const held = metadataComparatorHolds(comparator, key, metadata, {
    resolveOperand: (comparatorKey) => comparator[comparatorKey] as Primitive,
    onDecline: (reason) => declines.push(reason),
  });
  return { held, declines };
}

describe('isPrimitive', () => {
  it('admits exactly the three comparable types', () => {
    expect(isPrimitive(3)).toBe(true);
    expect(isPrimitive('x')).toBe(true);
    expect(isPrimitive(false)).toBe(true);
    expect(isPrimitive(null)).toBe(false);
    expect(isPrimitive(undefined)).toBe(false);
    expect(isPrimitive([1])).toBe(false);
    expect(isPrimitive({})).toBe(false);
  });
});

describe('metadataComparatorHolds — the declines', () => {
  it('declines an absent key', () => {
    expect(run({ eq: 1 }, 'rank', {})).toEqual({
      held: false,
      declines: ['the character has no such metadata key'],
    });
  });

  it('names what a non-primitive subject holds', () => {
    expect(run({ eq: 1 }, 'rank', { rank: null }).declines).toEqual([
      'the key holds null, which cannot be compared',
    ]);
    // A list of primitives now compares entry by entry (v4 `a53024d3c`), so
    // only a list holding a non-primitive still declines as uncomparable.
    expect(run({ eq: 1 }, 'rank', { rank: ['a', { b: 1 }] }).declines).toEqual([
      'the key holds an array that is not all numbers, strings and booleans, which cannot be compared',
    ]);
    expect(run({ eq: 1 }, 'rank', { rank: { deep: 1 } }).declines).toEqual([
      'the key holds an object, which cannot be compared',
    ]);
  });

  it('declines ordering against a non-number, naming the comparator and the subject', () => {
    expect(run({ gte: 3 }, 'rank', { rank: 'senior' }).declines).toEqual([
      'gte orders "senior", and only numbers can be ordered',
    ]);
    expect(run({ lt: 3 }, 'rank', { rank: true }).declines).toEqual([
      'lt orders true, and only numbers can be ordered',
    ]);
  });

  it('declines ordering when the OPERAND is not a number either', () => {
    // Reachable through the outcome table's resolver, not through a gate, whose
    // schema admits only literals — the table is shared, so the branch is real.
    const declines: string[] = [];
    const held = metadataComparatorHolds({ gt: 3 }, 'rank', { rank: 4 }, {
      resolveOperand: () => 'three',
      onDecline: (reason) => declines.push(reason),
    });
    expect(held).toBe(false);
    expect(declines).toEqual(['gt orders 4, and only numbers can be ordered']);
  });

  it('declines containment against a non-string, under both keys', () => {
    expect(run({ contains: 'x' }, 'abilities', { abilities: 7 }).declines).toEqual([
      'contains searches 7, and only a string can contain a substring',
    ]);
    expect(run({ ncontains: 'x' }, 'abilities', { abilities: 7 }).declines).toEqual([
      'ncontains searches 7, and only a string can contain a substring',
    ]);
  });

  it('fails an ordinary miss WITHOUT a decline — a false test is not a complaint', () => {
    expect(run({ gte: 5 }, 'rank', { rank: 3 })).toEqual({ held: false, declines: [] });
    expect(run({ eq: 5 }, 'rank', { rank: 3 })).toEqual({ held: false, declines: [] });
    expect(run({ neq: 3 }, 'rank', { rank: 3 })).toEqual({ held: false, declines: [] });
    expect(run({ contains: 'q' }, 'a', { a: 'xyz' })).toEqual({ held: false, declines: [] });
    expect(run({ ncontains: 'x' }, 'a', { a: 'xyz' })).toEqual({ held: false, declines: [] });
  });
});

describe('metadataComparatorHolds — the matches', () => {
  it('orders numbers with each of the four keys', () => {
    expect(run({ gt: 3 }, 'r', { r: 4 }).held).toBe(true);
    expect(run({ gte: 4 }, 'r', { r: 4 }).held).toBe(true);
    expect(run({ lt: 5 }, 'r', { r: 4 }).held).toBe(true);
    expect(run({ lte: 4 }, 'r', { r: 4 }).held).toBe(true);
  });

  it('compares eq/neq on the raw value, without coercion', () => {
    expect(run({ eq: 3 }, 'r', { r: 3 }).held).toBe(true);
    // `3 !== '3'`: the stored type is part of the test.
    expect(run({ eq: 3 }, 'r', { r: '3' }).held).toBe(false);
    expect(run({ neq: 3 }, 'r', { r: '3' }).held).toBe(true);
    expect(run({ eq: true }, 'r', { r: true }).held).toBe(true);
  });

  it('searches case-SENSITIVELY, unlike the consult answer', () => {
    expect(run({ contains: 'West' }, 'a', { a: 'the West Door' }).held).toBe(true);
    expect(run({ contains: 'west' }, 'a', { a: 'the West Door' }).held).toBe(false);
  });

  it('ANDs several comparators on one key', () => {
    expect(run({ gte: 3, lte: 5, neq: 4 }, 'r', { r: 5 }).held).toBe(true);
    expect(run({ gte: 3, lte: 5, neq: 4 }, 'r', { r: 4 }).held).toBe(false);
  });

  it('holds vacuously for a comparator with nothing to say', () => {
    // Unreachable through the schema (which demands at least one comparator),
    // but the table must not invent a verdict it was not asked for.
    expect(run({}, 'r', { r: 1 })).toEqual({ held: true, declines: [] });
  });
});

/**
 * The list rule (v4 `a53024d3c`) — v4's `the per-entry rule, in the one table`
 * describe (`__tests__/unit/lib/pascal/inventory-aggregate.test.ts`) and the
 * `knownLanguages` / `sealedOrders` rows of `custom-tools-execution.test.ts`,
 * case for case, with the decline REASONS pinned at v4's bytes.
 */
describe('metadataComparatorHolds — a list compares entry by entry', () => {
  const sheet = {
    knownLanguages: ['Trade Cant', 'Latin'],
    sealedOrders: [{ seal: 'red' }],
    empty: [],
    mixed: ['a', { b: 1 }],
    numbers: [1, 2],
    tags: ['brass', 3, true],
  };

  it('eq when any entry equals; neq when none does', () => {
    expect(run({ eq: 'Latin' }, 'knownLanguages', sheet).held).toBe(true);
    expect(run({ eq: 'latin' }, 'knownLanguages', sheet).held).toBe(false);
    expect(run({ neq: 'Greek' }, 'knownLanguages', sheet).held).toBe(true);
    expect(run({ neq: 'Latin' }, 'knownLanguages', sheet).held).toBe(false);
    expect(run({ eq: 2 }, 'numbers', sheet).held).toBe(true);
  });

  it('contains when any string entry contains; ncontains when none does', () => {
    expect(run({ contains: 'Cant' }, 'knownLanguages', sheet).held).toBe(true);
    expect(run({ ncontains: 'Greek' }, 'knownLanguages', sheet).held).toBe(true);
    expect(run({ ncontains: 'Cant' }, 'knownLanguages', sheet).held).toBe(false);
    expect(run({ ncontains: 'Trade' }, 'knownLanguages', sheet).held).toBe(false);
    expect(run({ eq: 'Trade' }, 'knownLanguages', sheet).held).toBe(false);
  });

  it('skips a non-string entry under contains rather than declining', () => {
    // `3` and `true` cannot contain anything; they are skipped, and `brass`
    // decides. A list of numbers alone therefore contains nothing — a miss, no
    // decline.
    expect(run({ contains: 'ras' }, 'tags', sheet)).toEqual({ held: true, declines: [] });
    expect(run({ contains: '3' }, 'tags', sheet)).toEqual({ held: false, declines: [] });
    expect(run({ contains: '1' }, 'numbers', sheet)).toEqual({ held: false, declines: [] });
  });

  it('declines ordering on a list, naming the key and the fold', () => {
    for (const key of ['gt', 'gte', 'lt', 'lte'] as const) {
      expect(run({ [key]: 1 }, 'numbers', sheet)).toEqual({
        held: false,
        declines: [`${key} orders numbers, which is a list; fold it with .min/.max/.sum/.count first`],
      });
    }
    expect(run({ gt: 1 }, 'knownLanguages', sheet).declines).toEqual([
      'gt orders knownLanguages, which is a list; fold it with .min/.max/.sum/.count first',
    ]);
  });

  it('declines an ordering key BEFORE any operand resolves', () => {
    const resolved: string[] = [];
    const held = metadataComparatorHolds({ gte: 1, eq: 'x' }, 'numbers', sheet, {
      resolveOperand: (k) => {
        resolved.push(k);
        return 1;
      },
    });
    expect(held).toBe(false);
    expect(resolved).toEqual([]);
  });

  it('declines a containment operand that is not a string', () => {
    // Unreachable from a load-validated literal; reachable from a resolver.
    const declines: string[] = [];
    const held = metadataComparatorHolds({ contains: 'x' }, 'knownLanguages', sheet, {
      resolveOperand: () => 7,
      onDecline: (reason) => declines.push(reason),
    });
    expect(held).toBe(false);
    expect(declines).toEqual(['contains looks for 7, and only a string can be a substring']);
  });

  it('reads an empty list as a fact: eq and contains miss, neq and ncontains hold', () => {
    expect(run({ eq: 'x' }, 'empty', sheet)).toEqual({ held: false, declines: [] });
    expect(run({ neq: 'x' }, 'empty', sheet).held).toBe(true);
    expect(run({ ncontains: 'x' }, 'empty', sheet).held).toBe(true);
    expect(run({ contains: 'x' }, 'empty', sheet).held).toBe(false);
  });

  it('still declines a list holding anything but primitives, whole', () => {
    const reason =
      'the key holds an array that is not all numbers, strings and booleans, which cannot be compared';
    for (const comparator of [{ eq: 'a' }, { neq: 'z' }, { contains: 'a' }, { ncontains: 'z' }]) {
      expect(run(comparator, 'mixed', sheet)).toEqual({ held: false, declines: [reason] });
    }
    expect(run({ eq: 'red' }, 'sealedOrders', sheet).declines).toEqual([reason]);
    expect(run({ contains: 'red' }, 'sealedOrders', sheet).declines).toEqual([reason]);
  });
});
