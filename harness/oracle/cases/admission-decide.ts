/**
 * Tier-1 ORACLE for v4's PURE admission decision (P4.D289 unit 7, pin
 * `83334b9a5`) — `lib/memory/admission/{decide,outcome,describe}.ts`
 * (`8af764514`, Phase C of the memory refactor) and the admission-threshold
 * block of `lib/memory/settings-defaults.ts` (the TF-IDF set from
 * `4ae0c4ca6`), plus `ADMISSION_TOP_K` (`admission/neighbours.ts` — imported
 * for the constant only; the module loads without a database).
 *
 * Drives v4's REAL modules, imported by FILE path (the `admission/index.ts`
 * barrel pulls `admit.ts` → repositories). Rows:
 *  - `constant` — the two threshold sets (and their `JSON.stringify` bytes),
 *    `ADMISSION_TOP_K`;
 *  - `provider` — `admissionThresholdsForProvider` (which set, by identity);
 *  - `distinct` — `occasionsAreDistinct` (`gate-date-guard.test.ts`'s half +
 *    falsy / date-only / offset / custom-guard rows);
 *  - `decide` — every expectation of `admission-decide.test.ts`,
 *    `provider-thresholds.test.ts`'s TF-IDF rows (0.45 / 0.85 / 0.29 / 0.08),
 *    equal cosines (stable), NaN cosines, `insert-linked`'s three reasons, a
 *    cold dangling digest, `via: ''`, subject / kind / source edges; each with
 *    the action and assessments as v4's `JSON.stringify` BYTES and the three
 *    outcome predicates;
 *  - `describe` — `describeAdmission` for every action kind (incl. a non-BMP
 *    id prefix, a fractional count, a NaN similarity, the fallbacks).
 *
 * Non-finite numbers in INPUTS ride as strings (`"NaN"`, `"Infinity"`); the
 * byte-pinned `*Json` fields are v4's own `JSON.stringify` output (a NaN
 * cosine writes `null` there, as it does on the Rust side).
 *
 * Run from inside a v4 checkout (or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/admission-decide.ts" > /tmp/oracle-admission-decide.ndjson
 */

import { decideAdmission, occasionsAreDistinct, type AdmissionCandidateView } from '@/lib/memory/admission/decide';
import { admissionInserted, admissionReinforced, isNearDuplicateDecision } from '@/lib/memory/admission/outcome';
import { describeAdmission } from '@/lib/memory/admission/describe';
import type { AdmissionNeighbour, AdmissionOutcome } from '@/lib/memory/admission/types';
import { ADMISSION_TOP_K } from '@/lib/memory/admission/neighbours';
import {
  ADMISSION_THRESHOLDS,
  ADMISSION_THRESHOLDS_TFIDF,
  admissionThresholdsForProvider,
  type AdmissionThresholds,
} from '@/lib/memory/settings-defaults';

const rows: unknown[] = [];
// Inputs: non-finite numbers as strings.
const enc = (_k: string, v: unknown) => (typeof v === 'number' && !Number.isFinite(v) ? String(v) : v);

const SETS: Record<string, AdmissionThresholds> = { neural: ADMISSION_THRESHOLDS, tfidf: ADMISSION_THRESHOLDS_TFIDF };
const setName = (t: AdmissionThresholds) => (t === ADMISSION_THRESHOLDS ? 'neural' : t === ADMISSION_THRESHOLDS_TFIDF ? 'tfidf' : 'other');

// ---- constants --------------------------------------------------------------
for (const [name, t] of Object.entries(SETS)) {
  rows.push({ kind: 'constant', id: `thresholds-${name}`, out: t, json: JSON.stringify(t) });
}
rows.push({ kind: 'constant', id: 'top-k', out: ADMISSION_TOP_K });

