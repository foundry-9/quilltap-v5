/**
 * The v4-side recorder behind `src/app/item-browser/item-browser.oracle.spec.ts`
 * (P4.D386, the `BROWSER` commit).
 *
 * v4 `b2b783b47` lifted the wardrobe's folder-view model into the shared
 * `lib/folder-browser/view.ts` and gave every folder-of-items browser one ⋮
 * menu (`components/item-browser/ItemActionMenu.tsx`, placed by `29ba4d3f8`'s
 * `hooks/useFixedMenu.ts`) and one state hook (`components/item-browser/
 * useFolderBrowserState.ts`). This recorder drives v4's REAL modules at the
 * pin — `tierLabel`, `childFolderRows`, `tierRows`, `fixedMenuPlacement`,
 * `presentEntries`, `pluralize` — over a fixed corpus, so the SPA's
 * transcriptions are compared row for row. The corpus carries the measured
 * byte shift (`tierLabel` of a character origin with an empty name is
 * `General` at the pin, where `236c04e35`'s wardrobe module said
 * `Quilltap General`).
 *
 * `fixedMenuPlacement(anchor, room)` reads `window.innerWidth` /
 * `innerHeight` and the anchor's `getBoundingClientRect()`: the recorder
 * installs a stub `window` per row (tsx runs without a DOM) and an anchor
 * object whose rect is the row's. The two `.tsx` / React-importing modules
 * load under tsx from the pin's own `node_modules` — `presentEntries` and
 * `pluralize` are plain functions; nothing renders.
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/…` modules
 * and would not compile in the SPA's own tsconfig.
 *
 * Run it from a pinned v4 worktree at `624d0d25c` (Node 24 at
 * `~/.nvm/versions/node/v24.13.1/bin`):
 *
 * ```bash
 * PIN=${PIN:?a detached v4 worktree at 624d0d25c}   # e.g. /tmp/qt-v4-pin-p4d386-624d0d25c
 * V5W=${V5W:-$HOME/source/quilltap-v5}
 * cp "$V5W/apps/web/oracle/item-browser.recorder.ts" "$PIN/"
 * OUT="$V5W/apps/web/src/testing/fixtures/item-browser.oracle.ndjson"
 * rm -f "$OUT"
 * (cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   npx tsx item-browser.recorder.ts > "$OUT")
 * wc -l "$OUT"                 # expect 64
 * grep -c '"fn":"tierLabel"' "$OUT"   # expect 9
 * ```
 *
 * Expect 64 lines (9 `tierLabel` + 15 `childFolderRows` + 8 `tierRows` + 22
 * `fixedMenuPlacement` + 6 `presentEntries` + 4 `pluralize`); a shorter file
 * means the recorder errored and the redirect already truncated the old one
 * (the empty-file trap).
 */

import { childFolderRows, tierLabel, tierRows, type BrowserOrigin, type BrowserViewItem } from '@/lib/folder-browser/view';
import { fixedMenuPlacement } from '@/hooks/useFixedMenu';
import { presentEntries, type MenuEntryLike } from '@/components/item-browser/ItemActionMenu';
import { pluralize } from '@/components/item-browser/useFolderBrowserState';

/** `undefined` cannot ride JSON — the spec maps this tag back. */
const UNDEFINED = { $undefined: true } as const;

const emit = (fn: string, id: string, args: unknown[], out: unknown): void => {
  console.log(JSON.stringify({ fn, id, args, out: out === undefined ? UNDEFINED : out }));
};

const revive = (v: unknown): unknown =>
  v !== null && typeof v === 'object' && '$undefined' in (v as object) ? undefined : v;

// ---- tierLabel(origin, generalName?) — 9 rows -------------------------------

const PROJECT: BrowserOrigin = { scope: 'project', id: 'p1', name: 'Blackwood' };
const GROUP: BrowserOrigin = { scope: 'group', id: 'g1', name: 'The Sisters' };
const GENERAL: BrowserOrigin = { scope: 'general', id: null, name: 'Quilltap General' };
const CHAR: BrowserOrigin = { scope: 'character', id: 'c1', name: 'Bertie' };
const CHAR_EMPTY: BrowserOrigin = { scope: 'character', id: 'c2', name: '' };

const LABELS: Array<[string, unknown[]]> = [
  ['null', [null]],
  ['undefined', [UNDEFINED]],
  ['general', [GENERAL]],
  ['project', [PROJECT]],
  ['group', [GROUP]],
  ['character named', [CHAR]],
  ['character empty name (the byte shift)', [CHAR_EMPTY]],
  ['character empty name, generalName given', [CHAR_EMPTY, 'Quilltap General']],
  ['general ignores generalName', [GENERAL, 'Quilltap General']],
];
for (const [id, args] of LABELS) {
  const real = args.map(revive) as [BrowserOrigin | null | undefined, string?];
  emit('tierLabel', id, args, real.length > 1 ? tierLabel(real[0], real[1]) : tierLabel(real[0]));
}

// ---- childFolderRows(current, items, tree?) — 15 rows -----------------------

