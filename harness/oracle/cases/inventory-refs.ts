/**
 * Differential oracle — the Inventory reference rule and the change-effect
 * pure helpers (P4.D328, v4 `e1872c94f` + `102a23648` + `a53024d3c`'s
 * `scopeObjects`). Tier 1, PURE — no fixture, no DB.
 *
 * Drives v4's REAL `lib/inventory/refs.ts` (`resolveInventoryRef`, `stateOf`,
 * `equippedKinCount`, `equippedKinCountByName`, `pickForGesture`,
 * `inventoryContextFrom` → `state` / `item` / `equippedKinCount` /
 * `scopeObjects`) and `lib/inventory/equip-effects.ts`
 * (`describeInventoryChange`, `recordPendingInventoryChange`,
 * `strikeFromPendingInventoryChange`) over hand-built `ResolvedInventoryItem`
 * literals, and emits each row's INPUT beside v4's output. The Rust side
 * (`inventory_refs_equivalence.rs`) rebuilds the pool from the row and runs
 * `inventory::refs` / `services::inventory_change_effects`; every output is
 * compared as `JSON.stringify` bytes (key order and `null`s pin).
 *
 * The rows: v4's twelve `refs.test.ts` cases, the aggregate test's four
 * real-context scope cases, the order's corners (the title round winning
 * pool-wide, an archetype named by id without the flag, own vs shared
 * archived, a half-equipped kit, whitespace / mixed case / empty refs, kin
 * through an archetype, `stateOf` over an archetype + an archived own match,
 * a whitespace-only title kept in a scope, a kit excluded, null metadata →
 * `{}`), and the describe / record / strike rows (the article and plural
 * rules, the Oxford list, the give line, picked-up excluded from "takes up";
 * netting both ways, pick-up then set-down, provenances first-seen, push-
 * unique; the strike's three lists and an unknown key).
 *
 * Run (Node 24, from the v4 checkout or a pinned worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx ~/source/quilltap-v5/harness/oracle/cases/inventory-refs.ts \
 *     > /tmp/oracle-inventory-refs.ndjson
 */

import {
  equippedKinCount,
  equippedKinCountByName,
  inventoryContextFrom,
  pickForGesture,
  resolveInventoryRef,
  stateOf,
  type ResolvedInventoryPool,
} from '@/lib/inventory/refs';
import {
  describeInventoryChange,
  recordPendingInventoryChange,
  strikeFromPendingInventoryChange,
  type InventoryChange,
  type InventoryChangeProvenance,
} from '@/lib/inventory/equip-effects';
import type { PendingInventoryChange } from '@/lib/wardrobe/turn-containers';
import type { ResolvedInventoryItem } from '@/lib/schemas/inventory.types';

type Item = Record<string, unknown>;
interface PoolSpec {
  items: Item[];
  own: string[];
  equipped: string[];
}
type Opts = { includeArchetypes?: boolean; includeArchived?: boolean };

const T = '2026-10-18T00:00:00.000Z';
function obj(id: string, fields: Item = {}): Item {
  return {
    id,
    kind: 'item',
    name: 'sword',
    title: null,
    equippedByDefault: false,
    maxEquipped: 1,
    canCopy: false,
    isArchetype: false,
    parentId: null,
    itemIds: [],
    metadata: {},
    folder: '',
    archivedAt: null,
    createdAt: T,
    updatedAt: T,
    ...fields,
  };
}

function poolOf(spec: PoolSpec): ResolvedInventoryPool {
  const owned = new Set(spec.own);
  return { items: spec.items as unknown as ResolvedInventoryItem[], owns: (id) => owned.has(id) };
}

function emit(row: Record<string, unknown>): void {
  process.stdout.write(JSON.stringify(row) + '\n');
}

function matchesOut(m: ReturnType<typeof resolveInventoryRef>): unknown {
  return m.map((x) => ({ item: x.item, equipped: x.equipped, own: x.own }));
}

