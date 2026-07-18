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
