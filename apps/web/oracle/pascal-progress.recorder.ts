/**
 * The v4-side recorder behind `src/app/pascal/progress-arms.oracle.spec.ts`.
 *
 * `0587d1e96` gave Pascal's format a `progress` family — a `when.progress`
 * subject, a gate `progress` subject beside a now-OPTIONAL `metadata`, a third
 * `progress.<id>.<field>` effect target family, the reserved-key refusal on the
 * `metadata.` family, and `{{now}}` / `{{progress.…}}` placeholders. The SPA's
 * hand-ported schema twin has to phrase every one of those rejections exactly
 * as the server does: the Workbench renders `formatDefinitionIssues` verbatim
 * for a draft the author is repairing.
 *
 * The SHARED corpus (`harness/oracle/cases/pascal-custom-tool-definition.ts`,
 * owned by the Pascal lane) carries the rows that MOVED; these are the rows
 * that did not exist before, recorded here so this lane's hunks are pinned
 * without editing another lane's generator. When the Pascal lane extends the
 * shared corpus with its own progress rows, these become the redundant half and
 * can retire — the unifier's call.
 *
 * Run it from a pinned v4 worktree, exactly as
 * `progressions-schema.recorder.ts` documents:
 *
 * ```bash
 * PIN=/tmp/qt-v4-pin-p4d170-25f534c0b
 * cp <V5>/apps/web/oracle/pascal-progress.recorder.ts "$PIN/"
 * cd "$PIN" && npx tsx pascal-progress.recorder.ts \
 *   > <V5>/apps/web/src/testing/fixtures/pascal-progress.oracle.ndjson
 * ```
 *
 * Expect 71 lines.
 */

import {
  QtapCustomToolSchema,
  formatDefinitionIssues,
  parseEffectTarget,
} from '@/lib/pascal/custom-tool.types';
import { classifyPlaceholder } from '@/lib/pascal/placeholders';
import { evaluateToolGate } from '@/lib/pascal/tool-gate';
import { flattenProgressions } from '@/lib/progressions/engine';

/** A definition that parses, so a row only ever fails on the arm it means to test. */
function definition(extra: string): string {
  return `{"name":"probe","description":"a probe","roll":"1d20","outcomes":[{"when":true,"message":"m","state":"success"}]${extra}}`;
}

/** A two-row table whose FIRST row carries the `when` under test. */
function whenRow(whenJson: string): string {
  return `{"name":"probe","description":"a probe","roll":"1d20","outcomes":[{"when":${whenJson},"message":"m","state":"success"},{"when":true,"message":"c","state":"info"}]}`;
}

