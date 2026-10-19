/**
 * The v4-side recorder behind `src/app/wardrobe/wearable-ref.oracle.spec.ts`
 * (P4.D320).
 *
 * `5cb0e0190` gave v4 ONE lookup rule for a string that names a garment or
 * outfit — `lib/wardrobe/wearable-ref.ts`: id first, then trimmed
 * case-insensitive title, first match in pool order, and a templated
 * reference (anything `scanPlaceholders` finds a `{{…}}` in) never resolves.
 * The Workbench's chips, its unresolved advisory and the server pool's
 * `findByTitle` all run on it, so the SPA's transcription is compared row for
 * row against v4's REAL module (which imports v4's REAL `scanPlaceholders`).
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/lib/...` and
 * would not compile in the SPA's own tsconfig.
 *
 * Each row: `{ fn, id, pool, arg, out }` — `arg` is the reference (or the
 * title, for `matchWearableTitle`), `out` the matched item's `id` or `null`
 * (a boolean for `isTemplatedWearableRef`).
 *
 * Run it from a pinned v4 worktree at `e8e17b2d9` (Node 24 at
 * `~/.nvm/versions/node/v24.13.1/bin`):
 *
 * ```bash
 * PIN=${PIN:?a detached v4 worktree at e8e17b2d9}
 * V5W=${V5W:-$HOME/source/quilltap-v5}
 * cp "$V5W/apps/web/oracle/wearable-ref.recorder.ts" "$PIN/"
 * OUT="$V5W/apps/web/src/testing/fixtures/wearable-ref.oracle.ndjson"
 * rm -f "$OUT"
 * (cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   npx tsx wearable-ref.recorder.ts > "$OUT")
 * wc -l "$OUT"                                  # expect 53
 * grep -c '"fn":"resolveWearableRef"' "$OUT"    # expect 31
 * ```
 *
 * Expect 53 lines (31 `resolveWearableRef` + 10 `matchWearableTitle` + 12
 * `isTemplatedWearableRef`); a shorter file means the recorder errored and the
 * redirect already truncated the old one (the empty-file trap).
 */

import {
  isTemplatedWearableRef,
  matchWearableTitle,
  resolveWearableRef,
} from '@/lib/wardrobe/wearable-ref';

type Item = { id: string; title: string; archivedAt?: string | null };

/** v4's own test pool (`wearable-ref.test.ts`). */
const V4_POOL: Item[] = [
  { id: 'a', title: 'Cloak of Night' },
  { id: 'b', title: 'cloak of night' },
  { id: 'c', title: 'b' },
  { id: 'd', title: 'Linen Shirt' },
];

const WIDE_POOL: Item[] = [
  { id: 'item-1', title: 'Linen Shirt' },
  { id: 'item-2', title: 'item-1' },
  { id: 'item-3', title: 'Riding  Boots' },
  { id: 'item-4', title: 'ÉTOLE' },
  { id: 'item-5', title: 'İstanbul Scarf' },
  { id: 'item-6', title: 'Old Coat', archivedAt: '2026-10-01T00:00:00.000Z' },
  { id: 'item-7', title: 'Sundress' },
  { id: 'item-8', title: 'sundress' },
  { id: '{{x}}', title: 'Templated Id' },
  { id: ' padded-id ', title: 'Padded Id' },
];

