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
import { mockInventoryContext, type MockInventoryRow } from '@/lib/pascal/bench-inventory';

interface Case {
  id: string;
  definition: unknown;
  params?: Record<string, unknown> | null;
  runs: number;
  metadata?: Record<string, unknown>;
  /** P4.d10: mock merged state for `$state` refs, held fixed across every draw. */
  state?: Record<string, unknown>;
  /**
   * The 616930db fixed consult, held constant across every draw. `simulateOutcomes`
   * NEVER invokes — the audit must not spend ten thousand LLM calls — so this is
   * a plain subject, not an invoker.
   */
  llm?: { ok: boolean; output: string };
  /** P4.D329: the bench's mock bag rows (`null` = the bag is unreadable). */
  inventory?: MockInventoryRow[] | null;
}

// === P4.D329 (v4 `e1872c94f` + `a53024d3c`): the audit under a mock bag ====
const BAG: MockInventoryRow[] = [
  { ref: 'sword', state: 'equipped', metadata: { bonus: 3, damage: '2d6' } },
  { ref: 'ring', state: 'equipped', count: 2, metadata: { power: 4 } },
  { ref: 'lantern', state: 'carried', metadata: { lumens: 40 } },
  { ref: 'rope', state: 'reachable' },
];
const INVENTORY_TABLE = {
  name: 'bag',
  description: 'b',
  roll: { min: 5, max: 5 },
  outcomes: [
    { when: { inventory: { sword: { eq: 'equipped' } }, inventoryCount: { ring: { gte: 2 } } }, message: 'armed', state: 'success' },
    { when: { inventoryCount: { axe: { lt: 1 } } }, message: 'no axe', state: 'info' },
    { when: true, message: 'fallback', state: 'failure' },
  ],
};
const AGGREGATE_TABLE = {
  name: 'stock',
  description: 's',
  roll: { min: 1, max: 1 },
  outcomes: [
    { when: { inventoryAggregate: { 'equipped.count': { gte: 3 }, 'owned.metadata.power.max': { eq: 4 } } }, message: 'stocked', state: 'success' },
    { when: { inventoryAggregate: { 'reachable.count': { gte: 1 } } }, message: 'some', state: 'info' },
    { when: true, message: 'bare', state: 'failure' },
  ],
};
const inventoryCases: Case[] = [
  { id: 'inventory-mock-bag', definition: INVENTORY_TABLE, runs: 40, inventory: BAG },
  { id: 'inventory-null-bag', definition: INVENTORY_TABLE, runs: 40, inventory: null },
  { id: 'inventory-absent-bag', definition: INVENTORY_TABLE, runs: 40 },
  { id: 'inventory-sword-carried', definition: INVENTORY_TABLE, runs: 40, inventory: [{ ref: 'sword', state: 'carried' }, { ref: 'axe', state: 'equipped' }] },
  { id: 'aggregate-mock-bag', definition: AGGREGATE_TABLE, runs: 30, inventory: BAG },
  { id: 'aggregate-null-bag', definition: AGGREGATE_TABLE, runs: 30, inventory: null },
  { id: 'aggregate-empty-bag', definition: AGGREGATE_TABLE, runs: 30, inventory: [] },
  {
    id: 'inventory-operand-roll-bound',
    definition: {
      name: 'bonus',
      description: 'b',
      roll: { min: { $inventory: 'sword.metadata.bonus', fallback: 0 }, max: { $inventory: 'sword.metadata.bonus', fallback: 0 } },
      outcomes: [
        { when: { gte: { $inventory: 'ring.metadata.power', fallback: 9 } }, message: 'hi', state: 'success' },
        { when: true, message: 'lo', state: 'failure' },
      ],
    },
    runs: 20,
    inventory: BAG,
  },
  {
    id: 'inventory-param-default-from-bag',
    definition: {
      name: 'pdef',
      description: 'p',
      parameters: { b: { type: 'number', default: { $inventory: 'sword.metadata.bonus', fallback: 1 } } },
      roll: { min: 0, max: 0, offset: { $param: 'b' } },
      outcomes: [{ when: { eq: 3 }, message: 'three', state: 'success' }, { when: true, message: 'other', state: 'failure' }],
    },
    runs: 10,
    inventory: BAG,
  },
];
// === end P4.D329 ===

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
  // ---- $state threading (P4.d10, f48f34dc): the mock state steers hit rates,
  // held fixed across every draw.
  {
    id: 'state-gate-passes-every-draw',
    definition: {
      name: 'gate',
      description: 'g',
      roll: { min: 5, max: 5 },
      outcomes: [
        { when: { gte: { $state: 'game.difficulty', fallback: 10 } }, message: 'passed', state: 'success' },
        { when: true, message: 'fallback', state: 'info' },
      ],
    },
    runs: 50,
    state: { game: { difficulty: 1 } },
  },
  {
    id: 'state-gate-falls-back-every-draw',
    definition: {
      name: 'gate',
      description: 'g',
      roll: { min: 5, max: 5 },
      outcomes: [
        { when: { gte: { $state: 'game.difficulty', fallback: 10 } }, message: 'passed', state: 'success' },
        { when: true, message: 'fallback', state: 'info' },
      ],
    },
    runs: 50,
  },
  // A $state roll bound under the audit (deterministic min == max).
  {
    id: 'state-roll-bound-in-audit',
    definition: {
      name: 'fixed',
      description: 'f',
      roll: { min: { $state: 'game.n', fallback: 2 }, max: { $state: 'game.n', fallback: 2 } },
      outcomes: [{ when: true, message: 'x', state: 'success' }],
    },
    runs: 25,
    state: { game: { n: 9 } },
  },
];

const rows: unknown[] = [];
for (const c of [...cases, ...inventoryCases]) {
  const definition = QtapCustomToolSchema.parse(c.definition);
  const bag = c.inventory === undefined ? undefined : c.inventory === null ? null : mockInventoryContext(c.inventory);
  const result = simulateOutcomes(definition, c.params ?? undefined, c.runs, c.metadata, c.llm, c.state ?? {}, {}, bag);
  rows.push({
    ...(c.inventory === undefined ? {} : { inventory: c.inventory }),
    id: c.id,
    definition: c.definition,
    params: c.params ?? null,
    runs: c.runs,
    metadata: c.metadata ?? null,
    llm: c.llm ?? null,
    state: c.state ?? null,
    result,
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
