/**
 * The v4-side recorder behind `src/app/pascal/inventory-twins.oracle.spec.ts`
 * (P4.D334 R-2 / R-13).
 *
 * The Workbench's Proving Bench evaluates a mock bag in the browser, so v5
 * carries four client-safe twins of v4 modules marked `CLIENT-SAFE: pure`:
 * `lib/pascal/inventory-aggregate.ts`, `lib/pascal/inventory-subjects.ts`,
 * `lib/pascal/bench-inventory.ts` and the client half of
 * `lib/inventory/refs.ts`. This drives v4's REAL functions at `322a5488e`.
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/lib/...`.
 *
 * Rows `{ fn, id, needs, args, out }`; `out` is the result's `JSON.stringify`.
 * `needs` names the sibling lane whose code the row's truth reads
 * (`P4D333`: the list rule / `compareOrdered` / `splitInventoryReference`;
 * `P4D330`: `kitState`) — the consumer `it.skipIf`s by it (R-1).
 *
 * Contexts are SCRIPTED (`{ states, counts, items, scopes }`) so the
 * aggregate / subject rows do not lean on the mock-bag builder; the
 * `mockInventoryContext` rows probe that builder through its own surface.
 *
 * Run it from a pinned v4 worktree at `322a5488e` (Node 24):
 *
 * ```bash
 * PIN=${PIN:?a detached v4 worktree at 322a5488e}
 * V5W=${V5W:-$HOME/source/quilltap-v5}
 * cp "$V5W/apps/web/oracle/pascal-inventory-twins.recorder.ts" "$PIN/"
 * OUT="$V5W/apps/web/src/testing/fixtures/pascal-inventory-twins.oracle.ndjson"
 * rm -f "$OUT"
 * (cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   npx tsx pascal-inventory-twins.recorder.ts > "$OUT")
 * wc -l "$OUT"                                          # expect the header count
 * grep -c '"fn":"parseAggregateKey"' "$OUT"             # expect > 0
 * ```
 */

import {
  aggregateKeyEndsInFold,
  inventoryAggregateHolds,
  isListAggregate,
  parseAggregateKey,
} from '@/lib/pascal/inventory-aggregate';
import { mockInventoryContext } from '@/lib/pascal/bench-inventory';
import {
  gateNamesInventory,
  inventoryCountHolds,
  inventoryStateHolds,
  parseInventoryOperand,
  resolveInventoryValue,
} from '@/lib/pascal/inventory-subjects';
import { resolveInventoryRef, stateOf } from '@/lib/inventory/refs';

type Needs = Array<'P4D333' | 'P4D330'>;

function row(fn: string, id: string, needs: Needs, args: unknown, out: unknown): void {
  console.log(JSON.stringify({ fn, id, needs, args, out: JSON.stringify(out) }));
}

// ---------------------------------------------------------------------------
// Scripted contexts
// ---------------------------------------------------------------------------

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

const SWORD: Obj = { name: 'sword', title: 'Excalibur', metadata: { damage: '2d6', toolVersion: 3, toolAbilities: ['suppress', 'amplify'], tag: 'bright' } };
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

function scripted(spec: CtxSpec | null) {
  if (!spec) return null;
  return {
    characterId: 'scripted',
    chatId: null,
    state: (ref: string) => spec.states[ref.trim().toLowerCase()] ?? 'absent',
    equippedKinCount: (ref: string) => spec.counts[ref.trim().toLowerCase()] ?? 0,
    item: (ref: string) => {
      const found = spec.items[ref.trim().toLowerCase()];
      return found ? ({ metadata: found.metadata } as never) : null;
    },
    scopeObjects: (scope: 'equipped' | 'owned' | 'reachable') => spec.scopes[scope],
  };
}

// ---------------------------------------------------------------------------
// parseAggregateKey / isListAggregate / aggregateKeyEndsInFold
// ---------------------------------------------------------------------------

