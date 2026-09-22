/**
 * P4.D208 pure tier-1 oracle: the scenario-seeded-summary predicate (v4 bug 158,
 * `da9c4f34f`). Drives v4's REAL `lib/chat/scenario-seeded-summary.ts` over a
 * fixed corpus of chat rows and emits, per row, what `isScenarioSeededSummary`
 * answered, what `stripScenarioSeededSummary` returned, and whether it returned
 * the SAME object (v4's identity contract: an untouched row is not copied).
 *
 * The corpus is v4's own nine test cases plus the ''/null/absent permutations
 * its test file does not enumerate — the absent-vs-null distinction matters
 * because v4's guard is `typeof scenario !== 'string'`, which collapses them,
 * and v5's `Value::as_str()` must collapse them the same way.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/scenario-seeded-summary.ts \
 *     > /tmp/oracle-scenario-seeded-summary.ndjson
 */

import {
  isScenarioSeededSummary,
  stripScenarioSeededSummary,
  type ScenarioSeededSummaryFields,
} from '@/lib/chat/scenario-seeded-summary';

const SCENARIO = "# Scenario: Amy's Pool\n\nAmy is in her pool, and Charlie walks up the path.";

/** A row carries the two columns plus whatever else the ingest path hands it. */
type Row = ScenarioSeededSummaryFields & Record<string, unknown>;

const cases: Array<[string, Row]> = [
  // ---- v4's own nine test cases -------------------------------------------
  ['seeded-byte-identical', { contextSummary: SCENARIO, scenarioText: SCENARIO }],
  [
    'real-summary-quoting-the-scenario',
    { contextSummary: `${SCENARIO}\n\nThen they argued about the spyglass.`, scenarioText: SCENARIO },
  ],
  ['no-scenario-null', { contextSummary: SCENARIO, scenarioText: null }],
  ['no-scenario-absent', { contextSummary: SCENARIO }],
  ['no-summary-null', { contextSummary: null, scenarioText: SCENARIO }],
  ['no-summary-absent', { scenarioText: SCENARIO }],
  ['both-empty-strings', { contextSummary: '', scenarioText: '' }],
  ['whitespace-difference', { contextSummary: `${SCENARIO}\n`, scenarioText: SCENARIO }],
  [
    'strip-keeps-the-other-columns',
    { title: 'Damp Curtains and Cold Water', contextSummary: SCENARIO, scenarioText: SCENARIO },
  ],
  ['strip-returns-a-real-summary-by-identity', { contextSummary: 'They argued about a wall.', scenarioText: SCENARIO }],

  // ---- the ''/null/absent permutations ------------------------------------
  ['both-null', { contextSummary: null, scenarioText: null }],
  ['both-absent', {}],
  ['summary-empty-scenario-set', { contextSummary: '', scenarioText: SCENARIO }],
  ['summary-set-scenario-empty', { contextSummary: SCENARIO, scenarioText: '' }],
  ['summary-null-scenario-empty', { contextSummary: null, scenarioText: '' }],
  ['summary-absent-scenario-empty', { scenarioText: '' }],
  ['summary-empty-scenario-null', { contextSummary: '', scenarioText: null }],
  ['summary-empty-scenario-absent', { contextSummary: '' }],

  // ---- shapes the ingest paths can actually carry --------------------------
  ['seeded-single-char', { contextSummary: 'x', scenarioText: 'x' }],
  ['seeded-whitespace-only-both', { contextSummary: '   ', scenarioText: '   ' }],
  ['seeded-leading-trailing-space', { contextSummary: ' padded ', scenarioText: ' padded ' }],
  ['seeded-unicode-astral', { contextSummary: 'a 𝒳 ☃ b', scenarioText: 'a 𝒳 ☃ b' }],
  ['seeded-crlf', { contextSummary: 'line one\r\nline two', scenarioText: 'line one\r\nline two' }],
  ['case-differs', { contextSummary: 'Amy is in her pool.', scenarioText: 'amy is in her pool.' }],
  [
    'seeded-with-a-full-row-of-siblings',
    {
      id: 'chat-1',
      title: 'A Pool, A Path',
      userId: 'user-1',
      messageCount: 0,
      contextSummary: SCENARIO,
      scenarioText: SCENARIO,
      tags: ['pool'],
    },
  ],

  // ---- the `typeof` guard, reachable only off the type system --------------
  // v4's guard is `typeof scenario !== 'string'`, and its equality is strict —
  // so a numeric column never matches, in either position. v5 reads both through
  // `Value::as_str()`, which answers `None` for a non-string, and must agree.
  ['scenario-not-a-string', { contextSummary: '123', scenarioText: 123 as unknown as string }],
  ['summary-not-a-string', { contextSummary: 123 as unknown as string, scenarioText: '123' }],
];

for (const [id, row] of cases) {
  // A frozen snapshot of the input, taken BEFORE the call, so the "does not
  // mutate the row it was given" contract is checked on the v4 side too.
  const before = JSON.stringify(row);
  const is = isScenarioSeededSummary(row);
  const out = stripScenarioSeededSummary(row);
  process.stdout.write(
    JSON.stringify({
      id,
      row: JSON.parse(before),
      is,
      out,
      // v4 returns the SAME object reference when it strips nothing.
      same: out === row,
      // The input must be untouched either way.
      inputUnmutated: JSON.stringify(row) === before,
    }) + '\n'
  );
}
