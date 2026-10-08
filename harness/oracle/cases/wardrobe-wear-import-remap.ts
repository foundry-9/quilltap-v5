/**
 * P4.D264 — tier-1 oracle for the PURE half of the `.qtap` import's phase 7e
 * (v4 `lib/import/quilltap-import/import-wardrobe-wear.ts`, `3ee3b1342` #81):
 * `remapWardrobeWearRows(incoming, ctx)` and
 * `buildImportedWardrobeItemIdMap(documents, idMaps)`, driven through v4's
 * REAL exports. Every line carries its own INPUT, so the Rust side
 * (`wardrobe_wear_import_remap_equivalence`) replays exactly what v4 saw: the
 * resolution tables stand in for `resolveWearer` / `resolveChat`, `mintId` is
 * a counter (`00000000-0000-4000-8000-<12 digits, from 1>`), `now` is pinned.
 *
 * Run (Node 24, from the v4 checkout so `@/` resolves to the pinned lib):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/npx tsx "$V5W/harness/oracle/cases/wardrobe-wear-import-remap.ts" \
 *     > /tmp/oracle-wardrobe-wear-import-remap.ndjson
 */

import {
  buildImportedWardrobeItemIdMap,
  remapWardrobeWearRows,
} from '@/lib/import/quilltap-import/import-wardrobe-wear';

const NOW = '2026-10-08T12:00:00.000Z';
const ITEM_A = '11111111-1111-4111-8111-111111111111';
const ITEM_B = '22222222-2222-4222-8222-222222222222';
const DST_A = '33333333-3333-4333-8333-333333333333';
const DST_B = '44444444-4444-4444-8444-444444444444';

type Row = Record<string, unknown>;

function row(over: Row = {}): Row {
  return {
    id: 'aaaaaaaa-0000-4000-8000-000000000001',
    itemId: ITEM_A,
    wearerCharacterId: 'char-src',
    wearCount: 4,
    firstWornAt: '2026-01-01T00:00:00.000Z',
    lastWornAt: '2026-02-01T00:00:00.000Z',
    lastWornChatId: 'chat-src',
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-02-01T00:00:00.000Z',
    ...over,
  };
}

interface RemapCase {
  name: string;
  incoming: unknown[];
  itemIds: Array<[string, string]>;
  /** source → destination, or `null` (unresolvable). Absent ids → null. */
  wearers: Array<[string, string | null]>;
  chats: Array<[string, string | null]>;
  existing: Row[];
}