const KEYS: string[] = [
  'equipped.count',
  'equipped.names',
  'equipped.titles',
  'owned.count',
  'owned.names',
  'owned.titles',
  'reachable.count',
  'reachable.names',
  'reachable.titles',
  'equipped.metadata.toolVersion',
  'owned.metadata.toolAbilities',
  'reachable.metadata.tag',
  'equipped.metadata.toolVersion.min',
  'equipped.metadata.toolVersion.max',
  'equipped.metadata.toolVersion.sum',
  'equipped.metadata.toolVersion.count',
  'owned.metadata.weight.max',
  'reachable.metadata.reach.sum',
  // a dotted metadata key, and keys that only look like folds
  'equipped.metadata.a.b',
  'equipped.metadata.a.b.max',
  'equipped.metadata.max',
  'equipped.metadata.stats.max',
  'equipped.metadata.sum',
  'equipped.metadata.a.',
  // malformed
  '',
  '.',
  'equipped',
  'equipped.',
  '.count',
  'equipped.metadata.',
  'equipped.metadata',
  'equipped.bag',
  'equipped.Count',
  'bag.count',
  'everything.count',
  'Equipped.count',
  'EQUIPPED.count',
  ' equipped.count',
  'equipped.count ',
  'equipped.metadata.x y',
  '{{params.scope}}.count',
  'equipped.{{params.field}}',
  'equipped.metadata.{{params.k}}',
  'equipped.count.max',
  'equipped.names.max',
  'a.b.c',
];
for (const key of KEYS) {
  const parsed = parseAggregateKey(key);
  row('parseAggregateKey', `parse ${JSON.stringify(key)}`, [], [key], parsed);
  if (parsed.ok) {
    row('isListAggregate', `list ${JSON.stringify(key)}`, [], [key], isListAggregate(parsed.parsed));
    row('aggregateKeyEndsInFold', `fold ${JSON.stringify(key)}`, [], [key], aggregateKeyEndsInFold(parsed.parsed));
  }
}

// ---------------------------------------------------------------------------
// inventoryAggregateHolds
// ---------------------------------------------------------------------------

interface AggCase {
  id: string;
  key: string;
  comparator: Record<string, unknown>;
  /** True when the sheet value is a LIST — P4.D333's list rule. */
  list: boolean;
}
const AGG: AggCase[] = [
  { id: 'count eq 1', key: 'equipped.count', comparator: { eq: 1 }, list: false },
  { id: 'count gt 0', key: 'equipped.count', comparator: { gt: 0 }, list: false },
  { id: 'count gte 2', key: 'owned.count', comparator: { gte: 2 }, list: false },
  { id: 'count lt 3', key: 'reachable.count', comparator: { lt: 3 }, list: false },
  { id: 'count lte 3', key: 'reachable.count', comparator: { lte: 3 }, list: false },
  { id: 'count neq 0', key: 'equipped.count', comparator: { neq: 0 }, list: false },
  { id: 'count eq 0', key: 'equipped.count', comparator: { eq: 0 }, list: false },
  { id: 'count contains', key: 'equipped.count', comparator: { contains: 'x' }, list: false },
  { id: 'fold max gte 3', key: 'equipped.metadata.toolVersion.max', comparator: { gte: 3 }, list: false },
  { id: 'fold max gt 4 (owned)', key: 'owned.metadata.toolVersion.max', comparator: { gt: 4 }, list: false },
  { id: 'fold min eq 3', key: 'owned.metadata.toolVersion.min', comparator: { eq: 3 }, list: false },
  { id: 'fold sum eq 8', key: 'reachable.metadata.toolVersion.sum', comparator: { eq: 8 }, list: false },
  { id: 'fold count eq 2', key: 'owned.metadata.toolVersion.count', comparator: { eq: 2 }, list: false },
  { id: 'fold of text declines', key: 'reachable.metadata.tag.max', comparator: { gt: 0 }, list: false },
  { id: 'fold of nothing declines', key: 'equipped.metadata.nothing.sum', comparator: { gte: 0 }, list: false },
  { id: 'fold count of nothing is 0', key: 'equipped.metadata.nothing.count', comparator: { eq: 0 }, list: false },
  { id: 'bad key declines', key: 'equipped.bag', comparator: { eq: 1 }, list: false },
  { id: 'names contains', key: 'equipped.names', comparator: { contains: 'sword' }, list: true },
  { id: 'names eq', key: 'owned.names', comparator: { eq: 'bow' }, list: true },
  { id: 'names neq', key: 'owned.names', comparator: { neq: 'bow' }, list: true },
  { id: 'names ncontains', key: 'owned.names', comparator: { ncontains: 'lamp' }, list: true },
  { id: 'titles eq', key: 'equipped.titles', comparator: { eq: 'Excalibur' }, list: true },
  { id: 'titles lt declines', key: 'equipped.titles', comparator: { lt: 1 }, list: true },
  { id: 'metadata list contains', key: 'owned.metadata.toolAbilities', comparator: { contains: 'suppress' }, list: true },
  { id: 'metadata list eq', key: 'owned.metadata.toolAbilities', comparator: { eq: 'amplify' }, list: true },
  { id: 'metadata list neq', key: 'owned.metadata.toolAbilities', comparator: { neq: 'amplify' }, list: true },
  { id: 'metadata list numbers', key: 'owned.metadata.toolVersion', comparator: { gte: 5 }, list: true },
  { id: 'metadata list lte', key: 'owned.metadata.toolVersion', comparator: { lte: 2 }, list: true },
  { id: 'metadata scalar tag', key: 'reachable.metadata.tag', comparator: { eq: 'dim' }, list: true },
  { id: 'metadata list absent key', key: 'equipped.metadata.nothing', comparator: { eq: 'x' }, list: true },
];
for (const [ctxName, spec] of Object.entries(CONTEXTS)) {
  for (const c of AGG) {
    const declines: string[] = [];
    const ctx = scripted(spec);
    const holds = inventoryAggregateHolds(c.comparator as never, c.key, ctx as never, {
      resolveOperand: (k) => c.comparator[k] as never,
      onDecline: (reason) => declines.push(reason),
    });
    // A list walked by the comparison table is P4.D333's list rule; a null /
    // empty context never reaches a list comparison that differs.
    const needs: Needs = c.list && ctxName !== 'null' ? ['P4D333'] : [];
    row('inventoryAggregateHolds', `${ctxName}: ${c.id}`, needs, { ctx: ctxName, key: c.key, comparator: c.comparator }, { holds, declines });
  }
}