function resolveRow(id: string, pool: PoolSpec, ref: string, opts: Opts = {}): void {
  emit({ fn: 'resolve', id, pool, ref, opts, out: matchesOut(resolveInventoryRef(ref, poolOf(pool), new Set(pool.equipped), opts)) });
}
function stateRow(id: string, pool: PoolSpec, ref: string, opts: Opts = {}): void {
  emit({ fn: 'state', id, pool, ref, opts, out: stateOf(resolveInventoryRef(ref, poolOf(pool), new Set(pool.equipped), opts)) });
}
function kinRow(id: string, pool: PoolSpec, ref: string): void {
  emit({ fn: 'kin', id, pool, ref, out: equippedKinCount(ref, poolOf(pool), new Set(pool.equipped)) });
}
function kinByNameRow(id: string, pool: PoolSpec, name: string): void {
  emit({ fn: 'kinByName', id, pool, name, out: equippedKinCountByName(name, poolOf(pool), new Set(pool.equipped)) });
}
function pickRow(id: string, pool: PoolSpec, ref: string, action: 'equip' | 'unequip', opts: Opts = {}): void {
  const m = pickForGesture(resolveInventoryRef(ref, poolOf(pool), new Set(pool.equipped), opts), action);
  emit({ fn: 'pick', id, pool, ref, action, opts, out: m ? { item: m.item, equipped: m.equipped, own: m.own } : null });
}
function contextRow(id: string, pool: PoolSpec, refs: string[], chatId: string | null = 'chat'): void {
  const ctx = inventoryContextFrom('c', chatId, poolOf(pool), new Set(pool.equipped));
  emit({
    fn: 'context',
    id,
    pool,
    refs,
    chatId,
    out: {
      characterId: ctx.characterId,
      chatId: ctx.chatId,
      reads: refs.map((ref) => ({ ref, state: ctx.state(ref), item: ctx.item(ref), kin: ctx.equippedKinCount(ref) })),
      scopes: {
        equipped: ctx.scopeObjects('equipped'),
        owned: ctx.scopeObjects('owned'),
        reachable: ctx.scopeObjects('reachable'),
      },
    },
  });
}

// ---- v4 refs.test.ts (12) ----
const excalibur = obj('aaaa', { title: 'Excalibur' });
const sharedSword = obj('bbbb', { title: null });
const p1: PoolSpec = { items: [sharedSword, excalibur], own: ['aaaa'], equipped: [] };
resolveRow('v4-id-first-case-insensitive', p1, 'AAAA');
resolveRow('v4-title', p1, 'excalibur');
resolveRow('v4-name-own-first', p1, 'Sword');
resolveRow('v4-title-wins-over-name', { items: [obj('cccc', { name: 'Excalibur' }), excalibur], own: [], equipped: [] }, 'excalibur');
const bow = obj('bow', { name: 'magic bow', isArchetype: true });
const oldLantern = obj('old', { name: 'lantern', archivedAt: T });
const q: PoolSpec = { items: [bow, oldLantern], own: ['old'], equipped: [] };
resolveRow('v4-archetype-excluded', q, 'magic bow');
resolveRow('v4-archetype-included', q, 'magic bow', { includeArchetypes: true });
resolveRow('v4-archived-excluded', q, 'lantern');
resolveRow('v4-archived-own-included', q, 'lantern', { includeArchived: true });
resolveRow('v4-empty-ref', p1, '  ');
const ownS = obj('own');
const sharedS = obj('shared');
stateRow('v4-state-equipped', { items: [ownS, sharedS], own: ['own'], equipped: ['shared'] }, 'sword');
stateRow('v4-state-carried', { items: [ownS, sharedS], own: ['own'], equipped: [] }, 'sword');
stateRow('v4-state-reachable', { items: [sharedS], own: [], equipped: [] }, 'sword');
stateRow('v4-state-absent', { items: [ownS, sharedS], own: ['own'], equipped: [] }, 'axe');
stateRow(
  'v4-state-shared-lantern',
  { items: [obj('lantern', { name: 'lantern', origin: { scope: 'project', id: 'p', name: 'Project' } })], own: [], equipped: [] },
  'lantern',
);
const rings: PoolSpec = {
  items: [obj('r1', { name: 'ring' }), obj('r2', { name: 'Ring' }), obj('r3', { name: 'ring' })],
  own: ['r1', 'r2', 'r3'],
  equipped: ['r1', 'r2'],
};
kinRow('v4-kin-two', rings, 'ring');
kinRow('v4-kin-by-id', { ...rings, equipped: ['r1'] }, 'r3');
kinRow('v4-kin-absent', { ...rings, equipped: ['r1'] }, 'axe');
kinRow(
  'v4-kin-archetype-counts-instances',
  { items: [obj('bow', { name: 'bow', isArchetype: true }), obj('mine', { name: 'bow', parentId: 'bow' })], own: ['mine'], equipped: ['mine'] },
  'bow',
);
const ab: PoolSpec = { items: [obj('a'), obj('b')], own: ['a'], equipped: [] };
pickRow('v4-pick-equip-own-first', ab, 'sword', 'equip');
pickRow('v4-pick-equip-next', { ...ab, equipped: ['a'] }, 'sword', 'equip');
pickRow('v4-pick-equip-none', { ...ab, equipped: ['a', 'b'] }, 'sword', 'equip');
pickRow('v4-pick-unequip-first-equipped', { ...ab, equipped: ['b'] }, 'sword', 'unequip');
pickRow('v4-pick-unequip-none', ab, 'sword', 'unequip');
contextRow('v4-context-equipped-first', { items: [obj('a', { title: 'Mine' }), obj('b', { title: 'Theirs' })], own: ['a'], equipped: ['b'] }, ['sword']);

