/**
 * Oracle case: the availability GATE over any-of clauses, driven directly
 * (P4.D333, v4 `a53024d3c`).
 *
 * Drives the REAL code:
 *   evaluateToolGate, gateNamesProgress  (lib/pascal/tool-gate.ts)
 *   QtapCustomToolSchema                 (lib/pascal/custom-tool.types.ts)
 *
 * The definition corpus's `gate` rows prove the same verdicts through the
 * committed corpus; this family is the regen-cheap direct drive that stands
 * alone. A verdict is compared as its serialized BYTES: `heldGroup` is
 * present only for an any-of clause — the group that OPENED an
 * `availableWhen` or CLOSED a `withheldWhen` — and absent on an
 * `availableWhen` miss and for every bare-object clause, in the key order
 * `available, withheldBy, heldGroup`.
 *
 * `gateNamesProgress` is the roster's "derive the progress sheet?" test: since
 * `a53024d3c` it counts a group's `progress` KEYS, so an authored-but-empty
 * `progress: {}` no longer names it.
 *
 * Run from inside the server checkout (v4 @ 322a5488e, Node 24
 * `~/.nvm/versions/node/v24.13.1/bin`):
 *   cd ~/source/quilltap-server
 *   TZ=UTC npx tsx \
 *     <V5W>/harness/oracle/cases/pascal-tool-gate.ts \
 *     > /tmp/oracle-pascal-tool-gate.ndjson
 */

import { QtapCustomToolSchema } from '@/lib/pascal/custom-tool.types';
import { evaluateToolGate, gateNamesProgress } from '@/lib/pascal/tool-gate';

const BASE = {
  name: 'probe',
  description: 'A probe.',
  outcomes: [{ when: true, message: '-', state: 'info' }],
};

const TWO = [{ metadata: { clearance: { gte: 3 } } }, { metadata: { house: { eq: 'Aurum' } } }];
const CLOSE = [{ metadata: { novice: { eq: true } } }, { metadata: { probation: { eq: true } } }];
// A flattened progress sheet, as `flattenProgressions` hands the gate one.
const CHARGED = { 'cannon.complete': true, 'cannon.percent': 110 };
const CHARGING = { 'cannon.complete': false, 'cannon.percent': 10 };

const gateCases: Array<[string, Record<string, unknown>, Record<string, unknown> | null, Record<string, unknown>?]> = [
  ['available-anyof-opens-on-0', { availableWhen: TWO }, { clearance: 4, house: 'Aurum' }],
  ['available-anyof-opens-on-1', { availableWhen: TWO }, { clearance: 1, house: 'Aurum' }],
  ['available-anyof-opens-on-none', { availableWhen: TWO }, { clearance: 1, house: 'Ferro' }],
  ['available-anyof-opens-on-0-and-1', { availableWhen: TWO }, { clearance: 9, house: 'Aurum' }],
  ['available-one-element-array', { availableWhen: [TWO[0]] }, { clearance: 4 }],
  ['available-bare-object', { availableWhen: TWO[0] }, { clearance: 4 }],
  ['available-bare-object-misses', { availableWhen: TWO[0] }, { clearance: 1 }],
  ['withheld-anyof-closes-on-1', { withheldWhen: CLOSE }, { novice: false, probation: true }],
  ['withheld-anyof-closes-on-0', { withheldWhen: CLOSE }, { novice: true, probation: true }],
  ['withheld-anyof-offers', { withheldWhen: CLOSE }, { novice: false, probation: false }],
  ['withheld-bare-object-closes', { withheldWhen: CLOSE[0] }, { novice: true }],
  ['available-anyof-empty-sheet', { availableWhen: TWO }, {}],
  ['available-anyof-null-sheet', { availableWhen: TWO }, null],
  ['withheld-anyof-empty-sheet', { withheldWhen: CLOSE }, {}],
  ['withheld-anyof-null-sheet', { withheldWhen: CLOSE }, null],
  ['available-anyof-progress-group', { availableWhen: [{ metadata: { pilot: { eq: true } } }, { progress: { 'cannon.complete': { eq: true } } }] }, { pilot: false }, CHARGED],
  ['available-anyof-progress-group-charging', { availableWhen: [{ metadata: { pilot: { eq: true } } }, { progress: { 'cannon.complete': { eq: true } } }] }, { pilot: false }, CHARGING],
  ['available-anyof-both-sheets-one-group', { availableWhen: [{ metadata: { rank: { gte: 5 } } }, { metadata: { pilot: { eq: true } }, progress: { 'cannon.percent': { gte: 100 } } }] }, { rank: 1, pilot: true }, CHARGED],
  ['available-anyof-list-rule', { availableWhen: [{ metadata: { rank: { gte: 5 } } }, { metadata: { knownLanguages: { contains: 'Cant' } } }] }, { rank: 1, knownLanguages: ['Trade Cant'] }],
  ['available-anyof-eight-groups-last', { availableWhen: Array.from({ length: 8 }, (_, i) => ({ metadata: { [`k${i}`]: { eq: true } } })) }, { k7: true }],
  ['ungated', {}, { clearance: 4 }],
];

const namesCases: Array<[string, unknown]> = [
  ['names-progress-undefined', undefined],
  ['names-progress-bare-with-progress', { progress: { 'cannon.complete': { eq: true } } }],
  ['names-progress-bare-empty-record', { metadata: { rank: { gte: 1 } }, progress: {} }],
  ['names-progress-bare-metadata-only', { metadata: { rank: { gte: 1 } } }],
  ['names-progress-array-second-group-names-it', [{ metadata: { rank: { gte: 1 } } }, { progress: { 'cannon.complete': { eq: true } } }]],
  ['names-progress-array-no-group-names-it', [{ metadata: { rank: { gte: 1 } } }, { metadata: { a: { eq: 1 } }, progress: {} }]],
];

const rows: unknown[] = [];
for (const [id, gate, metadata, progress] of gateCases) {
  const definition = { ...BASE, ...gate };
  const parsed = QtapCustomToolSchema.safeParse(definition);
  if (!parsed.success) throw new Error(`gate case '${id}': definition does not load`);
  rows.push({
    kind: 'gate',
    id,
    inputJson: JSON.stringify(definition),
    metadata,
    ...(progress === undefined ? {} : { progress }),
    verdict:
      progress === undefined
        ? JSON.stringify(evaluateToolGate(parsed.data, metadata))
        : JSON.stringify(evaluateToolGate(parsed.data, metadata, progress as Record<string, number | boolean>)),
  });
}
for (const [id, gate] of namesCases) {
  const definition = gate === undefined ? BASE : { ...BASE, availableWhen: gate };
  const parsed = QtapCustomToolSchema.safeParse(definition);
  if (!parsed.success) throw new Error(`names case '${id}': definition does not load`);
  rows.push({
    kind: 'namesProgress',
    id,
    inputJson: JSON.stringify(definition),
    namesProgress: gateNamesProgress(parsed.data.availableWhen),
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