// ---------------------------------------------------------------------------
// parseInventoryOperand / resolveInventoryValue / state / count / gateNames
// ---------------------------------------------------------------------------

for (const path of [
  'sword.metadata.damage',
  'Excalibur.metadata.damage',
  'magic bow.metadata.reach',
  'sword.metadata.a.b',
  'sword.metadata.',
  '.metadata.damage',
  'sword.title',
  'sword',
  '',
  'sword.metadata.damage.metadata.x',
]) {
  row('parseInventoryOperand', `operand ${JSON.stringify(path)}`, ['P4D333'], [path], parseInventoryOperand(path));
}

const OPERANDS: Array<[string, { $inventory: string; fallback: number | string | boolean }]> = [
  ['string match', { $inventory: 'sword.metadata.damage', fallback: '1d4' }],
  ['number match', { $inventory: 'sword.metadata.toolVersion', fallback: 0 }],
  ['boolean match', { $inventory: 'sword.metadata.bright', fallback: false }],
  ['type mismatch falls back', { $inventory: 'sword.metadata.damage', fallback: 7 }],
  ['absent key falls back', { $inventory: 'sword.metadata.nothing', fallback: 'x' }],
  ['absent object falls back', { $inventory: 'axe.metadata.damage', fallback: '1d8' }],
  ['unparseable path falls back', { $inventory: 'sword.title', fallback: 'x' }],
  ['by title', { $inventory: 'Excalibur.metadata.reach', fallback: 1 }],
  ['non-finite number falls back', { $inventory: 'bow.metadata.reach', fallback: 9 }],
  ['string where number wanted', { $inventory: 'bow.metadata.reach', fallback: 'near' }],
];
for (const [ctxName, spec] of Object.entries(CONTEXTS)) {
  for (const [id, operand] of OPERANDS) {
    const needs: Needs = ctxName === 'null' ? [] : ['P4D333'];
    row('resolveInventoryValue', `${ctxName}: ${id}`, needs, { ctx: ctxName, operand }, resolveInventoryValue(operand, scripted(spec) as never));
  }
}
// a non-finite metadata value
{
  const odd: CtxSpec = { ...MIXED, items: { sword: { metadata: { v: null, w: Number.POSITIVE_INFINITY } } } };
  row('resolveInventoryValue', 'odd: null value falls back', ['P4D333'], { ctx: 'odd', operand: { $inventory: 'sword.metadata.v', fallback: 4 } }, resolveInventoryValue({ $inventory: 'sword.metadata.v', fallback: 4 }, scripted(odd) as never));
}

