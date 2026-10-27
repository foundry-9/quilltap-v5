import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/pascal-inventory-twins.oracle.ndjson';
import { mockInventoryContext } from './bench-inventory';
import {
  aggregateKeyEndsInFold,
  inventoryAggregateHolds,
  isListAggregate,
  parseAggregateKey,
} from './inventory-aggregate';
import { resolveInventoryRef, stateOf, type InventoryContext } from './inventory-refs';
import {
  gateNamesInventory,
  inventoryCountHolds,
  inventoryStateHolds,
  parseInventoryOperand,
  resolveInventoryValue,
} from './inventory-subjects';

/**
 * The corpus differential for the Workbench's client-safe Inventory
 * evaluators at the pin `322a5488e` (P4.D334 R-2 / R-13): v4's REAL
 * `inventory-aggregate`, `inventory-subjects`, `bench-inventory` and the client
 * half of `inventory/refs`, recorded by
 * `apps/web/oracle/pascal-inventory-twins.recorder.ts`.
 *
 * Every row carries `needs` — the sibling lane whose code the row's truth
 * reads (the list rule / `compareOrdered` / `splitInventoryReference` are
 * P4.D333's, `kitState` is P4.D330's); both landed at the round's
 * unification, so every row replays.
 */

type Need = 'P4D333' | 'P4D330';
interface Row {
  fn: string;
  id: string;
  needs: Need[];
  args: Record<string, unknown> | unknown[];
  out: string;
}

interface Obj {
  name: string;
  title: string;
  metadata: Record<string, unknown>;
}
interface CtxSpec {
  states: Record<string, 'equipped' | 'carried' | 'reachable' | 'absent'>;
  counts: Record<string, number>;
  items: Record<string, { metadata: Record<string, unknown> }>;
  scopes: { equipped: Obj[]; owned: Obj[]; reachable: Obj[] };
}

const SWORD: Obj = {
  name: 'sword',
  title: 'Excalibur',
  metadata: { damage: '2d6', toolVersion: 3, toolAbilities: ['suppress', 'amplify'], tag: 'bright' },
};
const BOW: Obj = { name: 'bow', title: 'bow', metadata: { toolVersion: 5, toolAbilities: ['suppress'], weight: 2 } };
const LAMP: Obj = { name: 'lamp', title: 'Storm lamp', metadata: { tag: 'dim' } };
const EMPTY: CtxSpec = { states: {}, counts: {}, items: {}, scopes: { equipped: [], owned: [], reachable: [] } };
const MIXED: CtxSpec = {
  states: { excalibur: 'equipped', sword: 'equipped', bow: 'carried', lamp: 'reachable' },
  counts: { excalibur: 1, sword: 1, bow: 0, lamp: 0, dagger: 2 },
  items: {
    sword: { metadata: { damage: '2d6', toolVersion: 3, reach: 2, bright: true } },
    excalibur: { metadata: { damage: '2d6', toolVersion: 3, reach: 2, bright: true } },
    bow: { metadata: { damage: 'bad dice', reach: 'far' } },
  },
  scopes: { equipped: [SWORD], owned: [SWORD, BOW], reachable: [SWORD, BOW, LAMP] },
};
const CONTEXTS: Record<string, CtxSpec | null> = { empty: EMPTY, mixed: MIXED, null: null };

function scripted(spec: CtxSpec | null): InventoryContext | null {
  if (!spec) return null;
  return {
    characterId: 'scripted',
    chatId: null,
    state: (ref) => spec.states[ref.trim().toLowerCase()] ?? 'absent',
    equippedKinCount: (ref) => spec.counts[ref.trim().toLowerCase()] ?? 0,
    item: (ref) => {
      const found = spec.items[ref.trim().toLowerCase()];
      return found ? ({ metadata: found.metadata } as never) : null;
    },
    scopeObjects: (scope) => spec.scopes[scope],
  };
}

const origin = (scope: string, name: string) => ({ scope, id: scope === 'general' ? null : `${scope}-1`, name });
function item(over: Record<string, unknown>) {
  return {
    id: 'x',
    kind: 'item',
    name: 'x',
    title: null,
    isArchetype: false,
    parentId: null,
    itemIds: [],
    metadata: {},
    folder: '',
    archivedAt: null,
    ...over,
  };
}
const POOL_ITEMS = [
  item({ id: 'id-sword', name: 'sword', title: 'Excalibur', origin: origin('character', 'Imogen') }),
  item({ id: 'id-bow-arch', name: 'magic bow', isArchetype: true, origin: origin('project', 'Thornfield') }),
  item({ id: 'id-bow', name: 'magic bow', parentId: 'id-bow-arch', origin: origin('character', 'Imogen') }),
  item({ id: 'id-old', name: 'rusty axe', archivedAt: '2026-01-01T00:00:00.000Z', origin: origin('character', 'Imogen') }),
  item({ id: 'id-shared-old', name: 'shared axe', archivedAt: '2026-01-01T00:00:00.000Z', origin: origin('general', 'Quilltap General') }),
  item({ id: 'id-lamp', name: 'lamp', title: 'Storm lamp', origin: origin('general', 'Quilltap General') }),
  item({ id: 'id-torch', name: 'torch', title: 'lamp', origin: origin('general', 'Quilltap General') }),
  item({ id: 'id-dup-shared', name: 'dagger', origin: origin('general', 'Quilltap General') }),
  item({ id: 'id-dup-own', name: 'dagger', origin: origin('character', 'Imogen') }),
  item({ id: 'id-kit', kind: 'kit', name: 'kit', itemIds: ['id-sword', 'id-dup-own'], origin: origin('character', 'Imogen') }),
];
const OWN = new Set(['id-sword', 'id-bow', 'id-old', 'id-dup-own', 'id-kit']);

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