type Tree = Array<{ path: string; itemCount: number }>;
const FOLDER_CASES: Array<[string, string, BrowserViewItem[], Tree | typeof UNDEFINED]> = [
  ['root, items only', '', [{ folder: 'Formal/Evening' }, { folder: '' }, { folder: 'Casual' }], UNDEFINED],
  ['root, tree folder absent from the items', '', [{ folder: 'Formal' }], [{ path: 'Rainy days', itemCount: 0 }]],
  ['nested current', 'Formal', [{ folder: 'Formal/Evening/Gala' }, { folder: 'Formal' }], UNDEFINED],
  ['never the current itself nor a sibling prefix', 'Formal', [{ folder: 'Formal' }, { folder: 'Formalwear' }, { folder: 'Casual/Formal' }], UNDEFINED],
  ['case-insensitive dedupe, the tree spelling first', '', [{ folder: 'formal' }], [{ path: 'Formal', itemCount: 3 }]],
  ['a server count beats the item count', '', [{ folder: 'Hats' }, { folder: 'Hats' }], [{ path: 'hats', itemCount: 7 }]],
  ['an item count: live, directly inside, any casing', '', [{ folder: 'Hats' }, { folder: 'Hats', archivedAt: '2026-01-01T00:00:00.000Z' }, { folder: 'Hats/Summer' }, { folder: 'hats' }], UNDEFINED],
  ['archived items in both counts', 'Formal', [{ folder: 'Formal/Evening', archivedAt: '2026-01-01' }, { folder: 'Formal/Evening' }], [{ path: 'Formal/Evening', itemCount: 0 }]],
  ['mixed-case and accented names sort base-insensitively', '', [{ folder: 'beta' }, { folder: 'Alpha' }, { folder: 'Écharpes' }, { folder: 'delta' }, { folder: 'echo' }, { folder: 'Zeta' }], UNDEFINED],
  ['accent-equal names keep insertion order under base sensitivity', '', [{ folder: 'resume' }, { folder: 'Résumé' }, { folder: 'RESUME' }], UNDEFINED],
  ['null and absent folders are the root', '', [{ folder: null }, {}], UNDEFINED],
  ['a deep tree contributes only the direct child', 'A', [], [{ path: 'A/B/C', itemCount: 2 }, { path: 'A/B', itemCount: 5 }, { path: 'A', itemCount: 1 }]],
  ['v4 view.test.ts: unions the tree with item folders', '', [{ folder: 'formal/Evening' }, { folder: 'Casual' }, { folder: '' }], [{ path: 'Formal', itemCount: 4 }, { path: 'Empty', itemCount: 0 }]],
  ['v4 view.test.ts: live items directly inside', 'Formal', [{ folder: 'Formal/Evening' }, { folder: 'Formal/Evening', archivedAt: '2026-01-01' }, { folder: 'Formal/Evening/Gala' }], UNDEFINED],
  ['empty everything', '', [], []],
];
for (const [id, current, items, tree] of FOLDER_CASES) {
  const realTree = revive(tree) as Tree | undefined;
  emit(
    'childFolderRows',
    id,
    [current, items, tree],
    realTree === undefined ? childFolderRows(current, items) : childFolderRows(current, items, realTree),
  );
}

// ---- tierRows(items, tierOf) — 8 rows ---------------------------------------
// The tierOf is the recorder's fixed rule (own = no origin, else `scope:id`),
// replayed identically by the spec — the function cannot ride JSON.

type TierItem = BrowserViewItem & { id: string };
const tierKey = (i: TierItem): string => (i.origin ? `${i.origin.scope}:${i.origin.id ?? ''}` : 'own');
const TIER_CASES: Array<[string, TierItem[]]> = [
  ['own only', [{ id: 'a' }, { id: 'b' }]],
  ['first-seen order, nearest first', [{ id: 'o' }, { id: 'g', origin: GROUP }, { id: 'p', origin: PROJECT }, { id: 'x', origin: GENERAL }, { id: 'g2', origin: GROUP }]],
  ['a borrowed tier seen twice, one archived', [{ id: 'a', origin: PROJECT, archivedAt: '2026-01-01' }, { id: 'b', origin: PROJECT }]],
  ['an archived-only tier opens at zero', [{ id: 'a', origin: GROUP, archivedAt: '2026-01-01' }]],
  ['a character origin with an empty name (the byte shift)', [{ id: 'a', origin: CHAR_EMPTY }]],
  ['a character origin with a name', [{ id: 'a', origin: CHAR }]],
  ['v4 view.test.ts: skips the own tier, counts live, first-seen', [{ id: 'a', origin: { scope: 'project', id: 'p1', name: 'Quest' } }, { id: 'b', origin: GENERAL }, { id: 'c', origin: { scope: 'project', id: 'p1', name: 'Quest' }, archivedAt: '2026-01-01' }, { id: 'd', origin: { scope: 'project', id: 'p1', name: 'Quest' } }, { id: 'e' }]],
  ['empty', []],
];
for (const [id, items] of TIER_CASES) {
  emit('tierRows', id, [items], tierRows(items, tierKey));
}

