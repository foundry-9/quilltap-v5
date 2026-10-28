/**
 * Tier-1 ORACLE for v4's memory candidate (P4.D289, pin `83334b9a5`) —
 * `lib/memory/candidate.ts` (`e0ac7493a`): `MemoryCandidateSchema.safeParse`
 * (the parsed data, or zod's issue list verbatim), `candidateFromDraft`,
 * `toAdmissionRow`, `normaliseTargeting`, `targetingKeywords`,
 * `splitTargetingKeywords`, `coerceImportance`, `anchorTimeMsFrom`, and the
 * constants. Mirrors `__tests__/unit/lib/memory/memory-candidate.test.ts`'s
 * pure rows and adds every issue kind the schema can raise.
 *
 * Drives v4's REAL module, imported by FILE path:
 *   lib/memory/candidate.ts
 *
 * Row kinds: `const`, `normalise`, `tkw`, `split`, `coerce`, `anchor`,
 * `draft`, `parse`, `parse_nonfinite`. A key the JS input lacks is ABSENT
 * from the row (JSON has no `undefined`). Non-finite numbers ride as strings
 * (`"NaN"`, `"Infinity"`, `"-Infinity"`) since JSON has no spelling for them;
 * each row that carries one says so (`nonFinite: true`).
 *
 * Run from inside a v4 checkout (or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/memory-candidate.ts" > /tmp/oracle-memory-candidate.ndjson
 */

import {
  MemoryCandidateSchema,
  candidateFromDraft,
  toAdmissionRow,
  normaliseTargeting,
  splitTargetingKeywords,
  targetingKeywords,
  coerceImportance,
  anchorTimeMsFrom,
  DEFAULT_TARGETING,
  MEMORY_ENTITY_CAP,
  type MemoryCandidate,
  type ResolvedCandidateTime,
} from '@/lib/memory/candidate';

const emit = (row: unknown) => process.stdout.write(JSON.stringify(row) + '\n');

/** JSON.stringify with non-finite numbers as their string tokens. */
const tok = (x: unknown): unknown =>
  JSON.parse(
    JSON.stringify(x, (_k, v) => (typeof v === 'number' && !Number.isFinite(v) ? String(v) : v)),
  );

emit({ kind: 'const', MEMORY_ENTITY_CAP, DEFAULT_TARGETING });

// ---- normaliseTargeting ---------------------------------------------------
const NORMALISE: Array<[string, unknown, Record<string, string> | undefined]> = [
  ['test row: PAST / " narrow " / gossip', { temporal: 'PAST', scope: ' narrow ', context: 'gossip' }, undefined],
  ['test row: null + fallback temporal', null, { temporal: 'past' }],
  ['" PAST "', { temporal: ' PAST ', scope: 'WIDE', context: 'Banter' }, undefined],
  ['unknown word everywhere', { temporal: 'yesterday', scope: 'broad', context: 'gossip' }, undefined],
  ['unknown words take the fallback axes', { temporal: 'later', scope: 'x', context: 'y' }, { temporal: 'future', scope: 'narrow', context: 'trivia' }],
  ['partial fallback', { temporal: 'nope' }, { scope: 'narrow' }],
  ['valid beats fallback', { temporal: 'moment', scope: 'narrow', context: 'history' }, { temporal: 'past', scope: 'wide', context: 'banter' }],
  ['non-string axes', { temporal: 1, scope: true, context: ['banter'] }, undefined],
  ['undefined raw', undefined, undefined],
  ['a string raw', 'past', undefined],
  ['an array raw', ['past'], undefined],
  ['empty object', {}, {}],
  ['tab/newline padding', { temporal: '\tfuture\n', scope: ' narrow ', context: '  philosophy' }, undefined],
  ['inner whitespace refused', { temporal: 'pa st', scope: 'scope: narrow', context: 'information ' }, undefined],
];
for (const [label, raw, fallback] of NORMALISE) {
  const out = fallback === undefined
    ? normaliseTargeting(raw as never)
    : normaliseTargeting(raw as never, fallback as never);
  emit({ kind: 'normalise', label, ...(raw === undefined ? {} : { raw }), ...(fallback === undefined ? {} : { fallback }), out });
}

