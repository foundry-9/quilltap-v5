/**
 * Generator: the AI Wizard's prompt-constants module (P4.9K2).
 *
 * Reads the NDJSON dump produced by
 *   harness/oracle/cases/generators-wizard-prompts.ts
 * and writes the generated Rust data module
 *   crates/quilltap-core/src/generators/wizard_prompts.rs
 * holding one byte-exact raw-string constant per prompt plus the ordered
 * FIELD_PROMPTS table.
 *
 * Deliberately mechanical: the stored bytes ARE v4's, so the differential
 * proves the transcription rather than re-deriving the prose.
 *
 * Regen recipe (after a v4 wizard-prompt drift):
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/generators-wizard-prompts.ts \
 *     > /tmp/oracle-generators-wizard-prompts.ndjson
 *   node ~/source/quilltap-v5/harness/oracle/tools/gen-wizard-prompts.mjs \
 *     /tmp/oracle-generators-wizard-prompts.ndjson \
 *     ~/source/quilltap-v5/crates/quilltap-core/src/generators/wizard_prompts.rs
 *   (then re-run generators_wizard_prompts_equivalence to confirm)
 */

import { readFileSync, writeFileSync } from 'node:fs';

const [, , ndjsonPath, outPath] = process.argv;
if (!ndjsonPath || !outPath) {
  console.error('usage: gen-wizard-prompts.mjs <ndjson> <out.rs>');
  process.exit(1);
}

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

const fieldRows = rows.filter((r) => r.kind === 'field_prompt');
const constRows = rows.filter((r) => r.kind === 'constant');

const constBody = constRows
  .map((r) => `/// v4 \`${r.name}\` (byte-exact).\npub const ${r.name}: &str = ${rawString(r.value)};`)
  .join('\n\n');

const fieldConsts = fieldRows
  .map(
    (r) =>
      `/// v4 \`FIELD_PROMPTS.${r.name}\` (byte-exact).\npub const FIELD_PROMPT_${r.name.replace(/([A-Z])/g, '_$1').toUpperCase()}: &str = ${rawString(r.value)};`,
  )
  .join('\n\n');

const tableRows = fieldRows
  .map((r) => `    ("${r.name}", FIELD_PROMPT_${r.name.replace(/([A-Z])/g, '_$1').toUpperCase()}),`)
  .join('\n');

const out = `//! v4 \`lib/services/character-wizard.service.ts\` — the AI Wizard's prompt
//! constants.
//!
//! **GENERATED FILE — do not edit by hand.** Written by
//! \`harness/oracle/tools/gen-wizard-prompts.mjs\` from the NDJSON dump of v4's
//! real exports (\`harness/oracle/cases/generators-wizard-prompts.ts\`); the
//! regen recipe is in the generator's header. Several of these interpolate K0's
//! field-semantics exports, so the recorded bytes also keep that substrate
//! honest.
//!
//! ⚠ **Measured, and it corrects this round's order:** \`FIELD_PROMPTS\` has
//! **${fieldRows.length}** keys, not the 13 the work order states. Thirteen is the size of
//! \`WizardRequest.fieldsToGenerate\`'s union; the three extra members
//! (\`properties\`, \`physicalDescription\`, \`wardrobeItems\`) are served by
//! dedicated generators, not by this table.

${constBody}

${fieldConsts}

/// v4 \`FIELD_PROMPTS\`, in v4's own insertion order — the order is recorded
/// rather than sorted, and the differential's coverage row pins it.
pub const FIELD_PROMPTS: &[(&str, &str)] = &[
${tableRows}
];

/// Look up a field prompt by v4's key.
pub fn field_prompt(name: &str) -> Option<&'static str> {
    FIELD_PROMPTS
        .iter()
        .find(|(k, _)| *k == name)
        .map(|(_, v)| *v)
}
`;

writeFileSync(outPath, out);
console.error(`wrote ${outPath} (${fieldRows.length} field prompts + ${constRows.length} constants)`);
