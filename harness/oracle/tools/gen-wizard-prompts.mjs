/**
 * Generator: the AI Wizard's prompt-constants module (P4.9K2).
 *
 * Reads the NDJSON dump produced by
 *   harness/oracle/cases/generators-wizard-prompts.ts
 * and writes the generated Rust data module
 *   crates/quilltap-core/src/generators/wizard_prompts.rs
 * holding one byte-exact raw-string constant per prompt plus the ordered
 * FIELD_PROMPTS table, and (P4.D371, v4 3ae4cf582) one constant per
 * generator direction for the kind-routed system-prompt field, with a FIXED
 * template tail: `build_system_prompt_field_prompt(kind)` (selected through
 * `kind_policy`, v4's chokepoint) and `wizard_field_prompt(field, kind)`.
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
 *   cargo fmt -p quilltap-core
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
const systemPromptRows = rows.filter((r) => r.kind === 'system_prompt_field');
const directionConst = (d) => `SYSTEM_PROMPT_FIELD_PROMPT_${d.toUpperCase()}`;
for (const d of ['stakes', 'task']) {
  if (!systemPromptRows.some((r) => r.direction === d)) {
    console.error(`the NDJSON carries no system_prompt_field row for '${d}' (re-run the recorder)`);
    process.exit(1);
  }
}

const constBody = constRows
  .map((r) => `/// v4 \`${r.name}\` (byte-exact).\npub const ${r.name}: &str = ${rawString(r.value)};`)
  .join('\n\n');

const fieldConsts = fieldRows
  .map(
    (r) =>
      `/// v4 \`FIELD_PROMPTS.${r.name}\` (byte-exact).\npub const FIELD_PROMPT_${r.name.replace(/([A-Z])/g, '_$1').toUpperCase()}: &str = ${rawString(r.value)};`,
  )
  .join('\n\n');

const systemPromptConsts = systemPromptRows
  .map(
    (r) =>
      `/// v4 \`buildSystemPromptFieldPrompt('${r.characterKind}')\` — the \`${r.direction}\` direction (byte-exact).\npub const ${directionConst(r.direction)}: &str = ${rawString(r.value)};`,
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
//! ⚠ **Measured:** \`FIELD_PROMPTS\` has **${fieldRows.length}** keys at v4 \`3ae4cf582\` (the
//! system prompt moved to a kind-routed builder; it was 10 before). Thirteen is
//! the size of \`WizardRequest.fieldsToGenerate\`'s union; \`systemPrompt\` is
//! served by [\`build_system_prompt_field_prompt\`], and \`properties\`,
//! \`physicalDescription\`, \`wardrobeItems\` by dedicated generators.

use crate::characters::kind::{kind_policy, CharacterKind, GeneratorDirectionKey};

${constBody}

${fieldConsts}

${systemPromptConsts}

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

/// v4 \`buildSystemPromptFieldPrompt(kind)\` (\`3ae4cf582\`): the
/// default-system-prompt meta-prompt for a character of the given kind. The
/// kind-specific direction sits between the trust safeguards and the companion
/// gate; the two texts are recorded per direction key and chosen through
/// [\`kind_policy\`], so the kind becomes behaviour only there.
pub fn build_system_prompt_field_prompt(kind: CharacterKind) -> &'static str {
    match kind_policy(kind).generator_direction {
        GeneratorDirectionKey::Stakes => SYSTEM_PROMPT_FIELD_PROMPT_STAKES,
        GeneratorDirectionKey::Task => SYSTEM_PROMPT_FIELD_PROMPT_TASK,
    }
}

/// v4 \`wizardFieldPrompt(field, kind)\` (\`3ae4cf582\`): \`systemPrompt\` depends
/// on the character's kind; every other field is kind-free. An unknown field
/// is \`None\` (v4's \`undefined\`).
pub fn wizard_field_prompt(field: &str, kind: CharacterKind) -> Option<&'static str> {
    if field == "systemPrompt" {
        Some(build_system_prompt_field_prompt(kind))
    } else {
        field_prompt(field)
    }
}
`;

writeFileSync(outPath, out);
console.error(`wrote ${outPath} (${fieldRows.length} field prompts + ${constRows.length} constants)`);