// ---- v4 inventory-aggregate.test.ts: scopeObjects on a real context (4) ----
const aggItems = [
  obj('lockpick', { name: 'lockpick', title: 'Lockpick', metadata: { toolAbilities: ['pick', 'probe'], toolVersion: 2 } }),
  obj('resonator', { name: 'ansible resonator', metadata: { toolAbilities: 'suppress, amplify', toolVersion: 4 } }),
  obj('spare', { name: 'spare pick', metadata: { toolVersion: 7 } }),
  obj('shelf-rope', { name: 'rope', origin: { scope: 'general', id: 'g', name: 'General' } }),
  obj('template', { name: 'template', isArchetype: true, metadata: { toolVersion: 99 } }),
  obj('old', { name: 'old', archivedAt: T, metadata: { toolVersion: 99 } }),
  obj('kit', { name: 'kit', kind: 'kit', itemIds: ['lockpick'] }),
  obj('inst', { name: 'template', parentId: 'template', metadata: { toolVersion: 5 } }),
];
contextRow(
  'v4-aggregate-real-context-scopes',
  { items: aggItems, own: ['lockpick', 'resonator', 'spare', 'template', 'old', 'kit', 'inst'], equipped: ['lockpick', 'resonator', 'kit'] },
  ['lockpick', 'kit', 'template', 'old'],
);

// ---- the order's corners ----
// The title round wins POOL-WIDE: "Lantern" is one object's title and another's name.
const titled = obj('t1', { name: 'lamp', title: 'Lantern' });
const named = obj('n1', { name: 'lantern' });
resolveRow('corner-title-round-pool-wide', { items: [named, titled], own: ['n1'], equipped: [] }, 'lantern');
// …but an untitled instance named by its kind is found by the NAME round beside a titled sibling.
resolveRow(
  'corner-name-round-beside-titled-sibling',
  { items: [obj('x1', { name: 'rope', title: 'Hempen Coil' }), obj('x2', { name: 'rope' })], own: ['x2'], equipped: [] },
  'rope',
);
resolveRow('corner-archetype-by-id-without-flag', { items: [bow], own: [], equipped: [] }, 'BOW');
resolveRow('corner-archetype-by-id-with-flag', { items: [bow], own: [], equipped: [] }, 'BOW', { includeArchetypes: true });
const sharedArchived = obj('sa', { name: 'lantern', archivedAt: T });
resolveRow('corner-shared-archived-with-flag', { items: [sharedArchived], own: [], equipped: [] }, 'lantern', { includeArchived: true });
resolveRow('corner-own-archived-without-flag', { items: [oldLantern], own: ['old'], equipped: [] }, 'lantern');
const kitHalf = obj('kit1', { name: 'traveller kit', kind: 'kit', itemIds: ['m1', 'm2'] });
const m1 = obj('m1', { name: 'flint' });
const m2 = obj('m2', { name: 'tinder' });
const halfKit: PoolSpec = { items: [kitHalf, m1, m2], own: ['kit1', 'm1', 'm2'], equipped: ['m1'] };
resolveRow('corner-kit-half-equipped', halfKit, 'traveller kit');
pickRow('corner-kit-half-equipped-unequip-null', halfKit, 'traveller kit', 'unequip');
pickRow('corner-kit-half-equipped-equip-picks-kit', halfKit, 'traveller kit', 'equip');
resolveRow('corner-kit-all-equipped', { ...halfKit, equipped: ['m1', 'm2'] }, 'traveller kit');
resolveRow('corner-ref-surrounding-whitespace', p1, '  Excalibur \n');
resolveRow('corner-mixed-case-id', { items: [obj('AbCd-01')], own: [], equipped: [] }, 'abcd-01');
resolveRow('corner-empty-ref', p1, '');
resolveRow('corner-whitespace-title-is-not-a-title', { items: [obj('w1', { name: 'bell', title: '   ' })], own: [], equipped: [] }, '   bell');
kinRow(
  'corner-kin-through-archetype-counts-equipped-instances',
  {
    items: [obj('bow', { name: 'Bow', isArchetype: true }), obj('i1', { name: 'bow', parentId: 'bow' }), obj('i2', { name: 'BOW ', parentId: 'bow' }), obj('i3', { name: 'bow', archivedAt: T })],
    own: ['i1', 'i2', 'i3'],
    equipped: ['i1', 'i2', 'i3'],
  },
  'Bow',
);
kinByNameRow('corner-kin-by-name-skips-kit-and-archived', {
  items: [obj('k', { name: 'rope', kind: 'kit', itemIds: [] }), obj('r', { name: 'Rope' }), obj('ra', { name: 'rope', archivedAt: T })],
  own: [],
  equipped: ['k', 'r', 'ra'],
}, ' rope ');
stateRow(
  'corner-state-ignores-archetype-and-archived-own',
  { items: [obj('arch', { name: 'shield', isArchetype: true }), obj('ao', { name: 'shield', archivedAt: T })], own: ['arch', 'ao'], equipped: ['arch', 'ao'] },
  'shield',
  { includeArchetypes: true, includeArchived: true },
);
contextRow(
  'corner-scopes-whitespace-title-kept-kit-excluded-null-metadata',
  {
    items: [
      obj('s1', { name: 'bell', title: '  ', metadata: null }),
      obj('s2', { name: 'kit', kind: 'kit', itemIds: ['s1'] }),
      obj('s3', { name: 'candle', title: 'Tallow', metadata: { lit: true, burn: 3.5 } }),
    ],
    own: ['s1', 's2'],
    equipped: ['s1', 's2'],
  },
  ['bell', 'candle', 'nothing'],
  null,
);

