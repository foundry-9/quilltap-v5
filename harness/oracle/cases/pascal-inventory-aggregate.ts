/**
 * Oracle case: the `inventoryAggregate` subject (P4.D329, v4 `a53024d3c` —
 * `lib/pascal/inventory-aggregate.ts` WHOLE).
 *
 * Drives the REAL code:
 *   parseAggregateKey, isListAggregate, aggregateKeyEndsInFold,
 *   aggregateValue, inventoryAggregateHolds, describeAggregateValue
 *     (lib/pascal/inventory-aggregate.ts)
 *   mockInventoryContext (lib/pascal/bench-inventory.ts — the bench's bag)
 *   inventoryContextFrom (lib/inventory/refs.ts — a REAL context, its answers
 *     RECORDED: `../lib/recorded-inventory.ts`)
 *
 * v4's 22 `it`s of `inventory-aggregate.test.ts` ride here as rows, plus the
 * rows the port needs that v4's suite does not pose: a `-0` against `+0`
 * (`Math.min` / `Math.max` order the zeros; `f64::min` does not promise to),
 * and a float-ORDER sum (`0.1 + 0.2 + 0.3` folded left differs from folded
 * right — `sum` is a left fold in list order).
 *
 * Numbers in a value are encoded so `-0` survives JSON (`"-0"`); an absent
 * fold is `"absent"`. `objectsJson` is hand-written TEXT so a `-0.0` reaches
 * both parsers as negative zero (`JSON.stringify` would print it `0`).
 *
 * The LIST rows (`names`, `titles`, un-folded `metadata.<key>`) compare
 * entry by entry through `metadataComparatorHolds` — P4.D333's list rule. On a
 * branch without it, those `holds` rows are named residue.
 *
 * Run from inside the server checkout (Node 24 `~/.nvm/versions/node/v24.13.1/bin`):
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TZ=UTC npx tsx \
 *     "$V5W/harness/oracle/cases/pascal-inventory-aggregate.ts" \
 *     > /tmp/oracle-pascal-inventory-aggregate.ndjson
 */

import {
  aggregateKeyEndsInFold,
  aggregateValue,
  describeAggregateValue,
  inventoryAggregateHolds,
  isListAggregate,
  parseAggregateKey,
  type AggregateObject,
} from '@/lib/pascal/inventory-aggregate';
import { mockInventoryContext, type MockInventoryRow } from '@/lib/pascal/bench-inventory';
import type { MetadataComparator } from '@/lib/pascal/custom-tool.types';
import type { Primitive } from '@/lib/pascal/metadata-match';
import type { InventoryContext } from '@/lib/inventory/refs';
import type { ResolvedInventoryItem } from '@/lib/schemas/inventory.types';
import { contextOver, obj, recording, type RecordedInventory } from '../lib/recorded-inventory';

const rows: unknown[] = [];

/** `-0`-safe value encoding (see the header). */
function enc(v: unknown): string {
  if (v === undefined) return 'absent';
  if (typeof v === 'number') return Object.is(v, -0) ? '-0' : JSON.stringify(v);
  return JSON.stringify(v);
}

// ------------------------------------------------------------------- parse
const parseKeys = [
  'equipped.count',
  'owned.names',
  'reachable.titles',
  'equipped.metadata.toolAbilities',
  'equipped.metadata.toolVersion.max',
  'owned.metadata.weight.sum',
  'owned.metadata.a.b',
  'owned.metadata.a.b.min',
  'owned.metadata.count',
  'equipped.metadata.a',
  'equipped.metadata.a.b',
  'equipped.metadata.stats.max',
  'equipped.metadata..max',
  'equipped.metadata.',
  'equipped.metadata.x.count',
  '.count',
  '',
  'names',
  'nowhere.count',
  'carried.names',
  'equipped',
  'equipped.',
  'equipped.weight',
  'equipped.count.max',
  'Equipped.count',
  'equipped.Names',
  'reachable.metadata.ünïcode.sum',
];
for (const key of parseKeys) {
  const result = parseAggregateKey(key);
  rows.push({
    kind: 'parse',
    id: `parse-${key || 'empty'}`,
    key,
    result,
    ...(result.ok ? { isList: isListAggregate(result.parsed), endsInFold: aggregateKeyEndsInFold(result.parsed) } : {}),
  });
}