// ---- provider ---------------------------------------------------------------
const providers: Array<[string, string | null | undefined]> = [
  ['BUILTIN', 'BUILTIN'],
  ['OPENAI', 'OPENAI'],
  ['undefined', undefined],
  ['null', null],
  ['empty', ''],
  ['lower-builtin', 'builtin'],
  ['OLLAMA', 'OLLAMA'],
];
for (const [id, p] of providers) {
  rows.push({ kind: 'provider', id, provider: p ?? null, out: setName(admissionThresholdsForProvider(p)) });
}

// ---- occasionsAreDistinct ---------------------------------------------------
const distinct: Array<[string, string | null | undefined, string | null | undefined, number]> = [
  ['beyond-window', '2026-03-10T00:00:00.000Z', '2026-07-14T00:00:00.000Z', 7],
  ['within-window', '2026-07-14T00:00:00.000Z', '2026-07-16T00:00:00.000Z', 7],
  ['exactly-window', '2026-07-07T00:00:00.000Z', '2026-07-14T00:00:00.000Z', 7],
  ['null-candidate', null, '2026-07-14T00:00:00.000Z', 7],
  ['undefined-existing', '2026-07-14T00:00:00.000Z', undefined, 7],
  ['garbage', 'garbage', '2026-07-14T00:00:00.000Z', 7],
  ['empty-string', '', '2026-07-14T00:00:00.000Z', 7],
  ['both-garbage', 'nope', 'nada', 7],
  ['just-over-window', '2026-07-07T00:00:00.000Z', '2026-07-14T00:00:00.001Z', 7],
  ['reversed-order', '2026-07-14T00:00:00.000Z', '2026-03-10T00:00:00.000Z', 7],
  ['date-only', '2026-07-01', '2026-07-09', 7],
  ['offset-forms', '2026-07-08T01:00:00+02:00', '2026-07-01T00:00:00Z', 7],
  ['guard-zero-same-instant', '2026-07-01T00:00:00.000Z', '2026-07-01T00:00:00.000Z', 0],
  ['guard-zero-one-ms', '2026-07-01T00:00:00.000Z', '2026-07-01T00:00:00.001Z', 0],
  ['guard-fractional', '2026-07-01T00:00:00.000Z', '2026-07-01T13:00:00.000Z', 0.5],
  ['guard-nan', '2026-03-10T00:00:00.000Z', '2026-07-14T00:00:00.000Z', NaN],
  ['guard-negative', '2026-07-01T00:00:00.000Z', '2026-07-01T00:00:00.000Z', -1],
];
for (const [id, a, b, days] of distinct) {
  rows.push({ kind: 'distinct', id, a: a ?? null, b: b ?? null, days, out: occasionsAreDistinct(a, b, days) });
}

// ---- decideAdmission --------------------------------------------------------
const HOLDER = 'char-holder';
const BOB = 'char-bob';
// `admission-decide.test.ts`'s `mem()` fixture.
const mem = (id: string, overrides: Record<string, unknown> = {}): any => ({
  id,
  characterId: HOLDER,
  content: `content ${id}`,
  summary: `summary ${id}`,
  keywords: [],
  tags: [],
  importance: 0.5,
  aboutCharacterId: null,
  source: 'AUTO',
  kind: 'semantic',
  reinforcementCount: 1,
  relatedMemoryIds: [],
  reinforcedImportance: 0.5,
  tier: 'hot',
  supersededById: null,
  consolidatedFrom: [],
  entities: [],
  occurredAt: null,
  createdAt: '2026-01-01T00:00:00.000Z',
  updatedAt: '2026-01-01T00:00:00.000Z',
  ...overrides,
});
const n = (memory: any, cosine: number, extra: Partial<AdmissionNeighbour> = {}): AdmissionNeighbour => ({
  memory,
  cosine,
  ...extra,
});
const AUTO_SELF: AdmissionCandidateView = { holderId: HOLDER, aboutId: null, kind: 'semantic', source: 'AUTO', occurredAt: null };
// `provider-thresholds.test.ts`'s minimal row and candidate.
const PT_CANDIDATE: AdmissionCandidateView = { holderId: 'c-1', aboutId: null, kind: 'semantic', source: 'AUTO', occurredAt: null };
const ptRow = (id: string): any => ({ id, characterId: 'c-1', kind: 'semantic', source: 'AUTO', tier: 'hot' });