const REMAP_CASES: RemapCase[] = [
  {
    name: 'one_row_fully_mapped',
    incoming: [row()],
    itemIds: [[ITEM_A, DST_A]],
    wearers: [['char-src', 'char-dst']],
    chats: [['chat-src', 'chat-dst']],
    existing: [],
  },
  {
    name: 'missing_item_dropped',
    incoming: [row({ itemId: ITEM_B })],
    itemIds: [[ITEM_A, DST_A]],
    wearers: [['char-src', 'char-dst']],
    chats: [['chat-src', 'chat-dst']],
    existing: [],
  },
  {
    name: 'wearer_folded_chat_cleared',
    incoming: [row()],
    itemIds: [[ITEM_A, DST_A]],
    wearers: [['char-src', null]],
    chats: [['chat-src', null]],
    existing: [],
  },
  {
    name: 'three_unknown_wearers_sum_into_unattributed',
    incoming: [
      row({ id: 'aaaaaaaa-0000-4000-8000-000000000001', wearerCharacterId: 'w1', wearCount: 2, firstWornAt: '2026-01-05T00:00:00.000Z', lastWornAt: '2026-01-10T00:00:00.000Z', lastWornChatId: 'c1', createdAt: '2026-01-05T00:00:00.000Z' }),
      row({ id: 'aaaaaaaa-0000-4000-8000-000000000002', wearerCharacterId: 'w2', wearCount: 4, firstWornAt: '2026-01-01T00:00:00.000Z', lastWornAt: '2026-03-01T00:00:00.000Z', lastWornChatId: 'c2', createdAt: '2026-01-01T00:00:00.000Z' }),
      row({ id: 'aaaaaaaa-0000-4000-8000-000000000003', wearerCharacterId: null, wearCount: 1, firstWornAt: '2026-02-01T00:00:00.000Z', lastWornAt: '2026-02-02T00:00:00.000Z', lastWornChatId: null, createdAt: '2026-02-01T00:00:00.000Z' }),
    ],
    itemIds: [[ITEM_A, DST_A]],
    wearers: [],
    chats: [['c1', 'c1-dst'], ['c2', 'c2-dst']],
    existing: [],
  },
  {
    name: 'equal_last_wear_keeps_the_first',
    incoming: [
      row({ id: 'aaaaaaaa-0000-4000-8000-000000000001', wearerCharacterId: null, lastWornChatId: 'c1' }),
      row({ id: 'aaaaaaaa-0000-4000-8000-000000000002', wearerCharacterId: null, lastWornChatId: 'c2' }),
    ],
    itemIds: [[ITEM_A, DST_A]],
    wearers: [],
    chats: [['c1', 'c1-dst'], ['c2', 'c2-dst']],
    existing: [],
  },
  {
    name: 'live_row_takes_max_and_keeps_id_and_created',
    incoming: [row({ wearCount: 3, firstWornAt: '2025-12-01T00:00:00.000Z', lastWornAt: '2026-04-01T00:00:00.000Z' })],
    itemIds: [[ITEM_A, DST_A]],
    wearers: [['char-src', 'char-dst']],
    chats: [['chat-src', 'chat-dst']],
    existing: [
      {
        id: 'live0000-0000-4000-8000-000000000001',
        itemId: DST_A,
        wearerCharacterId: 'char-dst',
        wearCount: 5,
        firstWornAt: '2026-01-02T00:00:00.000Z',
        lastWornAt: '2026-03-01T00:00:00.000Z',
        lastWornChatId: 'chat-live',
        createdAt: '2025-06-01T00:00:00.000Z',
        updatedAt: '2026-03-01T00:00:00.000Z',
      },
    ],
  },
  {
    name: 'idempotent_reimport_is_max_not_sum',
    incoming: [row({ wearerCharacterId: null, lastWornChatId: null })],
    itemIds: [[ITEM_A, DST_A]],
    wearers: [],
    chats: [],
    existing: [
      {
        id: 'live0000-0000-4000-8000-000000000002',
        itemId: DST_A,
        wearerCharacterId: null,
        wearCount: 4,
        firstWornAt: '2026-01-01T00:00:00.000Z',
        lastWornAt: '2026-02-01T00:00:00.000Z',
        lastWornChatId: null,
        createdAt: '2026-01-01T00:00:00.000Z',
        updatedAt: '2026-02-01T00:00:00.000Z',
      },
    ],
  },
  {
    name: 'two_live_rows_on_one_key_the_later_wins',
    incoming: [row({ wearerCharacterId: null, lastWornChatId: null, wearCount: 1 })],
    itemIds: [[ITEM_A, DST_A]],
    wearers: [],
    chats: [],
    existing: [
      { id: 'live0000-0000-4000-8000-00000000000a', itemId: DST_A, wearerCharacterId: null, wearCount: 9, firstWornAt: '2026-01-01T00:00:00.000Z', lastWornAt: '2026-01-02T00:00:00.000Z', lastWornChatId: null, createdAt: '2026-01-01T00:00:00.000Z', updatedAt: '2026-01-02T00:00:00.000Z' },
      { id: 'live0000-0000-4000-8000-00000000000b', itemId: DST_A, wearerCharacterId: null, wearCount: 2, firstWornAt: '2026-01-03T00:00:00.000Z', lastWornAt: '2026-01-04T00:00:00.000Z', lastWornChatId: null, createdAt: '2026-01-03T00:00:00.000Z', updatedAt: '2026-01-04T00:00:00.000Z' },
    ],
  },
  {
    name: 'empty_string_wearer_and_chat_are_no_wearer_no_chat',
    incoming: [row({ wearerCharacterId: '', lastWornChatId: '' })],
    itemIds: [[ITEM_A, DST_A]],
    wearers: [['', 'should-not-be-asked']],
    chats: [['', 'should-not-be-asked']],
    existing: [],
  },
  {
    name: 'two_items_keep_first_seen_order',
    incoming: [
      row({ id: 'aaaaaaaa-0000-4000-8000-000000000001', itemId: ITEM_B, wearerCharacterId: null, lastWornChatId: null }),
      row({ id: 'aaaaaaaa-0000-4000-8000-000000000002', itemId: ITEM_A, wearerCharacterId: null, lastWornChatId: null }),
      row({ id: 'aaaaaaaa-0000-4000-8000-000000000003', itemId: ITEM_B, wearerCharacterId: null, lastWornChatId: null, wearCount: 1 }),
    ],
    itemIds: [[ITEM_A, DST_A], [ITEM_B, DST_B]],
    wearers: [],
    chats: [],
    existing: [],
  },
  {
    name: 'malformed_rows_are_counted_and_dropped',
    incoming: [
      { itemId: ITEM_A },
      null,
      'a string',
      row({ id: 'not-a-uuid' }),
      row({ wearCount: -1 }),
      row({ wearCount: 1.5 }),
      row({ wearCount: '3' }),
      row({ itemId: '' }),
      row({ itemId: 7 }),
      row({ lastWornAt: 'yesterday' }),
      row({ createdAt: undefined }),
      (() => {
        const r = row();
        delete r.wearerCharacterId;
        return r;
      })(),
      (() => {
        const r = row();
        delete r.lastWornChatId;
        return r;
      })(),
      row({ wearerCharacterId: 5 }),
      row({ id: 'aaaaaaaa-0000-4000-8000-0000000000ff', extra: 'stripped by Zod', wearCount: 0 }),
    ],
    itemIds: [[ITEM_A, DST_A]],
    wearers: [['char-src', 'char-dst']],
    chats: [['chat-src', 'chat-dst']],
    existing: [],
  },
];

