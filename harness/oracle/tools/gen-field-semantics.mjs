/**
 * Generator: the character field-semantics constants module (P4.9K0).
 *
 * Reads the NDJSON dump produced by
 *   harness/oracle/cases/generators-field-semantics.ts
 * (each line `{ name, value }`, the byte-exact export from v4
 * `lib/services/character-field-semantics.ts`) and writes the generated Rust
 * data module
 *   crates/quilltap-core/src/generators/field_semantics.rs
 * holding one byte-exact raw-string constant per export, v4's
 * CORE_VAULT_FILE_SUMMARIES record (P4.D371 — recorded, never hand-carried),
 * and a FIXED template block for v4's two kind-routed functions
 * (`generatorDirectionFor` / `kindDriftSignalFor`, v4 3ae4cf582) plus the
 * summaries' lookup and its unit test. A second regen from the same NDJSON
 * followed by `cargo fmt -p quilltap-core` is a no-op (the idempotence rule).
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
 *   cargo fmt -p quilltap-core
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

/** Emit `s` as an ordinary Rust string literal (non-ASCII kept literal). */
function rustString(s) {
  let out = '"';
  for (const ch of s) {
    const c = ch.codePointAt(0);
    if (ch === '"') out += '\\"';
    else if (ch === '\\') out += '\\\\';
    else if (ch === '\n') out += '\\n';
    else if (ch === '\t') out += '\\t';
    else if (ch === '\r') out += '\\r';
    else if (c < 0x20 || c === 0x7f) out += `\\u{${c.toString(16)}}`;
    else out += ch;
  }
  return out + '"';
}

const allRows = readFileSync(ndjsonPath, 'utf8')
  .split('\n')
  .filter((l) => l.trim().length > 0)
  .map((l) => JSON.parse(l));
const rows = allRows.filter((r) => typeof r.value === 'string');
const summariesRow = allRows.find((r) => r.name === 'CORE_VAULT_FILE_SUMMARIES');
if (!summariesRow || !Array.isArray(summariesRow.entries)) {
  console.error('the NDJSON carries no CORE_VAULT_FILE_SUMMARIES row (re-run the recorder)');
  process.exit(1);
}
for (const required of [
  'STAKES_AND_REACTION_DIRECTION',
  'ASSISTANT_TASK_DIRECTION',
  'FLATTENING_DRIFT_SIGNAL',
]) {
  if (!rows.some((r) => r.name === required)) {
    console.error(`the NDJSON carries no ${required} row; the template block needs it`);
    process.exit(1);
  }
}

