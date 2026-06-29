/**
 * Tier-1 oracle case — the legacy `wardrobe.json` parser.
 *
 * Drives v4's REAL parseLegacyWardrobeJson
 * (lib/database/repositories/vault-overlay/parsers.ts) over a corpus of raw file
 * strings (valid + every interesting violation), emitting the parsed `{ items }`
 * object or null. The Rust port
 * (quilltap_core::vault_overlay::parse_legacy_wardrobe_json) must match exactly —
 * including the safeParse "fall back to null on any violation" semantics, the
 * unknown-key stripping (root `presets`, per-item extras, in-`outfit` extras),
 * the `.default()` materialization (componentItemIds/isDefault/replace), and the
 * z.uuid() / z.iso.datetime() string formats (leap years, Z-only zone, trailing
 * newline rejection).
 *
 * Run from the v4 server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-legacy-wardrobe.ts \
 *     > /tmp/oracle-vault-legacy-wardrobe.ndjson
 */

import { parseLegacyWardrobeJson } from '@/lib/database/repositories/vault-overlay/parsers';

type Row = { id: string; raw: string; out: unknown };

const rows: Row[] = [];
const CID = 'char-1';

function wCase(id: string, raw: string) {
  rows.push({ id, raw, out: parseLegacyWardrobeJson(raw, CID) ?? null });
}

const J = (v: unknown) => JSON.stringify(v);

const U1 = '123e4567-e89b-12d3-a456-426614174000';
const U2 = '00000000-0000-0000-0000-000000000000';
const U3 = 'aaaaaaaa-aaaa-8aaa-8aaa-aaaaaaaaaaaa';
const T1 = '2024-01-01T00:00:00.000Z';
const T2 = '2024-01-02T00:00:00.000Z';

// A fully-populated item (every field, including the nullable-optionals as
// values), with scrambled key order + an unknown key to prove schema-order
// output + stripping.
const fullItem = {
  updatedAt: T2,
  createdAt: T1,
  archivedAt: T1,
  migratedFromClothingRecordId: U2,
  replace: true,
  isDefault: true,
  appropriateness: 'formal',
  componentItemIds: [U2, U3],
  types: ['top', 'bottom'],
  imagePrompt: 'a burnished coat',
  description: 'A long coat.',
  title: 'Greatcoat',
  characterId: U3,
  id: U1,
  extraJunk: 99,
};

// Minimal item — only the required fields; defaults materialize, optionals omit.
const minItem = { id: U1, title: 'Hat', types: ['accessories'], createdAt: T1, updatedAt: T2 };

// Item with the nullable-optionals present-as-null.
const nullItem = {
  id: U1,
  characterId: null,
  title: 'Scarf',
  description: null,
  imagePrompt: null,
  types: ['accessories'],
  appropriateness: null,
  migratedFromClothingRecordId: null,
  archivedAt: null,
  createdAt: T1,
  updatedAt: T2,
};

const clone = (o: object) => JSON.parse(JSON.stringify(o));

// ── valid shapes ─────────────────────────────────────────────────────────────
wCase('w-full', J({ items: [fullItem] }));
wCase('w-min', J({ items: [minItem] }));
wCase('w-nulls', J({ items: [nullItem] }));
wCase('w-multi', J({ items: [minItem, nullItem] }));
wCase('w-empty-items', J({ items: [] }));
wCase('w-presets-stripped', J({ items: [minItem], presets: [1, 2, 3] }));
wCase('w-outfit-valid', J({ items: [minItem], outfit: { top: U1, bottom: null, extra: 9 } }));
wCase('w-archived-value', J({ items: [{ ...clone(minItem), archivedAt: T1 }] }));
wCase('w-leap-day-valid', J({ items: [{ ...clone(minItem), createdAt: '2024-02-29T12:00:00Z' }] }));
wCase('w-ts-no-ms', J({ items: [{ ...clone(minItem), createdAt: '2024-06-15T08:30:45Z' }] }));
wCase('w-ts-micros', J({ items: [{ ...clone(minItem), updatedAt: '2024-06-15T08:30:45.123456Z' }] }));

// ── root-level violations ────────────────────────────────────────────────────
wCase('w-invalid-json', '{ not valid');
wCase('w-not-object', J([1, 2]));
wCase('w-missing-items', J({ outfit: {} }));
wCase('w-items-not-array', J({ items: 'nope' }));
wCase('w-item-not-object', J({ items: ['x'] }));

// ── per-item field violations ────────────────────────────────────────────────
wCase('w-bad-id', J({ items: [{ ...clone(minItem), id: 'not-a-uuid' }] }));
wCase('w-missing-id', J({ items: [{ title: 'X', types: ['top'], createdAt: T1, updatedAt: T2 }] }));
wCase('w-empty-title', J({ items: [{ ...clone(minItem), title: '' }] }));
wCase('w-missing-title', J({ items: [{ id: U1, types: ['top'], createdAt: T1, updatedAt: T2 }] }));
wCase('w-types-empty', J({ items: [{ ...clone(minItem), types: [] }] }));
wCase('w-types-bad-enum', J({ items: [{ ...clone(minItem), types: ['hat'] }] }));
wCase('w-types-nonstring', J({ items: [{ ...clone(minItem), types: [5] }] }));
wCase('w-cii-bad-uuid', J({ items: [{ ...clone(minItem), componentItemIds: ['nope'] }] }));
wCase('w-cii-nonarray', J({ items: [{ ...clone(minItem), componentItemIds: 'x' }] }));
wCase('w-cii-null', J({ items: [{ ...clone(minItem), componentItemIds: null }] }));
wCase('w-isdefault-nonbool', J({ items: [{ ...clone(minItem), isDefault: 'yes' }] }));
wCase('w-isdefault-null', J({ items: [{ ...clone(minItem), isDefault: null }] }));
wCase('w-charid-bad-uuid', J({ items: [{ ...clone(minItem), characterId: 'abc' }] }));
wCase('w-description-nonstring', J({ items: [{ ...clone(minItem), description: 5 }] }));
wCase('w-bad-created', J({ items: [{ ...clone(minItem), createdAt: '2024-13-01T00:00:00Z' }] }));
wCase('w-missing-created', J({ items: [{ id: U1, title: 'X', types: ['top'], updatedAt: T2 }] }));
wCase('w-leap-day-invalid', J({ items: [{ ...clone(minItem), createdAt: '2023-02-29T00:00:00Z' }] }));
wCase('w-ts-offset-zone', J({ items: [{ ...clone(minItem), createdAt: '2024-01-01T00:00:00+00:00' }] }));
wCase('w-ts-no-zone', J({ items: [{ ...clone(minItem), createdAt: '2024-01-01T00:00:00' }] }));
wCase('w-ts-trailing-newline', J({ items: [{ ...clone(minItem), createdAt: T1 + '\n' }] }));
wCase('w-archived-bad', J({ items: [{ ...clone(minItem), archivedAt: 'whenever' }] }));

// ── outfit violations ────────────────────────────────────────────────────────
wCase('w-outfit-bad-field', J({ items: [], outfit: { top: 5 } }));
wCase('w-outfit-null', J({ items: [], outfit: null }));

for (const row of rows) {
  process.stdout.write(JSON.stringify(row) + '\n');
}