// ---- targetingKeywords ----------------------------------------------------
for (const t of [
  { temporal: 'moment', scope: 'narrow', context: 'banter' },
  DEFAULT_TARGETING,
  { temporal: 'past', scope: 'wide', context: 'relationships' },
] as const) {
  emit({ kind: 'tkw', t, out: targetingKeywords(t as never) });
}

// ---- splitTargetingKeywords -----------------------------------------------
const SPLITS: Array<[string, unknown[]]> = [
  ['test row: Harbour + tags', ['Harbour', 'moment', 'scope: narrow', 'banter']],
  ['last of each axis wins', ['past', 'future', 'scope: wide', 'scope: narrow', 'banter', 'trivia', 'kept']],
  ['non-strings skipped', [1, null, 'Harbour', true, { a: 1 }, ['past'], 'present']],
  ['case + padding', ['  SCOPE: Narrow ', ' PAST', 'History ', '  Mixed Case  ']],
  ['unknown scope value dropped', ['scope: broad', 'scope:', 'scope:wide', 'free']],
  ['blank words dropped', ['', '   ', '\t', 'word']],
  ['scope with inner padding', ['scope:   narrow  ', 'scopes: wide']],
  ['empty', []],
];
for (const [label, keywords] of SPLITS) {
  emit({ kind: 'split', label, keywords, out: splitTargetingKeywords(keywords as never) });
}

// ---- coerceImportance -----------------------------------------------------
const COERCE: Array<[string, unknown]> = [
  ['-0.2', -0.2], ['0', 0], ['0.3', 0.3], ['0.73', 0.73], ['1', 1], ['1.7', 1.7], ['5', 5], ['-1', -1],
  ['NaN', NaN], ['Infinity', Infinity], ['-Infinity', -Infinity],
  ["'0.5'", '0.5'], ["'0.9'", '0.9'], ['undefined', undefined], ['null', null], ['true', true],
  ['[0.5]', [0.5]], ['{}', {}],
];
for (const [label, raw] of COERCE) {
  const nonFinite = typeof raw === 'number' && !Number.isFinite(raw);
  emit({ kind: 'coerce', label, ...(raw === undefined ? {} : { raw: tok(raw) }), ...(nonFinite ? { nonFinite } : {}), out: coerceImportance(raw) });
}

// ---- anchorTimeMsFrom -----------------------------------------------------
const ANCHORS: Array<[string, Array<string | null | undefined>, number]> = [
  ['test row: empty, garbage, then ISO', ['', 'not a date', '2026-10-12T19:00:00.000Z'], 7],
  ['test row: null/undefined/empty → fallback', [null, undefined, ''], 42],
  ['first parseable wins', ['2026-01-02T03:04:05.000Z', '2026-10-12T19:00:00.000Z'], 7],
  ['date-only', ['2026-10-12'], 7],
  ['offset form', ['2026-10-12T19:00:00+02:00'], 7],
  ['month 13 is not a date', ['2026-13-01T00:00:00.000Z', null], 99],
  ['empty list', [], 123456789],
];
for (const [label, isos, fallback] of ANCHORS) {
  emit({ kind: 'anchor', label, isos: isos.map(x => (x === undefined ? null : x)), fallback, out: anchorTimeMsFrom(isos, fallback) });
}

