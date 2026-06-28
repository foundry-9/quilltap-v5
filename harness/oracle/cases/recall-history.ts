/**
 * Oracle case #4: Commonplace Book recall anti-repetition ring buffer.
 *
 * Drives the REAL `parseRecallHistory`, `recentlyWhisperedIdSet`, and
 * `appendRecallTurn` from lib/memory/recall-history.ts over a fixed corpus of
 * arbitrary / deliberately-malformed JSON inputs and emits NDJSON. This is the
 * producer side of the `recentlyWhisperedIds` set that recall-tags.ts consumes.
 *
 * The module is pure + I/O-free (no DB, no logging) — its only job is to coerce
 * a `chats.commonplaceRecallHistory` JSON column (typed `unknown`) into a clean
 * `string[][]`, union it, and append+trim a ring buffer of the last
 * RECALL_HISTORY_TURNS whisper-turns.
 *
 * Because the inputs are arbitrary JSON (the whole point is the coercion of
 * malformed data), each row carries BOTH the input `raw` and the expected
 * output: the Rust differential test feeds the SAME `raw` (parsed to
 * serde_json::Value) through the port and compares. This avoids transcribing
 * subtle malformed inputs twice and guarantees both sides see identical bytes.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/recall-history.ts \
 *     > /tmp/oracle-recall-history.ndjson
 */

import {
  parseRecallHistory,
  recentlyWhisperedIdSet,
  appendRecallTurn,
} from '@/lib/memory/recall-history';

// Row kinds — Rust dispatches on `kind`. Each carries the input `raw` verbatim.
type Row =
  | { kind: 'parse'; id: string; raw: unknown; out: string[][] }
  | { kind: 'set'; id: string; raw: unknown; out: string[] }
  | { kind: 'append'; id: string; raw: unknown; newIds: string[]; out: { turns: string[][] } };

const rows: Row[] = [];

// ---------------------------------------------------------------------------
// parseRecallHistory — coercion of arbitrary/malformed JSON to string[][].
// ---------------------------------------------------------------------------
const parseCases: Array<[string, unknown]> = [
  ['null', null],
  ['number', 42],
  ['string', 'hi'],
  ['array-not-object', ['a']], // an array has no 'turns' key → []
  ['object-no-turns', { foo: 1 }],
  ['turns-null', { turns: null }],
  ['turns-string', { turns: 'x' }],
  ['turns-empty', { turns: [] }],
  ['simple', { turns: [['a', 'b'], ['c']] }],
  // drops non-string entries, empty strings, and a whole non-array "turn"
  ['drop-nonstring', { turns: [['a', 1, '', 'b'], 'notarray', ['c', null, true]] }],
  ['empty-inner-preserved', { turns: [[], ['a']] }], // empty inner array survives
  ['inner-all-dropped', { turns: [[1, 2, '']] }], // inner filters to [] but stays
  ['four-turns-no-trim', { turns: [['a'], ['b'], ['c'], ['d']] }], // parse does NOT trim
  ['nested-arrays-as-nonstring', { turns: [[['x']]] }], // inner element is an array → dropped
];
for (const [id, raw] of parseCases) {
  rows.push({ kind: 'parse', id, raw, out: parseRecallHistory(raw) });
}

// ---------------------------------------------------------------------------
// recentlyWhisperedIdSet — union (dedup) across retained turns. Emitted as the
// insertion-ordered [...set]; the Rust port returns a HashSet, so the test
// compares set membership (order-independent).
// ---------------------------------------------------------------------------
const setCases: Array<[string, unknown]> = [
  ['empty', { turns: [] }],
  ['dedup-across-turns', { turns: [['a', 'b'], ['b', 'c']] }],
  ['dedup-within', { turns: [['a', 'a', 'b']] }],
  ['with-malformed', { turns: [['a', 1, ''], ['b']] }],
  ['null', null],
];
for (const [id, raw] of setCases) {
  rows.push({ kind: 'set', id, raw, out: [...recentlyWhisperedIdSet(raw)] });
}

// ---------------------------------------------------------------------------
// appendRecallTurn — dedup newIds (no empty-string filter), push, trim last N.
// Empty newIds → no-op (returns parsed turns UNCHANGED, not trimmed).
// ---------------------------------------------------------------------------
const appendCases: Array<[string, unknown, string[]]> = [
  ['append-to-empty', null, ['a', 'b']],
  ['dedup-newids', { turns: [] }, ['a', 'a', 'b']],
  ['empty-newids-noop', { turns: [['a'], ['b']] }, []],
  ['empty-newids-no-trim', { turns: [['a'], ['b'], ['c'], ['d']] }, []], // over-cap, untrimmed
  ['trim-to-three', { turns: [['a'], ['b'], ['c']] }, ['d']], // oldest drops out
  ['newids-empty-string-kept', { turns: [] }, ['', 'a', 'a']], // append does NOT filter ''
  ['append-parses-malformed-raw', { turns: [['a', 1], 'x', ['b']] }, ['c']],
];
for (const [id, raw, newIds] of appendCases) {
  rows.push({ kind: 'append', id, raw, newIds, out: appendRecallTurn(raw, newIds) });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
