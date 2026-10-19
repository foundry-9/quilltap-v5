/**
 * P4.D317 (Tier 2, R-12) — tier-1 oracle for the PURE half of the `.qtap`
 * import's phase 7f: `remapWardrobeHistoryRows(incoming, ctx)`
 * (`lib/import/quilltap-import/import-wardrobe-history.ts`, `7953da3a0` +
 * `4ae20977f`), driven through v4's REAL export — and with it v4's REAL
 * `ChatWardrobeHistoryRowSchema` (the whole-row Zod gate) and
 * `hashEquippedSlots`. The state family (`system_import_state`) reaches one
 * malformed row; this corpus walks the whole schema.
 *
 * Every line carries its own INPUT (the raw rows — malformed ones included —
 * and the three id tables); `mintId` is a counter (`00000000-0000-4000-8000-
 * <12 digits, from 1>`), so the mint ORDER is compared. The Rust side
 * (`wardrobe_history_import_remap_equivalence`) replays it and byte-compares.
 *
 * Run (Node 24, from the v4 checkout so `@/` resolves to the pinned lib):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/npx tsx "$V5W/harness/oracle/cases/wardrobe-history-import-remap.ts" \
 *     > /tmp/oracle-wardrobe-history-import-remap.ndjson
 */

import { remapWardrobeHistoryRows } from '@/lib/import/quilltap-import/import-wardrobe-history';

type Row = Record<string, unknown>;

const CHAT = '11111111-1111-4111-8111-111111111111';
const CHAT_DST = '22222222-2222-4222-8222-222222222222';
const ITEM = '33333333-3333-4333-8333-333333333333';
const ITEM_DST = '44444444-4444-4444-8444-444444444444';
const ID = '55555555-5555-4555-8555-555555555555';

function row(over: Row = {}, drop: string[] = []): Row {
  const r: Row = {
    id: ID,
    chatId: CHAT,
    characterId: 'char-src',
    entryIndex: 0,
    slots: { top: [ITEM], bottom: [], footwear: [], accessories: [], hair: [] },
    slotsHash: '0123456789abcdef',
    source: 'character',
    actorCharacterId: 'char-src',
    actorToolName: null,
    isCurrent: true,
    createdAt: '2026-03-01T00:00:00.000Z',
    ...over,
  };
  for (const k of drop) delete r[k];
  return r;
}

interface Case {
  name: string;
  incoming: unknown[];
  chats: Array<[string, string]>;
  characters: Array<[string, string]>;
  items: Array<[string, string]>;
}

const MAPS = {
  chats: [[CHAT, CHAT_DST]] as Array<[string, string]>,
  characters: [
    ['char-src', 'char-dst'],
    ['actor-src', 'actor-dst'],
  ] as Array<[string, string]>,
  items: [[ITEM, ITEM_DST]] as Array<[string, string]>,
};

const valid = (name: string, incoming: unknown[]): Case => ({ name, incoming, ...MAPS });