// ---- candidateFromDraft + toAdmissionRow ----------------------------------
const ANCHOR_MS = Date.parse('2026-10-12T19:00:00.000Z');
type Prov = MemoryCandidate['provenance'];
const prov = (over: Partial<Prov> = {}): Prov => ({
  source: 'AUTO',
  witnessedContext: 'user_present',
  chatId: 'chat-1',
  projectId: null,
  sourceMessageId: 'msg-1',
  anchorTimeMs: ANCHOR_MS,
  pass: 'self-turn',
  ...over,
});
const SUBJ = { holderId: 'char-holder', aboutId: null as string | null };
const RESOLVED: ResolvedCandidateTime = { occurredAt: '2026-10-12T19:00:00.000Z', narrativeTime: null };
const ENT13 = Array.from({ length: 13 }, (_, i) => `E${i}`);

const DRAFTS: Array<[string, Record<string, unknown>, typeof SUBJ, Prov, ResolvedCandidateTime]> = [
  ['test row: blank draft', { content: '   ' }, SUBJ, prov(), RESOLVED],
  ['test row: blank content, summary kept', { content: '  ', summary: 'kept' }, SUBJ, prov(), RESOLVED],
  ['content untrimmed kept', { content: '  Body text.  ', summary: ' S ' }, SUBJ, prov(), RESOLVED],
  ['blank summary falls back to the untrimmed content', { content: ' Body ', summary: '   ' }, SUBJ, prov(), RESOLVED],
  ['no content, summary only', { summary: 'only summary' }, SUBJ, prov(), RESOLVED],
  ['whitespace-only both → null', { content: ' \t ', summary: '\n' }, SUBJ, prov(), RESOLVED],
  ['neither field → null', { keywords: ['k'] }, SUBJ, prov(), RESOLVED],
  ['importance -0.2', { content: 'c', importance: -0.2 }, SUBJ, prov(), RESOLVED],
  ['importance 0 stays 0', { content: 'c', importance: 0 }, SUBJ, prov(), RESOLVED],
  ['importance 0.3', { content: 'c', importance: 0.3 }, SUBJ, prov(), RESOLVED],
  ['importance 1.7', { content: 'c', importance: 1.7 }, SUBJ, prov(), RESOLVED],
  ['importance NaN', { content: 'c', importance: NaN }, SUBJ, prov(), RESOLVED],
  ["importance '0.5'", { content: 'c', importance: '0.5' }, SUBJ, prov(), RESOLVED],
  ['importance undefined', { content: 'c' }, SUBJ, prov(), RESOLVED],
  ['entities 13 → 12', { content: 'c', entities: ENT13 }, SUBJ, prov(), RESOLVED],
  ['entities 12 kept', { content: 'c', entities: ENT13.slice(0, 12) }, SUBJ, prov(), RESOLVED],
  ['a non-string entity → null', { content: 'c', entities: ['A', 2] }, SUBJ, prov(), RESOLVED],
  ['a non-string entity past the cap is sliced away', { content: 'c', entities: [...ENT13.slice(0, 12), 99] }, SUBJ, prov(), RESOLVED],
  ['a non-string keyword → null', { content: 'c', keywords: ['a', 1] }, SUBJ, prov(), RESOLVED],
  ['kind episodic', { content: 'c', kind: 'episodic', when: 'last night' }, SUBJ, prov(), RESOLVED],
  ['kind EPISODIC → semantic', { content: 'c', kind: 'EPISODIC' }, SUBJ, prov(), RESOLVED],
  ['kind semantic', { content: 'c', kind: 'semantic' }, SUBJ, prov(), RESOLVED],
  ['kind bogus → semantic', { content: 'c', kind: 'procedural' }, SUBJ, prov(), RESOLVED],
  ['targeting normalised', { content: 'c', targeting: { temporal: ' PAST ', scope: 'Narrow', context: 'gossip' } }, SUBJ, prov(), RESOLVED],
  ['targeting valid', { content: 'c', keywords: ['dark'], targeting: { temporal: 'present', scope: 'wide', context: 'relationships' } }, SUBJ, prov(), RESOLVED],
  ['anchorTimeMs Infinity → null', { content: 'c' }, SUBJ, prov({ anchorTimeMs: Infinity }), RESOLVED],
  ['anchorTimeMs NaN → null', { content: 'c' }, SUBJ, prov({ anchorTimeMs: NaN }), RESOLVED],
  ['anchorTimeMs -Infinity → null', { content: 'c' }, SUBJ, prov({ anchorTimeMs: -Infinity }), RESOLVED],
  ['empty holderId → null', { content: 'c' }, { holderId: '', aboutId: null }, prov(), RESOLVED],
  ['empty aboutId → null', { content: 'c' }, { holderId: 'h', aboutId: '' }, prov(), RESOLVED],
  ['empty chatId → null', { content: 'c' }, SUBJ, prov({ chatId: '' }), RESOLVED],
  ['empty sourceMessageId → null', { content: 'c' }, SUBJ, prov({ sourceMessageId: '' }), RESOLVED],
  ['test row: other-turn, model 0', { content: 'Amy is afraid of the dark.', summary: 'Amy fears the dark', keywords: ['dark'], importance: 0, targeting: { temporal: 'present', scope: 'wide', context: 'relationships' }, kind: 'semantic' }, { holderId: 'char-holder', aboutId: 'char-other' }, prov({ pass: 'other-turn', projectId: 'proj-1' }), { occurredAt: null, narrativeTime: null, sourceMessageTimestamp: '2026-10-12T19:00:00.000Z' }],
  ['sourceMessageId null → undefined on the row', { content: 'c', keywords: ['k1', 'k2'] }, SUBJ, prov({ sourceMessageId: null, witnessedContext: null, chatId: null }), { occurredAt: null, narrativeTime: 'the third bell' }],
  ['episode-fold + MANUAL + autonomous_room', { content: 'c' }, SUBJ, prov({ pass: 'episode-fold', source: 'MANUAL', witnessedContext: 'autonomous_room' }), RESOLVED],
  ['carina + consolidated', { content: 'c' }, SUBJ, prov({ pass: 'carina', source: 'CONSOLIDATED', witnessedContext: 'manual' }), RESOLVED],
  ['other-fold', { content: 'c', when: 'yesterday', entities: [] }, SUBJ, prov({ pass: 'other-fold' }), RESOLVED],
];
for (const [label, draft, subject, provenance, resolved] of DRAFTS) {
  const out = candidateFromDraft(draft as never, subject, provenance);
  const nonFinite = !Number.isFinite(provenance.anchorTimeMs) || (typeof draft.importance === 'number' && !Number.isFinite(draft.importance));
  emit({
    kind: 'draft',
    label,
    draft: tok(draft),
    subject,
    provenance: tok(provenance),
    resolved,
    ...(nonFinite ? { nonFinite } : {}),
    out,
    row: out ? toAdmissionRow(out, resolved) : null,
  });
}

