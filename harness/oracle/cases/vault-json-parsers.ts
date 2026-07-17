/**
 * Tier-1 oracle case — the vault JSON projection parsers.
 *
 * Drives v4's REAL parseVaultProperties + parseVaultPhysicalPrompts
 * (lib/database/repositories/vault-overlay/parsers.ts) over a corpus of raw file
 * strings (valid + every interesting schema violation), emitting the parsed
 * object or null. The Rust port (quilltap_core::vault_overlay::{
 * parse_vault_properties, parse_vault_physical_prompts}) must match exactly —
 * including the safeParse "fall back to null on any violation" semantics and the
 * unknown-key stripping.
 *
 * Run from the v4 server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-json-parsers.ts \
 *     > /tmp/oracle-vault-json-parsers.ndjson
 */

import {
  parseVaultProperties,
  parseVaultPhysicalPrompts,
  parseVaultMetadata,
} from '@/lib/database/repositories/vault-overlay/parsers';

type Row =
  | { kind: 'properties'; id: string; raw: string; out: unknown }
  | { kind: 'physical'; id: string; raw: string; out: unknown }
  | { kind: 'metadata'; id: string; raw: string; out: unknown };

const rows: Row[] = [];
const CID = 'char-1';

// `raw` is the file content (a JSON string). Most cases stringify an object;
// the invalid-JSON case passes a deliberately broken string.
function propsCase(id: string, raw: string) {
  rows.push({ kind: 'properties', id, raw, out: parseVaultProperties(raw, CID) ?? null });
}
function physCase(id: string, raw: string) {
  rows.push({ kind: 'physical', id, raw, out: parseVaultPhysicalPrompts(raw, CID) ?? null });
}
// metadata: fail-soft null on invalid JSON / non-object; success passes the
// object verbatim (JsonSchema = z.record(string, unknown), unconstrained keys).
function metaCase(id: string, raw: string) {
  rows.push({ kind: 'metadata', id, raw, out: parseVaultMetadata(raw, CID, 'mp-1') ?? null });
}

const J = (v: unknown) => JSON.stringify(v);

// ── parseVaultProperties ───────────────────────────────────────────────────
propsCase(
  'p-full',
  J({
    pronouns: { subject: 'she', object: 'her', possessive: 'hers' },
    aliases: ['Al', 'Ali'],
    title: 'Hero',
    firstMessage: 'Hi there',
    talkativeness: 0.5,
  }),
);
propsCase(
  'p-nulls',
  J({ pronouns: null, aliases: [], title: null, firstMessage: null, talkativeness: 1.0 }),
);
propsCase(
  'p-extra-stripped',
  J({
    pronouns: null,
    aliases: ['X'],
    title: 'T',
    firstMessage: 'M',
    talkativeness: 0.1,
    extraKey: 123,
    another: { nested: true },
  }),
);
propsCase(
  'p-pronoun-extra-stripped',
  J({
    pronouns: { subject: 'they', object: 'them', possessive: 'theirs', extra: 'x' },
    aliases: [],
    title: null,
    firstMessage: null,
    talkativeness: 0.7,
  }),
);
propsCase('p-invalid-json', '{ not valid json');
propsCase('p-not-object', J(['a', 'b']));
propsCase(
  'p-missing-talkativeness',
  J({ pronouns: null, aliases: [], title: null, firstMessage: null }),
);
propsCase(
  'p-talk-low',
  J({ pronouns: null, aliases: [], title: null, firstMessage: null, talkativeness: 0.05 }),
);
propsCase(
  'p-talk-high',
  J({ pronouns: null, aliases: [], title: null, firstMessage: null, talkativeness: 1.5 }),
);
propsCase(
  'p-talk-boundary-low',
  J({ pronouns: null, aliases: [], title: null, firstMessage: null, talkativeness: 0.1 }),
);
propsCase(
  'p-aliases-nonarray',
  J({ pronouns: null, aliases: 'nope', title: null, firstMessage: null, talkativeness: 0.5 }),
);
propsCase(
  'p-aliases-nonstring',
  J({ pronouns: null, aliases: ['ok', 5], title: null, firstMessage: null, talkativeness: 0.5 }),
);
propsCase(
  'p-pronoun-missing-field',
  J({
    pronouns: { subject: 'she', object: 'her' },
    aliases: [],
    title: null,
    firstMessage: null,
    talkativeness: 0.5,
  }),
);
propsCase(
  'p-pronoun-too-long',
  J({
    pronouns: { subject: 'x'.repeat(21), object: 'her', possessive: 'hers' },
    aliases: [],
    title: null,
    firstMessage: null,
    talkativeness: 0.5,
  }),
);
propsCase(
  'p-pronoun-empty',
  J({
    pronouns: { subject: '', object: 'her', possessive: 'hers' },
    aliases: [],
    title: null,
    firstMessage: null,
    talkativeness: 0.5,
  }),
);
propsCase(
  'p-title-number',
  J({ pronouns: null, aliases: [], title: 42, firstMessage: null, talkativeness: 0.5 }),
);
propsCase(
  'p-missing-pronouns',
  J({ aliases: [], title: null, firstMessage: null, talkativeness: 0.5 }),
);

// ── parseVaultPhysicalPrompts ──────────────────────────────────────────────
physCase(
  'ph-full',
  J({ headAndShoulders: 'hs', short: 's', medium: 'm', long: 'l', complete: 'c' }),
);
physCase('ph-no-head', J({ short: 's', medium: 'm', long: 'l', complete: 'c' }));
physCase(
  'ph-tier-nulls',
  J({ headAndShoulders: 'hs', short: null, medium: null, long: null, complete: null }),
);
physCase('ph-invalid-json', 'nope');
physCase('ph-missing-tier', J({ short: 's', medium: 'm', long: 'l' }));
physCase(
  'ph-tier-wrongtype',
  J({ short: 5, medium: 'm', long: 'l', complete: 'c' }),
);
physCase(
  'ph-extra-stripped',
  J({ short: 's', medium: 'm', long: 'l', complete: 'c', legacyField: 'x' }),
);

// ── parseVaultMetadata ─────────────────────────────────────────────────────
// The fact sheet: any JSON object passes verbatim; anything else → null.
metaCase('m-empty', J({}));
metaCase(
  'm-mixed-values',
  J({ hasAnsibleAccess: true, clearanceLevel: 3, codename: 'Aria', notes: null }),
);
metaCase('m-nested', J({ profile: { role: 'lead', teams: ['red', 'blue'] }, active: true }));
metaCase('m-key-order', J({ zebra: 1, apple: 2, mango: 3 })); // insertion order preserved
metaCase('m-invalid-json', '{ not: valid');
metaCase('m-top-array', J([1, 2, 3])); // a top-level array is not a record → null
metaCase('m-bare-string', J('just a string')); // bare scalar → null
metaCase('m-bare-number', J(42)); // bare scalar → null
metaCase('m-bare-bool', J(true)); // bare scalar → null
metaCase('m-null', J(null)); // JSON null → null

for (const row of rows) {
  process.stdout.write(JSON.stringify(row) + '\n');
}