const DEFINITIONS: Array<[string, string]> = [
  // --- the gate's two subjects
  [
    'gate-progress-only',
    definition(`,"availableWhen":{"progress":{"cannon.complete":{"eq":true}}}`),
  ],
  [
    'gate-both-subjects',
    definition(
      `,"availableWhen":{"metadata":{"rank":{"eq":"captain"}},"progress":{"cannon.complete":{"eq":true}}}`,
    ),
  ],
  [
    'gate-metadata-absent-progress-present',
    definition(`,"withheldWhen":{"progress":{"cannon.percent":{"lt":100}}}`),
  ],
  ['gate-empty-object', definition(`,"availableWhen":{}`)],
  ['gate-both-empty', definition(`,"availableWhen":{"metadata":{},"progress":{}}`)],
  [
    'gate-empty-progress-nonempty-metadata',
    definition(`,"availableWhen":{"metadata":{"rank":{"eq":"x"}},"progress":{}}`),
  ],
  [
    'gate-empty-metadata-nonempty-progress',
    definition(`,"availableWhen":{"metadata":{},"progress":{"cannon.complete":{"eq":true}}}`),
  ],
  ['gate-progress-key-no-dot', definition(`,"availableWhen":{"progress":{"cannon":{"eq":true}}}`)],
  [
    'gate-progress-key-bad-id',
    definition(`,"availableWhen":{"progress":{"Cannon.complete":{"eq":true}}}`),
  ],
  [
    'gate-progress-key-bad-field',
    definition(`,"availableWhen":{"progress":{"cannon.per cent":{"eq":true}}}`),
  ],
  [
    'gate-progress-key-empty-field',
    definition(`,"availableWhen":{"progress":{"cannon.":{"eq":true}}}`),
  ],
  ['gate-progress-not-a-record', definition(`,"availableWhen":{"progress":"cannon"}`)],
  [
    'gate-progress-comparator-reference',
    definition(`,"availableWhen":{"progress":{"cannon.percent":{"gt":{"$param":"x"}}}}`),
  ],
  [
    'gate-unknown-subject',
    definition(`,"availableWhen":{"progression":{"cannon.complete":{"eq":true}}}`),
  ],

  // --- when.progress on an outcome row (a catch-all always follows, so the
  //     "final outcome must be a catch-all" refine never masks the arm)
  ['when-progress-only', whenRow(`{"progress":{"cannon.complete":{"eq":true}}}`)],
  ['when-progress-empty', whenRow(`{"progress":{}}`)],
  ['when-progress-bad-key', whenRow(`{"progress":{"Cannon.complete":{"eq":true}}}`)],
  ['when-progress-no-dot-key', whenRow(`{"progress":{"cannon":{"eq":true}}}`)],
  ['when-progress-not-a-record', whenRow(`{"progress":[]}`)],
  [
    'when-progress-with-metadata',
    whenRow(`{"metadata":{"rank":{"eq":"x"}},"progress":{"cannon.started":{"eq":true}}}`),
  ],
  [
    'when-progress-with-param-ref',
    `{"name":"probe","description":"a probe","roll":"1d20","parameters":{"n":{"type":"number","description":"d","default":1}},"outcomes":[{"when":{"progress":{"cannon.percent":{"gt":{"$param":"n"}}}},"message":"m","state":"success"},{"when":true,"message":"c","state":"info"}]}`,
  ],

  // --- effect.when.progress
  [
    'effect-when-progress',
    definition(
      `,"effects":[{"when":{"progress":{"cannon.complete":{"eq":true}}},"target":"state.x","value":1}]`,
    ),
  ],

  // --- the effect target's third family
  [
    'effect-target-progress-ok',
    definition(`,"effects":[{"target":"progress.cannon.endTime","value":1}]`),
  ],
  [
    'effect-target-progress-remove',
    definition(`,"effects":[{"target":"progress.cannon.remove","value":true}]`),
  ],
  [
    'effect-target-progress-nested-field',
    definition(`,"effects":[{"target":"progress.cannon.quantity.total","value":1}]`),
  ],
  [
    'effect-target-progress-no-field',
    definition(`,"effects":[{"target":"progress.cannon","value":1}]`),
  ],
  [
    'effect-target-progress-trailing-dot',
    definition(`,"effects":[{"target":"progress.cannon.","value":1}]`),
  ],
  [
    'effect-target-progress-leading-dot',
    definition(`,"effects":[{"target":"progress..complete","value":1}]`),
  ],
  [
    'effect-target-progress-bad-id',
    definition(`,"effects":[{"target":"progress.Cannon.endTime","value":1}]`),
  ],
  [
    'effect-target-progress-unwritable-field',
    definition(`,"effects":[{"target":"progress.cannon.percent","value":1}]`),
  ],
  [
    'effect-target-progress-updated-at',
    definition(`,"effects":[{"target":"progress.cannon.updatedAt","value":1}]`),
  ],
  [
    'effect-target-progress-whole-quantity',
    definition(`,"effects":[{"target":"progress.cannon.quantity","value":1}]`),
  ],
  [
    'effect-target-metadata-progressions',
    definition(`,"effects":[{"target":"metadata.progressions","value":1}]`),
  ],
  [
    'effect-target-metadata-progressions-child',
    definition(`,"effects":[{"target":"metadata.progressions.cannon","value":1}]`),
  ],
  [
    'effect-target-metadata-progressions-notes',
    definition(`,"effects":[{"target":"metadata.progressionsNotes","value":1}]`),
  ],
  ['effect-target-bare-progress', definition(`,"effects":[{"target":"progress","value":1}]`)],

  // --- the new placeholders inside an expression value
  [
    'effect-value-now',
    definition(`,"effects":[{"target":"progress.cannon.endTime","value":"{{now}} + 600000"}]`),
  ],
  [
    'effect-value-progress-ref',
    definition(`,"effects":[{"target":"state.x","value":"{{progress.cannon.percent}}"}]`),
  ],
];