const RESOLVE: Array<[string, Item[], string]> = [
  // v4's seven test inputs
  ['v4-id-over-title', V4_POOL, 'b'],
  ['v4-title-case-trim', V4_POOL, '  LINEN shirt '],
  ['v4-first-title-match', V4_POOL, 'CLOAK OF NIGHT'],
  ['v4-templated-params', V4_POOL, '{{params.garment}}'],
  ['v4-templated-own-title', [{ id: 'x', title: '{{params.garment}}' }], '{{params.garment}}'],
  ['v4-blank', V4_POOL, '   '],
  ['v4-unknown', V4_POOL, 'Bowler Hat'],
  // an id that is also another item's title: the id wins
  ['id-also-a-title', WIDE_POOL, 'item-1'],
  ['id-padded-ref', WIDE_POOL, '  item-1  '],
  ['id-wrong-case', WIDE_POOL, 'ITEM-7'],
  // case + surrounding whitespace, JS trim's whole set
  ['title-nbsp-padded', WIDE_POOL, ' Linen Shirt '],
  ['title-ideographic-space', WIDE_POOL, '　linen shirt　'],
  ['title-tab-newline', WIDE_POOL, '\tLINEN SHIRT\n'],
  ['title-zero-width-not-trimmed', WIDE_POOL, '​Linen Shirt'],
  // inner whitespace is never collapsed
  ['title-inner-double-space', WIDE_POOL, 'riding  boots'],
  ['title-inner-single-space-misses', WIDE_POOL, 'Riding Boots'],
  // two same-titled items: the first in pool order
  ['title-two-same', WIDE_POOL, 'SUNDRESS'],
  ['title-two-same-reversed', [...WIDE_POOL].reverse(), 'Sundress'],
  // templated: scanPlaceholders' own rule
  ['templated-character-armour', WIDE_POOL, "{{character}}'s armour"],
  ['templated-params-x', WIDE_POOL, '{{params.x}}'],
  ['templated-unknown', WIDE_POOL, '{{mystery}}'],
  ['templated-spaced', WIDE_POOL, 'a {{ b }} c'],
  ['templated-as-id', WIDE_POOL, '{{x}}'],
  ['single-braces-not-templated', WIDE_POOL, '{ not a template }'],
  ['empty-braces-not-templated', WIDE_POOL, '{{}}'],
  // blanks
  ['empty', WIDE_POOL, ''],
  ['spaces', WIDE_POOL, '   '],
  // Unicode case: toLowerCase on both sides
  ['unicode-etole-lower', WIDE_POOL, 'étole'],
  ['unicode-dotted-i', WIDE_POOL, 'i̇stanbul scarf'],
  ['unicode-dotless-ascii-i', WIDE_POOL, 'istanbul scarf'],
  // an archived match: the rule has no archived filter
  ['archived-match', WIDE_POOL, 'old coat'],
];

const MATCH: Array<[string, Item[], string]> = [
  ['v4-ignores-ids', V4_POOL, 'a'],
  ['title-padded', WIDE_POOL, '  linen shirt  '],
  ['title-blank', WIDE_POOL, '  '],
  ['title-empty', WIDE_POOL, ''],
  ['title-id-of-another', WIDE_POOL, 'item-1'],
  ['title-templated-is-plain-text', [{ id: 'x', title: '{{params.garment}}' }], '{{PARAMS.GARMENT}}'],
  ['title-padded-pool-title-not-trimmed', [{ id: 'p', title: ' Hat ' }], 'hat'],
  ['title-padded-pool-title-exact', [{ id: 'p', title: ' Hat ' }], ' hat '],
  ['title-first-match', WIDE_POOL, 'sundress'],
  ['title-empty-pool', [], 'Hat'],
];

const TEMPLATED: Array<[string, string]> = [
  ['v4-params', '{{params.garment}}'],
  ['v4-metadata-inside', 'The {{metadata.hat}} hat'],
  ['v4-plain', 'Cloak of Night'],
  ['v4-single-braces', '{ not a template }'],
  ['character', '{{character}}'],
  ['character-possessive', "{{character}}'s armour"],
  ['unknown-key', '{{mystery}}'],
  ['spaced-key', '{{ b }}'],
  ['empty-key', '{{}}'],
  ['blank-key', '{{ }}'],
  ['unclosed', '{{params.x'],
  ['empty-string', ''],
];

const idOf = (item: Item | undefined): string | null => (item ? item.id : null);

for (const [id, pool, arg] of RESOLVE) {
  console.log(JSON.stringify({ fn: 'resolveWearableRef', id, pool, arg, out: idOf(resolveWearableRef(pool, arg)) }));
}
for (const [id, pool, arg] of MATCH) {
  console.log(JSON.stringify({ fn: 'matchWearableTitle', id, pool, arg, out: idOf(matchWearableTitle(pool, arg)) }));
}
for (const [id, arg] of TEMPLATED) {
  console.log(JSON.stringify({ fn: 'isTemplatedWearableRef', id, pool: [], arg, out: isTemplatedWearableRef(arg) }));
}