// ------------------------------------------------------------------- value
/** `aggregateValue` over literal objects (v4's own `the folds` block + the port's rows). */
const valueCases: Array<[string, string, string]> = [
  // v4 `the folds` — `two`
  ['folds-max', 'equipped.metadata.toolVersion.max', '[{"name":"a","title":"a","metadata":{"toolVersion":2}},{"name":"b","title":"b","metadata":{"toolVersion":4}},{"name":"c","title":"c","metadata":{}}]'],
  ['folds-min', 'equipped.metadata.toolVersion.min', '[{"name":"a","title":"a","metadata":{"toolVersion":2}},{"name":"b","title":"b","metadata":{"toolVersion":4}},{"name":"c","title":"c","metadata":{}}]'],
  ['folds-sum', 'equipped.metadata.toolVersion.sum', '[{"name":"a","title":"a","metadata":{"toolVersion":2}},{"name":"b","title":"b","metadata":{"toolVersion":4}},{"name":"c","title":"c","metadata":{}}]'],
  ['folds-count', 'equipped.metadata.toolVersion.count', '[{"name":"a","title":"a","metadata":{"toolVersion":2}},{"name":"b","title":"b","metadata":{"toolVersion":4}},{"name":"c","title":"c","metadata":{}}]'],
  ['count-objects', 'equipped.count', '[{"name":"a","title":"a","metadata":{"toolVersion":2}},{"name":"b","title":"b","metadata":{"toolVersion":4}},{"name":"c","title":"c","metadata":{}}]'],
  // v4 — `words`
  ['words-max-absent', 'equipped.metadata.toolVersion.max', '[{"name":"a","title":"a","metadata":{"toolVersion":"beta"}},{"name":"b","title":"b","metadata":{"toolVersion":true}}]'],
  ['empty-sum-absent', 'equipped.metadata.toolVersion.sum', '[]'],
  ['words-count-zero', 'equipped.metadata.toolVersion.count', '[{"name":"a","title":"a","metadata":{"toolVersion":"beta"}},{"name":"b","title":"b","metadata":{"toolVersion":true}}]'],
  // v4 — one level of list entries, the rest skipped
  ['list-one-level', 'owned.metadata.tags', '[{"name":"a","title":"a","metadata":{"tags":["x",1,true,null,{"y":1},["z"]]}},{"name":"b","title":"b","metadata":{"tags":null}},{"name":"c","title":"c","metadata":{"tags":"w"}}]'],
  ['names', 'owned.names', '[{"name":"lockpick","title":"Lockpick","metadata":{}},{"name":"rope","title":"rope","metadata":{}}]'],
  ['titles', 'owned.titles', '[{"name":"lockpick","title":"Lockpick","metadata":{}},{"name":"rope","title":"rope","metadata":{}}]'],
  ['empty-names', 'equipped.names', '[]'],
  ['empty-count', 'equipped.count', '[]'],
  // the port's rows
  ['neg-zero-min', 'equipped.metadata.v.min', '[{"name":"a","title":"a","metadata":{"v":0}},{"name":"b","title":"b","metadata":{"v":-0.0}}]'],
  ['neg-zero-min-reversed', 'equipped.metadata.v.min', '[{"name":"a","title":"a","metadata":{"v":-0.0}},{"name":"b","title":"b","metadata":{"v":0}}]'],
  ['neg-zero-max', 'equipped.metadata.v.max', '[{"name":"a","title":"a","metadata":{"v":-0.0}},{"name":"b","title":"b","metadata":{"v":0}}]'],
  ['neg-zero-max-alone', 'equipped.metadata.v.max', '[{"name":"a","title":"a","metadata":{"v":-0.0}}]'],
  ['neg-zero-sum', 'equipped.metadata.v.sum', '[{"name":"a","title":"a","metadata":{"v":-0.0}}]'],
  ['float-order-sum', 'equipped.metadata.w.sum', '[{"name":"a","title":"a","metadata":{"w":0.1}},{"name":"b","title":"b","metadata":{"w":0.2}},{"name":"c","title":"c","metadata":{"w":0.3}}]'],
  ['float-order-sum-in-arrays', 'owned.metadata.w.sum', '[{"name":"a","title":"a","metadata":{"w":[0.3,0.2]}},{"name":"b","title":"b","metadata":{"w":0.1}}]'],
  ['count-fold-array-entries', 'owned.metadata.w.count', '[{"name":"a","title":"a","metadata":{"w":["x",2]}},{"name":"b","title":"b","metadata":{"w":["y"]}},{"name":"c","title":"c","metadata":{"w":[]}}]'],
  ['dotted-key-list', 'owned.metadata.a.b', '[{"name":"a","title":"a","metadata":{"a.b":7,"a":{"b":9}}}]'],
  ['fold-word-as-key', 'owned.metadata.count', '[{"name":"a","title":"a","metadata":{"count":3}},{"name":"b","title":"b","metadata":{"count":[4,"n"]}}]'],
  ['ends-in-fold-takes-fold', 'equipped.metadata.stats.max', '[{"name":"a","title":"a","metadata":{"stats.max":100,"stats":5}}]'],
  ['proto-key-not-own', 'owned.metadata.toString', '[{"name":"a","title":"a","metadata":{}}]'],
];
for (const [id, key, objectsJson] of valueCases) {
  const parsed = parseAggregateKey(key);
  if (!parsed.ok) throw new Error(`value case ${id}: ${parsed.reason}`);
  const objects = JSON.parse(objectsJson) as AggregateObject[];
  rows.push({ kind: 'value', id: `value-${id}`, key, objectsJson, value: enc(aggregateValue(parsed.parsed, objects)) });
}

