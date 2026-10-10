/**
 * Tier-1 oracle — v4's consolidation clusterer (P4.D271, v4 `d58548051`,
 * byte-identical at the `01a83539d` pin), driven through v4's REAL exports of
 * `lib/memory/consolidation-clustering.ts`: `cosine`, `clusterBucket`,
 * `selectClusters`, `effectiveMinClusterSize`, `clusterHasNewMaterial`.
 *
 * Every input is emitted WITH its output (embeddings as the Float32Array's
 * values, each f32 promoted to a JSON double exactly), so the Rust side
 * replays v4's exact inputs rather than regenerating them.
 *
 * Corpus: seeded unit vectors at dims 3 / 8 / 16 / 384; engineered pairs at
 * the 0.72 threshold ± 1 f32 ulp; digest seeds whose attraction is capped by
 * `maxClusterSize` 2 / 3 / 30 and two digests that never merge; episodic
 * windows incl. null times and a span that only fits pairwise; mismatched
 * dims (-1); exact ties at every strict `>`; budgets 0 / 1 / 40 (+ NaN);
 * stale / belowMin / the 2×maturity floor; an empty-member cluster.
 *
 * Output: one NDJSON line per case `{case, …}`.
 *
 * Run (Node 24, from the v4 checkout or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   rm -f /tmp/qt-consolidation-clustering.ndjson
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/consolidation-clustering.ts > /tmp/qt-consolidation-clustering.ndjson
 */

import {
  clusterBucket,
  clusterHasNewMaterial,
  cosine,
  effectiveMinClusterSize,
  selectClusters,
  type Cluster,
  type ClusterItem,
  type ClusterParams,
} from '@/lib/memory/consolidation-clustering';

const DAY = 86_400_000;
const NOW = Date.parse('2026-10-08T12:00:00.000Z');

// Deterministic PRNG (mulberry32).
function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function unit(v: number[]): Float32Array {
  const n = Math.sqrt(v.reduce((s, x) => s + x * x, 0)) || 1;
  return new Float32Array(v.map((x) => x / n));
}

/** A random unit vector near `base` (noise scaled by `spread`). */
function near(base: number[], spread: number, r: () => number): Float32Array {
  return unit(base.map((x) => x + (r() - 0.5) * spread));
}

function randomDir(dim: number, r: () => number): number[] {
  return Array.from({ length: dim }, () => r() - 0.5);
}

function at(deg: number): Float32Array {
  const rad = (deg * Math.PI) / 180;
  return new Float32Array([Math.cos(rad), Math.sin(rad), 0]);
}

function item(id: string, embedding: Float32Array, o: Partial<ClusterItem> = {}): ClusterItem {
  return {
    id,
    embedding,
    kind: 'semantic',
    isDigest: false,
    eventTimeMs: NOW - 10 * DAY,
    createdAtMs: NOW - 10 * DAY,
    consideredAtMs: null,
    weight: 0.5,
    ...o,
  };
}

function itemJson(i: ClusterItem) {
  return {
    id: i.id,
    embedding: Array.from(i.embedding),
    kind: i.kind,
    isDigest: i.isDigest,
    eventTimeMs: i.eventTimeMs,
    createdAtMs: i.createdAtMs,
    consideredAtMs: i.consideredAtMs,
    weight: i.weight,
  };
}

function emit(obj: unknown): void {
  process.stdout.write(JSON.stringify(obj) + '\n');
}

function bucketCase(name: string, items: ClusterItem[], params: ClusterParams): void {
  const clusters = clusterBucket(items, params);
  emit({
    case: 'clusterBucket',
    name,
    items: items.map(itemJson),
    params: {
      threshold: params.threshold,
      maxClusterSize: params.maxClusterSize,
      episodicWindowMs: params.episodicWindowMs ?? null,
    },
    clusters,
  });
}

