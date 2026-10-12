/**
 * Oracle case (P4.D279, keystone C1 §1): the Wearable Pool's PURE surface —
 * v4 `lib/wardrobe/pool.ts` (`4a38222e6`) `buildWearablePool` /
 * `componentGraph`, and `lib/wardrobe/wearable-pool.ts` `mergeWearableTiers` /
 * `mergeWearablePool`.
 *
 * Drives the REAL functions over a corpus lifted from
 * `__tests__/unit/lib/wardrobe/pool.test.ts` (the precedence, archived, lookup
 * and I/O-shape cases, each made pure by handing the loader's tiers straight to
 * `buildWearablePool`) plus the three `buildWearablePool` cases of
 * `archived-never-auditions.test.ts`. No mocks, no reimplementation.
 *
 * Every case emits the WHOLE battery over its pool, so the Rust twin
 * (`crates/quilltap-harness/tests/wearable_pool_equivalence.rs`) rebuilds the
 * identical pool from the row's own layers and compares each surface exactly:
 *   - `byId` as an ORDERED array (`Array.from(pool.byId.values())` — v4's `Map`
 *     insertion order: a shadowing item keeps the weaker tier's position);
 *   - `wearable()` and a SECOND call (memoised — must be the same list);
 *   - `get(id)` for every probe id (every corpus id + an unknown);
 *   - `getMany(probes)` with unknowns and duplicates;
 *   - `findByTitle(q)` for a query list (own-first archived-included, trimmed,
 *     case-folded, an archived shared never found, empty / whitespace → none);
 *   - `owns(item)` for every pooled item AND the foreign probes (an object
 *     carrying the character's id for an own id but not from the own layer,
 *     and an impostor shared item claiming the character's id);
 *   - `componentGraph(pool, roots)` as the ORDERED id list (discovery order);
 *   - `mergeWearableTiers([general, project, group, own])` and
 *     `mergeWearablePool(shared, own)` over the same layers.
 *
 * Items are plain JSON (`id`, `title`, `types`, `componentItemIds`,
 * `archivedAt`, `characterId`, `isDefault`, `origin`), tagged with their origin
 * the way the loader tags them (`{ ...item, origin }`, `origin` LAST).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx <v5>/harness/oracle/cases/wearable-pool.ts \
 *     > /tmp/oracle-wearable-pool.ndjson
 */

import { buildWearablePool, componentGraph } from '@/lib/wardrobe/pool';
import type { WardrobeTierMounts } from '@/lib/wardrobe/pool';
import { mergeWearablePool, mergeWearableTiers } from '@/lib/wardrobe/wearable-pool';
import type { WardrobeItem } from '@/lib/schemas/wardrobe.types';
import type { WardrobeItemWithOrigin, WardrobeOrigin } from '@/lib/wardrobe/wardrobe-container';

const CHAR_ID = 'c1c1c1c1-0000-0000-0000-000000000001';
const ARCHIVED = '2026-02-02T00:00:00.000Z';
const TS = '2026-01-01T00:00:00.000Z';

const OWN: WardrobeOrigin = { scope: 'character', id: CHAR_ID, name: '' };
const GENERAL: WardrobeOrigin = { scope: 'general', id: null, name: 'Quilltap General' };
const PROJECT: WardrobeOrigin = { scope: 'project', id: null, name: 'Project' };
const GROUP: WardrobeOrigin = { scope: 'group', id: 'G1', name: 'The Guild' };

/** `pool.test.ts`'s `item(id, tier, overrides)` — the title names the tier that won. */
function item(id: string, tier: string, overrides: Partial<WardrobeItem> = {}): WardrobeItem {
  return {
    id,
    characterId: null,
    title: `${id} (${tier})`,
    types: ['top'],
    componentItemIds: [],
    isDefault: false,
    replace: false,
    archivedAt: null,
    createdAt: TS,
    updatedAt: TS,
    ...overrides,
  } as WardrobeItem;
}