for (const [id, inputJson] of DEFINITIONS) {
  const result = QtapCustomToolSchema.safeParse(JSON.parse(inputJson));
  console.log(
    JSON.stringify({
      kind: 'definition',
      id,
      inputJson,
      success: result.success,
      reason: result.success ? null : formatDefinitionIssues(result.error),
      data: result.success ? JSON.stringify(result.data) : null,
    }),
  );
}

/** `parseEffectTarget` directly, so the parsed target's SHAPE is compared too. */
const TARGETS = [
  'progress.cannon.endTime',
  'progress.cannon.quantity.total',
  'progress.cannon.remove',
  'progress.cannon',
  'progress.cannon.',
  'progress..complete',
  'progress.Cannon.endTime',
  'progress.cannon.percent',
  'progress.',
  'progress',
  'metadata.progressions',
  'metadata.progressions.cannon',
  'metadata.progressionsNotes',
  'metadata.progressionsomething',
  'state.progressions',
  'elsewhere.x',
];
for (const target of TARGETS) {
  const result = parseEffectTarget(target);
  console.log(
    JSON.stringify({
      kind: 'effect-target',
      id: target,
      out: JSON.stringify(result),
    }),
  );
}

/** `classifyPlaceholder`, whose two new families are the SPA's own concern. */
const KEYS = [
  'now',
  'Now',
  'now.x',
  'progress.cannon.percent',
  'progress.cannon.quantity.total',
  'progress.cannon',
  'progress.cannon.',
  'progress..percent',
  'progress.',
  'progress',
];
for (const key of KEYS) {
  console.log(
    JSON.stringify({ kind: 'placeholder', id: key, out: JSON.stringify(classifyPlaceholder(key)) }),
  );
}

/** `evaluateToolGate`'s third argument, against a derived sheet. */
const NOW = Date.parse('2026-09-08T14:05:00Z');
const SHEET = flattenProgressions(
  {
    progressions: {
      cannon: {
        name: 'Cannon recharge',
        startTime: '2026-09-08T14:00:00Z',
        endTime: '2026-09-08T14:10:00Z',
        timeIncrement: 'minute',
      },
    },
  },
  NOW,
);
const GATES: Array<[string, string, boolean]> = [
  [
    'gate-eval-progress-holds',
    `{"availableWhen":{"progress":{"cannon.started":{"eq":true}}}}`,
    true,
  ],
  [
    'gate-eval-progress-fails',
    `{"availableWhen":{"progress":{"cannon.complete":{"eq":true}}}}`,
    true,
  ],
  [
    'gate-eval-progress-absent-id',
    `{"availableWhen":{"progress":{"ghost.complete":{"eq":true}}}}`,
    true,
  ],
  [
    'gate-eval-withheld-progress',
    `{"withheldWhen":{"progress":{"cannon.percent":{"gt":40}}}}`,
    true,
  ],
  [
    'gate-eval-no-sheet-available',
    `{"availableWhen":{"progress":{"cannon.started":{"eq":true}}}}`,
    false,
  ],
  [
    'gate-eval-no-sheet-withheld',
    `{"withheldWhen":{"progress":{"cannon.started":{"eq":true}}}}`,
    false,
  ],
  [
    'gate-eval-both-subjects',
    `{"availableWhen":{"metadata":{"rank":{"eq":"captain"}},"progress":{"cannon.started":{"eq":true}}}}`,
    true,
  ],
];
for (const [id, definitionJson, withSheet] of GATES) {
  const verdict = evaluateToolGate(
    JSON.parse(definitionJson),
    { rank: 'captain' },
    withSheet ? SHEET : null,
  );
  console.log(
    JSON.stringify({
      kind: 'gate-eval',
      id,
      definitionJson,
      withSheet,
      sheet: JSON.stringify(SHEET),
      out: JSON.stringify(verdict),
    }),
  );
}