// ---- MemoryCandidateSchema.safeParse --------------------------------------
const BASE = {
  content: 'Holly prefers tea.',
  summary: 'prefers tea',
  keywords: ['tea'],
  targeting: null,
  importance: 0.6,
  kind: 'semantic',
  when: null,
  subject: { holderId: 'char-holder', aboutId: null },
  provenance: prov({ pass: 'manual', source: 'MANUAL', witnessedContext: 'manual' }),
};
const P = BASE.provenance;
const PARSES: Array<[string, unknown, ResolvedCandidateTime?]> = [
  ['manual: targeting null', BASE, { occurredAt: null, narrativeTime: null }],
  ['fold episode', {
    content: 'They went down to the cellar and found the crates.', summary: 'the cellar expedition',
    keywords: ['cellar', 'crates'], targeting: { temporal: 'past', scope: 'narrow', context: 'history' },
    importance: 0.7, kind: 'episodic', when: 'last night', entities: ['Cellar', 'Crates'],
    subject: { holderId: 'char-holder', aboutId: null }, provenance: prov({ pass: 'episode-fold' }),
  }, { occurredAt: '2026-10-11T22:00:00.000Z', narrativeTime: null, sourceMessageTimestamp: '2026-10-12T19:00:00.000Z' }],
  ['consolidation digest', { ...BASE, keywords: ['harbour'], targeting: { temporal: 'present', scope: 'narrow', context: 'banter' }, importance: 0.8, subject: { holderId: 'char-holder', aboutId: 'char-other' }, provenance: prov({ pass: 'consolidation', source: 'CONSOLIDATED' }) }, { occurredAt: null, narrativeTime: null }],
  ['import pass', { ...BASE, provenance: { ...P, pass: 'import' } }, { occurredAt: null, narrativeTime: null }],
  ['importance 0 / 1 bounds', { ...BASE, importance: 0 }, { occurredAt: null, narrativeTime: null }],
  ['importance 1', { ...BASE, importance: 1, tags: ['t-1', 't-2'], entities: ENT13.slice(0, 12) }, { occurredAt: null, narrativeTime: null }],
  ['unknown keys stripped', { ...BASE, extra: 1, subject: { holderId: 'h', aboutId: 'a', z: 1 }, provenance: { ...P, more: true }, targeting: { temporal: 'past', scope: 'wide', context: 'trivia', x: 1 } }, { occurredAt: null, narrativeTime: null }],
  ['integral anchor + whitespace text', { ...BASE, content: ' ', summary: '\t', provenance: { ...P, anchorTimeMs: 0 } }, { occurredAt: null, narrativeTime: null }],
  ['fractional anchor', { ...BASE, provenance: { ...P, anchorTimeMs: 1.5 } }, { occurredAt: null, narrativeTime: null }],
  // ---- refusals --------------------------------------------------------
  ['null input', null],
  ['array input', []],
  ['string input', 'candidate'],
  ['empty object', {}],
  ['test row: empty content', { ...BASE, content: '' }],
  ['test row: importance 1.2', { ...BASE, importance: 1.2 }],
  ['importance -0.5', { ...BASE, importance: -0.5 }],
  ["importance '0.5'", { ...BASE, importance: '0.5' }],
  ['importance null', { ...BASE, importance: null }],
  ['empty summary + wrong-type content', { ...BASE, content: 5, summary: '' }],
  ['13 entities', { ...BASE, entities: ENT13 }],
  ['13 non-string entities', { ...BASE, entities: Array(13).fill(1) }],
  // P4.D311 — zod 4.6.7's `capSize`: an array past its `.max(MEMORY_ENTITY_CAP)`
  // fails with ONLY the array's `too_big`, before any element is parsed (4.6.5
  // reported the element issues first). A non-string at [0] is the falsifier;
  // an empty string at [0] is valid here (`z.string()`, no `.min(1)`).
  ['13 entities, a non-string first', { ...BASE, entities: [7, ...ENT13.slice(1)] }],
  ['13 entities, an empty string first', { ...BASE, entities: ['', ...ENT13.slice(1)] }],
  ['keywords null', { ...BASE, keywords: null }],
  ['keywords a string', { ...BASE, keywords: 'tea' }],
  ['mixed keywords + tags', { ...BASE, keywords: ['a', 1, null], tags: [true] }],
  ['entities null', { ...BASE, entities: null }],
  // P4.169 (F-2, `ZOD_STRING_LENGTH_PAST_INVALID_TYPE`): zod 4.6.7 runs the
  // array's `.max` on any input with a `.length` even after `invalid_type` —
  // a string past the cap adds a string-origin `too_big`; one at the cap, an
  // object carrying a numeric `length`, and a cap-less field (`keywords`) do
  // not / do as measured.
  ['entities a 13-character string', { ...BASE, entities: 'abcdefghijklm' }],
  ['entities a 12-character string', { ...BASE, entities: 'abcdefghijkl' }],
  ['entities an astral 13-code-point string', { ...BASE, entities: '\u{1F600}'.repeat(13) }],
  ['entities an object with length 13', { ...BASE, entities: { length: 13 } }],
  ['keywords a long string (no cap)', { ...BASE, keywords: 'x'.repeat(40) }],
  ['targeting missing', (() => { const { targeting: _t, ...rest } = BASE; return rest; })()],
  ['targeting an array', { ...BASE, targeting: [] }],
  ['targeting bad axes', { ...BASE, targeting: { temporal: 'x', scope: 1 } }],
  ['targeting upper-case (no normalising in the schema)', { ...BASE, targeting: { temporal: 'PAST', scope: 'wide', context: 'history' } }],
  ['kind bad', { ...BASE, kind: 'EPISODIC' }],
  ['kind missing', (() => { const { kind: _k, ...rest } = BASE; return rest; })()],
  ['when missing', (() => { const { when: _w, ...rest } = BASE; return rest; })()],
  ['when a number', { ...BASE, when: 5 }],
  ['null subject', { ...BASE, subject: null }],
  ['subject an array', { ...BASE, subject: ['a'] }],
  ['empty holderId', { ...BASE, subject: { holderId: '', aboutId: null } }],
  ['empty aboutId', { ...BASE, subject: { holderId: 'h', aboutId: '' } }],
  ['subject keys missing', { ...BASE, subject: {} }],
  ['holderId a number', { ...BASE, subject: { holderId: 7, aboutId: 8 } }],
  ['provenance missing', (() => { const { provenance: _p, ...rest } = BASE; return rest; })()],
  ['provenance a string', { ...BASE, provenance: 'x' }],
  ['provenance empty', { ...BASE, provenance: {} }],
  ['unknown pass', { ...BASE, provenance: { ...P, pass: 'dream' } }],
  ['bad source (lower-case)', { ...BASE, provenance: { ...P, source: 'manual' } }],
  ['bad witnessedContext', { ...BASE, provenance: { ...P, witnessedContext: 'USER_PRESENT' } }],
  ['empty provenance ids', { ...BASE, provenance: { ...P, chatId: '', projectId: '', sourceMessageId: '' } }],
  ['anchorTimeMs a string', { ...BASE, provenance: { ...P, anchorTimeMs: '2026-10-12' } }],
  ['anchorTimeMs null', { ...BASE, provenance: { ...P, anchorTimeMs: null } }],
  ['everything wrong at once', {
    content: '', summary: null, keywords: [2], targeting: 'past', importance: 3, kind: null, when: false,
    entities: 'E', tags: {}, subject: { holderId: null }, provenance: { source: 'AUTO', witnessedContext: null, chatId: 1, projectId: null, sourceMessageId: null, anchorTimeMs: true, pass: null },
  }],
];
for (const [label, input, resolved] of PARSES) {
  const r = MemoryCandidateSchema.safeParse(input);
  emit(r.success
    ? { kind: 'parse', label, input, success: true, data: r.data, resolved, row: toAdmissionRow(r.data, resolved!) }
    : { kind: 'parse', label, input, success: false, issues: r.error.issues });
}

// ---- non-finite numbers the schema refuses (JSON cannot carry them) --------
// Zod 4 answers `{expected, code, received: "<NaN|Infinity>", path, message}`
// — a key no other issue in the slice carries. Recorded so a reader sees the
// shape; v5's `Value`-typed twin cannot receive the input (the typed draft
// path refuses a non-finite anchor — the `draft` rows above).
for (const [label, input] of [
  ['anchorTimeMs Infinity', { ...BASE, provenance: { ...P, anchorTimeMs: Infinity } }],
  ['anchorTimeMs NaN', { ...BASE, provenance: { ...P, anchorTimeMs: NaN } }],
  ['importance NaN', { ...BASE, importance: NaN }],
] as const) {
  const r = MemoryCandidateSchema.safeParse(input);
  emit({ kind: 'parse_nonfinite', label, input: tok(input), nonFinite: true, success: r.success, issues: r.success ? [] : r.error.issues });
}