// ---- describeInventoryChange ----
function item(id: string, name: string, title: string | null = null): Item {
  return obj(id, { name, title });
}
function describeRow(id: string, characterName: string, change: Record<string, unknown>): void {
  emit({ fn: 'describe', id, characterName, change, out: describeInventoryChange(characterName, change as unknown as InventoryChange) });
}
describeRow('desc-titled', 'Vivienne', { equipped: [item('e', 'sword', 'Excalibur')], unequipped: [] });
describeRow('desc-untitled', 'Vivienne', { equipped: [item('r', 'signet ring')], unequipped: [] });
for (const [i, name] of ['lockpicks', 'gloves', 'compass', 'abacus', 'iris', 'amulet', 'unicorn horn', 'one-eyed idol', 'use', 'Eustace', 'old axe', 'hour-glass', 'Ünter hat', 'once-blade', 'utility belt', 'ESS'].entries()) {
  describeRow(`desc-article-${i}`, 'Mara', { equipped: [item(`a${i}`, name)], unequipped: [] });
}
describeRow('desc-title-surrounding-whitespace', 'Mara', { equipped: [item('tw', 'sword', '  Excalibur  ')], unequipped: [] });
describeRow('desc-whitespace-only-title', 'Mara', { equipped: [item('wo', 'apple', '   ')], unequipped: [] });
describeRow('desc-name-surrounding-whitespace', 'Mara', { equipped: [item('nw', '  apple  ')], unequipped: [] });
describeRow('desc-one', 'Mara', { equipped: [], unequipped: [item('l', 'lantern')] });
describeRow('desc-two', 'Mara', { equipped: [], unequipped: [item('l', 'lantern'), item('c', 'compass', 'The Navigator')] });
describeRow('desc-three', 'Mara', { equipped: [], unequipped: [item('l', 'lantern'), item('c', 'compass'), item('g', 'gloves')] });
describeRow('desc-give', 'Mara', { equipped: [], unequipped: [], given: { item: item('l', 'lantern'), toName: 'Marcus' } });
describeRow('desc-give-titled', 'Mara', { equipped: [], unequipped: [], given: { item: item('l', 'lantern', 'Old Faithful'), toName: 'Marcus' } });
describeRow('desc-picked-up-excluded-from-takes-up', 'Mara', {
  equipped: [item('p', 'bow', 'Whisperwind'), item('k', 'knife')],
  unequipped: [],
  pickedUp: [item('p', 'bow', 'Whisperwind')],
});
describeRow('desc-all-three-clauses', 'Mara', {
  equipped: [item('e1', 'apple'), item('p1', 'bow')],
  unequipped: [item('u1', 'torch'), item('u2', 'oars')],
  pickedUp: [item('p1', 'bow')],
});
describeRow('desc-picked-up-not-equipped', 'Mara', { equipped: [], unequipped: [], pickedUp: [item('p1', 'arrows')] });
describeRow('desc-nothing', 'Mara', { equipped: [], unequipped: [] });
describeRow('desc-nothing-empty-picked-up', 'Mara', { equipped: [], unequipped: [], pickedUp: [] });
describeRow('desc-someone', 'Someone', { equipped: [item('x', 'umbrella')], unequipped: [] });

