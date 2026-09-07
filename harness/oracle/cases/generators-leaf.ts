/**
 * Oracle case: the four character-generator leaf modules (P4.9K0).
 *
 * Drives v4's REAL exports — no reimplementation:
 *   stripCodeFences / escapeControlCharsInStrings / repairTruncatedJson /
 *   parseLLMJson / parseLLMJsonObject   (lib/llm/llm-json.ts)
 *   parseGeneratedProperties / describeGeneratedProperties
 *                                       (lib/characters/generated-properties.ts)
 *   sanitizePronouns                    (lib/characters/sanitize-pronouns.ts)
 * plus every export of lib/services/character-field-semantics.ts.
 *
 * The corpus is the committed harness/oracle/fixtures/generators-leaf.json; the
 * final `coverage` row names every corpus id so the Rust side can assert the
 * two sets agree in BOTH directions (a corpus case the oracle silently skipped
 * would otherwise be invisible).
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/generators-leaf.ts \
 *     > /tmp/oracle-generators-leaf.ndjson
 */

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  stripCodeFences,
  escapeControlCharsInStrings,
  repairTruncatedJson,
  parseLLMJson,
  parseLLMJsonObject,
} from '@/lib/llm/llm-json';
import {
  parseGeneratedProperties,
  describeGeneratedProperties,
} from '@/lib/characters/generated-properties';
import { sanitizePronouns } from '@/lib/characters/sanitize-pronouns';
import {
  FIELD_SEMANTICS_PREAMBLE,
  PROMPT_SEMANTICS,
  PROPERTIES_SEMANTICS,
  PHYSICAL_DESCRIPTION_SEMANTICS,
  WARDROBE_SEMANTICS,
  FULL_FIELD_SEMANTICS,
} from '@/lib/services/character-field-semantics';

interface TextCase {
  id: string;
  text: string;
}
interface ValueCase {
  id: string;
  text: unknown;
}
interface Corpus {
  missingPronounsLabel: string;
  llmJson: TextCase[];
  pronouns: ValueCase[];
  properties: TextCase[];
}

const here = dirname(fileURLToPath(import.meta.url));
const corpus = JSON.parse(
  readFileSync(join(here, '..', 'fixtures', 'generators-leaf.json'), 'utf8'),
) as Corpus;

const out = (row: unknown): void => {
  process.stdout.write(JSON.stringify(row) + '\n');
};

/** `{ ok: true, value }` or `{ ok: false }` — v4 throws where v5 returns Err. */
const attempt = (fn: () => unknown): { ok: boolean; value?: unknown } => {
  try {
    return { ok: true, value: fn() };
  } catch {
    return { ok: false };
  }
};

// ---- lib/llm/llm-json.ts ---------------------------------------------------
for (const c of corpus.llmJson) {
  out({
    kind: 'llm_json',
    id: c.id,
    text: c.text,
    stripCodeFences: stripCodeFences(c.text),
    escapeControlCharsInStrings: escapeControlCharsInStrings(c.text),
    repairTruncatedJson: repairTruncatedJson(c.text),
    parseLLMJson: attempt(() => parseLLMJson(c.text)),
    parseLLMJsonObject: attempt(() => parseLLMJsonObject(c.text)),
  });
}

// ---- lib/characters/sanitize-pronouns.ts -----------------------------------
for (const c of corpus.pronouns) {
  out({
    kind: 'pronouns',
    id: c.id,
    raw: c.text,
    // `undefined` would vanish from JSON.stringify, so the absent answer is
    // spelled null on the wire.
    out: sanitizePronouns(c.text) ?? null,
  });
}

// ---- lib/characters/generated-properties.ts --------------------------------
for (const c of corpus.properties) {
  const parsed = attempt(() => parseGeneratedProperties(c.text));
  out({
    kind: 'properties',
    id: c.id,
    text: c.text,
    ok: parsed.ok,
    props: parsed.ok
      ? {
          pronouns: (parsed.value as { pronouns: unknown }).pronouns,
          aliases: (parsed.value as { aliases: string[] }).aliases,
        }
      : null,
    describe: parsed.ok
      ? describeGeneratedProperties(
          parsed.value as never,
          corpus.missingPronounsLabel,
        )
      : null,
  });
}

// ---- lib/services/character-field-semantics.ts -----------------------------
const semantics: Array<[string, string]> = [
  ['FIELD_SEMANTICS_PREAMBLE', FIELD_SEMANTICS_PREAMBLE],
  ['PROMPT_SEMANTICS', PROMPT_SEMANTICS],
  ['PROPERTIES_SEMANTICS', PROPERTIES_SEMANTICS],
  ['PHYSICAL_DESCRIPTION_SEMANTICS', PHYSICAL_DESCRIPTION_SEMANTICS],
  ['WARDROBE_SEMANTICS', WARDROBE_SEMANTICS],
  ['FULL_FIELD_SEMANTICS', FULL_FIELD_SEMANTICS],
];
for (const [name, value] of semantics) {
  out({ kind: 'field_semantics', id: name, value });
}

// ---- coverage --------------------------------------------------------------
out({
  kind: 'coverage',
  llmJson: corpus.llmJson.map((c) => c.id),
  pronouns: corpus.pronouns.map((c) => c.id),
  properties: corpus.properties.map((c) => c.id),
  fieldSemantics: semantics.map(([name]) => name),
  missingPronounsLabel: corpus.missingPronounsLabel,
});