const SPRING = '2026-03-10T00:00:00.000Z';
const SUMMER = '2026-07-14T00:00:00.000Z';
const SUMMER_RETOLD = '2026-07-16T00:00:00.000Z';

type DecideCase = [string, AdmissionNeighbour[], Partial<AdmissionCandidateView>, string];
const decideCases: DecideCase[] = [
  // threshold bands (it.each)
  ['bands-no-neighbours', [], {}, 'neural'],
  ['bands-all-below', [n(mem('a'), 0.69)], {}, 'neural'],
  ['bands-related', [n(mem('a'), 0.7)], {}, 'neural'],
  ['bands-just-under-reinforce', [n(mem('a'), 0.8499)], {}, 'neural'],
  ['bands-reinforce', [n(mem('a'), 0.85)], {}, 'neural'],
  ['bands-just-under-near', [n(mem('a'), 0.8999)], {}, 'neural'],
  ['bands-near', [n(mem('a'), 0.9)], {}, 'neural'],
  ['bands-several-link-best-first', [n(mem('b'), 0.72), n(mem('a'), 0.81), n(mem('c'), 0.5)], {}, 'neural'],
  ['bands-closest-eligible-wins', [n(mem('a'), 0.87), n(mem('b'), 0.93)], {}, 'neural'],
  // subject, kind and source
  ['subject-someone-else-linked', [n(mem('a', { aboutCharacterId: BOB }), 0.95)], {}, 'neural'],
  ['subject-self-about-holder-row', [n(mem('a', { aboutCharacterId: HOLDER }), 0.87)], {}, 'neural'],
  ['subject-self-about-holder-candidate', [n(mem('a'), 0.87)], { aboutId: HOLDER }, 'neural'],
  ['subject-bob-into-bob', [n(mem('a', { aboutCharacterId: BOB }), 0.87)], { aboutId: BOB }, 'neural'],
  ['kind-different-linked', [n(mem('a', { kind: 'episodic' }), 0.95)], {}, 'neural'],
  ['source-manual-over-auto', [n(mem('a'), 0.97)], { source: 'MANUAL' }, 'neural'],
  ['source-manual-into-manual', [n(mem('a', { source: 'MANUAL' }), 0.87)], { source: 'MANUAL' }, 'neural'],
  ['source-manual-into-digest', [n(mem('d', { source: 'CONSOLIDATED' }), 0.95)], { source: 'MANUAL' }, 'neural'],
  ['source-auto-into-manual', [n(mem('a', { source: 'MANUAL' }), 0.95)], {}, 'neural'],
  ['skip-ineligible-closest', [n(mem('bob', { aboutCharacterId: BOB }), 0.97), n(mem('self'), 0.88)], {}, 'neural'],
  // date guard over every close neighbour
  ['date-guard-rank-one', [n(mem('a', { occurredAt: SPRING }), 0.96)], { occurredAt: SUMMER }, 'neural'],
  [
    'date-guard-looks-past',
    [n(mem('spring', { occurredAt: SPRING }), 0.96), n(mem('summer', { occurredAt: SUMMER_RETOLD }), 0.88)],
    { occurredAt: SUMMER },
    'neural',
  ],
  [
    'date-guard-links-every',
    [
      n(mem('spring', { occurredAt: SPRING }), 0.96),
      n(mem('autumn', { occurredAt: '2025-10-01T00:00:00.000Z' }), 0.91),
      n(mem('band'), 0.75),
    ],
    { occurredAt: SUMMER },
    'neural',
  ],
  ['date-guard-needs-both', [n(mem('a', { occurredAt: SPRING }), 0.96)], {}, 'neural'],
  // digests and the cold tier
  ['digest-via-reinforce-band', [n(mem('digest', { source: 'CONSOLIDATED' }), 0.87, { via: 'member' })], {}, 'neural'],
  ['digest-via-near', [n(mem('digest', { source: 'CONSOLIDATED' }), 0.95, { via: 'member' })], {}, 'neural'],
  ['digest-direct-reinforce', [n(mem('digest', { source: 'CONSOLIDATED' }), 0.87)], {}, 'neural'],
  ['cold-promote-reinforce', [n(mem('cold', { tier: 'cold' }), 0.87)], {}, 'neural'],
  ['cold-promote-absorb', [n(mem('cold', { tier: 'cold' }), 0.95)], {}, 'neural'],
  ['cold-dangling-digest', [n(mem('cold', { tier: 'cold', supersededById: 'gone' }), 0.95, { danglingDigest: true })], {}, 'neural'],
  ['cold-in-band-linked', [n(mem('cold', { tier: 'cold' }), 0.75)], {}, 'neural'],
  // the trace
  [
    'trace-every-neighbour',
    [n(mem('low'), 0.4), n(mem('bob', { aboutCharacterId: BOB }), 0.96), n(mem('band'), 0.75), n(mem('ok'), 0.88)],
    {},
    'neural',
  ],
  // provider-thresholds.test.ts — the TF-IDF scale
  ['tfidf-restatement-0.45', [n(ptRow('m'), 0.45)], PT_CANDIDATE, 'tfidf'],
  ['tfidf-near-0.85', [n(ptRow('m'), 0.85)], PT_CANDIDATE, 'tfidf'],
  ['tfidf-same-topic-0.29', [n(ptRow('m'), 0.29)], PT_CANDIDATE, 'tfidf'],
  ['tfidf-different-fact-0.08', [n(ptRow('m'), 0.08)], PT_CANDIDATE, 'tfidf'],
  ['tfidf-exact-near-0.80', [n(ptRow('m'), 0.8)], PT_CANDIDATE, 'tfidf'],
  ['tfidf-exact-reinforce-0.40', [n(ptRow('m'), 0.4)], PT_CANDIDATE, 'tfidf'],
  ['tfidf-exact-related-0.20', [n(ptRow('m'), 0.2)], PT_CANDIDATE, 'tfidf'],
  ['neural-same-0.45', [n(ptRow('m'), 0.45)], PT_CANDIDATE, 'neural'],
  // equal cosines keep their given order (stable sort)
  ['ties-eligible-first-given-wins', [n(mem('x'), 0.88), n(mem('y'), 0.88), n(mem('z'), 0.88)], {}, 'neural'],
  ['ties-band-link-order', [n(mem('q'), 0.75), n(mem('p'), 0.75), n(mem('r'), 0.8), n(mem('s'), 0.75)], {}, 'neural'],
  ['ties-mixed-verdicts', [n(mem('bob', { aboutCharacterId: BOB }), 0.9), n(mem('ok'), 0.9), n(mem('ok2'), 0.9)], {}, 'neural'],
  // NaN cosines: fail both band tests, are assessed like a close neighbour, never `near`
  ['nan-alone', [n(mem('a'), NaN)], {}, 'neural'],
  ['nan-before-eligible', [n(mem('a'), NaN), n(mem('b'), 0.88)], {}, 'neural'],
  ['nan-after-eligible', [n(mem('b'), 0.88), n(mem('a'), NaN)], {}, 'neural'],
  ['nan-middle', [n(mem('c'), 0.75), n(mem('a'), NaN), n(mem('b'), 0.95), n(mem('d'), 0.2)], {}, 'neural'],
  ['nan-excluded', [n(mem('a', { aboutCharacterId: BOB }), NaN)], {}, 'neural'],
  ['nan-two', [n(mem('a'), NaN), n(mem('b'), NaN), n(mem('c'), 0.9)], {}, 'neural'],
  ['infinity-cosine', [n(mem('a'), Infinity), n(mem('b'), 0.95)], {}, 'neural'],
  ['negative-and-over-one', [n(mem('a'), -0.3), n(mem('b'), 1.2)], {}, 'neural'],
  ['cosine-exactly-one', [n(mem('a'), 1)], {}, 'neural'],
  ['cosine-zero', [n(mem('a'), 0)], {}, 'neural'],
  // insert-linked's three reasons, mixed
  ['reason-mismatch-plus-band', [n(mem('k', { kind: 'episodic' }), 0.9), n(mem('b'), 0.72)], {}, 'neural'],
  [
    'reason-date-guard-beats-mismatch',
    [n(mem('k', { kind: 'episodic' }), 0.95), n(mem('d', { occurredAt: SPRING }), 0.9)],
    { occurredAt: SUMMER },
    'neural',
  ],
  ['reason-manual-over-auto-is-mismatch', [n(mem('a'), 0.9), n(mem('b'), 0.71)], { source: 'MANUAL' }, 'neural'],
  // cold tier edges
  ['cold-superseded-not-dangling', [n(mem('cold', { tier: 'cold', supersededById: 'digest' }), 0.87)], {}, 'neural'],
  [
    'cold-superseded-dangling-false',
    [n(mem('cold', { tier: 'cold', supersededById: 'digest' }), 0.95, { danglingDigest: false })],
    {},
    'neural',
  ],
  ['cold-dangling-reinforce-band', [n(mem('cold', { tier: 'cold', supersededById: 'gone' }), 0.87, { danglingDigest: true })], {}, 'neural'],
  ['cold-with-via', [n(mem('cold', { tier: 'cold' }), 0.87, { via: 'member' })], {}, 'neural'],
  ['cold-superseded-empty-string', [n(mem('cold', { tier: 'cold', supersededById: '' }), 0.87)], {}, 'neural'],
  // via edges
  ['via-empty-string', [n(mem('digest'), 0.87, { via: '' })], {}, 'neural'],
  ['via-on-band-neighbour', [n(mem('digest'), 0.75, { via: 'member-long-id-123456' })], {}, 'neural'],
  ['via-on-below-neighbour', [n(mem('digest'), 0.3, { via: 'm' })], {}, 'neural'],
  // duplicate ids, later eligibles
  ['duplicate-id-band', [n(mem('a'), 0.75), n(mem('a'), 0.72)], {}, 'neural'],
  ['duplicate-id-excluded-and-band', [n(mem('a', { aboutCharacterId: BOB }), 0.95), n(mem('a'), 0.75)], {}, 'neural'],
  ['later-eligibles-assessed', [n(mem('a'), 0.86), n(mem('b'), 0.99), n(mem('c'), 0.91)], {}, 'neural'],
  // subject / kind / source edges
  ['subject-row-about-empty-string', [n(mem('a', { aboutCharacterId: '' }), 0.87)], {}, 'neural'],
  ['subject-candidate-about-empty-string', [n(mem('a'), 0.87)], { aboutId: '' }, 'neural'],
  ['subject-row-about-absent', [n(ptRow('a'), 0.87)], PT_CANDIDATE, 'neural'],
  ['subject-other-holder', [n(mem('a', { characterId: 'char-other' }), 0.87)], {}, 'neural'],
  ['kind-row-null-candidate-undefined', [n(mem('a', { kind: null }), 0.87)], { kind: undefined }, 'neural'],
  ['kind-row-episodic-candidate-undefined', [n(mem('a', { kind: 'episodic' }), 0.87)], { kind: undefined }, 'neural'],
  ['kind-both-episodic', [n(mem('a', { kind: 'episodic' }), 0.87)], { kind: 'episodic' }, 'neural'],
  ['source-manual-row-null', [n(mem('a', { source: null }), 0.87)], { source: 'MANUAL' }, 'neural'],
  ['source-manual-row-absent', [n(ptRow('a'), 0.87)], { ...PT_CANDIDATE, source: 'MANUAL' }, 'neural'],
  ['source-consolidated-candidate-over-auto', [n(mem('a'), 0.87)], { source: 'CONSOLIDATED' }, 'neural'],
  ['source-undefined-candidate', [n(mem('a'), 0.87)], { source: undefined }, 'neural'],
  // date guard edges
  ['date-guard-within-reinforces', [n(mem('a', { occurredAt: SUMMER_RETOLD }), 0.87)], { occurredAt: SUMMER }, 'neural'],
  ['date-guard-band-row-not-checked', [n(mem('a', { occurredAt: SPRING }), 0.75)], { occurredAt: SUMMER }, 'neural'],
  ['date-guard-tfidf', [n(ptRow('a'), 0.5)], { ...PT_CANDIDATE, occurredAt: SUMMER }, 'tfidf'],
  [
    'date-guard-tfidf-distinct',
    [n({ ...ptRow('a'), occurredAt: SPRING }, 0.5)],
    { ...PT_CANDIDATE, occurredAt: SUMMER },
    'tfidf',
  ],
  ['date-guard-unparseable-row', [n(mem('a', { occurredAt: 'last spring' }), 0.87)], { occurredAt: SUMMER }, 'neural'],
  // first exclusion wins
  [
    'exclusion-order-subject-first',
    [n(mem('a', { aboutCharacterId: BOB, kind: 'episodic', occurredAt: SPRING }), 0.95)],
    { occurredAt: SUMMER, source: 'MANUAL' },
    'neural',
  ],
  [
    'exclusion-order-kind-before-source',
    [n(mem('a', { kind: 'episodic', occurredAt: SPRING }), 0.95)],
    { occurredAt: SUMMER, source: 'MANUAL' },
    'neural',
  ],
  [
    'exclusion-order-source-before-date',
    [n(mem('a', { occurredAt: SPRING }), 0.95)],
    { occurredAt: SUMMER, source: 'MANUAL' },
    'neural',
  ],
];

