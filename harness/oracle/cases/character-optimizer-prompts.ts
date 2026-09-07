/**
 * Oracle case: the character optimizer's PURE arms (P4.9K1, unit 1).
 *
 * Drives v4's REAL exports from `lib/services/character-optimizer.service.ts` —
 * no reimplementation:
 *   coerceSuggestionArray                    (bug 119's fix, v4 `15573c3a1`)
 *   buildCharacterContext / buildMemoryContext
 *   getAnalysisPrompt
 *   getGeneralFieldsSuggestionsPrompt / getScenarioSuggestionPrompt /
 *   getSystemPromptSuggestionPrompt / getPhysicalDescriptionSuggestionPrompt /
 *   getWardrobeSuggestionPrompt / getPropertiesSuggestionPrompt /
 *   getNewSystemPromptsSuggestionPrompt
 * plus the module's exported constants, whose bytes reach a paid model verbatim.
 *
 * Every prompt builder embeds `JSON.stringify(analysis, null, 2)`, so the
 * analysis objects in the corpus are also a pretty-printer comparand.
 *
 * The corpus is the committed harness/oracle/fixtures/character-optimizer-
 * prompts.json; the final `coverage` row names every case id so the Rust side
 * can assert the two sets agree in BOTH directions.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/character-optimizer-prompts.ts \
 *     > /tmp/oracle-character-optimizer-prompts.ndjson
 */

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  coerceSuggestionArray,
  buildCharacterContext,
  buildMemoryContext,
  getAnalysisPrompt,
  getGeneralFieldsSuggestionsPrompt,
  getScenarioSuggestionPrompt,
  getSystemPromptSuggestionPrompt,
  getPhysicalDescriptionSuggestionPrompt,
  getWardrobeSuggestionPrompt,
  getPropertiesSuggestionPrompt,
  getNewSystemPromptsSuggestionPrompt,
} from '@/lib/services/character-optimizer.service';

interface Corpus {
  coerce: Array<{ id: string; value: unknown }>;
  contexts: Array<{ id: string; character: unknown; wardrobeItems?: unknown }>;
  memories: Array<{ id: string; entries: unknown }>;
  analyses: Array<{ id: string; analysis: unknown }>;
  scenarios: Array<{ id: string; scenario: unknown }>;
  systemPrompts: Array<{ id: string; prompt: unknown }>;
  physicals: Array<{ id: string; physical: unknown }>;
  wardrobes: Array<{ id: string; items: unknown }>;
}

const here = dirname(fileURLToPath(import.meta.url));
const corpus = JSON.parse(
  readFileSync(join(here, '..', 'fixtures', 'character-optimizer-prompts.json'), 'utf8'),
) as Corpus;

const out = (row: unknown): void => {
  process.stdout.write(JSON.stringify(row) + '\n');
};

// The analysis used for every per-item prompt row (the FIRST corpus analysis);
// the `analyses` list drives the general/new-prompts rows on its own.
const primaryAnalysis = corpus.analyses[0].analysis;

// ---- coerceSuggestionArray (bug 119) --------------------------------------
for (const c of corpus.coerce) {
  out({ kind: 'coerce', id: c.id, value: c.value, out: coerceSuggestionArray(c.value) });
}

// ---- buildCharacterContext -------------------------------------------------
for (const c of corpus.contexts) {
  out({
    kind: 'context',
    id: c.id,
    character: c.character,
    wardrobeItems: c.wardrobeItems ?? null,
    out: buildCharacterContext(c.character as never, c.wardrobeItems as never),
  });
}

// ---- buildMemoryContext ----------------------------------------------------
for (const c of corpus.memories) {
  out({ kind: 'memory', id: c.id, entries: c.entries, out: buildMemoryContext(c.entries as never) });
}

// ---- getAnalysisPrompt (no inputs) -----------------------------------------
out({ kind: 'analysis_prompt', id: 'analysis-prompt', out: getAnalysisPrompt() });

// ---- the analysis-only prompts, once per corpus analysis -------------------
for (const a of corpus.analyses) {
  out({
    kind: 'general_prompt',
    id: a.id,
    analysis: a.analysis,
    out: getGeneralFieldsSuggestionsPrompt(a.analysis as never),
  });
  out({
    kind: 'new_prompts_prompt',
    id: a.id,
    analysis: a.analysis,
    out: getNewSystemPromptsSuggestionPrompt(a.analysis as never),
  });
}

// ---- the per-item prompts --------------------------------------------------
for (const c of corpus.scenarios) {
  out({
    kind: 'scenario_prompt',
    id: c.id,
    analysis: primaryAnalysis,
    scenario: c.scenario,
    out: getScenarioSuggestionPrompt(primaryAnalysis as never, c.scenario as never),
  });
}
for (const c of corpus.systemPrompts) {
  out({
    kind: 'system_prompt_prompt',
    id: c.id,
    analysis: primaryAnalysis,
    prompt: c.prompt,
    out: getSystemPromptSuggestionPrompt(primaryAnalysis as never, c.prompt as never),
  });
}
for (const c of corpus.physicals) {
  out({
    kind: 'physical_prompt',
    id: c.id,
    analysis: primaryAnalysis,
    physical: c.physical,
    out: getPhysicalDescriptionSuggestionPrompt(primaryAnalysis as never, c.physical as never),
  });
}
for (const c of corpus.wardrobes) {
  out({
    kind: 'wardrobe_prompt',
    id: c.id,
    analysis: primaryAnalysis,
    items: c.items,
    out: getWardrobeSuggestionPrompt(primaryAnalysis as never, c.items as never),
  });
}
for (const c of corpus.contexts) {
  out({
    kind: 'properties_prompt',
    id: c.id,
    analysis: primaryAnalysis,
    character: c.character,
    out: getPropertiesSuggestionPrompt(primaryAnalysis as never, c.character as never),
  });
}

// ---- coverage --------------------------------------------------------------
out({
  kind: 'coverage',
  coerce: corpus.coerce.map((c) => c.id),
  contexts: corpus.contexts.map((c) => c.id),
  memories: corpus.memories.map((c) => c.id),
  analyses: corpus.analyses.map((c) => c.id),
  scenarios: corpus.scenarios.map((c) => c.id),
  systemPrompts: corpus.systemPrompts.map((c) => c.id),
  physicals: corpus.physicals.map((c) => c.id),
  wardrobes: corpus.wardrobes.map((c) => c.id),
});
