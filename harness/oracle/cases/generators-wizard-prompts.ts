/**
 * Recorder: the AI Wizard's prompt constants (P4.9K2).
 *
 * Prints v4 `lib/services/character-wizard.service.ts`'s exported prompt
 * constants as NDJSON, byte-for-byte, so the generated Rust module carries v4's
 * exact strings (`byte-exact-static-data-transcription` — ship the generator).
 * Several `FIELD_PROMPTS` entries interpolate K0's field-semantics exports, so
 * recording the RESOLVED string is also a check that the substrate is intact.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/generators-wizard-prompts.ts \
 *     > /tmp/oracle-generators-wizard-prompts.ndjson
 */

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  FIELD_PROMPTS,
  PROPERTIES_PROMPT,
  HEAD_AND_SHOULDERS_PHYSICAL_PROMPT,
  buildContextPrompt,
  buildSystemPromptFieldPrompt,
  wizardFieldPrompt,
  wizardFieldMaxTokens,
} from '@/lib/services/character-wizard.service';

interface ContextCase {
  id: string;
  characterName: string;
  background: string;
  existingData?: unknown;
  imageDescription?: string;
  documentContent?: string;
}

const here = dirname(fileURLToPath(import.meta.url));
const corpus = JSON.parse(
  readFileSync(join(here, '..', 'fixtures', 'generators-wizard-prompts.json'), 'utf8'),
) as { contexts: ContextCase[] };

// FIELD_PROMPTS is a Record; its INSERTION ORDER is the NINE field names (ten
// before v4 3ae4cf582 moved `systemPrompt` to a kind-routed builder) in v4's
// source order and is recorded as-is (the Rust table keeps it).
for (const [name, value] of Object.entries(FIELD_PROMPTS)) {
  process.stdout.write(JSON.stringify({ kind: 'field_prompt', name, value }) + '\n');
}
process.stdout.write(
  JSON.stringify({ kind: 'constant', name: 'PROPERTIES_PROMPT', value: PROPERTIES_PROMPT }) + '\n',
);
process.stdout.write(
  JSON.stringify({
    kind: 'constant',
    name: 'HEAD_AND_SHOULDERS_PHYSICAL_PROMPT',
    value: HEAD_AND_SHOULDERS_PHYSICAL_PROMPT,
  }) + '\n',
);

// ---- P4.D371 (v4 3ae4cf582): the kind-routed system-prompt field ----------
// One row per generator direction key — the generator emits one constant per
// row and the Rust builder selects through kindPolicy, as v4 does.
const systemPromptKinds: Array<['stakes' | 'task', 'roleplay' | 'assistant']> = [
  ['stakes', 'roleplay'],
  ['task', 'assistant'],
];
for (const [direction, characterKind] of systemPromptKinds) {
  process.stdout.write(
    JSON.stringify({
      kind: 'system_prompt_field',
      direction,
      characterKind,
      value: buildSystemPromptFieldPrompt(characterKind),
    }) + '\n',
  );
}
// Every member of WizardRequest.fieldsToGenerate's 13-member union, plus one
// name v4 does not know.
const wizardFields = [
  'name',
  'title',
  'identity',
  'description',
  'manifesto',
  'personality',
  'scenarios',
  'exampleDialogues',
  'firstMessage',
  'systemPrompt',
  'properties',
  'physicalDescription',
  'wardrobeItems',
  'bogus',
];
for (const field of wizardFields) {
  for (const [, characterKind] of systemPromptKinds) {
    process.stdout.write(
      JSON.stringify({
        kind: 'wizard_field_prompt',
        field,
        characterKind,
        value: wizardFieldPrompt(field, characterKind) ?? null,
      }) + '\n',
    );
  }
}
for (const field of wizardFields) {
  process.stdout.write(
    JSON.stringify({ kind: 'wizard_field_max_tokens', field, value: wizardFieldMaxTokens(field) }) +
      '\n',
  );
}

// ---- buildContextPrompt over the committed corpus ---------------------------
for (const c of corpus.contexts) {
  process.stdout.write(
    JSON.stringify({
      kind: 'context_prompt',
      id: c.id,
      characterName: c.characterName,
      background: c.background,
      existingData: c.existingData ?? null,
      imageDescription: c.imageDescription ?? null,
      documentContent: c.documentContent ?? null,
      out: buildContextPrompt(
        c.characterName,
        c.background,
        c.existingData as never,
        c.imageDescription,
        c.documentContent,
      ),
    }) + '\n',
  );
}

process.stdout.write(
  JSON.stringify({
    kind: 'coverage',
    fieldPrompts: Object.keys(FIELD_PROMPTS),
    systemPromptFields: systemPromptKinds.map(([direction]) => direction),
    wizardFields,
    contexts: corpus.contexts.map((c) => c.id),
  }) + '\n',
);