interface MapCase {
  name: string;
  documents: Row[];
  mountPoints: Array<[string, string]>;
  wardrobeItems: Array<[string, string]>;
}

const FM = (id: string, title: string) => `---\nid: ${id}\ntitle: ${title}\ntypes: [top]\n---\n`;

const MAP_CASES: MapCase[] = [
  {
    name: 'documents_win_over_scaffold_ids',
    documents: [
      { mountPointId: 'mp-src', relativePath: 'Wardrobe/Coat.md', content: FM(ITEM_A, 'Coat') },
      { mountPointId: 'mp-src', relativePath: 'Wardrobe/Hat.md', content: '---\ntitle: Hat\ntypes: [accessories]\n---\n' },
      { mountPointId: 'mp-src', relativePath: 'WARDROBE/Scarf.MD', content: '---\nid: not-a-uuid\ntitle: Scarf\ntypes: [accessories]\n---\n' },
      { mountPointId: 'mp-src', relativePath: 'Wardrobe/instructions.md', content: 'Dress for rain.' },
      { mountPointId: 'mp-src', relativePath: 'Wardrobe/Old/Coat.md', content: FM(ITEM_B, 'Old Coat') },
      { mountPointId: 'mp-src', relativePath: 'Notes/Hat.md', content: FM(ITEM_B, 'Notes Hat') },
      { mountPointId: 'mp-skipped', relativePath: 'Wardrobe/Boots.md', content: FM(ITEM_B, 'Boots') },
    ],
    mountPoints: [['mp-src', 'mp-dst']],
    wardrobeItems: [[ITEM_A, 'scaffold-id'], ['legacy-src', 'legacy-dst']],
  },
  {
    name: 'no_documents_keeps_the_scaffold_map',
    documents: [],
    mountPoints: [],
    wardrobeItems: [[ITEM_A, 'scaffold-a'], [ITEM_B, 'scaffold-b']],
  },
];

let counter = 0;
const mintId = (): string => `00000000-0000-4000-8000-${String(++counter).padStart(12, '0')}`;

const out: string[] = [];
for (const c of REMAP_CASES) {
  counter = 0;
  const wearers = new Map(c.wearers);
  const chats = new Map(c.chats);
  const result = remapWardrobeWearRows(c.incoming as never, {
    itemIds: new Map(c.itemIds),
    resolveWearer: (id: string) => wearers.get(id) ?? null,
    resolveChat: (id: string) => chats.get(id) ?? null,
    existing: c.existing as never,
    mintId,
    now: NOW,
  });
  // `JSON.stringify` drops an `undefined` key; the corpus rows ride as text.
  out.push(JSON.stringify({ kind: 'remap', name: c.name, input: { ...c, now: NOW }, result }));
}
for (const c of MAP_CASES) {
  const map = buildImportedWardrobeItemIdMap(c.documents as never, {
    mountPoints: new Map(c.mountPoints),
    wardrobeItems: new Map(c.wardrobeItems),
  } as never);
  out.push(JSON.stringify({ kind: 'map', name: c.name, input: c, map: Array.from(map.entries()) }));
}
process.stdout.write(out.join('\n') + '\n');