const DOC = {
  FIELD_SEMANTICS_PREAMBLE:
    'The four vantage points plus the manifesto — the preamble every generator opens with.',
  PROMPT_SEMANTICS:
    'The system-prompt bucket ("Prompt"): named, sometimes model-specific instruction documents.',
  CONVERSATIONAL_VOICE_DIRECTION:
    'How a generated system prompt must direct the character to LISTEN and TALK (v4 `c3eefa752`). NOT part of `FULL_FIELD_SEMANTICS`: it is interpolated only where a system prompt is written — the AI Wizard `systemPrompt` field, Summon From Lore\'s `system_prompts` step, the optimizer\'s new-prompts pass, and (since v4 `ca363178d`) the External Prompt meta prompt.',
  TRUST_SAFEGUARDS_DIRECTION:
    'The six universal trust safeguards (v4 `ca363178d`, the sixth from `ba6c8fe0f`) — narration is fact, the conversation outranks notes, no invented setting facts, consent only in plain words, disagreement without procedure, and never assert {{user}}\'s interior. NOT part of `FULL_FIELD_SEMANTICS`: interpolated by every generator that writes or rewrites a system prompt (the AI Wizard, Summon From Lore, the optimizer\'s refine and new-prompts passes, the External Prompt meta prompt).',
  COMPANION_TRUST_DISPOSITION:
    'The relationship-scaled trust disposition (v4 `ca363178d`), only for a character established as {{user}}\'s companion, partner, family, or crew. Since v4 `ba6c8fe0f` it carries the positive half of trust as six bullets against "the probation" (tallying {{user}}\'s slips, conditions on the character\'s own yes); v4\'s rule: never reintroduce a "right as readily as they slipped" balance line — models read it as a ledger. The optimizer uses it BARE under its own framing gates; the other generators take `GATED_COMPANION_TRUST_DISPOSITION`.',
  COMPANION_TRUST_DISPOSITION_GATE:
    'The gate sentence that precedes the companion trust disposition (v4 `ca363178d`): the model infers the relationship from the material, default omit.',
  GATED_COMPANION_TRUST_DISPOSITION:
    'The gate plus the disposition as ONE paragraph (gate, one newline, disposition — v4 `ca363178d`), for the AI Wizard, Summon From Lore and the External Prompt meta prompt.',
  COMMITTEE_DRIFT_GUARDRAIL:
    'The committee and the probation as drift signals (v4 `ca363178d`; the probation line from `ba6c8fe0f`) — bare in the optimizer\'s analysis pass, a bullet in its refine pass (only the FIRST line is bulleted there; the probation line sits unbulleted, as in v4).',
  STAKES_AND_REACTION_DIRECTION:
    'How a generated or refined system prompt directs a STORY character (`roleplay`) to react when the story turns (v4 `3ae4cf582`; the off-screen-{{user}} bullet from `7ff5fda89`) — chosen by [`generator_direction_for`], never pasted beside it by hand.',
  ASSISTANT_TASK_DIRECTION:
    'How a generated or refined system prompt directs an ASSISTANT character (`assistant`): a character whose role is doing work for {{user}}, who still keeps a voice (v4 `3ae4cf582`) — chosen by [`generator_direction_for`].',
  FLATTENING_DRIFT_SIGNAL:
    'Flattening as drift, the companion to `COMMITTEE_DRIFT_GUARDRAIL` for a roleplay character in the optimizer\'s analysis and refine passes (v4 `3ae4cf582`) — chosen by [`kind_drift_signal_for`].',
  EXAMPLE_DIALOGUE_COVERAGE:
    'What a generated set of example dialogues must show (v4 `c3eefa752`; the fourth bullet from `ba6c8fe0f`) — a joke caught, a casual line answered briefly, a serious turn, and a plain yes. Interpolated by the AI Wizard `exampleDialogues` field, Summon From Lore\'s `first_message` step, and the optimizer\'s general-fields pass.',
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

const summaries = summariesRow.entries
  .map(([k, v]) => `    (${rustString(k)}, ${rustString(v)}),`)
  .join('\n');

const template = `/// v4's private \`GENERATOR_DIRECTIONS\` table (v4 \`3ae4cf582\`): the text for
/// one direction key. Exhaustive — a new key fails to compile until it has
/// a direction here.
fn generator_direction_text(key: GeneratorDirectionKey) -> &'static str {
    match key {
        GeneratorDirectionKey::Stakes => STAKES_AND_REACTION_DIRECTION,
        GeneratorDirectionKey::Task => ASSISTANT_TASK_DIRECTION,
    }
}

/// v4 \`generatorDirectionFor\` (\`3ae4cf582\`): the kind-specific direction every
/// system-prompt generator includes beside the voice and trust directions.
/// The one place a generator learns what a character's kind asks of its
/// prompt — through [\`kind_policy\`], v4's chokepoint.
pub fn generator_direction_for(kind: CharacterKind) -> &'static str {
    generator_direction_text(kind_policy(kind).generator_direction)
}

/// v4 \`kindDriftSignalFor\` (\`3ae4cf582\`): the kind-specific drift signal for
/// the Optimizer, or \`None\` when the kind has none. Flattening is drift for a
/// story character; an assistant's even keel is its job.
pub fn kind_drift_signal_for(kind: CharacterKind) -> Option<&'static str> {
    match kind_policy(kind).generator_direction {
        GeneratorDirectionKey::Stakes => Some(FLATTENING_DRIFT_SIGNAL),
        GeneratorDirectionKey::Task => None,
    }
}

/// v4 \`CORE_VAULT_FILE_SUMMARIES\` (\`074e1e01a\`, byte-exact, recorded by the
/// generator since P4.D371): the short version of what belongs in each core
/// file of a character vault, addressed to the character who owns it (the
/// \`self_inventory\` tool's \`vault.coreFiles\` checklist). Keyed by the overlay's
/// own path list in its order — v4 types it
/// \`Record<(typeof SINGLE_FILE_OVERLAY_PATHS)[number], string>\` so a core file
/// added there fails to compile until it is described here; the unit test
/// below holds the same rule.
pub const CORE_VAULT_FILE_SUMMARIES: [(&str, &str); ${summariesRow.entries.length}] = [
${summaries}
];

/// The summary [\`CORE_VAULT_FILE_SUMMARIES\`] holds for one core path.
pub fn core_vault_file_summary(path: &str) -> &'static str {
    CORE_VAULT_FILE_SUMMARIES
        .iter()
        .find(|(p, _)| *p == path)
        .map(|(_, s)| *s)
        .unwrap_or_default()
}`;

const tests = `#[cfg(test)]
mod core_vault_file_summary_tests {
    /// v4's compile-time rule as a test: the nine keys are the overlay's nine
    /// single-file paths, element for element.
    #[test]
    fn the_summaries_are_keyed_by_the_overlay_paths_in_order() {
        let keys: Vec<&str> = super::CORE_VAULT_FILE_SUMMARIES
            .iter()
            .map(|(k, _)| *k)
            .collect();
        assert_eq!(
            keys,
            crate::db::vault_read_overlay::SINGLE_FILE_OVERLAY_PATHS.to_vec()
        );
    }
}`;

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
//! string is what keeps those two copies honest. The two kind-routed functions,
//! the core-vault lookup and its test are the generator's fixed template.
//!
//! Consumers (v4's own list): the character optimizer, the AI Wizard, Summon
//! From Lore, and the External Prompt generator.

use crate::characters::kind::{kind_policy, CharacterKind, GeneratorDirectionKey};

${body}

${template}

/// Every string export, name-keyed — the differential's coverage comparand (a
/// new v4 export that never reaches this table is a hole the family can see).
/// Not the summaries record, not the functions.
pub const ALL_EXPORTS: &[(&str, &str)] = &[
${names}
];

${tests}
`;

writeFileSync(outPath, out);
console.error(
  `wrote ${outPath} (${rows.length} exports + CORE_VAULT_FILE_SUMMARIES[${summariesRow.entries.length}])`,
);
