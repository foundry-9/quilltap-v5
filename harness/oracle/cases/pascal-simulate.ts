/**
 * Oracle case: the Workbench outcome-table audit (P4.6ay unit 12). Drives v4's
 * REAL `simulateOutcomes` (lib/pascal/custom-tools.ts) so v5's
 * `pascal::custom_tools::simulate_outcomes` is diffed.
 *
 * v4's `simulateOutcomes` draws through the crypto directly (no seam), so a
 * cross-language draw-for-draw diff is impossible. The corpus therefore uses
 * DETERMINISTIC definitions (min===max ranges — no draw — single all-covering
 * rows, metadata-gated rows under an empty vs supplied sheet), so `runs`/`hits`/
 * `share`/min/max/mean are exact on both sides. Stochastic spread is covered by
 * v5-side statistical unit tests mirroring v4's own `custom-tools-simulate.test.ts`.
 *
 * Run (v4 @ d68638b4, Node 24):
 *   cd ~/source/quilltap-server
 *   npx tsx <V5W>/harness/oracle/cases/pascal-simulate.ts > /tmp/oracle-pascal-simulate.ndjson
 */

import { QtapCustomToolSchema } from '@/lib/pascal/custom-tool.types';
import { simulateOutcomes } from '@/lib/pascal/custom-tools';

interface Case {
  id: string;
  definition: unknown;
  params?: Record<string, unknown> | null;
  runs: number;
  metadata?: Record<string, unknown>;
  /**
   * The 616930db fixed consult, held constant across every draw. `simulateOutcomes`
   * NEVER invokes — the audit must not spend ten thousand LLM calls — so this is
   * a plain subject, not an invoker.
   */
  llm?: { ok: boolean; output: string };
}

const cases: Case[] = [
  // A fixed value, one catch-all → 100% on outcome 0.
  {
    id: 'certain',
    definition: {
      name: 'certain',
      description: 'always',
      roll: { min: 0.7, max: 0.7 },
      outcomes: [{ when: true, message: 'x', state: 'success' }],
    },
    runs: 1000,
  },
  // A fixed value across a two-row band → the first row always wins.
  {
    id: 'band-high',
    definition: {
      name: 'band',
      description: 'band',
      roll: { min: 0.7, max: 0.7 },
      outcomes: [
        { when: { gte: 0.6 }, message: 'hi', state: 'success' },
        { when: true, message: 'lo', state: 'failure' },
      ],
    },
    runs: 500,
  },
  // Metadata gate, EMPTY sheet → declines → catch-all (outcome 1) 100%.
  {
    id: 'metadata-empty',
    definition: {
      name: 'gate',
      description: 'gate',
      roll: { min: 0.7, max: 0.7 },
      outcomes: [
        { when: { metadata: { luck: { gte: 5 } } }, message: 'open', state: 'success' },
        { when: true, message: 'shut', state: 'failure' },
      ],
    },
    runs: 300,
    metadata: {},
  },
  // Same gate, sheet supplies the key → outcome 0 100%.
  {
    id: 'metadata-supplied',
    definition: {
      name: 'gate',
      description: 'gate',
      roll: { min: 0.7, max: 0.7 },
      outcomes: [
        { when: { metadata: { luck: { gte: 5 } } }, message: 'open', state: 'success' },
        { when: true, message: 'shut', state: 'failure' },
      ],
    },
    runs: 300,
    metadata: { luck: 7 },
  },
  // A param-driven fixed value (min===max via $param) with an offset+round.
  {
    id: 'param-scaled',
    definition: {
      name: 'scaled',
      description: 'scaled',
      parameters: { x: { type: 'integer', default: 3 } },
      roll: { min: { $param: 'x' }, max: { $param: 'x' }, offset: 5, round: true },
      outcomes: [
        { when: { gte: 8 }, message: 'hi', state: 'info' },
        { when: true, message: 'lo', state: 'info' },
      ],
    },
    runs: 200,
  },
  // ---- the 616930db fixed consult -----------------------------------------
  // An llm-gated row with a scripted ANSWER that satisfies it → outcome 0 100%.
  {
    id: 'llm-fixed-answer-hits',
    definition: {
      name: 'oracle',
      description: 'oracle',
      roll: { min: 0.7, max: 0.7 },
      llm: { prompt: 'Speak.', errorMessage: 'The wire went dead.' },
      outcomes: [
        { when: { llm: { eq: 'YES' } }, message: 'open', state: 'success' },
        { when: true, message: 'shut', state: 'failure' },
      ],
    },
    runs: 300,
    llm: { ok: true, output: 'yes.' },
  },
  // The SAME table with a scripted FAILURE → the llm row declines every draw.
  {
    id: 'llm-fixed-failure-declines',
    definition: {
      name: 'oracle',
      description: 'oracle',
      roll: { min: 0.7, max: 0.7 },
      llm: { prompt: 'Speak.', errorMessage: 'The wire went dead.' },
      outcomes: [
        { when: { llm: { eq: 'YES' } }, message: 'open', state: 'success' },
        { when: true, message: 'shut', state: 'failure' },
      ],
    },
    runs: 300,
    llm: { ok: false, output: 'The wire went dead.' },
  },
  // No consult supplied at all → the llm row fails SOFT on every draw, exactly
  // like a metadata key the character lacks.
  {
    id: 'llm-absent-declines',
    definition: {
      name: 'oracle',
      description: 'oracle',
      roll: { min: 0.7, max: 0.7 },
      llm: { prompt: 'Speak.', errorMessage: 'The wire went dead.' },
      outcomes: [
        { when: { llm: { ok: true } }, message: 'open', state: 'success' },
        { when: true, message: 'shut', state: 'failure' },
      ],
    },
    runs: 300,
  },
  // runs = 0 → all-zero, min/max/mean 0.
  {
    id: 'zero-runs',
    definition: {
      name: 'certain',
      description: 'always',
      roll: { min: 0.7, max: 0.7 },
      outcomes: [{ when: true, message: 'x', state: 'success' }],
    },
    runs: 0,
  },
];

const rows: unknown[] = [];
for (const c of cases) {
  const definition = QtapCustomToolSchema.parse(c.definition);
  const result = simulateOutcomes(definition, c.params ?? undefined, c.runs, c.metadata, c.llm);
  rows.push({
    id: c.id,
    definition: c.definition,
    params: c.params ?? null,
    runs: c.runs,
    metadata: c.metadata ?? null,
    llm: c.llm ?? null,
    result,
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
