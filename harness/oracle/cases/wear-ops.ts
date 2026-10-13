/**
 * Tier-1 oracle case (P4.D279, keystone C1 §5) — the pure half of v4's
 * `lib/wardrobe/wear-ops.ts` + `lib/wardrobe/wearable.ts` at `4a38222e6`.
 *
 * Drives the REAL `buildWearablePool` (`@/lib/wardrobe/pool`), `findInPool` /
 * `resolveWearable` / `wardrobeItemNotFoundMessage` (`@/lib/wardrobe/wear-ops`)
 * and `archivedWearMessage` / `wearRefusal` / `newlyWornArchivedItems`
 * (`@/lib/wardrobe/wearable`) — no mocks, no reimplementation. The corpus is
 * v4's `__tests__/unit/lib/wardrobe/wear-ops.test.ts` pure describes case-for-
 * case plus the shapes it leaves implicit (empty / whitespace references, an
 * archived SHARED item by title, every mode × an archived item, a slot the item
 * covers vs one it does not, `newlyWornArchivedItems` with and without slots).
 * The persisted half (`wearItem` / `takeOffItem`) is NOT in this corpus:
 * `outfit_displacement_tier2_equivalence` compares `applyDisplacement` itself
 * and never reaches the wrappers. Their mode → effect / `slotsAffected`
 * mapping, the pool's `byId` handed as the composite lookup, the source
 * passthrough and the per-slot take-off loop are pinned by `wear_ops.rs`'s
 * in-crate tests (v4's `describe('wearItem')` / `describe('takeOffItem')`,
 * run against the real v5 `apply_displacement`).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx $V5W/harness/oracle/cases/wear-ops.ts > /tmp/oracle-wear-ops.ndjson
 */

import { buildWearablePool } from '@/lib/wardrobe/pool';
import {
  findInPool,
  resolveWearable,
  wardrobeItemNotFoundMessage,
  type PutOnMode,
} from '@/lib/wardrobe/wear-ops';
import { archivedWearMessage, newlyWornArchivedItems, wearRefusal } from '@/lib/wardrobe/wearable';
import type { EquippedSlots, WardrobeItem, WardrobeItemType } from '@/lib/schemas/wardrobe.types';
import type { WardrobeItemWithOrigin, WardrobeOrigin } from '@/lib/wardrobe/wardrobe-container';

const BASELINE = 'p4.d279-wear-ops';
const CHAR_ID = 'char-1';
const STAMP = '2026-01-01T00:00:00.000Z';

function item(
  id: string,
  title: string,
  types: WardrobeItemType[],
  extra: Partial<WardrobeItem> = {},
): WardrobeItem {
  return {
    id,
    characterId: CHAR_ID,
    title,
    description: null,
    imagePrompt: null,
    types,
    componentItemIds: [],
    appropriateness: null,
    isDefault: false,
    replace: false,
    migratedFromClothingRecordId: null,
    imageFileId: null,
    archivedAt: null,
    folder: '',
    createdAt: STAMP,
    updatedAt: STAMP,
    ...extra,
  } as WardrobeItem;
}

const OWN: WardrobeOrigin = { scope: 'character', id: CHAR_ID, name: '' };
const GENERAL: WardrobeOrigin = { scope: 'general', id: null, name: 'Quilltap General' };
const tag = (i: WardrobeItem, origin: WardrobeOrigin): WardrobeItemWithOrigin => ({ ...i, origin });

const shirt = item('shirt', 'Linen Shirt', ['top']);
const coat = item('coat', 'Old Coat', ['top'], { archivedAt: STAMP });
const dress = item('dress', 'Sundress', ['top', 'bottom'], { replace: true });
const generalHat = { ...item('hat', 'Bowler Hat', ['accessories']), characterId: null };
const generalGone = { ...item('gone', 'Gone Cloak', ['top'], { archivedAt: STAMP }), characterId: null };
const generalSame = { ...item('shirt', 'General Shirt', ['top']), characterId: null };

const layers = {
  own: [tag(shirt, OWN), tag(coat, OWN), tag(dress, OWN)],
  group: [],
  project: [],
  general: [tag(generalHat, GENERAL), tag(generalGone, GENERAL), tag(generalSame, GENERAL)],
};
const pool = buildWearablePool(CHAR_ID, { groupMountPointIds: [], projectMountPointIds: [] }, layers);

const out: string[] = [];
const emit = (row: Record<string, unknown>) => out.push(JSON.stringify(row));

emit({
  kind: 'meta',
  baseline: BASELINE,
  layers: {
    own: layers.own,
    group: layers.group,
    project: layers.project,
    general: layers.general,
  },
});

// ── wardrobeItemNotFoundMessage ──────────────────────────────────────────────
for (const [id, itemId, itemTitle] of [
  ['nf-none', undefined, undefined],
  ['nf-null', null, null],
  ['nf-id', 'x1', undefined],
  ['nf-title', undefined, 'Hat'],
  ['nf-both', 'x1', 'Hat'],
  ['nf-empty', '', ''],
  ['nf-empty-id-title', '', 'Hat'],
] as const) {
  emit({ kind: 'not_found_message', id, itemId: itemId ?? null, itemTitle: itemTitle ?? null, out: wardrobeItemNotFoundMessage(itemId, itemTitle) });
}