const byFn = (fn: string) => ROWS.filter((r) => r.fn === fn);

function replay(row: Row, run: (row: Row) => unknown): void {
  it(row.id, () => expect(JSON.stringify(run(row))).toBe(row.out));
}

describe('the Inventory evaluator twins agree with v4 row for row', () => {
  it('carries the whole recorded corpus', () => {
    expect(ROWS).toHaveLength(636);
    expect(byFn('parseAggregateKey')).toHaveLength(46);
    expect(byFn('inventoryAggregateHolds')).toHaveLength(90);
    expect(byFn('resolveInventoryRef')).toHaveLength(320);
    expect(ROWS.filter((r) => r.needs.length === 0)).toHaveLength(536);
  });

  describe('parseAggregateKey', () => {
    for (const row of byFn('parseAggregateKey')) {
      replay(row, (r) => parseAggregateKey((r.args as string[])[0]));
    }
  });

  describe('isListAggregate', () => {
    for (const row of byFn('isListAggregate')) {
      replay(row, (r) => {
        const parsed = parseAggregateKey((r.args as string[])[0]);
        return parsed.ok && isListAggregate(parsed.parsed);
      });
    }
  });

  describe('aggregateKeyEndsInFold', () => {
    for (const row of byFn('aggregateKeyEndsInFold')) {
      replay(row, (r) => {
        const parsed = parseAggregateKey((r.args as string[])[0]);
        return parsed.ok && aggregateKeyEndsInFold(parsed.parsed);
      });
    }
  });

  describe('inventoryAggregateHolds', () => {
    for (const row of byFn('inventoryAggregateHolds')) {
      replay(row, (r) => {
        const { ctx, key, comparator } = r.args as { ctx: string; key: string; comparator: Record<string, unknown> };
        const declines: string[] = [];
        const holds = inventoryAggregateHolds(comparator as never, key, scripted(CONTEXTS[ctx]), {
          resolveOperand: (k) => comparator[k] as never,
          onDecline: (reason) => declines.push(reason),
        });
        return { holds, declines };
      });
    }
  });

  describe('parseInventoryOperand', () => {
    for (const row of byFn('parseInventoryOperand')) {
      replay(row, (r) => parseInventoryOperand((r.args as string[])[0]));
    }
  });

  describe('resolveInventoryValue', () => {
    for (const row of byFn('resolveInventoryValue')) {
      replay(row, (r) => {
        const { ctx, operand } = r.args as { ctx: string; operand: { $inventory: string; fallback: number } };
        if (ctx === 'odd') {
          const odd: CtxSpec = { ...MIXED, items: { sword: { metadata: { v: null, w: Number.POSITIVE_INFINITY } } } };
          return resolveInventoryValue(operand, scripted(odd));
        }
        return resolveInventoryValue(operand, scripted(CONTEXTS[ctx]));
      });
    }
  });

  describe('inventoryStateHolds', () => {
    for (const row of byFn('inventoryStateHolds')) {
      replay(row, (r) => {
        const { ctx, test, ref } = r.args as { ctx: string; test: { eq?: string; neq?: string }; ref: string };
        const declines: string[] = [];
        const holds = inventoryStateHolds(test, ref, scripted(CONTEXTS[ctx]), (reason) => declines.push(reason));
        return { holds, declines };
      });
    }
  });

  describe('inventoryCountHolds', () => {
    for (const row of byFn('inventoryCountHolds')) {
      replay(row, (r) => {
        const { ctx, test, ref } = r.args as { ctx: string; test: Record<string, number>; ref: string };
        const declines: string[] = [];
        const holds = inventoryCountHolds(test, ref, scripted(CONTEXTS[ctx]), (reason) => declines.push(reason));
        return { holds, declines };
      });
    }
  });

  describe('gateNamesInventory', () => {
    for (const row of byFn('gateNamesInventory')) {
      replay(row, (r) => gateNamesInventory((r.args as { gate: never }).gate));
    }
  });

  describe('mockInventoryContext', () => {
    const PROBES = ['sword', 'SWORD', ' sword ', 'bow', 'magic bow', 'lamp', 'ghost', 'unlisted', ''];
    for (const row of byFn('mockInventoryContext')) {
      replay(row, (r) => {
        const ctx = mockInventoryContext((r.args as { rows: never[] }).rows);
        return {
          characterId: ctx.characterId,
          chatId: ctx.chatId,
          state: PROBES.map((p) => ctx.state(p)),
          count: PROBES.map((p) => ctx.equippedKinCount(p)),
          item: PROBES.map((p) => ctx.item(p)),
          equipped: ctx.scopeObjects('equipped'),
          owned: ctx.scopeObjects('owned'),
          reachable: ctx.scopeObjects('reachable'),
        };
      });
    }
  });

  describe('resolveInventoryRef / stateOf', () => {
    for (const row of byFn('resolveInventoryRef')) {
      replay(row, (r) => {
        const { equipped, opts, ref } = r.args as { equipped: string[]; opts: Record<string, boolean>; ref: string };
        const pool = { items: POOL_ITEMS as never[], owns: (id: string) => OWN.has(id) };
        const matches = resolveInventoryRef(ref, pool, new Set(equipped), opts);
        return {
          matches: matches.map((m) => ({ id: m.item.id, equipped: m.equipped, own: m.own })),
          state: stateOf(matches),
        };
      });
    }
  });
});
