/**
 * Generator: the character field-semantics constants module (P4.9K0).
 *
 * Reads the NDJSON dump produced by
 *   harness/oracle/cases/generators-field-semantics.ts
 * (each line `{ name, value }`, the byte-exact export from v4
 * `lib/services/character-field-semantics.ts`) and writes the generated Rust
 * data module
 *   crates/quilltap-core/src/generators/field_semantics.rs
 * holding one byte-exact raw-string constant per export.
 *
 * Deliberately mechanical: the stored bytes ARE v4's, so the differential
 * proves the transcription rather than re-deriving the prose.
 *
 * Regen recipe (after a v4 field-semantics drift):
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/generators-field-semantics.ts \
 *     > /tmp/oracle-generators-field-semantics.ndjson
 *   node ~/source/quilltap-v5/harness/oracle/tools/gen-field-semantics.mjs \
 *     /tmp/oracle-generators-field-semantics.ndjson \
 *     ~/source/quilltap-v5/crates/quilltap-core/src/generators/field_semantics.rs
 *   (then re-run generators_leaf_equivalence to confirm)
 */

import { readFileSync, writeFileSync } from 'node:fs';

const [, , ndjsonPath, outPath] = process.argv;
if (!ndjsonPath || !outPath) {
  console.error('usage: gen-field-semantics.mjs <ndjson> <out.rs>');
  process.exit(1);
}

/** Emit `s` as a Rust raw string with the minimal hash level that is safe. */
function rawString(s) {
  let n = 1;
  while (s.includes('"' + '#'.repeat(n))) n += 1;
  const h = '#'.repeat(n);
  return `r${h}"${s}"${h}`;
}

const rows = readFileSync(ndjsonPath, 'utf8')
  .split('\n')
  .filter((l) => l.trim().length > 0)
  .map((l) => JSON.parse(l));

const DOC = {
  FIELD_SEMANTICS_PREAMBLE:
    'The four vantage points plus the manifesto — the preamble every generator opens with.',
  PROMPT_SEMANTICS:
    'The system-prompt bucket ("Prompt"): named, sometimes model-specific instruction documents.',
  PROPERTIES_SEMANTICS:
    'The properties bucket: pronouns + aliases as data, never prose. The freeform metadata fact sheet is user-authored only and deliberately NOT part of this bucket.',
  PHYSICAL_DESCRIPTION_SEMANTICS:
    'The physical-description bucket: the person with nothing removable. Interpolates v4 `HAIR_PHYSICAL_DESCRIPTION_NOTE`.',
  WARDROBE_SEMANTICS:
    'The wardrobe bucket: slot-typed clothing/accessory items and composite outfits. Interpolates v4 `HAIR_PHYSICAL_BOUNDARY`.',
  FULL_FIELD_SEMANTICS:
    'The complete bucket map — the vantage-point preamble plus every other bucket, for a system that needs the whole taxonomy at once (the optimizer analysis pass, Summon From Lore extraction).',
};

const body = rows
  .map((r) => {
    const doc = DOC[r.name] ?? 'A character field-semantics export.';
    return `/// v4 \`${r.name}\` (byte-exact).\n///\n/// ${doc}\npub const ${r.name}: &str = ${rawString(r.value)};`;
  })
  .join('\n\n');

const names = rows.map((r) => `    ("${r.name}", ${r.name}),`).join('\n');

const out = `//! v4 \`lib/services/character-field-semantics.ts\` — "Single source of truth
//! for the prose definitions of every character-data bucket that an AI
//! generation/editing system may read or write".
//!
//! **GENERATED FILE — do not edit by hand.** Written by
//! \`harness/oracle/tools/gen-field-semantics.mjs\` from the NDJSON dump of v4's
//! real exports (\`harness/oracle/cases/generators-field-semantics.ts\`); the
//! regen recipe is in the generator's header. The bytes ARE v4's, including the
//! two interpolations of \`lib/wardrobe/slot-guidance\`
//! (\`HAIR_PHYSICAL_BOUNDARY\` / \`HAIR_PHYSICAL_DESCRIPTION_NOTE\`), which v5
//! also carries independently in [\`crate::wardrobe\`] — recording the RESOLVED
//! string is what keeps those two copies honest.
//!
//! Consumers (v4's own list): the character optimizer, the AI Wizard, and
//! Summon From Lore.

${body}

/// Every export, name-keyed — the differential's coverage comparand (a new v4
/// export that never reaches this table is a hole the family can see).
pub const ALL_EXPORTS: &[(&str, &str)] = &[
${names}
];
`;

writeFileSync(outPath, out);
console.error(`wrote ${outPath} (${rows.length} exports)`);