// ------------------------------------------------------- holds / describe
type Bag =
  | { kind: 'null' }
  | { kind: 'rows'; rows: MockInventoryRow[] }
  | { kind: 'real'; items: ResolvedInventoryItem[]; own: string[]; equipped: string[] };

function bagOf(bag: Bag): { ctx: InventoryContext | null; recorded: () => unknown } {
  if (bag.kind === 'null') return { ctx: null, recorded: () => null };
  if (bag.kind === 'rows') return { ctx: mockInventoryContext(bag.rows), recorded: () => ({ rows: bag.rows }) };
  const r = recording(contextOver('c', 'chat', bag));
  return { ctx: r.ctx, recorded: () => ({ recorded: r.recorded as RecordedInventory }) };
}

const REAL_ITEMS = [
  obj('lockpick', { name: 'lockpick', title: 'Lockpick', metadata: { toolAbilities: ['pick', 'probe'], toolVersion: 2 } }),
  obj('resonator', { name: 'ansible resonator', metadata: { toolAbilities: 'suppress, amplify', toolVersion: 4 } }),
  obj('spare', { name: 'spare pick', metadata: { toolVersion: 7 } }),
  obj('shelf-rope', { name: 'rope', origin: { scope: 'general', id: 'g', name: 'General' } } as Partial<ResolvedInventoryItem>),
  obj('template', { name: 'template', isArchetype: true, metadata: { toolVersion: 99 } }),
  obj('old', { name: 'old', archivedAt: '2026-10-20T00:00:00.000Z', metadata: { toolVersion: 99 } }),
  obj('kit', { name: 'kit', kind: 'kit', itemIds: ['lockpick'] } as Partial<ResolvedInventoryItem>),
  obj('inst', { name: 'template', parentId: 'template', metadata: { toolVersion: 5 } }),
];
const REAL: Bag = {
  kind: 'real',
  items: REAL_ITEMS,
  own: ['lockpick', 'resonator', 'spare', 'template', 'old', 'kit', 'inst'],
  equipped: ['lockpick', 'resonator', 'kit'],
};
const MOCK: Bag = {
  kind: 'rows',
  rows: [
    { ref: 'Lockpick', state: 'equipped', count: 2, metadata: { toolVersion: 3 } },
    { ref: 'Resonator', state: 'carried', metadata: { toolAbilities: ['suppress'] } },
    { ref: 'Rope', state: 'reachable' },
    { ref: 'Ghost', state: 'absent', metadata: { toolVersion: 9 } },
  ],
};
const EMPTY: Bag = { kind: 'real', items: [], own: [], equipped: [] };
const NULL: Bag = { kind: 'null' };
const suppressor = (toolAbilities: unknown, own: string[], equipped: string[]): Bag => ({
  kind: 'real',
  items: [obj('suppressor', { metadata: { toolAbilities } as Record<string, unknown> })],
  own,
  equipped,
});
const versions = (a: unknown, b?: unknown): Bag => ({
  kind: 'real',
  items: [obj('a', { metadata: { toolVersion: a } as Record<string, unknown> }), obj('b', { metadata: b === undefined ? {} : ({ toolVersion: b } as Record<string, unknown>) })],
  own: ['a', 'b'],
  equipped: ['a', 'b'],
});

