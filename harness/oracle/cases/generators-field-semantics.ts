/**
 * Recorder: the character field-semantics exports (P4.9K0).
 *
 * Prints every export of v4 `lib/services/character-field-semantics.ts` as one
 * NDJSON row `{ name, value }`, byte-for-byte, so the generated Rust constants
 * module carries v4's exact strings (the `byte-exact-static-data-transcription`
 * rule — ship the generator, never transcribe by hand). Two of the strings
 * interpolate `HAIR_PHYSICAL_BOUNDARY` / `HAIR_PHYSICAL_DESCRIPTION_NOTE` from
 * `lib/wardrobe/slot-guidance`, so recording the RESOLVED string is what proves
 * v5's own copies of those two constants are still byte-equal.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/generators-field-semantics.ts \
 *     > /tmp/oracle-generators-field-semantics.ndjson
 */

import {
  FIELD_SEMANTICS_PREAMBLE,
  PROMPT_SEMANTICS,
  CONVERSATIONAL_VOICE_DIRECTION,
  TRUST_SAFEGUARDS_DIRECTION,
  COMPANION_TRUST_DISPOSITION,
  COMPANION_TRUST_DISPOSITION_GATE,
  GATED_COMPANION_TRUST_DISPOSITION,
  COMMITTEE_DRIFT_GUARDRAIL,
  EXAMPLE_DIALOGUE_COVERAGE,
  PROPERTIES_SEMANTICS,
  PHYSICAL_DESCRIPTION_SEMANTICS,
  WARDROBE_SEMANTICS,
  FULL_FIELD_SEMANTICS,
} from '@/lib/services/character-field-semantics';

const exports_: Array<[string, string]> = [
  ['FIELD_SEMANTICS_PREAMBLE', FIELD_SEMANTICS_PREAMBLE],
  ['PROMPT_SEMANTICS', PROMPT_SEMANTICS],
  // P4.D237 (v4 `c3eefa752`): the two voice-direction exports, in v4's source
  // order (declared between PROMPT_SEMANTICS and PROPERTIES_SEMANTICS).
  ['CONVERSATIONAL_VOICE_DIRECTION', CONVERSATIONAL_VOICE_DIRECTION],
  // P4.D241 (v4 `ca363178d`): the five trust-safeguard exports, in v4's
  // source order (declared between CONVERSATIONAL_VOICE_DIRECTION and
  // EXAMPLE_DIALOGUE_COVERAGE).
  ['TRUST_SAFEGUARDS_DIRECTION', TRUST_SAFEGUARDS_DIRECTION],
  ['COMPANION_TRUST_DISPOSITION', COMPANION_TRUST_DISPOSITION],
  ['COMPANION_TRUST_DISPOSITION_GATE', COMPANION_TRUST_DISPOSITION_GATE],
  ['GATED_COMPANION_TRUST_DISPOSITION', GATED_COMPANION_TRUST_DISPOSITION],
  ['COMMITTEE_DRIFT_GUARDRAIL', COMMITTEE_DRIFT_GUARDRAIL],
  ['EXAMPLE_DIALOGUE_COVERAGE', EXAMPLE_DIALOGUE_COVERAGE],
  ['PROPERTIES_SEMANTICS', PROPERTIES_SEMANTICS],
  ['PHYSICAL_DESCRIPTION_SEMANTICS', PHYSICAL_DESCRIPTION_SEMANTICS],
  ['WARDROBE_SEMANTICS', WARDROBE_SEMANTICS],
  ['FULL_FIELD_SEMANTICS', FULL_FIELD_SEMANTICS],
];

for (const [name, value] of exports_) {
  process.stdout.write(JSON.stringify({ name, value }) + '\n');
}