function main(): void {
  // ── cosine ──────────────────────────────────────────────────────────────
  const cosPairs: Array<[Float32Array, Float32Array]> = [
    [at(0), at(10)],
    [new Float32Array([1, 0]), new Float32Array([1, 0, 0])],
    [new Float32Array([]), new Float32Array([])],
    [unit([1, 2, 3]), unit([3, 2, 1])],
    [new Float32Array([0.1, 0.2, 0.3]), new Float32Array([0.7, 0.8, 0.9])],
  ];
  {
    const r = rng(7);
    for (const dim of [8, 16, 384]) cosPairs.push([unit(randomDir(dim, r)), unit(randomDir(dim, r))]);
  }
  for (const [a, b] of cosPairs) {
    emit({ case: 'cosine', a: Array.from(a), b: Array.from(b), out: cosine(a, b) });
  }

  // ── clusterBucket: the v4 jest shapes ───────────────────────────────────
  const P = (threshold: number, maxClusterSize: number, episodicWindowMs?: number): ClusterParams =>
    episodicWindowMs === undefined ? { threshold, maxClusterSize } : { threshold, maxClusterSize, episodicWindowMs };

  bucketCase('threshold-basic', [item('a', at(0)), item('b', at(10)), item('c', at(90))], P(0.9, 30));
  bucketCase('average-linkage', [item('a', at(0)), item('b', at(20)), item('c', at(40))], P(0.9, 30));
  bucketCase('chain-five', [0, 5, 10, 15, 20].map((d, i) => item(`r${i}`, at(d))), P(0.9, 30));
  bucketCase('max-size-2', [0, 1, 2, 3, 4].map((d, i) => item(`r${i}`, at(d))), P(0.9, 2));
  bucketCase('max-size-1', [0, 1, 2].map((d, i) => item(`r${i}`, at(d))), P(0.9, 1));
  bucketCase('max-size-0', [0, 1].map((d, i) => item(`r${i}`, at(d))), P(0.9, 0));

  // Mixed kinds never share a cluster.
  bucketCase(
    'kinds-split',
    [
      item('s1', at(0)),
      item('e1', at(1), { kind: 'episodic' }),
      item('s2', at(2)),
      item('e2', at(3), { kind: 'episodic' }),
    ],
    P(0.9, 30),
  );

  // Digest attraction capped by maxClusterSize 2 / 3 / 30.
  for (const cap of [2, 3, 30]) {
    bucketCase(
      `digest-cap-${cap}`,
      [
        item('d1', at(0), { isDigest: true, weight: 0.9 }),
        item('a', at(1)),
        item('b', at(2)),
        item('c', at(3)),
        item('e', at(-2)),
        item('far', at(80)),
      ],
      P(0.9, cap),
    );
  }
  // Two digests never merge; rows go to the nearer.
  bucketCase(
    'two-digests',
    [
      item('d1', at(0), { isDigest: true }),
      item('d2', at(3), { isDigest: true }),
      item('a', at(1)),
      item('b', at(2.6)),
      item('c', at(50)),
      item('c2', at(52)),
    ],
    P(0.9, 30),
  );
  // A row whose best neighbour is another ROW (not the digest) is not attracted.
  bucketCase(
    'best-is-row',
    [item('d1', at(0), { isDigest: true }), item('a', at(10)), item('b', at(10.5))],
    P(0.9, 30),
  );
  // A lone digest with no attraction is omitted.
  bucketCase('lone-digest', [item('d1', at(0), { isDigest: true }), item('a', at(60))], P(0.9, 30));

  // Episodic windows.
  const ep = (id: string, deg: number, t: number | null, o: Partial<ClusterItem> = {}) =>
    item(id, at(deg), { kind: 'episodic', eventTimeMs: t, ...o });
  bucketCase(
    'episodic-window',
    [ep('a', 0, NOW - 10 * DAY), ep('b', 1, NOW - 10 * DAY + 3600_000), ep('c', 2, NOW - 8 * DAY)],
    P(0.9, 30),
  );
  bucketCase(
    'episodic-null-times',
    [ep('a', 0, null), ep('b', 1, NOW - 3 * DAY), ep('c', 2, NOW - 30 * DAY)],
    P(0.9, 30),
  );
  // Pairwise fits (a–b, b–c within a day) but the span a–c does not.
  bucketCase(
    'episodic-pairwise-span',
    [ep('a', 0, NOW - 10 * DAY), ep('b', 1, NOW - 10 * DAY + 0.8 * DAY), ep('c', 2, NOW - 10 * DAY + 1.6 * DAY)],
    P(0.9, 30),
  );
  bucketCase(
    'episodic-exact-window',
    [ep('a', 0, NOW - 10 * DAY), ep('b', 1, NOW - 9 * DAY), ep('c', 2, NOW - 9 * DAY + 1)],
    P(0.9, 30),
  );
  bucketCase(
    'episodic-digest-span',
    [
      ep('d', 0, NOW - 10 * DAY, { isDigest: true }),
      ep('a', 1, NOW - 10 * DAY + 0.6 * DAY),
      ep('b', 1.5, NOW - 10 * DAY - 0.6 * DAY),
      ep('c', 0.5, null),
    ],
    P(0.9, 30),
  );
  bucketCase(
    'episodic-custom-window',
    [ep('a', 0, 1000), ep('b', 1, 1500), ep('c', 2, 2100)],
    P(0.9, 30, 1000),
  );

  // Mismatched dims never cluster.
  bucketCase(
    'dim-mismatch',
    [item('a', new Float32Array([1, 0, 0])), item('b', new Float32Array([1, 0])), item('c', new Float32Array([1, 0, 0]))],
    P(0.5, 30),
  );
  bucketCase('zero-dim', [item('a', new Float32Array([])), item('b', new Float32Array([]))], P(-2, 30));

  // Threshold ± 1 f32 ulp. b's x component IS the dot with a = [1, 0].
  {
    const f = Math.fround(0.72);
    const buf = new Float32Array(1);
    const ints = new Uint32Array(buf.buffer);
    buf[0] = f;
    ints[0] -= 1;
    const below = buf[0];
    buf[0] = f;
    ints[0] += 1;
    const above = buf[0];
    for (const [label, x] of [
      ['at', f],
      ['below', below],
      ['above', above],
    ] as const) {
      const b = new Float32Array([x, Math.sqrt(1 - x * x)]);
      bucketCase(`ulp-${label}`, [item('a', new Float32Array([1, 0])), item('b', b)], P(0.72, 30));
      bucketCase(
        `ulp-${label}-digest`,
        [item('d', new Float32Array([1, 0]), { isDigest: true }), item('b', b)],
        P(0.72, 30),
      );
    }
  }

  // R-H's storage rounding made observable: a pair whose f64 dot sits just
  // BELOW 0.72 but rounds (as v4 stores it, into the Float32Array) UP to
  // fround(0.72) >= 0.72 — v4 clusters it; an f64-stored similarity would not.
  {
    const a = unit([0.37, 0.91]);
    const lo = 0.7199999988;
    let found: Float32Array | null = null;
    // b near the angle whose cosine with a is 0.72; walk both components'
    // f32 bit patterns so the f64 dot lands in the rounding window.
    const target = Math.atan2(a[1], a[0]) - Math.acos(0.72);
    const bx = new Float32Array([Math.cos(target)]);
    const by = new Float32Array([Math.sin(target)]);
    const xi = new Uint32Array(bx.buffer);
    const yi = new Uint32Array(by.buffer);
    const x0 = xi[0];
    const y0 = yi[0];
    for (let i = -300; i <= 300 && !found; i++) {
      xi[0] = x0 + i;
      for (let j = -300; j <= 300 && !found; j++) {
        yi[0] = y0 + j;
        const b = new Float32Array([bx[0], by[0]]);
        const d = cosine(a, b);
        if (d > lo && d < 0.72 && Math.fround(d) >= 0.72) found = b;
      }
    }
    if (!found) throw new Error('no f32-rounding pair found');
    bucketCase('f32-storage-rounds-up', [item('a', a), item('b', found)], P(0.72, 30));
    bucketCase('f32-storage-rounds-up-digest', [item('d', a, { isDigest: true }), item('b', found)], P(0.72, 30));
  }

  // Exact ties at every strict `>`: identical vectors (all pairs 1).
  bucketCase('ties-identical', ['a', 'b', 'c', 'd'].map((id) => item(id, at(0))), P(0.9, 2));
  bucketCase('ties-identical-3', ['a', 'b', 'c', 'd', 'e'].map((id) => item(id, at(0))), P(0.9, 3));
  bucketCase(
    'ties-two-digests',
    [item('d1', at(0), { isDigest: true }), item('d2', at(0), { isDigest: true }), item('a', at(0)), item('b', at(0))],
    P(0.9, 30),
  );
  bucketCase(
    'ties-attraction-order',
    [item('d1', at(0), { isDigest: true }), item('a', at(1)), item('b', at(-1)), item('c', at(1))],
    P(0.9, 3),
  );

  // Seeded random buckets: dims 3 / 8 / 16 / 384, a few topics each.
  {
    const r = rng(20261009);
    for (const dim of [3, 8, 16, 384]) {
      for (let rep = 0; rep < 3; rep++) {
        const topics = Array.from({ length: 4 }, () => randomDir(dim, r));
        const items: ClusterItem[] = [];
        const count = 12 + Math.floor(r() * 24);
        for (let k = 0; k < count; k++) {
          const t = topics[Math.floor(r() * topics.length)];
          const episodic = r() < 0.35;
          const digest = r() < 0.12;
          items.push(
            item(`m${dim}-${rep}-${k}`, near(t, dim === 3 ? 0.5 : 0.9, r), {
              kind: episodic ? 'episodic' : 'semantic',
              isDigest: digest,
              eventTimeMs: r() < 0.15 ? null : NOW - Math.floor(r() * 6 * DAY),
              createdAtMs: NOW - Math.floor(r() * 40 * DAY),
              consideredAtMs: r() < 0.5 ? null : NOW - Math.floor(r() * 20 * DAY),
              weight: Math.round(r() * 1000) / 1000,
            }),
          );
        }
        for (const [threshold, cap] of [
          [0.72, 30],
          [0.6, 4],
          [0.85, 3],
        ] as const) {
          bucketCase(`random-${dim}-${rep}-${threshold}-${cap}`, items, P(threshold, cap));
        }
      }
    }
  }

  // ── effectiveMinClusterSize / clusterHasNewMaterial ─────────────────────
  for (const [created, minClusterSize, matureAfterDays] of [
    [[NOW - 10 * DAY], 3, 7],
    [[NOW - 14 * DAY], 3, 7],
    [[NOW - 14 * DAY - 1], 3, 7],
    [[NOW - 20 * DAY, NOW - 8 * DAY], 5, 7],
    [[], 3, 7],
    [[NOW - 1], 1, 0],
    [[NOW - 10 * DAY], 4.7, 7],
    [[NOW - 10 * DAY], -3, 7],
    [[NOW - 3 * DAY], 3, 1.5],
  ] as Array<[number[], number, number]>) {
    emit({
      case: 'effectiveMinClusterSize',
      created,
      minClusterSize,
      matureAfterDays,
      nowMs: NOW,
      out: effectiveMinClusterSize(created, { minClusterSize, matureAfterDays, nowMs: NOW }),
    });
  }
  const nm = (created: number, considered: number | null) => item(`x${created}`, at(0), { createdAtMs: created, consideredAtMs: considered });
  for (const members of [
    [],
    [nm(10, null)],
    [nm(10, 20), nm(15, 30)],
    [nm(10, 20), nm(25, 30)],
    [nm(10, 20), nm(20, 30)],
    [nm(10, 20), nm(15, null)],
  ]) {
    emit({ case: 'clusterHasNewMaterial', members: members.map(itemJson), out: clusterHasNewMaterial(members) });
  }

  // ── selectClusters ──────────────────────────────────────────────────────
  const selItems: ClusterItem[] = [
    item('fresh1', at(0), { createdAtMs: NOW - 9 * DAY }),
    item('fresh2', at(0), { createdAtMs: NOW - 9 * DAY }),
    item('fresh3', at(0), { createdAtMs: NOW - 9 * DAY }),
    item('old1', at(0), { createdAtMs: NOW - 30 * DAY }),
    item('old2', at(0), { createdAtMs: NOW - 30 * DAY }),
    item('seen1', at(0), { createdAtMs: NOW - 30 * DAY, consideredAtMs: NOW - 2 * DAY }),
    item('seen2', at(0), { createdAtMs: NOW - 31 * DAY, consideredAtMs: NOW - 2 * DAY }),
    item('newer', at(0), { createdAtMs: NOW - 1 * DAY, consideredAtMs: NOW - 1 * DAY }),
    item('seen3', at(0), { createdAtMs: NOW - 30 * DAY, consideredAtMs: NOW - 2 * DAY }),
  ];
  const C = (memberIds: string[], score: number, digestId: string | null = null): Cluster => ({
    kind: 'semantic',
    digestId,
    memberIds,
    score,
  });
  const clusterSets: Array<[string, Cluster[]]> = [
    [
      'mixed',
      [
        C(['fresh1', 'fresh2', 'fresh3'], 1.5),
        C(['fresh1', 'fresh2'], 1.0),
        C(['old1', 'old2'], 1.0),
        C(['seen1', 'seen2'], 2.0),
        C(['seen1', 'newer'], 0.9),
        C(['fresh1'], 0.5, 'dg'),
        C(['fresh1', 'fresh2'], 1.0, 'dg2'),
        C([], 0.3, 'dgEmpty'),
        C(['ghost'], 0.3),
        C(['seen3', 'ghost'], 0.3, 'dg3'),
        C(['fresh2', 'fresh3', 'old1'], 1.5),
        C(['fresh3', 'old2'], 1.5),
      ],
    ],
  ];
  for (const [name, clusters] of clusterSets) {
    const byId = new Map(selItems.map((i) => [i.id, i]));
    for (const maxClusters of [0, 1, 2, 40, 2.9, -1, NaN]) {
      for (const [minClusterSize, matureAfterDays] of [
        [3, 7],
        [2, 7],
        [4, 0],
      ]) {
        const opts = { minClusterSize, matureAfterDays, maxClusters, nowMs: NOW };
        const sel = selectClusters(clusters, byId, opts);
        emit({
          case: 'selectClusters',
          name,
          items: selItems.map(itemJson),
          clusters,
          opts: { ...opts, maxClusters: Number.isNaN(maxClusters) ? 'NaN' : maxClusters },
          out: sel,
        });
      }
    }
  }
  process.exit(0);
}

try {
  main();
} catch (err) {
  process.stderr.write(`consolidation-clustering oracle failed: ${(err as Error)?.stack ?? err}\n`);
  process.exit(1);
}
