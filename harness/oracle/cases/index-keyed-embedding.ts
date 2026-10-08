/**
 * P4.D258 tier-1 oracle — v4's REAL `decodeIndexKeyedEmbedding`
 * (`lib/backup/restore/index-keyed-embedding.ts`, new at `039f7017c`, bug 181
 * — this port's filing): the restore's decode of a pre-fix full backup's
 * memory embedding, `JSON.stringify(Float32Array)`'s index-keyed object
 * `{"0":v0,"1":v1,…}`, back into the `number[]` `MemorySchema` accepts.
 * Anything else is returned UNCHANGED (by identity) for the schema to accept
 * or refuse — `{}` included, which v5 used to decode to `[]` (converged,
 * P4.D258 R-A).
 *
 * One row per input: `{kind:'decode', id, input, changed, out?}` — `input` the
 * JSON value handed to the decoder (`absent: true` instead for `undefined`,
 * the shape a memory WITHOUT the key presents), `changed` whether v4 returned
 * a different value (`decoded !== input`, the restore's own test at
 * `restore.ts:268`), `out` the decoded array when it did. Inputs that cannot
 * cross JSON (a non-finite value, a live `Float32Array`, a null-prototype
 * object) are emitted as `{kind:'gap', id, reason, changed}` — v4's verdict
 * recorded, no v5 replay (a v5 archive is parsed JSON: none of those shapes
 * can reach the Rust decoder).
 *
 * Run from inside the v4 checkout (or a pinned worktree):
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/index-keyed-embedding.ts \
 *     > /tmp/oracle-index-keyed-embedding.ndjson
 */

import { decodeIndexKeyedEmbedding } from '@/lib/backup/restore/index-keyed-embedding';

const rows: unknown[] = [];

function decode(id: string, input: unknown): void {
  const out = decodeIndexKeyedEmbedding(input);
  const changed = out !== input;
  if (input === undefined) {
    rows.push({ kind: 'decode', id, absent: true, changed });
    return;
  }
  rows.push({ kind: 'decode', id, input, changed, ...(changed ? { out } : {}) });
}

function gap(id: string, reason: string, input: unknown): void {
  rows.push({ kind: 'gap', id, reason, changed: decodeIndexKeyedEmbedding(input) !== input });
}

/** `JSON.parse(JSON.stringify(Float32Array))` — the pre-fix archive's shape. */
function wire(values: number[]): unknown {
  return JSON.parse(JSON.stringify(new Float32Array(values)));
}

function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

// v4's jest (`index-keyed-embedding.test.ts`): the round trip, keys in any
// order, and the eleven unchanged inputs.
decode('jest_round_trip', wire([0.25, -0.5, 1]));
decode('jest_any_order', { '1': 2, '0': 1 });
decode('jest_null', null);
decode('jest_undefined', undefined);
decode('jest_number_array', [0.1, 0.2]);
decode('jest_string', '[0.1,0.2]');
decode('jest_empty_object', {});
decode('jest_gap', { '0': 1, '2': 3 });
decode('jest_non_canonical_key', { '00': 1 });
decode('jest_negative_key', { '-1': 1 });
decode('jest_non_numeric_key', { '0': 1, dims: 2 });
decode('jest_non_number_value', { '0': 1, '1': 'x' });
gap('jest_non_finite_value', 'NaN cannot cross JSON', { '0': Number.NaN });
gap('jest_float32array', 'a live Float32Array cannot cross JSON', new Float32Array([0.5]));

// Widths: the restore archive's 1-wide row, a 2-wide, the fixture's 4-wide.
decode('width_1_archive_row', { '0': 0.25 });
decode('width_2', wire([0.25, -0.5]));
decode('width_4_fixture', wire([0.25, -0.5, 0.125, 1]));
const rand = mulberry32(258);
const wide: number[] = [];
for (let i = 0; i < 1536; i++) wide.push(rand() * 2 - 1);
decode('width_1536_round_trip', wire(wide));

// Key order: integer-like keys, reversed past ten (JS orders them ascending in
// `Object.entries`; a JSON text may carry them in any order).
const reversed: Record<string, number> = {};
for (let i = 10; i >= 0; i--) reversed[String(i)] = i / 8;
decode('eleven_keys_reversed', reversed);

// More edges of the key and value rules.
decode('key_plus_sign', { '+0': 1 });
decode('key_leading_space', { ' 0': 1 });
decode('key_exponent', { '1e0': 1, '0': 2 });
decode('key_out_of_range', { '0': 1, '5': 2 });
decode('key_huge', { '0': 1, '99999999999999999999': 2 });
decode('value_true', { '0': true });
decode('value_null', { '0': null });
decode('value_numeric_string', { '0': '0.25' });
decode('value_nested_array', { '0': [0.25] });
decode('value_nested_object', { '0': {} });
decode('value_integer', { '0': 1, '1': 0 });
decode('value_extreme', { '0': 3.4028234663852886e38, '1': 1.401298464324817e-45, '2': -0.0 });
decode('a_number', 0.25);
decode('a_boolean', false);
decode('empty_array', []);
gap('null_prototype', 'JSON.parse never builds a null-prototype object', Object.assign(Object.create(null), { '0': 1 }));

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
