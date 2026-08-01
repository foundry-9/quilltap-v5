/**
 * Oracle case: the `run_custom` tool description builder (P4.6ay unit 4, the
 * pure half). Drives v4's REAL `buildRunCustomDescription`
 * (lib/tools/run-custom-tool.ts) over a corpus of definition rosters, so v5's
 * `tools::run_custom::build_run_custom_description` is diffed byte-for-byte —
 * the roster listing the model actually reads (roll spec, parameter bounds,
 * outcome tests, the `revealOdds: false` lock).
 *
 * Each row carries the RAW definition JSON(s) forming the roster; the Rust side
 * parses the same source through `safe_parse` and rebuilds the description. The
 * empty-roster form is pinned separately by the in-crate §2 byte-identity test
 * (`empty_roster_description_matches_data_rs`).
 *
 * Run (v4 @ d68638b4, Node 24):
 *   cd ~/source/quilltap-server
 *   npx tsx <V5W>/harness/oracle/cases/pascal-run-custom.ts > /tmp/oracle-pascal-run-custom.ndjson
 */

import { QtapCustomToolSchema } from '@/lib/pascal/custom-tool.types';
import { buildRunCustomDescription } from '@/lib/tools/run-custom-tool';
import type { DiscoveredCustomTool } from '@/lib/pascal/custom-tools';

/** A definition's raw JSON, wrapped as the minimal DiscoveredCustomTool the
 * builder reads (only `.definition` is consulted). */
function discovered(raw: unknown): DiscoveredCustomTool {
  const definition = QtapCustomToolSchema.parse(raw);
  return {
    definition,
    tier: 'character',
    mountPointId: 'm',
    mountName: 'M',
    definitionPath: 'Tools/x.tool.json',
  } as DiscoveredCustomTool;
}

const minimal = {
  name: 'coin',
  description: 'flip a coin',
  outcomes: [{ when: true, message: 'heads', state: 'success' }],
};

const paramsBounds = {
  name: 'attack',
  description: 'swing a blade',
  parameters: {
    bonus: { type: 'integer', default: 0, min: -5, max: 5, description: 'attack bonus' },
    blessed: { type: 'boolean', default: false },
  },
  roll: { min: 1, max: 20 },
  outcomes: [
    { when: { gte: 15 }, message: 'a hit', state: 'success' },
    { when: true, message: 'a miss', state: 'failure' },
  ],
};

const revealOddsFalse = {
  name: 'secret',
  description: 'a mystery',
  revealOdds: false,
  parameters: { key: { type: 'string', default: 'brass' } },
  roll: { min: 0, max: 1 },
  outcomes: [
    { when: { gte: 0.5 }, message: 'opens', state: 'success' },
    { when: true, message: 'stuck', state: 'failure' },
  ],
};

const rollTransform = {
  name: 'scaled',
  description: 'a scaled roll',
  parameters: { mult: { type: 'number', default: 2 } },
  roll: { min: 0, max: 1, multiplier: { $param: 'mult' }, offset: 5, round: true },
  outcomes: [
    { when: { gte: 5 }, message: 'x', state: 'info' },
    { when: true, message: 'y', state: 'info' },
  ],
};

const dice = {
  name: 'd20',
  description: 'roll a d20',
  roll: '1d20+3',
  outcomes: [
    { when: { gte: 18 }, message: 'crit', state: 'success' },
    { when: true, message: 'ok', state: 'partial' },
  ],
};

const whenSubjects = {
  name: 'complex',
  description: 'many tests',
  parameters: {
    difficulty: { type: 'number', default: 10 },
    color: { type: 'string', default: 'brass' },
    lucky: { type: 'boolean', default: false },
  },
  roll: { min: 0, max: 1 },
  outcomes: [
    {
      when: {
        gte: { $param: 'difficulty' },
        roll: { lt: 0.1 },
        params: { color: { eq: 'brass' }, lucky: { eq: true } },
        metadata: { luck: { gte: 5 } },
      },
      message: 'win',
      state: 'success',
    },
    { when: true, message: 'lose', state: 'failure' },
  ],
};

const stringQuoting = {
  name: 'pw',
  description: 'a password gate',
  parameters: { guess: { type: 'string', default: '' } },
  roll: { min: 0, max: 1 },
  outcomes: [
    { when: { params: { guess: { eq: 'say "friend"' } } }, message: 'open', state: 'success' },
    { when: true, message: 'no', state: 'failure' },
  ],
};

const bandAndNeq = {
  name: 'band',
  description: 'a banded roll',
  roll: { min: 0, max: 1 },
  outcomes: [
    { when: { gte: 0.3, lte: 0.6 }, message: 'mid', state: 'partial' },
    { when: { neq: 0 }, message: 'nonzero', state: 'info' },
    { when: true, message: 'zero', state: 'failure' },
  ],
};