const CASES: Case[] = [
  // ── the remap ──
  valid('one_row_fully_mapped', [row()]),
  valid('maps_miss_keep_ids', [
    row({ chatId: 'aaaaaaaa-0000-4000-8000-000000000001', characterId: 'char-other', actorCharacterId: 'actor-other',
          slots: { top: ['bbbbbbbb-0000-4000-8000-000000000002'] } }),
  ]),
  valid('actor_mapped_null_and_empty', [
    row({ actorCharacterId: 'actor-src' }),
    row({ id: 'aaaaaaaa-0000-4000-8000-000000000003', actorCharacterId: null }),
    row({ id: 'aaaaaaaa-0000-4000-8000-000000000004', actorCharacterId: '' }),
  ]),
  valid('mint_order_is_input_order_valid_rows_only', [
    row({ id: 'aaaaaaaa-0000-4000-8000-000000000005', entryIndex: 2 }),
    row({ slotsHash: 'short' }),
    row({ id: 'aaaaaaaa-0000-4000-8000-000000000006', entryIndex: 1 }),
  ]),
  valid('slots_absent_keys_default_and_extras_stripped', [
    row({ slots: { top: [ITEM], cape: ['x'] } }),
    row({ id: 'aaaaaaaa-0000-4000-8000-000000000007', slots: {} }),
  ]),
  valid('copies_index_cursor_source_tool_created', [
    row({ entryIndex: 7, isCurrent: false, source: 'pascal', actorToolName: 'dress_for_dinner',
          createdAt: '2026-10-16T12:34:56.789Z' }),
    row({ id: 'aaaaaaaa-0000-4000-8000-000000000008', source: 'seeded' }, ['actorToolName']),
  ]),
  valid('unknown_keys_stripped', [row({ extra: 'gone', slotsHash: 'fedcba9876543210' })]),
  valid('every_source_value', ['chat-start', 'participant-added', 'merge', 'operator', 'character', 'pascal', 'seeded']
    .map((source, i) => row({ id: `aaaaaaaa-0000-4000-8000-0000000001${String(i).padStart(2, '0')}`, source }))),
  valid('empty_incoming', []),
  // ── the Zod matrix (each row ALONE, so `droppedInvalid` names the branch) ──
  ...(
    [
      ['missing_id', row({}, ['id'])],
      ['id_not_uuid', row({ id: 'not-a-uuid' })],
      ['id_nil_uuid', row({ id: '00000000-0000-0000-0000-000000000000' })],
      ['id_wrong_variant', row({ id: '55555555-5555-4555-c555-555555555555' })],
      ['missing_chat_id', row({}, ['chatId'])],
      ['chat_id_number', row({ chatId: 5 })],
      ['missing_character_id', row({}, ['characterId'])],
      ['character_id_empty', row({ characterId: '' })],
      ['character_id_null', row({ characterId: null })],
      ['missing_entry_index', row({}, ['entryIndex'])],
      ['entry_index_float', row({ entryIndex: 1.5 })],
      ['entry_index_negative', row({ entryIndex: -1 })],
      ['entry_index_past_safe', row({ entryIndex: 2 ** 53 })],
      ['entry_index_string', row({ entryIndex: '1' })],
      ['entry_index_true', row({ entryIndex: true })],
      ['missing_slots', row({}, ['slots'])],
      ['slots_null', row({ slots: null })],
      ['slots_array', row({ slots: [] })],
      ['slot_not_array', row({ slots: { top: ITEM } })],
      ['slot_null', row({ slots: { top: null } })],
      ['slot_element_not_uuid', row({ slots: { top: ['item-1'] } })],
      ['slot_element_number', row({ slots: { hair: [5] } })],
      ['missing_hash', row({}, ['slotsHash'])],
      ['hash_15', row({ slotsHash: '0123456789abcde' })],
      ['hash_17', row({ slotsHash: '0123456789abcdef0' })],
      ['hash_number', row({ slotsHash: 1234567890123456 })],
      ['missing_source', row({}, ['source'])],
      ['source_bogus', row({ source: 'bogus' })],
      ['missing_actor', row({}, ['actorCharacterId'])],
      ['actor_number', row({ actorCharacterId: 5 })],
      ['tool_number', row({ actorToolName: 5 })],
      ['missing_is_current', row({}, ['isCurrent'])],
      ['is_current_one', row({ isCurrent: 1 })],
      ['is_current_string', row({ isCurrent: 'true' })],
      ['missing_created_at', row({}, ['createdAt'])],
      ['created_at_not_iso', row({ createdAt: 'yesterday' })],
      ['created_at_date_only', row({ createdAt: '2026-03-01' })],
      ['created_at_offset', row({ createdAt: '2026-03-01T00:00:00+02:00' })],
      ['row_null', null],
      ['row_array', []],
      ['row_string', 'x'],
    ] as Array<[string, unknown]>
  ).map(([name, r]) => valid(`zod_${name}`, [r])),
  // ── the matrix's passing edges ──
  valid('hash_16_astral_units', [row({ slotsHash: '😀😀😀😀😀😀😀😀' })]),
  valid('hash_16_with_8_astral_chars_is_16_units', [row({ slotsHash: '0123456789abcd😀' })]),
  valid('entry_index_one_point_zero', [row({ entryIndex: 1.0 })]),
  valid('entry_index_max_safe', [row({ entryIndex: 2 ** 53 - 1 })]),
  valid('character_id_one_astral', [row({ characterId: '😀' })]),
  valid('created_at_no_millis', [row({ createdAt: '2026-03-01T00:00:00Z' })]),
];

let counter = 0;
const mintId = (): string => `00000000-0000-4000-8000-${String(++counter).padStart(12, '0')}`;

const out: string[] = [];
for (const c of CASES) {
  counter = 0;
  const result = remapWardrobeHistoryRows(c.incoming, {
    chatIds: new Map(c.chats),
    characterIds: new Map(c.characters),
    itemIds: new Map(c.items),
    mintId,
  });
  out.push(JSON.stringify({ name: c.name, input: c, resultText: JSON.stringify(result) }));
}
process.stdout.write(out.join('\n') + '\n');
