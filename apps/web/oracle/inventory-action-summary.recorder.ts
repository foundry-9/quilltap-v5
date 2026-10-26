/**
 * The v4-side recorder behind `src/app/chat/inventory-action-summary.oracle.spec.ts`
 * (P4.D330).
 *
 * `e1872c94f` gave the Salon's tool rows an Inventory notice: `withArticle`,
 * `inventoryRowLabel` and `buildInventoryActionSummary`
 * (`components/chat/ToolMessage.tsx:175-245` at the pin `322a5488e`). All
 * three are module-PRIVATE, so this recorder drives an EXPORTING COPY of v4's
 * file, made by appending one line — the copy sits beside the original inside
 * the pin so its relative imports resolve from the pin's tree:
 *
 * ```bash
 * cp "$PIN/components/chat/ToolMessage.tsx" "$PIN/components/chat/zz-p4d330-ToolMessage.export.tsx"
 * printf '\nexport { withArticle, inventoryRowLabel, buildInventoryActionSummary };\n' \
 *   >> "$PIN/components/chat/zz-p4d330-ToolMessage.export.tsx"
 * ```
 *
 * Nothing else in the copy changes (`diff` shows the one appended line). The
 * client rule is recorded VERBATIM — `/^[aeiou]/i` → `an`, so `unicorn horn`
 * reads `an unicorn horn` where the server's Pascal line says `a` (R-6, the
 * recorded pair).
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/...`.
 *
 * Rows: `{ fn, id, args, out }` — `out` is `JSON.stringify` of the result.
 *
 * Run it from a pinned v4 worktree at `322a5488e` (Node 24):
 *
 * ```bash
 * PIN=/tmp/qt-v4-pin-p4d330-322a5488e
 * V5W=${V5W:-$HOME/source/quilltap-v5}
 * # (the two-line export copy above, first)
 * cp "$V5W/apps/web/oracle/inventory-action-summary.recorder.ts" "$PIN/zz-p4d330-inventory-action-summary.recorder.ts"
 * OUT="$V5W/apps/web/src/testing/fixtures/inventory-action-summary.oracle.ndjson"
 * rm -f "$OUT"
 * (cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH TZ=UTC \
 *   npx tsx zz-p4d330-inventory-action-summary.recorder.ts > "$OUT")
 * wc -l "$OUT"                                   # expect 58
 * grep -c 'an unicorn' "$OUT"                    # expect > 0
 * ```
 */

import {
  buildInventoryActionSummary,
  inventoryRowLabel,
  withArticle,
} from '@/components/chat/zz-p4d330-ToolMessage.export';

type Fn = 'withArticle' | 'inventoryRowLabel' | 'buildInventoryActionSummary';
const rows: Array<{ fn: Fn; id: string; args: unknown[] }> = [];
const add = (fn: Fn, id: string, ...args: unknown[]) => rows.push({ fn, id, args });

// --- withArticle: titles, every vowel, the u-words, h, a missing name ---
const articleRows: Array<[string, Record<string, unknown>]> = [
  ['titled', { name: 'sword', title: 'Excalibur' }],
  ['titled-padded', { name: 'sword', title: '  Excalibur ' }],
  ['title-empty', { name: 'lantern', title: '' }],
  ['title-null', { name: 'lantern', title: null }],
  ['title-whitespace', { name: 'lantern', title: '   ' }],
  ['vowel-a', { name: 'amulet' }],
  ['vowel-e', { name: 'egg' }],
  ['vowel-i', { name: 'inkwell' }],
  ['vowel-o', { name: 'oar' }],
  ['vowel-u', { name: 'umbrella' }],
  ['unicorn', { name: 'unicorn horn' }],
  ['Unicorn', { name: 'Unicorn horn' }],
  ['hour', { name: 'hourglass' }],
  ['consonant', { name: 'magic bow' }],
  ['upper-vowel', { name: 'Elixir' }],
  ['name-missing', {}],
  ['name-empty', { name: '' }],
  ['name-padded', { name: ' apple' }],
  ['astral', { name: '🍎 apple' }],
];
for (const [id, row] of articleRows) add('withArticle', id, row);