const holdsCases: Array<[string, string, MetadataComparator, Bag]> = [
  // v4 `scopeObjects on a real context`
  ['real-never-archetype', 'reachable.metadata.toolVersion.max', { lt: 99 }, REAL],
  ['real-names-neq-kit', 'reachable.names', { neq: 'kit' }, REAL],
  ['real-instance-max', 'owned.metadata.toolVersion.max', { eq: 7 }, REAL],
  ['real-instance-sum', 'owned.metadata.toolVersion.sum', { eq: 18 }, REAL],
  // v4 `the mock bag`
  ['mock-count', 'equipped.count', { eq: 2 }, MOCK],
  ['mock-sum-copies', 'equipped.metadata.toolVersion.sum', { eq: 6 }, MOCK],
  ['mock-list-eq', 'owned.metadata.toolAbilities', { eq: 'suppress' }, MOCK],
  // v4 `inventoryAggregateHolds` — no context, in both directions
  ['null-count-eq0', 'equipped.count', { eq: 0 }, NULL],
  ['null-count-neq', 'equipped.count', { neq: 'x' }, NULL],
  ['null-count-ncontains', 'equipped.count', { ncontains: 'x' }, NULL],
  ['null-count-lt1', 'equipped.count', { lt: 1 }, NULL],
  ['null-names-eq0', 'equipped.names', { eq: 0 }, NULL],
  ['null-names-neq', 'equipped.names', { neq: 'x' }, NULL],
  ['null-names-ncontains', 'equipped.names', { ncontains: 'x' }, NULL],
  ['null-names-lt1', 'equipped.names', { lt: 1 }, NULL],
  // v4 — an empty scope is a fact
  ['empty-count-eq0', 'equipped.count', { eq: 0 }, EMPTY],
  ['empty-names-eq', 'equipped.names', { eq: 'x' }, EMPTY],
  ['empty-names-neq', 'equipped.names', { neq: 'x' }, EMPTY],
  ['empty-names-ncontains', 'equipped.names', { ncontains: 'x' }, EMPTY],
  ['empty-max-gte0', 'equipped.metadata.v.max', { gte: 0 }, EMPTY],
  ['empty-max-lt0', 'equipped.metadata.v.max', { lt: 0 }, EMPTY],
  // v4 — a key that does not parse
  ['bad-key', 'pocket.names', { eq: 'x' }, EMPTY],
  ['bad-key-field', 'equipped.weight', { eq: 1 }, MOCK],
  // v4 §8 acceptance (a string of abilities; the list form is the next block)
  ['s8-equipped-string', 'equipped.metadata.toolAbilities', { contains: 'suppress' }, suppressor('suppress, amplify', ['suppressor'], ['suppressor'])],
  ['s8-equipped-string-stowed', 'equipped.metadata.toolAbilities', { contains: 'suppress' }, suppressor('suppress, amplify', ['suppressor'], [])],
  ['s8-owned-string-stowed', 'owned.metadata.toolAbilities', { contains: 'suppress' }, suppressor('suppress, amplify', ['suppressor'], [])],
  ['s8-owned-string-shelf', 'owned.metadata.toolAbilities', { contains: 'suppress' }, suppressor('suppress, amplify', [], [])],
  ['s8-reachable-string-shelf', 'reachable.metadata.toolAbilities', { contains: 'suppress' }, suppressor('suppress, amplify', [], [])],
  ['s8-equipped-list', 'equipped.metadata.toolAbilities', { contains: 'suppress' }, suppressor(['suppress', 'amplify'], ['suppressor'], ['suppressor'])],
  ['s8-reachable-list-shelf', 'reachable.metadata.toolAbilities', { contains: 'suppress' }, suppressor(['suppress', 'amplify'], [], [])],
  ['s8-names-exact', 'equipped.names', { eq: 'ansible resonator' }, { kind: 'real', items: [obj('r', { name: 'ansible resonator' })], own: ['r'], equipped: ['r'] }],
  ['s8-names-not-prefix', 'equipped.names', { eq: 'ansible resonator' }, { kind: 'real', items: [obj('r', { name: 'ansible resonator mk2' })], own: ['r'], equipped: ['r'] }],
  ['s8-names-not-case', 'equipped.names', { eq: 'ansible resonator' }, { kind: 'real', items: [obj('r', { name: 'Ansible Resonator' })], own: ['r'], equipped: ['r'] }],
  ['s8-fold-max-gte4', 'equipped.metadata.toolVersion.max', { gte: 4 }, versions(2, 4)],
  ['s8-fold-min-gt2', 'equipped.metadata.toolVersion.min', { gt: 2 }, versions(2, 4)],
  ['s8-fold-sum-eq6', 'equipped.metadata.toolVersion.sum', { eq: 6 }, versions(2, 4)],
  ['s8-fold-count-eq2', 'equipped.metadata.toolVersion.count', { eq: 2 }, versions(2, 4)],
  ['s8-none-numeric-max-gte0', 'equipped.metadata.toolVersion.max', { gte: 0 }, versions('two')],
  ['s8-none-numeric-max-lt0', 'equipped.metadata.toolVersion.max', { lt: 0 }, versions('two')],
  ['s8-none-numeric-count-eq0', 'equipped.metadata.toolVersion.count', { eq: 0 }, versions('two')],
  // the port's rows: a list ordered (declines), an absent fold against neq
  ['list-ordering-declines', 'owned.titles', { gt: 1 }, MOCK],
  ['absent-fold-neq', 'equipped.metadata.v.max', { neq: 3 }, EMPTY],
  ['number-contains-declines', 'equipped.count', { contains: '2' }, MOCK],
];
for (const [id, key, comparator, bagSpec] of holdsCases) {
  const bag = bagOf(bagSpec);
  const declines: string[] = [];
  const held = inventoryAggregateHolds(comparator, key, bag.ctx, {
    resolveOperand: (k) => comparator[k] as Primitive,
    onDecline: (reason) => declines.push(reason),
  });
  const described = describeAggregateValue(key, bag.ctx);
  rows.push({
    kind: 'holds',
    id: `holds-${id}`,
    key,
    comparator,
    list: (() => {
      const parsed = parseAggregateKey(key);
      return parsed.ok ? isListAggregate(parsed.parsed) : false;
    })(),
    inventory: bag.recorded(),
    holds: held,
    declines,
    described: described === null ? null : enc(described),
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