const STATE_TESTS: Array<[string, { eq?: string; neq?: string }, string]> = [
  ['eq equipped', { eq: 'equipped' }, 'sword'],
  ['eq equipped by title', { eq: 'equipped' }, 'EXCALIBUR'],
  ['eq absent on an unknown', { eq: 'absent' }, 'axe'],
  ['eq carried', { eq: 'carried' }, 'bow'],
  ['eq reachable', { eq: 'reachable' }, 'lamp'],
  ['neq absent', { neq: 'absent' }, 'sword'],
  ['neq absent on an unknown', { neq: 'absent' }, 'axe'],
  ['eq and neq together', { eq: 'equipped', neq: 'carried' }, 'sword'],
  ['eq and neq contradict', { eq: 'equipped', neq: 'equipped' }, 'sword'],
  ['empty test', {}, 'sword'],
];
for (const [ctxName, spec] of Object.entries(CONTEXTS)) {
  for (const [id, test, ref] of STATE_TESTS) {
    const declines: string[] = [];
    const holds = inventoryStateHolds(test, ref, scripted(spec) as never, (r) => declines.push(r));
    row('inventoryStateHolds', `${ctxName}: ${id}`, [], { ctx: ctxName, test, ref }, { holds, declines });
  }
}

const COUNT_TESTS: Array<[string, Record<string, number>, string, boolean]> = [
  ['eq 1', { eq: 1 }, 'sword', false],
  ['eq 0 on an unknown', { eq: 0 }, 'axe', false],
  ['neq 0', { neq: 0 }, 'dagger', false],
  ['neq 2', { neq: 2 }, 'dagger', false],
  ['gt 1', { gt: 1 }, 'dagger', true],
  ['gte 2', { gte: 2 }, 'dagger', true],
  ['lt 1', { lt: 1 }, 'axe', true],
  ['lte 2', { lte: 2 }, 'dagger', true],
  ['band', { gte: 1, lte: 3 }, 'dagger', true],
  ['band misses', { gt: 1, lt: 2 }, 'dagger', true],
  ['ordering plus eq', { gt: 0, eq: 2 }, 'dagger', true],
  ['empty test', {}, 'dagger', false],
];
for (const [ctxName, spec] of Object.entries(CONTEXTS)) {
  for (const [id, test, ref, ordered] of COUNT_TESTS) {
    const declines: string[] = [];
    const holds = inventoryCountHolds(test, ref, scripted(spec) as never, (r) => declines.push(r));
    row('inventoryCountHolds', `${ctxName}: ${id}`, ordered && ctxName !== 'null' ? ['P4D333'] : [], { ctx: ctxName, test, ref }, { holds, declines });
  }
}

const GATES: Array<[string, unknown]> = [
  ['undefined', undefined],
  ['metadata only', { metadata: { a: { eq: 1 } } }],
  ['inventory record', { inventory: { sword: { eq: 'equipped' } } }],
  ['count record', { inventoryCount: { sword: { gte: 1 } } }],
  ['aggregate record', { inventoryAggregate: { 'equipped.count': { gte: 1 } } }],
  ['empty records', { inventory: {}, inventoryCount: {}, inventoryAggregate: {} }],
  ['array, none', [{ metadata: { a: { eq: 1 } } }, { progress: { 'x.percent': { gt: 1 } } }]],
  ['array, second names it', [{ metadata: { a: { eq: 1 } } }, { inventory: { sword: { eq: 'equipped' } } }]],
  ['array, all empty', [{ inventory: {} }, { inventoryCount: {} }]],
];
for (const [id, gate] of GATES) {
  row('gateNamesInventory', id, ['P4D333'], { gate }, gateNamesInventory(gate as never));
}

// ---------------------------------------------------------------------------
// mockInventoryContext
// ---------------------------------------------------------------------------

