/**
 * The v4-side recorder behind `src/app/inventory/inventory-display.oracle.spec.ts`
 * (P4.D330).
 *
 * The Inventory's client-safe pure helpers at the pin `322a5488e`:
 * `inventoryDisplayName`, `inventoryLabel`, `isKin`, `isEquippable` and
 * `kitState` (`lib/inventory/resolve.ts`), and `inventoryOriginLabel` /
 * `containerOfOrigin` (`lib/inventory/container.ts`). This drives v4's REAL
 * functions; the SPA twins live in `inventory/inventory-display.ts` and
 * `inventory/inventory-container.ts`.
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/lib/...`.
 *
 * Rows: `{ fn, id, args, out }` — `args` is the argument LIST as passed (a
 * `kitState` resolver is passed as a plain map `{ id: member }` and wrapped
 * into a lookup on both sides), `out` is `JSON.stringify` of the result.
 *
 * Run it from a pinned v4 worktree at `322a5488e` (Node 24):
 *
 * ```bash
 * PIN=/tmp/qt-v4-pin-p4d330-322a5488e
 * V5W=${V5W:-$HOME/source/quilltap-v5}
 * cp "$V5W/apps/web/oracle/inventory-display.recorder.ts" "$PIN/zz-p4d330-inventory-display.recorder.ts"
 * OUT="$V5W/apps/web/src/testing/fixtures/inventory-display.oracle.ndjson"
 * rm -f "$OUT"
 * (cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH TZ=UTC \
 *   npx tsx zz-p4d330-inventory-display.recorder.ts > "$OUT")
 * wc -l "$OUT"                                   # expect 67
 * ```
 */

import { containerOfOrigin, inventoryOriginLabel } from '@/lib/inventory/container';
import { inventoryDisplayName, inventoryLabel, isEquippable, isKin, kitState } from '@/lib/inventory/resolve';

type Fn =
  | 'inventoryDisplayName'
  | 'inventoryLabel'
  | 'isKin'
  | 'isEquippable'
  | 'kitState'
  | 'inventoryOriginLabel'
  | 'containerOfOrigin';

const rows: Array<{ fn: Fn; id: string; args: unknown[] }> = [];
const add = (fn: Fn, id: string, ...args: unknown[]) => rows.push({ fn, id, args });

// --- display name / label: equal, by case, by whitespace, astral, absent ---
const names: Array<[string, { name: string; title?: string | null }]> = [
  ['title-differs', { name: 'sword', title: 'Excalibur' }],
  ['title-equal', { name: 'sword', title: 'sword' }],
  ['title-equal-by-case', { name: 'Sword', title: 'sWORD' }],
  ['title-equal-by-padding', { name: '  sword ', title: ' sword  ' }],
  ['title-padded-differs', { name: 'sword', title: '  Excalibur  ' }],
  ['title-null', { name: 'lantern', title: null }],
  ['title-empty', { name: 'lantern', title: '' }],
  ['title-whitespace', { name: 'lantern', title: '   ' }],
  ['title-absent', { name: 'lantern' }],
  ['title-astral', { name: 'ring', title: '💍 The One' }],
  ['name-astral-equal', { name: '🗡️ blade', title: '🗡️ BLADE' }],
  ['name-padded-title-absent', { name: '  bow  ' }],
  ['title-dotless-i', { name: 'istanbul', title: 'İSTANBUL' }],
  ['title-sharp-s', { name: 'strasse', title: 'STRAßE' }],
];
for (const [id, item] of names) add('inventoryDisplayName', id, item);
for (const [id, item] of names) add('inventoryLabel', id, item);

// --- kin ---
add('isKin', 'same', { name: 'sword' }, { name: 'sword' });
add('isKin', 'case', { name: 'Sword' }, { name: 'sWORD' });
add('isKin', 'padding', { name: ' sword ' }, { name: 'sword' });
add('isKin', 'differs', { name: 'sword' }, { name: 'swords' });
add('isKin', 'astral', { name: '💍 ring' }, { name: '💍 RING' });