// ---- fixedMenuPlacement(anchor, room?) — 22 rows -----------------------------
// args: [rect {top, bottom, right}, innerWidth, innerHeight, room | undefined]

type Rect = { top: number; bottom: number; right: number };
const PLACEMENTS: Array<[string, Rect, number, number, number | typeof UNDEFINED]> = [
  ['down, room to spare', { top: 100, bottom: 124, right: 900 }, 1000, 800, UNDEFINED],
  ['up, short below, longer above', { top: 700, bottom: 724, right: 900 }, 1000, 800, UNDEFINED],
  ['down, short both ways, below longer (room 2000)', { top: 100, bottom: 124, right: 900 }, 1000, 800, 2000],
  ['right clamp at the 4px gap', { top: 770, bottom: 790, right: 1000 }, 1000, 800, UNDEFINED],
  ['right beyond the viewport', { top: 0, bottom: 20, right: 1100 }, 1000, 800, UNDEFINED],
  ['the 120px height floor', { top: 50, bottom: 70, right: 500 }, 1000, 150, UNDEFINED],
  ['room exactly met below (below == room)', { top: 400, bottom: 456, right: 900 }, 1000, 800, 340],
  ['one px short of room, above longer', { top: 400, bottom: 457, right: 900 }, 1000, 800, 340],
  ['above == below (stays down)', { top: 300, bottom: 500, right: 900 }, 1000, 800, 340],
  ['above one longer than below (goes up)', { top: 301, bottom: 500, right: 900 }, 1000, 800, 340],
  ['room 120 (a folder row)', { top: 600, bottom: 690, right: 900 }, 1000, 800, 120],
  ['room 120, just short', { top: 600, bottom: 677, right: 900 }, 1000, 800, 120],
  ['room 200 (a scenario row)', { top: 500, bottom: 596, right: 900 }, 1000, 800, 200],
  ['room 200, just short', { top: 500, bottom: 597, right: 900 }, 1000, 800, 200],
  ['room 240 (the slot picker)', { top: 400, bottom: 556, right: 900 }, 1000, 800, 240],
  ['room 240, just short', { top: 400, bottom: 557, right: 900 }, 1000, 800, 240],
  ['fractional rect', { top: 100.5, bottom: 124.25, right: 899.75 }, 1000, 800, UNDEFINED],
  ['up with a floor-clamped height', { top: 60, bottom: 80, right: 900 }, 1000, 90, UNDEFINED],
  ['zero rect at the origin', { top: 0, bottom: 0, right: 0 }, 1000, 800, UNDEFINED],
  ['button below the viewport foot', { top: 900, bottom: 924, right: 900 }, 1000, 800, UNDEFINED],
  ['room 0', { top: 700, bottom: 724, right: 900 }, 1000, 800, 0],
  ['narrow phone viewport', { top: 500, bottom: 524, right: 360 }, 375, 812, UNDEFINED],
];
for (const [id, rect, innerWidth, innerHeight, room] of PLACEMENTS) {
  (globalThis as unknown as { window: unknown }).window = { innerWidth, innerHeight };
  const anchor = { getBoundingClientRect: () => rect } as unknown as HTMLElement;
  const realRoom = revive(room) as number | undefined;
  const out = realRoom === undefined ? fixedMenuPlacement(anchor) : fixedMenuPlacement(anchor, realRoom);
  emit('fixedMenuPlacement', id, [rect, innerWidth, innerHeight, room], out);
}

// ---- presentEntries(entries) — 6 rows ----------------------------------------
// Entries carry a key and label only (onSelect cannot ride JSON; the spec adds one).

const E = (key: string) => ({ key, label: key.toUpperCase(), onSelect: () => {} });
const ENTRY_CASES: Array<[string, MenuEntryLike[]]> = [
  ['the order’s row: false, null, undefined, entry', [false, null, undefined, E('edit')]],
  ['all falsy', [false, null, undefined]],
  ['empty', []],
  ['order kept around holes', [E('a'), false, E('b'), null, E('c')]],
  ['disabled and destructive survive', [{ ...E('x'), disabled: true }, { ...E('del'), destructive: true }]],
  ['a lone entry', [E('only')]],
];
const shape = (entries: MenuEntryLike[]) =>
  entries.map((e) => (e === undefined ? UNDEFINED : e === false || e === null ? e : { key: e.key, label: e.label, disabled: e.disabled ?? null, destructive: e.destructive ?? null }));
for (const [id, entries] of ENTRY_CASES) {
  emit('presentEntries', id, [shape(entries)], shape(presentEntries(entries)));
}

// ---- pluralize(n, word) — 4 rows ---------------------------------------------

for (const [n, word] of [
  [0, 'item'],
  [1, 'item'],
  [2, 'object'],
  [21, 'garment'],
] as const) {
  emit('pluralize', `${n} ${word}`, [n, word], pluralize(n, word));
}