// ---- the 616930db consult block + containment comparators -----------------
const llmTool = {
  name: 'oracle',
  description: 'ask the oracle',
  parameters: { question: { type: 'string', default: 'is it safe?' } },
  roll: { min: 1, max: 20 },
  llm: {
    prompt: 'Answer YES or NO: {{params.question}}',
    errorMessage: 'The wire went dead.',
    maxOutput: 40,
  },
  outcomes: [
    // Every describeLlmComparator arm at once: ok, an ordering key, eq, and
    // both containment keys — including a $param needle.
    { when: { llm: { ok: true, eq: 'YES' } }, message: 'yes', state: 'success' },
    { when: { llm: { ok: false } }, message: 'silence', state: 'failure' },
    { when: { llm: { gte: 5 } }, message: 'numeric', state: 'info' },
    { when: { llm: { contains: 'west door', ncontains: 'east' } }, message: 'door', state: 'info' },
    { when: { llm: { contains: { $param: 'question' } } }, message: 'echo', state: 'info' },
    { when: true, message: 'otherwise', state: 'info' },
  ],
};

// An llm tool with revealOdds:false: the "Consults a separate model" line must
// NOT appear, because the whole odds block is withheld.
const llmSecret = {
  name: 'sealed',
  description: 'a sealed oracle',
  revealOdds: false,
  llm: { prompt: 'Speak.', errorMessage: 'nope' },
  outcomes: [
    { when: { llm: { ok: true } }, message: 'a', state: 'success' },
    { when: true, message: 'b', state: 'failure' },
  ],
};

// Containment on params and metadata, away from the llm subject.
const containment = {
  name: 'manifest',
  description: 'search the manifest',
  parameters: {
    cargo: { type: 'string', default: 'silk' },
    sought: { type: 'string', default: 'opium' },
  },
  roll: { min: 0, max: 1 },
  outcomes: [
    {
      when: {
        params: { cargo: { contains: { $param: 'sought' }, ncontains: 'tea' } },
        metadata: { manifest: { contains: 'contraband' } },
      },
      message: 'seized',
      state: 'failure',
    },
    { when: true, message: 'cleared', state: 'success' },
  ],
};

// The c4d4b0de "Side effects: writes …" line — targets only, deduplicated, in
// DECLARATION order. Never values, never conditions.
const effectsTool = {
  name: 'lockpick',
  description: 'work the tumblers',
  effects: [
    { target: 'state.encounter.count', value: 1 },
    { when: { outcome: { eq: 'success' } }, target: 'metadata.lockpick', value: "'broken pick'" },
    { target: 'state.encounter.count', value: 2 },
    { target: 'state.alpha', value: 3 },
  ],
  outcomes: [
    { when: { gte: 0.5 }, message: 'open', state: 'success' },
    { when: true, message: 'stuck', state: 'failure' },
  ],
};

// The same tool with the odds withheld: the Side-effects line goes with them.
const effectsSecret = {
  name: 'sealed_lock',
  description: 'a sealed lock',
  revealOdds: false,
  effects: [{ target: 'state.encounter.count', value: 1 }],
  outcomes: [{ when: true, message: 'x', state: 'info' }],
};

// An EMPTY effects array declares none, so no line at all.
const effectsEmpty = {
  name: 'no_effects',
  description: 'declares an empty list',
  effects: [],
  outcomes: [{ when: true, message: 'x', state: 'info' }],
};

// Effects AND a consult: the two lines' order (consult first, then effects,
// then the outcome rows) is the description's contract.
const effectsAndLlm = {
  name: 'consulted_lock',
  description: 'ask, then write',
  llm: { prompt: 'Speak.', errorMessage: 'nope' },
  effects: [{ target: 'metadata.verdict', value: '{{llm}}' }],
  outcomes: [
    { when: { llm: { ok: true } }, message: 'a', state: 'success' },
    { when: true, message: 'b', state: 'failure' },
  ],
};

const cases: Array<[string, unknown[]]> = [
  ['minimal', [minimal]],
  ['params-bounds', [paramsBounds]],
  ['reveal-odds-false', [revealOddsFalse]],
  ['roll-transform', [rollTransform]],
  ['dice', [dice]],
  ['when-subjects', [whenSubjects]],
  ['string-quoting', [stringQuoting]],
  ['band-and-neq', [bandAndNeq]],
  // A multi-tool roster exercises the per-tool `\n•` join.
  ['multi-tool', [minimal, dice, paramsBounds]],
  // The 616930db additions.
  ['llm-tool', [llmTool]],
  ['llm-reveal-odds-false', [llmSecret]],
  ['containment', [containment]],
  ['multi-tool-with-llm', [minimal, llmTool, containment]],
  // The c4d4b0de additions.
  ['effects-line', [effectsTool]],
  ['effects-reveal-odds-false', [effectsSecret]],
  ['effects-empty-array', [effectsEmpty]],
  ['effects-and-llm-line-order', [effectsAndLlm]],
  ['multi-tool-with-effects', [minimal, effectsTool, effectsAndLlm]],
];

const rows: unknown[] = [];
for (const [id, defs] of cases) {
  const roster = defs.map(discovered);
  rows.push({
    id,
    definitions: defs,
    description: buildRunCustomDescription(roster),
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
