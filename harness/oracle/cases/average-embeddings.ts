/**
 * P4.D222 tier-1 oracle — v4's REAL `averageEmbeddings`
 * (`lib/embedding/embedding-service.ts`, new at `492771aff`, bug 168): the
 * unit-length mean of a help doc's section vectors, which becomes the doc's
 * own vector.
 *
 * `averageEmbeddings` sums into a `Float32Array` (every `sum[i] += v[i]` is an
 * f64 add rounded to f32 on the typed-array store) and then runs the REAL
 * `normalizeVector` over the sum. Every input and output vector is emitted as
 * the little-endian hex of its f32 bytes, so the Rust side starts from the
 * identical bits and compares BIT-EXACT (not 1e-12): `{kind:'average', id,
 * inputs:[hex…], out: hex|null}` or `{kind:'average', id, inputs, error}`.
 *
 * The random vectors come from a fixed mulberry32 stream, so the corpus is
 * the same on every regen.
 *
 * Run from inside the v4 checkout (or a pinned worktree):
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/average-embeddings.ts \
 *     > /tmp/oracle-average-embeddings.ndjson
 */

import { averageEmbeddings } from '@/lib/embedding/embedding-service';

const rows: unknown[] = [];

function hex(v: Float32Array): string {
  return Buffer.from(v.buffer, v.byteOffset, v.byteLength).toString('hex');
}

function fromBits(bits: number[]): Float32Array {
  return new Float32Array(new Uint32Array(bits).buffer);
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

function randomVector(rand: () => number, dims: number, scale = 1): Float32Array {
  const v = new Float32Array(dims);
  for (let i = 0; i < dims; i++) v[i] = (rand() * 2 - 1) * scale;
  return v;
}

function unit(v: Float32Array): Float32Array {
  let n = 0;
  for (let i = 0; i < v.length; i++) n += v[i] * v[i];
  const inv = 1 / Math.sqrt(n);
  const out = new Float32Array(v.length);
  for (let i = 0; i < v.length; i++) out[i] = v[i] * inv;
  return out;
}

function push(id: string, vectors: Float32Array[]): void {
  // `averageEmbeddings` never mutates its inputs (it sums into a fresh array),
  // but hex them first anyway so the emitted inputs are what went in.
  const inputs = vectors.map(hex);
  try {
    const out = averageEmbeddings(vectors);
    rows.push({ kind: 'average', id, inputs, out: out === null ? null : hex(out) });
  } catch (e) {
    rows.push({ kind: 'average', id, inputs, error: e instanceof Error ? e.message : String(e) });
  }
}

const f = (...xs: number[]) => new Float32Array(xs);

// ---- the edges ---------------------------------------------------------------
push('empty-set', []);
push('one-vector-unit', [f(1, 0)]);
push('one-vector-3-4', [f(3, 4)]);
push('one-vector-zero', [f(0, 0, 0)]);
push('two-zero-vectors', [f(0, 0), f(0, 0)]);
push('cancel-to-zero', [f(1, -2, 3), f(-1, 2, -3)]);
push('negative-zeros', [f(-0, -0), f(-0, -0)]);
push('zero-width-vectors', [f(), f()]);
push('v4-test-two-axes', [f(1, 0), f(0, 1)]);
push('v4-test-one-survivor', [f(0, 1)]);
push('three-identical', [f(0.6, 0.8), f(0.6, 0.8), f(0.6, 0.8)]);
push('opposite-halves', [f(1, 1), f(-1, 1)]);
push('single-dim', [f(2), f(-5), f(0.25)]);

// ---- f32 accumulation vs a naive f64 sum ---------------------------------------
// `1e8 + 1 + 1` in f32 loses each `+1` (the f32 ulp at 1e8 is 8); in f64 the
// sum would be exactly 100000002. The typed-array store is what v4 does.
push('f32-absorbs-small-adds', [f(1e8, 1), f(1, 1), f(1, 1)]);
push('f32-absorbs-many', Array.from({ length: 20 }, (_, i) => (i === 0 ? f(16777216, 0.5) : f(1, 0.5))));
push('f32-rounding-ties', [f(16777216, 3), f(1, 1), f(1, 1), f(1, 1)]);
push('fractional-sum', [f(0.1, 0.2, 0.3), f(0.7, 0.8, 0.9), f(1e-3, 1e-4, 1e-5)]);
push('mixed-magnitudes', [f(1e30, 1e-30, 1), f(-1e30, 1e-30, 1), f(1, 1, 1)]);
push('overflow-to-infinity', [f(3e38, 1), f(3e38, 1)]);
push('subnormal-inputs', [fromBits([1, 2, 0x007fffff]), fromBits([1, 0, 0x00000001])]);
push('subnormal-only-sum', [fromBits([0x00000003]), fromBits([0x00000005])]);
push('tiny-norm-underflow', [f(1e-30, 1e-30), f(1e-30, 1e-30)]);
push('max-finite', [fromBits([0x7f7fffff, 0]), fromBits([0, 0x7f7fffff])]);

// ---- the dimension refusal (unreachable from the job, which pre-filters) -------
push('differing-dims-second', [f(1, 0), f(1, 0, 0)]);
push('differing-dims-third', [f(1, 0), f(0, 1), f(1)]);
push('differing-dims-empty-first', [f(), f(1)]);

// ---- random vectors -------------------------------------------------------------
const rand = mulberry32(0x5eed2222);
for (const [dims, count] of [
  [8, 2],
  [8, 5],
  [64, 3],
  [384, 4],
  [768, 7],
  [1024, 2],
  [1536, 1],
  [1536, 3],
  [1536, 12],
  [3072, 2],
  [3072, 6],
] as const) {
  push(`random-${dims}x${count}`, Array.from({ length: count }, () => randomVector(rand, dims)));
}
// Unit inputs, as the job's section vectors really are.
for (const [dims, count] of [
  [1536, 4],
  [3072, 3],
  [768, 20],
] as const) {
  push(`random-unit-${dims}x${count}`, Array.from({ length: count }, () => unit(randomVector(rand, dims))));
}
// Wide magnitude spread in one set.
push('random-scaled-64x4', [
  randomVector(rand, 64, 1e-6),
  randomVector(rand, 64, 1),
  randomVector(rand, 64, 1e6),
  randomVector(rand, 64, 1e-20),
]);

for (const row of rows) process.stdout.write(JSON.stringify(row) + '\n');
