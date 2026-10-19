/**
 * Tier-1 oracle case (P4.D319, v4 `5cb0e0190`) — the proving bench's wardrobe
 * lens and the one wearable-reference rule.
 *
 * Drives the REAL `buildWearablePool` (`@/lib/wardrobe/pool`),
 * `QtapCustomToolSchema` + `executeCustomTool` (`@/lib/pascal/custom-tools`),
 * `resolveBenchWardrobeAgainst` (`@/lib/pascal/bench-wardrobe`) and
 * `resolveWearableRef` / `matchWearableTitle` / `isTemplatedWearableRef`
 * (`@/lib/wardrobe/wearable-ref`) — no mocks, no reimplementation.
 *
 * Rows:
 *   - one `meta` row carrying the pool's four layers (origin-tagged items), so
 *     both sides build the SAME pool through their own `buildWearablePool`;
 *   - `lens` rows: a deterministic definition (`roll: { min: 1, max: 1 }`, one
 *     outcome — no draw) dealt by v4's real `executeCustomTool` with the row's
 *     `character` override, then the lens over the RENDERED effects. Both the
 *     effects and the resolution are carried as JSON TEXT, so key order is part
 *     of the comparison. The corpus is v4's own `route-preview-wardrobe.test.ts`
 *     `it`s minus the mock-only fail arm and the route-level hand-typed arm,
 *     plus the shapes it leaves implicit;
 *   - `wearable_ref` rows: v4's seven `wearable-ref.test.ts` `it`s and two
 *     `matchWearableTitle` corners.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx $V5W/harness/oracle/cases/pascal-bench-wardrobe.ts > /tmp/oracle-pascal-bench-wardrobe.ndjson
 */

import { buildWearablePool } from '@/lib/wardrobe/pool';
import { executeCustomTool } from '@/lib/pascal/custom-tools';
import { QtapCustomToolSchema } from '@/lib/pascal/custom-tool.types';
import { resolveBenchWardrobeAgainst } from '@/lib/pascal/bench-wardrobe';
import {
  isTemplatedWearableRef,
  matchWearableTitle,
  resolveWearableRef,
} from '@/lib/wardrobe/wearable-ref';
import type { WardrobeItem } from '@/lib/schemas/wardrobe.types';
import type { WardrobeOrigin } from '@/lib/wardrobe/wardrobe-container';

const CHAR = 'char-imogen';
const STAMP = '2026-01-01T00:00:00.000Z';
const NOW = 1_788_004_800_000;

const OWN: WardrobeOrigin = { scope: 'character', id: CHAR, name: 'Imogen' };
const GENERAL: WardrobeOrigin = { scope: 'general', id: null, name: 'Quilltap General' };

function item(id: string, title: string, types: WardrobeItem['types'], extra: Partial<WardrobeItem> = {}): WardrobeItem {
  return {
    id,
    characterId: CHAR,
    title,
    types,
    componentItemIds: [],
    isDefault: false,
    replace: false,
    archivedAt: null,
    createdAt: STAMP,
    updatedAt: STAMP,
    ...extra,
  } as WardrobeItem;
}

// v4's own test pool (Sundress top+bottom, ARCHIVED Old Coat, General Cloak of
// Night), widened: an own item whose TITLE is another item's id (`cloak`), a
// mixed-case title, a title with a trailing space, an item carrying an image,
// `{{character}}`'s target, and an archived General item.
const layers = {
  own: [
    { ...item('dress', 'Sundress', ['top', 'bottom']), origin: OWN },
    { ...item('coat', 'Old Coat', ['top'], { archivedAt: STAMP }), origin: OWN },
    { ...item('decoy', 'cloak', ['accessories']), origin: OWN },
    { ...item('boots', 'Riding BOOTS', ['footwear'], { imageFileId: 'img-boots' } as Partial<WardrobeItem>), origin: OWN },
    { ...item('scarf', 'Silk Scarf ', ['accessories']), origin: OWN },
    { ...item('armour', "Imogen's armour", ['top', 'bottom']), origin: OWN },
  ],
  group: [],
  project: [],
  general: [
    { ...item('cloak', 'Cloak of Night', ['top'], { characterId: null }), origin: GENERAL },
    { ...item('gone', 'Gone Cloak', ['top'], { characterId: null, archivedAt: STAMP }), origin: GENERAL },
  ],
};