const tag = (items: WardrobeItem[], origin: WardrobeOrigin): WardrobeItemWithOrigin[] =>
  items.map((i) => ({ ...i, origin }));

/** The loader's own-layer coercion: `{ ...item, characterId, origin: ownOrigin }`. */
const own = (items: WardrobeItem[]): WardrobeItemWithOrigin[] =>
  items.map((i) => ({ ...i, characterId: CHAR_ID, origin: OWN }));

interface Case {
  name: string;
  tiers?: WardrobeTierMounts;
  own?: WardrobeItemWithOrigin[];
  group?: WardrobeItemWithOrigin[];
  project?: WardrobeItemWithOrigin[];
  general?: WardrobeItemWithOrigin[];
  /** Extra `findByTitle` queries beyond every pooled title. */
  titleQueries?: string[];
  /** `componentGraph` roots (default: every pooled id, one at a time, plus all at once). */
  graphRoots?: string[][];
}

const NO_MOUNTS: WardrobeTierMounts = { groupMountPointIds: [], projectMountPointIds: [] };

const CASES: Case[] = [
  // ── loadWearablePool — precedence (made pure) ────────────────────────────
  {
    name: 'precedence-character-group-project-general',
    tiers: { groupMountPointIds: ['grp-1'], projectMountPointIds: ['mp-1'] },
    general: tag([item('shared', 'general'), item('g-only', 'general')], GENERAL),
    project: tag([item('shared', 'project'), item('p-only', 'project')], PROJECT),
    group: tag([item('shared', 'group'), item('grp-only', 'group')], GROUP),
    own: own([item('c-only', 'character')]),
  },
  {
    name: 'tags-each-item-with-the-tier-that-won',
    tiers: { groupMountPointIds: ['grp-1'], projectMountPointIds: [] },
    general: tag([item('hat', 'general')], GENERAL),
    group: tag([item('sash', 'group')], GROUP),
    // The vault said someone else; the loader coerces to the character.
    own: own([item('coat', 'character', { characterId: 'someone-else' })]),
  },
  {
    name: 'character-shadows-a-group-item-they-hold-a-copy-of',
    tiers: { groupMountPointIds: ['grp-1'], projectMountPointIds: [] },
    group: tag([item('livery', 'group')], GROUP),
    own: own([item('livery', 'character')]),
  },
  {
    name: 'group-tier-with-no-project-in-scope',
    tiers: { groupMountPointIds: ['grp-1'], projectMountPointIds: [] },
    group: tag([item('house-livery', 'group')], GROUP),
  },
  {
    name: 'same-tier-collision-last-wins',
    tiers: { groupMountPointIds: [], projectMountPointIds: ['mp-1', 'mp-2'] },
    project: tag([item('livery', 'project-mp-1'), item('livery', 'project-mp-2')], PROJECT),
  },
  // ── loadWearablePool — archived items (made pure) ────────────────────────
  {
    name: 'archived-shared-dropped-from-wearable-kept-in-byid',
    general: tag([item('live', 'general'), item('gone', 'general', { archivedAt: ARCHIVED })], GENERAL),
  },
  {
    name: 'archived-own-copy-never-hides-the-shared-one',
    general: tag([item('livery', 'general')], GENERAL),
    own: own([item('livery', 'character', { archivedAt: ARCHIVED })]),
  },
  {
    name: 'archived-group-copy-does-not-hide-the-project-item',
    tiers: { groupMountPointIds: ['grp-1'], projectMountPointIds: ['mp-1'] },
    group: tag([item('cloak', 'group', { archivedAt: ARCHIVED })], GROUP),
    project: tag([item('cloak', 'project')], PROJECT),
  },
  {
    name: 'own-vault-archived-included',
    own: own([item('kept', 'character'), item('shelved', 'character', { archivedAt: ARCHIVED })]),
  },
  // ── WearablePool lookups ─────────────────────────────────────────────────
  {
    name: 'findByTitle-prefers-own-over-shared-same-title',
    own: own([item('mine', 'x', { title: 'Red Scarf' })]),
    general: tag([item('theirs', 'x', { title: 'Red Scarf' })], GENERAL),
    titleQueries: ['red scarf', 'RED SCARF', ' Red Scarf '],
  },
  {
    name: 'findByTitle-finds-an-archived-own-item',
    own: own([item('mine', 'x', { title: 'Red Scarf', archivedAt: ARCHIVED })]),
    general: tag([item('theirs', 'x', { title: 'Red Scarf' })], GENERAL),
    titleQueries: ['Red Scarf'],
  },
  {
    name: 'findByTitle-falls-back-to-wearable-shared-trimmed-case-insensitive',
    group: tag([item('sash', 'x', { title: 'Guild Sash' })], GROUP),
    titleQueries: ['  GUILD sash ', '\tguild SASH\n', ' Guild Sash ', 'guildsash'],
  },
  {
    name: 'findByTitle-never-matches-an-archived-shared-item',
    general: tag([item('old', 'x', { title: 'Old Hat', archivedAt: ARCHIVED })], GENERAL),
    titleQueries: ['Old Hat', 'old hat'],
  },
  {
    name: 'findByTitle-empty-title-is-undefined',
    own: own([item('a', 'x', { title: '' })]),
    titleQueries: ['', '   ', '\t\n', ' '],
  },
  {
    name: 'owns-true-only-for-the-own-vault',
    own: own([item('mine', 'x')]),
    general: tag([item('theirs', 'x')], GENERAL),
  },
  {
    name: 'owns-false-for-a-shared-impostor-claiming-the-character',
    general: tag([item('impostor', 'x', { characterId: CHAR_ID })], GENERAL),
  },
  {
    name: 'owns-false-for-another-characters-item-sharing-an-own-id',
    own: own([item('mine', 'x')]),
  },
  {
    name: 'getMany-keeps-asked-order-skips-unknowns-and-duplicates',
    own: own([item('a', 'x')]),
    general: tag([item('b', 'x')], GENERAL),
  },
  {
    name: 'componentGraph-walks-across-tiers-and-tolerates-cycles',
    own: own([item('outfit', 'x', { componentItemIds: ['sash', 'inner'] })]),
    group: tag([item('sash', 'x')], GROUP),
    general: tag(
      [item('inner', 'x', { componentItemIds: ['leaf', 'outfit'] }), item('leaf', 'x')],
      GENERAL,
    ),
    graphRoots: [['outfit'], ['inner'], ['leaf', 'sash'], ['nope'], ['outfit', 'nope', 'outfit'], []],
  },
  {
    name: 'componentGraph-unknown-component-and-self-reference',
    own: own([
      item('selfish', 'x', { componentItemIds: ['selfish', 'ghost', 'hat'] }),
      item('hat', 'x', { componentItemIds: ['ghost'] }),
    ]),
    graphRoots: [['selfish'], ['hat', 'selfish'], ['ghost']],
  },
  // ── loadWearablePool — I/O shape (the handed-over own items) ─────────────
  {
    name: 'ownItems-handed-over-are-owned',
    own: own([item('handed', 'character')]),
  },
  // ── archived-never-auditions.test.ts — buildWearablePool ×3 ─────────────
  {
    name: 'ana-resurfaces-the-shared-copy-while-byid-sees-the-archived-personal',
    own: own([item('coat', 'x', { title: 'My coat', archivedAt: ARCHIVED })]),
    general: tag([item('coat', 'x', { title: 'House coat' })], GENERAL),
    titleQueries: ['My coat', 'House coat'],
  },
  {
    name: 'ana-archived-group-copy-does-not-hide-the-live-general-one',
    group: tag([item('coat', 'x', { title: 'Group coat', archivedAt: ARCHIVED })], GROUP),
    general: tag([item('coat', 'x', { title: 'House coat' })], GENERAL),
    titleQueries: ['Group coat', 'House coat'],
  },
  {
    name: 'ana-candidate-list-the-house-coat-auditions-in-place-of-the-shelved-copy',
    own: own([item('coat', 'x', { title: 'My coat', archivedAt: ARCHIVED })]),
    general: tag([item('coat', 'x', { title: 'House coat' })], GENERAL),
  },
  // ── the empty pool ───────────────────────────────────────────────────────
  {
    name: 'empty-pool',
    titleQueries: ['anything'],
  },
];