// --- inventoryRowLabel ---
add('inventoryRowLabel', 'undefined');
add('inventoryRowLabel', 'titled', { name: 'sword', title: 'Excalibur' });
add('inventoryRowLabel', 'title-whitespace', { name: 'sword', title: '  ' });
add('inventoryRowLabel', 'name-only', { name: 'sword' });
add('inventoryRowLabel', 'name-empty', { name: '' });
add('inventoryRowLabel', 'nothing', {});

// --- buildInventoryActionSummary ---
const tool = (toolName: string, result: unknown, success = true) => ({ toolName, success, result });
const SWORD = { id: 'i1', name: 'sword', title: 'Excalibur' };
const RING = { id: 'i2', name: 'signet ring', title: null };
const LAMP = { id: 'i3', name: 'lantern', title: '' };
const summary = (id: string, data: unknown) => add('buildInventoryActionSummary', id, data);
summary('list-no-notice', tool('inventory_list', { items: [SWORD] }));
summary('read-no-notice', tool('inventory_read', { item: SWORD }));
summary('failed', tool('inventory_equip', { equipped: [SWORD] }, false));
summary('no-tool-name', { success: true, result: { equipped: [SWORD] } });
summary('result-string', tool('inventory_equip', 'Equipped it'));
summary('result-missing', tool('inventory_equip', undefined));
summary('result-null', tool('inventory_equip', null));
summary('wardrobe-tool', tool('wardrobe_wear', { operations: [] }));
summary('equip-one', tool('inventory_equip', { equipped: [SWORD], created: [], already_equipped: [], skipped: [] }));
summary('equip-three', tool('inventory_equip', { equipped: [SWORD, RING, LAMP] }));
summary('equip-empty', tool('inventory_equip', { equipped: [] }));
summary('equip-absent', tool('inventory_equip', {}));
summary('equip-label-less', tool('inventory_equip', { equipped: [{ id: 'x' }, SWORD] }));
summary('unequip-two', tool('inventory_unequip', { unequipped: [SWORD, RING], not_equipped: [] }));
summary('unequip-empty', tool('inventory_unequip', { unequipped: [] }));
summary('pick-up-equipped', tool('inventory_pick_up', { item: RING, archetype: { id: 'a', name: 'signet ring', title: null }, equipped: true }));
summary('pick-up-not-equipped', tool('inventory_pick_up', { item: RING, equipped: false }));
summary('pick-up-titled', tool('inventory_pick_up', { item: SWORD, equipped: true }));
summary('pick-up-unicorn', tool('inventory_pick_up', { item: { id: 'u', name: 'unicorn horn', title: null }, equipped: true }));
summary('pick-up-no-item', tool('inventory_pick_up', { equipped: true }));
summary('create-equipped', tool('inventory_create', { item: SWORD, equipped: true }));
summary('create-plain', tool('inventory_create', { item: LAMP, equipped: false }));
summary('create-no-item', tool('inventory_create', {}));
summary('update', tool('inventory_update', { item: RING, fields: ['description'] }));
summary('update-no-item', tool('inventory_update', { fields: [] }));
summary('archive-true', tool('inventory_archive', { item: SWORD, archived: true }));
summary('archive-false', tool('inventory_archive', { item: SWORD, archived: false }));
summary('archive-absent-flag', tool('inventory_archive', { item: LAMP }));
summary('give-titled', tool('inventory_give', { item: SWORD, recipient: { id: 'c', name: 'Vivienne' } }));
summary('give-untitled', tool('inventory_give', { item: LAMP, recipient: { id: 'c', name: 'Marcus' } }));
summary('give-no-recipient', tool('inventory_give', { item: SWORD }));
summary('give-recipient-nameless', tool('inventory_give', { item: SWORD, recipient: { id: 'c' } }));
summary('give-nameless-item', tool('inventory_give', { item: { id: 'z', title: null }, recipient: { id: 'c', name: 'Marcus' } }));

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const FNS: Record<Fn, (...args: any[]) => unknown> = {
  withArticle,
  inventoryRowLabel,
  buildInventoryActionSummary,
};

for (const row of rows) {
  const out = FNS[row.fn](...row.args);
  process.stdout.write(`${JSON.stringify({ ...row, out: JSON.stringify(out) ?? null })}\n`);
}
