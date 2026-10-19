/**
 * P4.D317 — tier-1 oracle for v4's pure bug-252 pass,
 * `remapCharacterKeyedChatFields` (`lib/chat/character-keyed-fields.ts`,
 * `e8e17b2d9`), driven through v4's REAL export. The Rust twin is
 * `quilltap_core::chat::character_keyed_fields` and the family is
 * `chat_keyed_fields_equivalence`.
 *
 * Remaps are DATA, so every line carries its own input and the Rust side
 * replays exactly what v4 saw:
 *   - `characters` — `[sourceId, newId | null]` pairs (`null` DROPS the entry,
 *     `""` drops too — v4's truthiness); a miss follows `characterMiss`
 *     (`drop` → null, `keep` → the id itself, `mint` → a fresh id);
 *   - `items` — a lookup, a miss passes the id through;
 *   - `files` / `messages` — a lookup, a miss MINTS `mint-<n>` from ONE
 *     counter shared with `characterMiss: 'mint'`, and every mint is recorded
 *     in `minted` (`{ kind, from, to }`) — so the mint ORDER is compared.
 * The patch rides as `patchText` (`JSON.stringify(patch)`) — byte-compared
 * on the Rust side (key order included; memory note
 * `serde-json-map-eq-ignores-order`), and readable there even for the
 * 129-deep row serde could not parse as a value.
 *
 * `expectedDifferent` marks R-2's three V8-only `JSON.parse` classes: v4 reads
 * the column, v5's `serde_json` refuses it and reports it `unreadable`
 * (recorded, not closed — P4.D317 R-2).
 *
 * Run (Node 24, from the v4 checkout so `@/` resolves to the pinned lib):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/npx tsx "$V5W/harness/oracle/cases/character-keyed-fields.ts" \
 *     > /tmp/oracle-character-keyed-fields.ndjson
 */

import {
  CHARACTER_KEYED_CHAT_FIELDS,
  remapCharacterKeyedChatFields,
} from '@/lib/chat/character-keyed-fields';

type Json = unknown;

interface Remaps {
  characters: Array<[string, string | null]>;
  characterMiss: 'drop' | 'keep' | 'mint';
  items: Array<[string, string]>;
  files: Array<[string, string]>;
  messages: Array<[string, string]>;
}

interface Case {
  name: string;
  chat: Record<string, Json>;
  remaps: Remaps;
  expectedDifferent?: string;
}

const EMPTY = { top: [], bottom: [], footwear: [], accessories: [], hair: [] };

/** v4's own test remaps (`character-keyed-fields.test.ts:14-19`). */
const V4_REMAPS: Remaps = {
  characters: [
    ['char-a', 'char-A'],
    ['char-b', 'char-B'],
  ],
  characterMiss: 'drop',
  items: [['item-1', 'item-ONE']],
  files: [['file-1', 'file-ONE']],
  messages: [['msg-1', 'msg-ONE']],
};

/** The restore's shape: a carried character, a mint on every unseen file / message. */
const MINTING: Remaps = {
  characters: [
    ['char-a', 'char-A'],
    ['char-b', 'char-B'],
  ],
  characterMiss: 'drop',
  items: [['item-1', 'item-ONE']],
  files: [['file-1', 'file-ONE']],
  messages: [],
};

const deep = (n: number): string => '['.repeat(n) + ']'.repeat(n);