const MOCKS: Array<[string, Array<Record<string, unknown>>]> = [
  ['empty', []],
  ['one equipped', [{ ref: 'sword', state: 'equipped' }]],
  ['equipped with a count', [{ ref: 'sword', state: 'equipped', count: 3 }]],
  ['equipped with count 0', [{ ref: 'sword', state: 'equipped', count: 0 }]],
  ['carried default count', [{ ref: 'bow', state: 'carried' }]],
  ['carried with a count', [{ ref: 'bow', state: 'carried', count: 4 }]],
  ['reachable and absent', [{ ref: 'lamp', state: 'reachable' }, { ref: 'ghost', state: 'absent' }]],
  ['a later duplicate wins', [{ ref: 'sword', state: 'equipped', count: 2 }, { ref: 'sword', state: 'carried' }]],
  ['a later duplicate by case wins', [{ ref: 'Sword', state: 'carried' }, { ref: ' sWORD ', state: 'equipped', count: 2 }]],
  ['a blank ref is skipped', [{ ref: '   ', state: 'equipped' }, { ref: 'bow', state: 'carried' }]],
  ['padded ref', [{ ref: '  Magic Bow ', state: 'equipped' }]],
  [
    'metadata rides along',
    [
      { ref: 'sword', state: 'equipped', count: 2, metadata: { toolVersion: 3, toolAbilities: ['a', 'b'] } },
      { ref: 'bow', state: 'carried', metadata: { toolVersion: 5 } },
      { ref: 'lamp', state: 'reachable', metadata: { tag: 'dim' } },
      { ref: 'ghost', state: 'absent', metadata: { tag: 'gone' } },
    ],
  ],
];
const PROBES = ['sword', 'SWORD', ' sword ', 'bow', 'magic bow', 'lamp', 'ghost', 'unlisted', ''];
for (const [id, rows] of MOCKS) {
  const ctx = mockInventoryContext(rows as never);
  row(
    'mockInventoryContext',
    id,
    [],
    { rows },
    {
      characterId: ctx.characterId,
      chatId: ctx.chatId,
      state: PROBES.map((p) => ctx.state(p)),
      count: PROBES.map((p) => ctx.equippedKinCount(p)),
      item: PROBES.map((p) => ctx.item(p)),
      equipped: ctx.scopeObjects('equipped'),
      owned: ctx.scopeObjects('owned'),
      reachable: ctx.scopeObjects('reachable'),
    },
  );
}

// ---------------------------------------------------------------------------
// resolveInventoryRef / stateOf
// ---------------------------------------------------------------------------

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
  // a title colliding with another's name — the title rule outranks the name rule
  item({ id: 'id-torch', name: 'torch', title: 'lamp', origin: origin('general', 'Quilltap General') }),
  item({ id: 'id-dup-shared', name: 'dagger', origin: origin('general', 'Quilltap General') }),
  item({ id: 'id-dup-own', name: 'dagger', origin: origin('character', 'Imogen') }),
  item({ id: 'id-kit', kind: 'kit', name: 'kit', itemIds: ['id-sword', 'id-dup-own'], origin: origin('character', 'Imogen') }),
];
const OWN = new Set(['id-sword', 'id-bow', 'id-old', 'id-dup-own', 'id-kit']);
const REFS = ['id-sword', 'ID-SWORD', 'sword', 'Excalibur', 'excalibur', ' EXCALIBUR ', 'magic bow', 'id-bow-arch', 'rusty axe', 'shared axe', 'lamp', 'torch', 'dagger', 'kit', 'nothing', ''];
const EQUIPPED_SETS: Array<[string, string[]]> = [
  ['nothing equipped', []],
  ['sword equipped', ['id-sword']],
  ['dagger own equipped', ['id-dup-own']],
  ['kit members equipped', ['id-sword', 'id-dup-own']],
  ['one kit member equipped', ['id-sword']],
];
const OPTIONS: Array<[string, Record<string, boolean>]> = [
  ['default', {}],
  ['archetypes', { includeArchetypes: true }],
  ['archived', { includeArchived: true }],
  ['both', { includeArchetypes: true, includeArchived: true }],
];
for (const [setName, ids] of EQUIPPED_SETS) {
  for (const [optName, opts] of OPTIONS) {
    for (const ref of REFS) {
      const pool = { items: POOL_ITEMS as never[], owns: (id: string) => OWN.has(id) };
      const matches = resolveInventoryRef(ref, pool as never, new Set(ids), opts);
      const out = matches.map((m) => ({ id: m.item.id, equipped: m.equipped, own: m.own }));
      const kit = matches.some((m) => m.item.kind === 'kit');
      row(
        'resolveInventoryRef',
        `${setName} / ${optName} / ${JSON.stringify(ref)}`,
        kit ? ['P4D330'] : [],
        { equipped: ids, opts, ref },
        { matches: out, state: stateOf(matches) },
      );
    }
  }
}