for (const [id, neighbours, candidatePatch, tname] of decideCases) {
  const thresholds = SETS[tname];
  const candidate = { ...AUTO_SELF, ...candidatePatch };
  const decision = decideAdmission({ candidate, neighbours, thresholds });
  rows.push({
    kind: 'decide',
    id,
    thresholds: tname,
    candidate: {
      holderId: candidate.holderId,
      aboutId: candidate.aboutId ?? null,
      kind: candidate.kind ?? null,
      source: candidate.source ?? null,
      occurredAt: candidate.occurredAt ?? null,
    },
    neighbours,
    actionJson: JSON.stringify(decision.action),
    assessmentsJson: JSON.stringify(decision.assessments),
    inserted: admissionInserted(decision),
    reinforced: admissionReinforced(decision),
    nearDuplicate: isNearDuplicateDecision(decision, thresholds),
  });
}

// ---- describeAdmission ------------------------------------------------------
const trace = (assessments: any[] = [], notes: string[] = []) => ({
  thresholds: ADMISSION_THRESHOLDS,
  assessments,
  embeddingAttempts: 1,
  orphansIgnored: 0,
  searchedK: 5,
  notes,
});
const NEIGHBOURS = [
  { id: 'below-row-0001', cosine: 0.31, verdict: 'below' },
  { id: 'band-row-00002', cosine: 0.7512, verdict: 'band' },
  { id: 'excl-row-00003', cosine: 0.95, verdict: 'excluded', excludedBy: 'manual-over-auto' },
  { id: 'elig-row-00004', cosine: 0.8765, via: 'member-abcdefgh-xyz', verdict: 'eligible' },
];
type DescribeCase = [string, AdmissionOutcome, string, string];
const out = (o: any): AdmissionOutcome => o as AdmissionOutcome;
const describeCases: DescribeCase[] = [
  ['insert', out({ action: { kind: 'insert' }, memory: { id: 'new-1' }, novelDetails: [], linkedIds: [], trace: trace() }), 'self memory', 'Friday keeps the ledger'],
  [
    'insert-with-neighbours',
    out({ action: { kind: 'insert' }, memory: { id: 'new-1' }, novelDetails: [], linkedIds: [], trace: trace(NEIGHBOURS) }),
    'self memory',
    'sum',
  ],
  [
    'insert-only-below',
    out({ action: { kind: 'insert' }, memory: null, novelDetails: [], linkedIds: [], trace: trace([NEIGHBOURS[0]]) }),
    'L',
    'S',
  ],
  [
    'insert-linked-from-link-to',
    out({ action: { kind: 'insert-linked', linkTo: ['a', 'b'], reason: 'band' }, memory: { id: 'n' }, novelDetails: [], linkedIds: [], trace: trace() }),
    'memory about Bob',
    'sum',
  ],
  [
    'insert-linked-from-linked-ids',
    out({
      action: { kind: 'insert-linked', linkTo: ['a', 'b', 'c'], reason: 'date-guard' },
      memory: { id: 'n' },
      novelDetails: [],
      linkedIds: ['a'],
      trace: trace(NEIGHBOURS),
    }),
    'L',
    'S',
  ],
  [
    'insert-linked-mismatch',
    out({ action: { kind: 'insert-linked', linkTo: ['a'], reason: 'mismatch' }, memory: null, novelDetails: [], linkedIds: [], trace: trace() }),
    'L',
    'S',
  ],
  [
    'reinforce',
    out({
      action: { kind: 'reinforce', into: 'target-1' },
      memory: { id: 'target-1', reinforcementCount: 3 },
      novelDetails: ['Lisbon', 'Porto'],
      linkedIds: [],
      trace: trace(NEIGHBOURS),
    }),
    'self memory',
    'sum',
  ],
  [
    'reinforce-no-memory',
    out({ action: { kind: 'reinforce', into: 'target-2' }, memory: null, novelDetails: [], linkedIds: [], trace: trace() }),
    'L',
    'S',
  ],
  [
    'reinforce-fractional-count',
    out({ action: { kind: 'reinforce', into: 't' }, memory: { id: 't', reinforcementCount: 2.5 }, novelDetails: ['x'], linkedIds: [], trace: trace() }),
    'L',
    'S',
  ],
  [
    'reinforce-count-null',
    out({ action: { kind: 'reinforce', into: 't' }, memory: { id: 't', reinforcementCount: null }, novelDetails: [], linkedIds: [], trace: trace() }),
    'L',
    'S',
  ],
  [
    'reinforce-memory-id-null',
    out({ action: { kind: 'reinforce', into: 'from-into' }, memory: { id: null, reinforcementCount: 1e21 }, novelDetails: [], linkedIds: [], trace: trace() }),
    'L',
    'S',
  ],
  [
    'reinforce-empty-into',
    out({ action: { kind: 'reinforce', into: '' }, memory: null, novelDetails: [], linkedIds: [], trace: trace() }),
    'L',
    'S',
  ],
  [
    'reinforce-numeric-id',
    out({ action: { kind: 'reinforce', into: 'x' }, memory: { id: 42, reinforcementCount: 0.1 }, novelDetails: [], linkedIds: [], trace: trace() }),
    'L',
    'S',
  ],
  [
    'promote-and-reinforce',
    out({
      action: { kind: 'promote-and-reinforce', into: 'cold-1', withFootnotes: true },
      memory: { id: 'cold-1', reinforcementCount: 4 },
      novelDetails: [],
      linkedIds: [],
      trace: trace(NEIGHBOURS),
    }),
    'L',
    'S',
  ],
  [
    'promote-no-memory',
    out({ action: { kind: 'promote-and-reinforce', into: 'cold-2', withFootnotes: false }, memory: null, novelDetails: [], linkedIds: [], trace: trace() }),
    'L',
    'S',
  ],
  [
    'absorb',
    out({
      action: { kind: 'absorb', into: 'excl-row-00003' },
      memory: { id: 'excl-row-00003', reinforcementCount: 2 },
      novelDetails: [],
      linkedIds: [],
      trace: trace(NEIGHBOURS),
    }),
    'L',
    'S',
  ],
  [
    'absorb-redirected',
    out({
      action: { kind: 'absorb', into: 'digest-1', redirectedFrom: 'member-1' },
      memory: { id: 'digest-1' },
      novelDetails: [],
      linkedIds: [],
      trace: trace([{ id: 'digest-1', cosine: 0.8765, via: 'member-1', verdict: 'eligible' }]),
    }),
    'L',
    'S',
  ],
  [
    'absorb-no-assessment',
    out({ action: { kind: 'absorb', into: 'gone' }, memory: null, novelDetails: [], linkedIds: [], trace: trace(NEIGHBOURS) }),
    'L',
    'S',
  ],
  [
    'absorb-nan-similarity',
    out({
      action: { kind: 'absorb', into: 'n1' },
      memory: { id: 'n1' },
      novelDetails: [],
      linkedIds: [],
      trace: trace([{ id: 'n1', cosine: NaN, verdict: 'eligible' }]),
    }),
    'L',
    'S',
  ],
  [
    'absorb-first-matching-assessment',
    out({
      action: { kind: 'absorb', into: 'dup' },
      memory: { id: 'dup' },
      novelDetails: [],
      linkedIds: [],
      trace: trace([
        { id: 'dup', cosine: 0.9105, verdict: 'eligible' },
        { id: 'dup', cosine: 0.75, verdict: 'band' },
      ]),
    }),
    'L',
    'S',
  ],
  [
    'absorb-rounding',
    out({
      action: { kind: 'absorb', into: 'r' },
      memory: { id: 'r' },
      novelDetails: [],
      linkedIds: [],
      trace: trace([{ id: 'r', cosine: 0.9995, verdict: 'eligible' }, { id: 'r2', cosine: 0.12345, verdict: 'band' }]),
    }),
    'L',
    'S',
  ],
  [
    'reject-embedding-failed',
    out({ action: { kind: 'reject', reason: 'embedding-failed' }, memory: null, novelDetails: [], linkedIds: [], trace: trace([], ['first', 'provider timed out']) }),
    'L',
    'S',
  ],
  [
    'reject-embedding-failed-no-notes',
    out({ action: { kind: 'reject', reason: 'embedding-failed' }, memory: null, novelDetails: [], linkedIds: [], trace: trace() }),
    'L',
    'S',
  ],
  [
    'reject-empty',
    out({ action: { kind: 'reject', reason: 'empty' }, memory: null, novelDetails: [], linkedIds: [], trace: trace(NEIGHBOURS) }),
    'L',
    'S',
  ],
  [
    'non-bmp-id-prefix',
    out({
      action: { kind: 'insert' },
      memory: { id: 'x' },
      novelDetails: [],
      linkedIds: [],
      trace: trace([
        { id: '\u{1F600}\u{1F601}\u{1F602}\u{1F603}\u{1F604}\u{1F605}', cosine: 0.75, verdict: 'band' },
        { id: 'ab\u{1F600}\u{1F601}\u{1F602}xyz', cosine: 0.8, via: '\u{1F4A9}\u{1F4A9}abcdwxyz', verdict: 'band' },
        { id: 'short', cosine: 0.9, verdict: 'eligible' },
        { id: 'café-été-long', cosine: 0.85, verdict: 'eligible' },
      ]),
    }),
    'L',
    'S',
  ],
  [
    'excluded-without-reason',
    out({ action: { kind: 'insert' }, memory: null, novelDetails: [], linkedIds: [], trace: trace([{ id: 'odd', cosine: 0.9, verdict: 'excluded' }]) }),
    'L',
    'S',
  ],
  [
    'via-empty-string',
    out({ action: { kind: 'insert' }, memory: null, novelDetails: [], linkedIds: [], trace: trace([{ id: 'v', cosine: 0.75, via: '', verdict: 'band' }]) }),
    'L',
    'S',
  ],
  [
    'empty-label-summary',
    out({ action: { kind: 'insert' }, memory: null, novelDetails: [], linkedIds: [], trace: trace() }),
    '',
    '',
  ],
];
for (const [id, outcome, label, summary] of describeCases) {
  rows.push({ kind: 'describe', id, outcome, label, summary, out: describeAdmission(outcome, label, summary) });
}

for (const r of rows) process.stdout.write(JSON.stringify(r, enc) + '\n');