const CASES: Case[] = [
  // ── v4's ten `it`s (`__tests__/unit/lib/chat/character-keyed-fields.test.ts:26-97`) ──
  {
    name: 'v4_re_keys_the_outfit_keeping_unknown_items',
    chat: { equippedOutfit: { 'char-a': { ...EMPTY, top: ['item-1', 'item-x'] } } },
    remaps: V4_REMAPS,
  },
  {
    name: 'v4_fills_a_pre_hair_four_slot_bag',
    chat: { equippedOutfit: { 'char-a': { top: [], bottom: [], footwear: ['item-1'], accessories: [] } } },
    remaps: V4_REMAPS,
  },
  {
    name: 'v4_repairs_a_malformed_slot',
    chat: { equippedOutfit: { 'char-a': { ...EMPTY, top: 'item-1' } } },
    remaps: V4_REMAPS,
  },
  {
    name: 'v4_re_keys_outfit_avatars',
    chat: { characterAvatars: { 'char-a': { imageId: 'file-1', generatedAt: 't', afterMessageCount: 3 } } },
    remaps: V4_REMAPS,
  },
  {
    name: 'v4_re_keys_courier_checkpoints',
    chat: { courierCheckpoints: { 'char-b': { lastResolvedMessageId: 'msg-1', resolvedAt: 't' } } },
    remaps: V4_REMAPS,
  },
  {
    name: 'v4_re_keys_queued_outfit_notices',
    chat: { pendingOutfitNotifications: { 'char-a': { text: 'changed', at: 't' } } },
    remaps: V4_REMAPS,
  },
  {
    name: 'v4_drops_an_unplaceable_character',
    chat: { equippedOutfit: { 'char-a': EMPTY, 'char-gone': EMPTY } },
    remaps: V4_REMAPS,
  },
  {
    name: 'v4_reads_a_column_stored_as_a_json_string',
    chat: { equippedOutfit: JSON.stringify({ 'char-a': { ...EMPTY, top: ['item-1'] } }) },
    remaps: V4_REMAPS,
  },
  {
    name: 'v4_skips_null_and_missing_reports_unreadable',
    chat: { equippedOutfit: null, courierCheckpoints: [], pendingOutfitNotifications: 'not json' },
    remaps: V4_REMAPS,
  },
  // (v4's first `it`, "lists the four keyed columns", is the `fields` line below.)

  // ── survey §4.2's `chat_keyed_edges`, one column per edge ──
  {
    name: 'edge_column_as_json_string_every_field',
    chat: {
      characterAvatars: JSON.stringify({ 'char-a': { imageId: 'file-1', generatedAt: 'g' } }),
      courierCheckpoints: JSON.stringify({ 'char-b': { lastResolvedMessageId: 'msg-9', resolvedAt: 'r' } }),
      pendingOutfitNotifications: JSON.stringify({ 'char-a': 'kept' }),
    },
    remaps: MINTING,
  },
  {
    name: 'edge_null_as_a_string_is_unreadable',
    chat: { equippedOutfit: 'null', characterAvatars: 'true', courierCheckpoints: '7', pendingOutfitNotifications: '[]' },
    remaps: V4_REMAPS,
  },
  {
    name: 'edge_number_array_true_are_unreadable',
    chat: { equippedOutfit: 5, characterAvatars: [{ imageId: 'file-1' }], courierCheckpoints: true, pendingOutfitNotifications: 0 },
    remaps: V4_REMAPS,
  },
  {
    name: 'edge_false_and_empty_string_are_unreadable_not_skipped',
    chat: { equippedOutfit: false, characterAvatars: '' },
    remaps: V4_REMAPS,
  },
  {
    name: 'edge_empty_object_is_readable',
    chat: { equippedOutfit: {}, characterAvatars: '{}' },
    remaps: V4_REMAPS,
  },
  {
    name: 'edge_null_and_absent_are_skipped',
    chat: { equippedOutfit: null, characterAvatars: null },
    remaps: V4_REMAPS,
  },
  {
    name: 'edge_no_columns_at_all',
    chat: { id: 'chat-1', title: 'Untouched' },
    remaps: V4_REMAPS,
  },
  {
    name: 'edge_outfit_entry_as_json_string_reads_empty',
    chat: { equippedOutfit: { 'char-a': JSON.stringify({ ...EMPTY, top: ['item-1'] }) } },
    remaps: V4_REMAPS,
  },
  {
    name: 'edge_outfit_entry_null_number_array',
    chat: { equippedOutfit: { 'char-a': null, 'char-b': [1, 2] } },
    remaps: V4_REMAPS,
  },
  {
    name: 'edge_outfit_slots_non_string_elements_and_extra_keys',
    chat: {
      equippedOutfit: {
        'char-a': { top: ['item-1', 7, null, 'item-2'], hair: 'braid', cape: ['item-1'], footwear: { x: 1 } },
      },
    },
    remaps: V4_REMAPS,
  },
  {
    name: 'edge_avatar_entry_json_string_with_string_image_id',
    chat: { characterAvatars: { 'char-a': JSON.stringify({ imageId: 'file-1', generatedAt: 'g', afterMessageCount: 2 }) } },
    remaps: V4_REMAPS,
  },
  {
    name: 'edge_avatar_entry_json_string_without_image_id_kept_a_string',
    chat: { characterAvatars: { 'char-a': JSON.stringify({ generatedAt: 'g' }), 'char-b': 'not json' } },
    remaps: V4_REMAPS,
  },
  {
    name: 'edge_avatar_image_id_number_kept_as_given',
    chat: { characterAvatars: { 'char-a': { imageId: 42, generatedAt: 'g' }, 'char-b': { imageId: null } } },
    remaps: MINTING,
  },
  {
    name: 'edge_checkpoint_entry_scalar_kept',
    chat: { courierCheckpoints: { 'char-a': 'msg-1', 'char-b': 12 } },
    remaps: MINTING,
  },
  {
    name: 'edge_empty_string_new_id_drops',
    chat: { pendingOutfitNotifications: { 'char-a': 1, 'char-empty': 2 } },
    remaps: { ...V4_REMAPS, characters: [...V4_REMAPS.characters, ['char-empty', '']] },
  },
  {
    name: 'edge_keep_unmapped_characters_import_shape',
    chat: { courierCheckpoints: { 'char-a': { lastResolvedMessageId: 'msg-1' }, 'char-other': { lastResolvedMessageId: 'msg-2' } } },
    remaps: { ...V4_REMAPS, characterMiss: 'keep' },
  },

  // ── mint order and own-key order ──
  {
    name: 'mint_order_across_columns_and_a_dropped_entry_never_mints',
    chat: {
      pendingOutfitNotifications: { 'char-b': 'n' },
      courierCheckpoints: {
        'char-gone': { lastResolvedMessageId: 'msg-gone' },
        'char-a': { lastResolvedMessageId: 'msg-a', resolvedAt: 'r' },
        'char-b': { lastResolvedMessageId: 'msg-b' },
      },
      characterAvatars: {
        'char-b': { imageId: 'file-b' },
        'char-gone': { imageId: 'file-gone' },
        'char-a': { imageId: 'file-1' },
      },
      equippedOutfit: { 'char-a': { top: ['item-1'] } },
    },
    remaps: MINTING,
  },
  {
    name: 'mint_characters_before_values',
    chat: { characterAvatars: { 'char-x': { imageId: 'file-x' }, 'char-y': { imageId: 'file-y' } } },
    remaps: { ...MINTING, characters: [], characterMiss: 'mint' },
  },
  {
    name: 'array_index_keys_sort_first_at_every_depth',
    chat: {
      pendingOutfitNotifications: JSON.stringify({ zed: { b: 1, '10': 2, '2': 3 }, '7': 'seven', alpha: [{ y: 0, '1': 1 }] }),
      characterAvatars: JSON.stringify({ 'char-a': { generatedAt: 'g', '3': 'three', imageId: 'file-1' } }),
    },
    remaps: { ...V4_REMAPS, characters: [['zed', '12'], ['7', 'seven'], ['alpha', '01'], ['char-a', '4']] },
  },
  {
    name: 'two_characters_onto_one_new_id_last_value_first_position',
    chat: { pendingOutfitNotifications: { 'char-a': 'first', mid: 'middle', 'char-b': 'second' } },
    remaps: { ...V4_REMAPS, characters: [['char-a', 'same'], ['mid', 'mid'], ['char-b', 'same']] },
  },
  {
    name: 'duplicate_keys_in_a_json_string_last_wins_first_position',
    chat: { pendingOutfitNotifications: '{"char-a":"one","char-b":"b","char-a":"two"}' },
    remaps: V4_REMAPS,
  },

  // ── R-2: V8 accepts, serde refuses (recorded, EXPECTED-DIFFERENT) ──
  {
    name: 'r2_number_past_f64',
    chat: { pendingOutfitNotifications: '{"char-a":1e400}' },
    remaps: V4_REMAPS,
    expectedDifferent: 'V8 JSON.parse reads 1e400 as Infinity; serde_json refuses a number past f64',
  },
  {
    name: 'r2_lone_surrogate_escape',
    chat: { pendingOutfitNotifications: '{"char-a":"\\ud800"}' },
    remaps: V4_REMAPS,
    expectedDifferent: 'V8 JSON.parse admits a lone-surrogate \\u escape; serde_json refuses it',
  },
  {
    name: 'r2_nesting_past_128',
    chat: { pendingOutfitNotifications: `{"char-a":${deep(129)}}` },
    remaps: V4_REMAPS,
    expectedDifferent: "V8 JSON.parse nests past 128; serde_json's recursion limit refuses it",
  },
];