// ── findInPool ───────────────────────────────────────────────────────────────
const refs: Array<[string, { itemId?: string | null; itemTitle?: string | null }]> = [
  ['fp-by-id', { itemId: 'shirt' }],
  ['fp-by-id-general', { itemId: 'hat' }],
  ['fp-by-title', { itemTitle: 'linen shirt' }],
  ['fp-id-miss-title', { itemId: 'nope', itemTitle: ' Linen Shirt ' }],
  ['fp-archived-own', { itemId: 'coat' }],
  ['fp-archived-own-title', { itemTitle: 'old coat' }],
  ['fp-archived-shared-title', { itemTitle: 'Gone Cloak' }],
  ['fp-own-shadows-shared-title', { itemTitle: 'General Shirt' }],
  ['fp-empty', {}],
  ['fp-empty-strings', { itemId: '', itemTitle: '' }],
  ['fp-whitespace-title', { itemTitle: '   ' }],
  ['fp-unknown', { itemId: 'ghost', itemTitle: 'Ghost' }],
];
for (const [id, ref] of refs) {
  const found = findInPool(pool, ref);
  emit({ kind: 'find_in_pool', id, ref: { itemId: ref.itemId ?? null, itemTitle: ref.itemTitle ?? null }, out: found ? found.id : null });
}

// ── resolveWearable ──────────────────────────────────────────────────────────
const modes: PutOnMode[] = ['wear', 'replace', 'add_to_slot'];
const cases: Array<[string, { itemId?: string | null; itemTitle?: string | null }, PutOnMode, WardrobeItemType | undefined]> = [];
for (const mode of modes) {
  cases.push([`rw-${mode}-shirt`, { itemId: 'shirt' }, mode, undefined]);
  cases.push([`rw-${mode}-shirt-top`, { itemId: 'shirt' }, mode, 'top']);
  cases.push([`rw-${mode}-shirt-footwear`, { itemId: 'shirt' }, mode, 'footwear']);
  cases.push([`rw-${mode}-coat`, { itemId: 'coat' }, mode, 'top']);
  cases.push([`rw-${mode}-coat-footwear`, { itemId: 'coat' }, mode, 'footwear']);
  cases.push([`rw-${mode}-hat`, { itemId: 'hat' }, mode, 'accessories']);
  cases.push([`rw-${mode}-dress-bottom`, { itemId: 'dress' }, mode, 'bottom']);
  cases.push([`rw-${mode}-unknown`, { itemId: 'ghost', itemTitle: 'Ghost Robe' }, mode, 'top']);
  cases.push([`rw-${mode}-gone-title`, { itemTitle: 'Gone Cloak' }, mode, 'top']);
  cases.push([`rw-${mode}-empty`, { itemId: '', itemTitle: '' }, mode, undefined]);
}
for (const [id, ref, mode, slot] of cases) {
  const r = resolveWearable(pool, ref, mode, slot);
  emit({
    kind: 'resolve_wearable',
    id,
    ref: { itemId: ref.itemId ?? null, itemTitle: ref.itemTitle ?? null },
    mode,
    slot: slot ?? null,
    out: r.ok ? { ok: true, item: r.item.id } : { ok: false, reason: r.reason, message: r.message },
  });
}

// ── wearable.ts ──────────────────────────────────────────────────────────────
for (const [id, title] of [['awm-plain', 'Old Coat'], ['awm-quotes', 'The "Good" Coat'], ['awm-empty', '']] as const) {
  emit({ kind: 'archived_wear_message', id, title, out: archivedWearMessage(title) });
}
for (const [id, it] of [
  ['wr-live', shirt],
  ['wr-archived', coat],
  ['wr-empty-string', item('e', 'Empty Stamp', ['top'], { archivedAt: '' as never })],
] as const) {
  emit({ kind: 'wear_refusal', id, item: it, out: wearRefusal(it) });
}
const items = [shirt, coat, dress, generalGone as WardrobeItem];
const slotsWithCoat: EquippedSlots = { top: ['coat'], bottom: [], footwear: [], accessories: [], hair: [] };
const slotsOther: EquippedSlots = { top: ['shirt'], bottom: [], footwear: ['gone'], accessories: [], hair: [] };
for (const [id, slots] of [
  ['nwa-null', null],
  ['nwa-coat-worn', slotsWithCoat],
  ['nwa-gone-worn-elsewhere', slotsOther],
  ['nwa-empty', { top: [], bottom: [], footwear: [], accessories: [], hair: [] }],
] as const) {
  emit({ kind: 'newly_worn_archived', id, slots, out: newlyWornArchivedItems(items, slots as EquippedSlots | null).map((i) => i.id) });
}

process.stdout.write(out.join('\n') + '\n');