// ---- recordPendingInventoryChange / strikeFromPendingInventoryChange ----
interface Step {
  who: { userId: string; chatId: string; characterId: string };
  change: Record<string, unknown>;
  provenance: InventoryChangeProvenance;
}
const WHO = { userId: 'u', chatId: 'chat-1', characterId: 'char-a' };
const WHO_B = { userId: 'u', chatId: 'chat-1', characterId: 'char-b' };
function run(steps: Step[]): Map<string, PendingInventoryChange> {
  const pending = new Map<string, PendingInventoryChange>();
  for (const s of steps) recordPendingInventoryChange(pending, s.who, s.change as unknown as InventoryChange, s.provenance);
  return pending;
}
function recordRow(id: string, steps: Step[]): void {
  emit({ fn: 'record', id, steps, out: Array.from(run(steps).entries()) });
}
function strikeRow(id: string, steps: Step[], key: string, itemId: string): void {
  const pending = run(steps);
  strikeFromPendingInventoryChange(pending, key, itemId);
  emit({ fn: 'strike', id, steps, key, itemId, out: Array.from(pending.entries()) });
}
const L = item('l', 'lantern');
const C = item('c', 'compass');
const B = item('b', 'bow', 'Whisperwind');
recordRow('rec-equip-then-unequip-nets', [
  { who: WHO, change: { equipped: [L], unequipped: [] }, provenance: 'character' },
  { who: WHO, change: { equipped: [], unequipped: [L] }, provenance: 'character' },
]);
recordRow('rec-unequip-then-equip-nets', [
  { who: WHO, change: { equipped: [], unequipped: [L] }, provenance: 'character' },
  { who: WHO, change: { equipped: [L], unequipped: [] }, provenance: 'pascal' },
]);
recordRow('rec-pick-up-then-set-down', [
  { who: WHO, change: { equipped: [B], unequipped: [], pickedUp: [B] }, provenance: 'character' },
  { who: WHO, change: { equipped: [], unequipped: [B] }, provenance: 'character' },
]);
recordRow('rec-two-provenances-first-seen', [
  { who: WHO, change: { equipped: [L], unequipped: [] }, provenance: 'pascal' },
  { who: WHO, change: { equipped: [C], unequipped: [] }, provenance: 'character' },
  { who: WHO, change: { equipped: [], unequipped: [C] }, provenance: 'pascal' },
]);
recordRow('rec-push-unique-on-repeat', [
  { who: WHO, change: { equipped: [L, L], unequipped: [C] }, provenance: 'character' },
  { who: WHO, change: { equipped: [L], unequipped: [C], pickedUp: [B, B] }, provenance: 'character' },
]);
recordRow('rec-two-characters-keyed-apart', [
  { who: WHO, change: { equipped: [L], unequipped: [] }, provenance: 'character' },
  { who: WHO_B, change: { equipped: [C], unequipped: [] }, provenance: 'operator' },
]);
recordRow('rec-empty-change-still-makes-entry', [{ who: WHO, change: { equipped: [], unequipped: [] }, provenance: 'character' }]);
const struck: Step[] = [
  { who: WHO, change: { equipped: [L, B], unequipped: [C], pickedUp: [B] }, provenance: 'character' },
  { who: WHO_B, change: { equipped: [B], unequipped: [] }, provenance: 'character' },
];
strikeRow('strike-three-lists-one-entry', struck, 'chat-1:char-a', 'b');
strikeRow('strike-unequipped-only', struck, 'chat-1:char-a', 'c');
strikeRow('strike-unknown-key-no-op', struck, 'chat-1:nobody', 'b');
strikeRow('strike-unknown-item-no-op', struck, 'chat-1:char-a', 'zzz');