let counter = 0;
const out: string[] = [];
out.push(JSON.stringify({ kind: 'fields', fields: [...CHARACTER_KEYED_CHAT_FIELDS] }));
for (const c of CASES) {
  counter = 0;
  const minted: Array<{ kind: string; from: string; to: string }> = [];
  const mint = (kind: string, from: string): string => {
    const to = `mint-${++counter}`;
    minted.push({ kind, from, to });
    return to;
  };
  const characters = new Map(c.remaps.characters);
  const items = new Map(c.remaps.items);
  const files = new Map(c.remaps.files);
  const messages = new Map(c.remaps.messages);
  const result = remapCharacterKeyedChatFields(c.chat as never, {
    characterId: (id) => {
      if (characters.has(id)) return characters.get(id) ?? null;
      if (c.remaps.characterMiss === 'keep') return id;
      if (c.remaps.characterMiss === 'mint') return mint('character', id);
      return null;
    },
    wardrobeItemId: (id) => items.get(id) ?? id,
    fileId: (id) => files.get(id) ?? mint('file', id),
    messageId: (id) => messages.get(id) ?? mint('message', id),
  });
  out.push(
    JSON.stringify({
      kind: 'remap',
      name: c.name,
      input: { chat: c.chat, remaps: c.remaps },
      ...(c.expectedDifferent ? { expectedDifferent: c.expectedDifferent } : {}),
      patchText: JSON.stringify(result.patch),
      unreadable: result.unreadable,
      droppedEntries: result.droppedEntries,
      minted,
    }),
  );
}
process.stdout.write(out.join('\n') + '\n');