// --- equippable ---
add('isEquippable', 'plain', { isArchetype: false, kind: 'item', archivedAt: null });
add('isEquippable', 'archetype', { isArchetype: true, kind: 'item', archivedAt: null });
add('isEquippable', 'kit', { isArchetype: false, kind: 'kit', archivedAt: null });
add('isEquippable', 'archived', { isArchetype: false, kind: 'item', archivedAt: '2026-10-01T00:00:00.000Z' });
add('isEquippable', 'archived-absent', { isArchetype: false, kind: 'item' });
add('isEquippable', 'archived-empty', { isArchetype: false, kind: 'item', archivedAt: '' });

// --- kit state: the resolver is a map ---
const member = (isArchetype: boolean, kind: 'item' | 'kit' = 'item', archivedAt: string | null = null) => ({
  isArchetype,
  kind,
  archivedAt,
});
const KIT = { itemIds: ['a', 'b', 'c'] };
const POOL = { a: member(false), b: member(false), c: member(true) };
add('kitState', 'none-carried', KIT, [], POOL);
add('kitState', 'some-carried', KIT, [{ id: 'a', parentId: null }], POOL);
add('kitState', 'all-via-instance', KIT, [
  { id: 'a', parentId: null },
  { id: 'b', parentId: null },
  { id: 'inst', parentId: 'c' },
], POOL);
add('kitState', 'archetype-id-itself-not-carried', KIT, [
  { id: 'a', parentId: null },
  { id: 'b', parentId: null },
  { id: 'c', parentId: null },
], POOL);
add('kitState', 'missing-members-ignored', { itemIds: ['a', 'gone'] }, [{ id: 'a', parentId: null }], POOL);
add('kitState', 'archived-member-ignored', { itemIds: ['a', 'x'] }, [{ id: 'a', parentId: null }], {
  ...POOL,
  x: member(false, 'item', '2026-10-01T00:00:00.000Z'),
});
add('kitState', 'nested-kit-ignored', { itemIds: ['a', 'k'] }, [{ id: 'a', parentId: null }], {
  ...POOL,
  k: member(false, 'kit'),
});
add('kitState', 'no-members', { itemIds: [] }, [{ id: 'a', parentId: null }], POOL);
add('kitState', 'only-unresolvable', { itemIds: ['gone', 'k'] }, [{ id: 'gone', parentId: null }], {
  k: member(false, 'kit'),
});
add('kitState', 'duplicate-member', { itemIds: ['a', 'a'] }, [{ id: 'a', parentId: null }], POOL);

// --- origin label ---
const origins: Array<[string, unknown]> = [
  ['null', null],
  ['general', { scope: 'general', id: null, name: 'Quilltap General' }],
  ['general-other-name', { scope: 'general', id: null, name: 'Elsewhere' }],
  ['project', { scope: 'project', id: 'p-1', name: 'The Quest' }],
  ['project-padded', { scope: 'project', id: 'p-1', name: '  The Quest ' }],
  ['group', { scope: 'group', id: 'g-1', name: 'The Regulars' }],
  ['character', { scope: 'character', id: 'c-viv', name: 'Vivienne' }],
];
for (const [id, origin] of origins) add('inventoryOriginLabel', id, origin);

// --- container of origin (fallback = a character, and General) ---
const FALLBACK = { scope: 'character', id: 'c-viv' };
for (const [id, origin] of origins) add('containerOfOrigin', id, origin, FALLBACK);
add('containerOfOrigin', 'project-no-id', { scope: 'project', id: null, name: 'Lost' }, FALLBACK);
add('containerOfOrigin', 'group-empty-id', { scope: 'group', id: '', name: 'Lost' }, FALLBACK);
add('containerOfOrigin', 'general-fallback-general', null, { scope: 'general', id: null });
add('containerOfOrigin', 'general-with-id', { scope: 'general', id: 'odd', name: 'Quilltap General' }, FALLBACK);

const FNS: Record<Fn, (...args: any[]) => unknown> = {
  inventoryDisplayName,
  inventoryLabel,
  isKin,
  isEquippable,
  kitState: (kit, equipped, pool: Record<string, unknown>) =>
    kitState(kit, equipped, (id: string) => pool[id] as never),
  inventoryOriginLabel,
  containerOfOrigin,
};

for (const row of rows) {
  const out = FNS[row.fn](...row.args);
  process.stdout.write(`${JSON.stringify({ ...row, out: JSON.stringify(out) ?? null })}\n`);
}