const pool = buildWearablePool(CHAR, { groupMountPointIds: [], projectMountPointIds: [] }, layers);

const rows: unknown[] = [];
rows.push({ kind: 'meta', characterId: CHAR, layers: JSON.stringify(layers) });

function definition(effects: unknown[], extra: Record<string, unknown> = {}) {
  return {
    name: 'dresser',
    description: 'x',
    roll: { min: 1, max: 1 },
    outcomes: [{ when: true, message: 'done', state: 'info' }],
    effects,
    ...extra,
  };
}
const wear = (items: string[], extra: Record<string, unknown> = {}) => ({ wardrobe: { action: 'wear', items, ...extra } });
const takeOff = (items: string[], extra: Record<string, unknown> = {}) => ({ wardrobe: { action: 'take_off', items, ...extra } });

const lensCases: Array<[string, Record<string, unknown>, string | null]> = [
  // ---- v4's `route-preview-wardrobe.test.ts` `it`s ----------------------
  ['finds-a-garment-by-title', definition([wear(['cloak of night'])]), 'Imogen'],
  ['finds-a-garment-by-id', definition([wear(['dress'])]), 'Imogen'],
  ['unknown-title-not-found', definition([wear(['Cloak of Nite'])]), 'Imogen'],
  ['archived-refused-under-wear', definition([wear(['Old Coat'])]), 'Imogen'],
  ['archived-found-under-take-off', definition([takeOff(['Old Coat'])]), 'Imogen'],
  ['slot-not-covered-under-add-to-slot', definition([wear(['Sundress'], { mode: 'add_to_slot', slot: 'footwear' })]), 'Imogen'],
  // ---- the shapes v4's suite leaves implicit ----------------------------
  ['slot-covered-under-add-to-slot', definition([wear(['Sundress'], { mode: 'add_to_slot', slot: 'bottom' })]), 'Imogen'],
  ['take-off-with-a-slot', definition([takeOff(['Sundress'], { slot: 'top' })]), 'Imogen'],
  ['take-off-unknown', definition([takeOff(['Bowler'])]), 'Imogen'],
  ['replace-mode', definition([wear(['Sundress'], { mode: 'replace' })]), 'Imogen'],
  ['replace-archived', definition([wear(['Old Coat'], { mode: 'replace' })]), 'Imogen'],
  ['multi-item-hit-and-miss-in-order', definition([wear(['Sundress', 'Nothing Here', 'cloak of night'])]), 'Imogen'],
  ['clear-slot-beside-wear', definition([{ wardrobe: { action: 'clear_slot', slot: 'hair' } }, wear(['Sundress'])]), 'Imogen'],
  ['non-wardrobe-effect-between-gestures', definition([wear(['dress']), { target: 'state.count', value: 1 }, takeOff(['coat'])]), 'Imogen'],
  ['skipped-effect-keeps-numbering', definition([{ when: { gte: 100 }, wardrobe: { action: 'wear', items: ['Sundress'] } }, wear(['Old Coat'])]), 'Imogen'],
  // A ref that keeps an unknown placeholder (an undeclared `params.x` is a
  // load-time refusal; an unknown FAMILY loads) is looked up as written.
  ['templated-ref-unrendered', definition([wear(['{{mystery}} hat'])]), 'Imogen'],
  ['character-ref-found', definition([wear(["{{character}}'s armour"])]), 'Imogen'],
  ['character-ref-nobody-misses-as-written', definition([wear(["{{character}}'s armour"])]), null],
  ['id-that-is-another-items-title', definition([wear(['cloak'])]), 'Imogen'],
  ['title-case-and-trim', definition([wear(['  RIDING boots  '])]), 'Imogen'],
  ['title-with-trailing-space-in-pool', definition([wear(['silk scarf'])]), 'Imogen'],
  ['general-item-by-title', definition([takeOff(['CLOAK OF NIGHT'])]), 'Imogen'],
  ['archived-general-by-title', definition([wear(['Gone Cloak'])]), 'Imogen'],
  ['archived-general-by-id', definition([wear(['gone'])]), 'Imogen'],
  ['archived-general-by-id-take-off', definition([takeOff(['gone'])]), 'Imogen'],
  ['image-file-id-carried', definition([wear(['boots'])]), 'Imogen'],
  ['only-clear-slot', definition([{ wardrobe: { action: 'clear_slot', slot: 'top' } }]), 'Imogen'],
];