function emit(row: Record<string, unknown>): void {
  process.stdout.write(`${JSON.stringify(row)}\n`);
}

// A stable, machine-checkable marker so a stale oracle file cannot pass
// silently (`oracle-regen-silent-stale-pass`).
emit({ kind: 'meta', baseline: 'p4.d279-wearable-pool', characterId: CHAR_ID });

for (const c of CASES) {
  const layers = {
    own: c.own ?? [],
    group: c.group ?? [],
    project: c.project ?? [],
    general: c.general ?? [],
  };
  const tiers = c.tiers ?? NO_MOUNTS;
  const pool = buildWearablePool(CHAR_ID, tiers, layers);

  const allItems = [...layers.general, ...layers.project, ...layers.group, ...layers.own];
  const ids = Array.from(new Set(allItems.map((i) => i.id)));
  const probeIds = [...ids, 'nope'];
  // getMany: every id twice (dupes) with unknowns sprinkled, in reverse order.
  const getManyIds = [...probeIds].reverse().concat(['ghost', ...ids]);
  const titleQueries = [
    ...Array.from(new Set(allItems.map((i) => i.title))),
    ...(c.titleQueries ?? []),
  ];
  const graphRoots = c.graphRoots ?? [...ids.map((id) => [id]), ids];

  const byId = Array.from(pool.byId.values());
  const wearable = pool.wearable();
  const wearableAgain = pool.wearable();

  const ownsProbes: Array<{ probe: unknown; owns: boolean }> = [];
  for (const it of byId) ownsProbes.push({ probe: { id: it.id, characterId: it.characterId }, owns: pool.owns(it) });
  for (const id of ids) {
    // The character's id on an id the own layer may hold — but not from the own layer.
    ownsProbes.push({ probe: { id, characterId: CHAR_ID }, owns: pool.owns({ id, characterId: CHAR_ID }) });
    ownsProbes.push({
      probe: { id, characterId: 'someone-else' },
      owns: pool.owns({ id, characterId: 'someone-else' }),
    });
    ownsProbes.push({ probe: { id, characterId: null }, owns: pool.owns({ id, characterId: null }) });
  }

  emit({
    kind: 'case',
    name: c.name,
    characterId: CHAR_ID,
    tiers,
    layers,
    poolTiers: pool.tiers,
    byId,
    byIdSize: pool.byId.size,
    wearable,
    wearableMemoised: wearableAgain === wearable,
    wearableAgain,
    get: probeIds.map((id) => ({ id, item: pool.get(id) ?? null })),
    getManyIds,
    getMany: pool.getMany(getManyIds),
    findByTitle: titleQueries.map((q) => ({ query: q, item: pool.findByTitle(q) ?? null })),
    owns: ownsProbes,
    componentGraph: graphRoots.map((roots) => ({
      roots,
      ids: Array.from(componentGraph(pool, roots).keys()),
      items: Array.from(componentGraph(pool, roots).values()),
    })),
    mergeWearableTiers: mergeWearableTiers([layers.general, layers.project, layers.group, layers.own]),
    mergeWearablePool: mergeWearablePool(
      [...layers.general, ...layers.project, ...layers.group],
      layers.own,
    ),
  });
}