async function main(): Promise<void> {
  for (const [id, doc, character] of lensCases) {
    const text = JSON.stringify(doc);
    const parsed = QtapCustomToolSchema.safeParse(JSON.parse(text));
    if (!parsed.success) throw new Error(`lens case '${id}': definition does not load — ${parsed.error.message}`);
    const result = await executeCustomTool(parsed.data, {}, {
      now: NOW,
      ...(character === null ? {} : { character }),
    });
    const effects = result.effects ?? [];
    rows.push({
      kind: 'lens',
      id,
      inputJson: text,
      character,
      effects: JSON.stringify(effects),
      resolution: JSON.stringify(resolveBenchWardrobeAgainst(pool, effects)),
    });
  }

  // ---- wearable_ref: v4's seven `wearable-ref.test.ts` its ---------------
  const refPool = [
    { id: 'a', title: 'Cloak of Night' },
    { id: 'b', title: 'cloak of night' },
    { id: 'c', title: 'b' },
    { id: 'd', title: 'Linen Shirt' },
  ];
  const templatedPool = [{ id: 'x', title: '{{params.garment}}' }];
  const refCases: Array<[string, 'resolve' | 'matchTitle' | 'isTemplated', Array<{ id: string; title: string }>, string]> = [
    ['prefers-id-over-title', 'resolve', refPool, 'b'],
    ['title-case-insensitive-trimmed', 'resolve', refPool, '  LINEN shirt '],
    ['first-title-match-in-pool-order', 'resolve', refPool, 'CLOAK OF NIGHT'],
    ['templated-never-resolves', 'resolve', refPool, '{{params.garment}}'],
    ['templated-never-resolves-even-on-a-templated-title', 'resolve', templatedPool, '{{params.garment}}'],
    ['blank-resolves-nothing', 'resolve', refPool, '   '],
    ['unknown-resolves-nothing', 'resolve', refPool, 'Bowler Hat'],
    ['match-title-ignores-ids', 'matchTitle', refPool, 'a'],
    ['match-title-blank', 'matchTitle', refPool, '  '],
    ['match-title-untrimmed-pool-title', 'matchTitle', [{ id: 's', title: 'Silk Scarf ' }], 'silk scarf'],
    ['match-title-untrimmed-pool-title-exact', 'matchTitle', [{ id: 's', title: 'Silk Scarf ' }], 'Silk Scarf '],
    ['is-templated-params', 'isTemplated', [], '{{params.garment}}'],
    ['is-templated-mid-string', 'isTemplated', [], 'The {{metadata.hat}} hat'],
    ['is-templated-plain', 'isTemplated', [], 'Cloak of Night'],
    ['is-templated-single-brace', 'isTemplated', [], '{ not a template }'],
    ['is-templated-character', 'isTemplated', [], "{{character}}'s armour"],
  ];
  for (const [id, fn, refs, r] of refCases) {
    const out =
      fn === 'resolve'
        ? (resolveWearableRef(refs, r)?.id ?? null)
        : fn === 'matchTitle'
          ? (matchWearableTitle(refs, r)?.id ?? null)
          : isTemplatedWearableRef(r);
    rows.push({ kind: 'wearable_ref', id, fn, pool: refs, ref: r, out });
  }

  for (const row of rows) process.stdout.write(JSON.stringify(row) + '\n');
}

main().catch((err) => {
  process.stderr.write(`pascal-bench-wardrobe oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
