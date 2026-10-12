/**
 * Tier-1 oracle — v4's consolidation write planner (P4.D271, v4 `d58548051`,
 * byte-identical at the `01a83539d` pin), driven through v4's REAL exports of
 * `lib/memory/consolidation-plan.ts`: `planConsolidationWrites`, `sharedValue`,
 * `deriveWitnessedContext`, `clampDigestImportance`, `normalizeDigestKeywords`,
 * `DIGEST_REINFORCEMENT_CAP`, with a deterministic `newId` (`d1`, `d2`, …).
 *
 * Every input is emitted WITH the whole plan (Maps as ordered `[key, value]`
 * arrays; creates' `data` and updates' `patch` in v4's literal key order;
 * embeddings as their f32 values).
 *
 * Corpus: creates / updates / both; contradictions incl. chains past the
 * 5-hop remap, a cycle, self-targets, a member re-superseded (Map position
 * kept); link rewiring incl. links to a sibling standalone, to another
 * cluster's digest and to an unknown row; keyword normalization (scope
 * variants, duplicates, >8 free words, member majority ties, episode `past`);
 * `localeCompare` ties over mixed ISO shapes; `sharedValue` / witnessed arms;
 * the reinforcement cap; the `lastReinforcedAt` reference time arms.
 *
 * Output: one NDJSON line per case `{case, …}`.
 *
 * Run (Node 24, from the v4 checkout or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   rm -f /tmp/qt-consolidation-plan.ndjson
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/consolidation-plan.ts > /tmp/qt-consolidation-plan.ndjson
 */

import {
  DIGEST_REINFORCEMENT_CAP,
  clampDigestImportance,
  deriveWitnessedContext,
  normalizeDigestKeywords,
  planConsolidationWrites,
  sharedValue,
  type LinkIndexRow,
  type ResolvedClusterOutcome,
} from '@/lib/memory/consolidation-plan';
import type { Memory } from '@/lib/schemas/types';

const HOLDER = 'holder-1';
const LAURA = 'laura-1';
const NOW = '2026-10-08T12:00:00.000Z';

function emit(obj: unknown): void {
  process.stdout.write(JSON.stringify(obj) + '\n');
}

function mem(id: string, o: Partial<Memory> = {}): Memory {
  return {
    id,
    characterId: HOLDER,
    aboutCharacterId: LAURA,
    content: `content ${id}`,
    summary: `summary ${id}`,
    keywords: ['present', 'scope: wide', 'information'],
    tags: [],
    importance: 0.6,
    source: 'AUTO',
    entities: [],
    kind: 'semantic',
    reinforcementCount: 1,
    relatedMemoryIds: [],
    reinforcedImportance: 0.6,
    tier: 'hot',
    supersededById: null,
    consolidatedFrom: [],
    consolidatedAt: null,
    chatId: 'chat-a',
    projectId: null,
    createdAt: '2026-08-01T00:00:00.000Z',
    updatedAt: '2026-08-01T00:00:00.000Z',
    ...o,
  } as Memory;
}

function outcome(o: Partial<ResolvedClusterOutcome>): ResolvedClusterOutcome {
  return {
    aboutCharacterId: LAURA,
    clusterKind: 'semantic',
    existingDigest: null,
    members: [],
    digests: [],
    keepStandalone: [],
    contradictions: [],
    embeddings: [],
    ...o,
  };
}

function dg(memberIds: string[], extra: Record<string, unknown> = {}) {
  return {
    content: 'Laura keeps the charts and prefers tea now.',
    summary: 'laura keeps charts prefers tea',
    keywords: ['charts', 'tea'],
    importance: 0.95,
    kind: 'semantic' as const,
    memberIds,
    ...extra,
  };
}

function f32(v: number[]): Float32Array {
  return new Float32Array(v);
}

function planJson(plan: ReturnType<typeof planConsolidationWrites>) {
  const digestJson = (d: any) => ({
    id: d.id,
    action: d.action,
    clusterIndex: d.clusterIndex,
    content: d.content,
    summary: d.summary,
    keywords: d.keywords,
    importance: d.importance,
    reinforcedImportance: d.reinforcedImportance,
    reinforcementCount: d.reinforcementCount,
    kind: d.kind,
    occurredAt: d.occurredAt,
    narrativeTime: d.narrativeTime,
    entities: d.entities,
    memberIds: d.memberIds,
    aboutCharacterId: d.aboutCharacterId,
    embedding: d.embedding ? Array.from(d.embedding as Float32Array) : null,
  });
  return {
    creates: plan.creates.map((c) => ({ digest: digestJson(c), data: c.data })),
    updates: plan.updates.map((u) => ({ digest: digestJson(u), patch: u.patch })),
    tierMoves: plan.tierMoves,
    considered: plan.considered,
    linkRewrites: plan.linkRewrites,
    supersededBy: Array.from(plan.supersededBy.entries()),
    digestsByCluster: plan.digestsByCluster.map((ds) => ds.map(digestJson)),
  };
}

function clusterJson(c: ResolvedClusterOutcome) {
  return {
    aboutCharacterId: c.aboutCharacterId,
    clusterKind: c.clusterKind,
    existingDigest: c.existingDigest,
    members: c.members,
    digests: c.digests,
    keepStandalone: c.keepStandalone,
    contradictions: c.contradictions,
    embeddings: c.embeddings.map((e) => (e ? Array.from(e) : null)),
  };
}

function planCase(name: string, clusters: ResolvedClusterOutcome[], rows: LinkIndexRow[], characterId = HOLDER): void {
  let n = 0;
  const rowIndex = new Map(rows.map((r) => [r.id, r]));
  const plan = planConsolidationWrites({ characterId, clusters, rowIndex, nowIso: NOW, newId: () => `d${++n}` });
  emit({
    case: 'plan',
    name,
    characterId,
    nowIso: NOW,
    clusters: clusters.map(clusterJson),
    rowIndex: Array.from(rowIndex.values()),
    plan: planJson(plan),
  });
}

const idx = (...ms: Memory[]): LinkIndexRow[] => ms.map((m) => ({ id: m.id, relatedMemoryIds: m.relatedMemoryIds ?? [] }));

function main(): void {
  emit({ case: 'constants', DIGEST_REINFORCEMENT_CAP });

  // ── a new digest (v4's jest shape) ───────────────────────────────────────
  {
    const a = mem('a', { reinforcementCount: 2, reinforcedImportance: 0.5, relatedMemoryIds: ['b', 'x', 'c'] });
    const b = mem('b', { reinforcementCount: 3, reinforcedImportance: 0.8, relatedMemoryIds: ['a', 'stand'] });
    const stand = mem('stand', { relatedMemoryIds: ['a', 'b', 'a'] });
    const x = mem('x', { relatedMemoryIds: ['b'] });
    const c = mem('c', { chatId: 'chat-b' });
    planCase(
      'new-digest',
      [
        outcome({
          members: [a, b, stand],
          digests: [dg(['a', 'b'])],
          keepStandalone: ['stand'],
          embeddings: [f32([0.1, 0.2, 0.30000001192092896])],
        }),
      ],
      idx(a, b, stand, x, c),
    );
  }

  // ── reinforcement cap; null counts; importance clamp both sides ───────────
  {
    const ms = Array.from({ length: 6 }, (_, i) =>
      mem(`r${i}`, { reinforcementCount: i === 0 ? (null as any) : 12, reinforcedImportance: i === 1 ? (null as any) : 0.3 + i * 0.05, importance: 0.45 }),
    );
    planCase('reinforcement-cap', [outcome({ members: ms, digests: [dg(ms.map((m) => m.id), { importance: 0.05 })] })], idx(...ms));
    planCase('importance-high', [outcome({ members: ms.slice(2, 4), digests: [dg(['r2', 'r3'], { importance: 1.7 })] })], idx(...ms));
  }

  // ── episodes: earliest member event time, mixed ISO shapes, narrative time ─
  {
    const e1 = mem('e1', { kind: 'episodic', occurredAt: '2026-08-01T00:00:00Z', narrativeTime: 'the third bell', createdAt: '2026-08-05T00:00:00.000Z' });
    const e2 = mem('e2', { kind: 'episodic', occurredAt: '2026-08-01T00:00:00.000Z', narrativeTime: 'dawn' });
    const e3 = mem('e3', { kind: 'episodic', occurredAt: null, createdAt: '2026-08-01', narrativeTime: null });
    const e4 = mem('e4', { kind: 'episodic', occurredAt: '2026-07-31T23:59:59.999Z', narrativeTime: undefined as any });
    planCase(
      'episode-earliest',
      [outcome({ clusterKind: 'episodic', members: [e1, e2, e3, e4], digests: [dg(['e1', 'e2', 'e3', 'e4'], { kind: 'semantic' })] })],
      idx(e1, e2, e3, e4),
    );
    planCase(
      'episode-ties',
      [outcome({ clusterKind: 'episodic', members: [e1, e2, e3], digests: [dg(['e3', 'e2', 'e1'])] })],
      idx(e1, e2, e3),
    );
    // Semantic cluster, model says episodic.
    planCase('model-episodic', [outcome({ members: [e1, e2], digests: [dg(['e1', 'e2'], { kind: 'episodic' })] })], idx(e1, e2));
  }

  // ── existing digest updated in place, plus a second new digest ────────────
  {
    const digest = mem('dig', {
      source: 'CONSOLIDATED',
      reinforcementCount: 5,
      reinforcedImportance: 0.7,
      consolidatedFrom: ['old1', 'OLD2'],
      relatedMemoryIds: ['n1', 'far', 'dig'],
      keywords: ['estate', 'past', 'scope: narrow', 'history'],
      entities: ['Estate', 'Laura'],
      witnessedContext: 'user_present' as any,
      chatId: 'chat-a',
    });
    const n1 = mem('n1', { reinforcementCount: 2, entities: ['laura', 'Map'], tags: ['t1'], witnessedContext: 'user_present' as any });
    const n2 = mem('n2', { tags: ['T1', 't2'], witnessedContext: null as any, chatId: 'chat-a' });
    const n3 = mem('n3', { createdAt: '2026-08-09T00:00:00.000Z', lastReinforcedAt: '2026-09-01T00:00:00.000Z' });
    const n4 = mem('n4', { createdAt: '2026-08-09T00:00:00.000Z', lastReinforcedAt: '2026-08-02T00:00:00.000Z' });
    const far = mem('far', { relatedMemoryIds: ['n1', 'n3'] });
    planCase(
      'existing-digest',
      [
        outcome({
          existingDigest: digest,
          members: [n1, n2, n3, n4],
          digests: [dg(['n1', 'n2'], { content: 'revised' }), dg(['n3', 'n4'], { content: 'second' })],
          embeddings: [f32([1, 0]), null],
        }),
      ],
      idx(digest, n1, n2, n3, n4, far),
    );
  }

  // ── contradictions + cross-cluster links ──────────────────────────────────
  {
    const a = mem('a', { createdAt: '2026-08-01T00:00:00.000Z' });
    const b = mem('b', { createdAt: '2026-08-05T00:00:00.000Z' });
    const s1 = mem('s1', { relatedMemoryIds: ['a', 'c', 's2'] });
    const s2 = mem('s2', { relatedMemoryIds: ['s1'] });
    const c = mem('c', { relatedMemoryIds: ['a', 'q'] });
    const q = mem('q', { relatedMemoryIds: ['c', 'zzz-unknown'] });
    planCase(
      'contradictions',
      [
        outcome({
          members: [a, b, s1, s2],
          digests: [dg(['b'])],
          keepStandalone: ['a', 's1', 's2'],
          contradictions: [
            { olderId: 'a', newerId: 'b', note: 'changed' },
            { olderId: 's1', newerId: 's2', note: '' },
            { olderId: 'b', newerId: 'b', note: 'self' },
          ],
        }),
        outcome({
          members: [c, q],
          digests: [dg(['c', 'q'])],
          contradictions: [{ olderId: 's2', newerId: 'c', note: 'cross' }],
        }),
      ],
      idx(a, b, s1, s2, c, q),
    );
    // A member re-superseded keeps its Map position; a contradiction whose
    // target is the older row itself is skipped.
    planCase(
      'resupersede',
      [
        outcome({
          members: [a, b, c],
          digests: [dg(['a', 'b'])],
          keepStandalone: ['c'],
          contradictions: [
            { olderId: 'a', newerId: 'c', note: 'a then c' },
            { olderId: 'b', newerId: 'a', note: 'b then a, a is in the digest' },
          ],
        }),
      ],
      idx(a, b, c),
    );
  }

  // ── remap: a 7-hop chain (stops at 5) and a cycle ─────────────────────────
  {
    const chain = Array.from({ length: 8 }, (_, i) => mem(`h${i}`, { relatedMemoryIds: i === 7 ? ['h0'] : [] }));
    const contradictions = chain.slice(0, 7).map((m, i) => ({ olderId: m.id, newerId: chain[i + 1].id, note: '' }));
    const link = mem('link', { relatedMemoryIds: ['h0', 'h1'] });
    planCase(
      'remap-chain',
      [outcome({ members: chain, digests: [], keepStandalone: chain.map((m) => m.id), contradictions })],
      idx(...chain, link),
    );
    const x = mem('x', { relatedMemoryIds: ['y'] });
    const y = mem('y', { relatedMemoryIds: ['x'] });
    const z = mem('z', { relatedMemoryIds: ['x', 'y', 'X'] });
    planCase(
      'remap-cycle',
      [
        outcome({ members: [x, y], keepStandalone: ['x', 'y'], contradictions: [{ olderId: 'x', newerId: 'y', note: '' }] }),
        outcome({ members: [x, y], keepStandalone: ['y'], contradictions: [{ olderId: 'y', newerId: 'x', note: '' }] }),
      ],
      idx(x, y, z),
    );
  }

  // ── keyword normalization through the planner ─────────────────────────────
  {
    const m1 = mem('k1', { keywords: ['past', 'scope: narrow', 'banter'] });
    const m2 = mem('k2', { keywords: ['future', 'scope: narrow', 'trivia'] });
    const m3 = mem('k3', { keywords: ['past', 'scope: wide', 'trivia', 'banter'] });
    const m4 = mem('k4', { keywords: null as any });
    planCase(
      'keywords',
      [
        outcome({
          members: [m1, m2, m3, m4],
          digests: [
            dg(['k1', 'k2'], { keywords: ['  Charts ', 'charts', 'TEA', '', 'scope:  Narrow', 'scope: everywhere', 'Moment', 'mannerisms', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'] }),
            dg(['k3', 'k4'], { keywords: [] }),
          ],
        }),
      ],
      idx(m1, m2, m3, m4),
    );
  }

  // ── sharedValue / witnessed; a digest naming a member not loaded ──────────
  {
    const p1 = mem('p1', { projectId: 'proj', chatId: '' as any, witnessedContext: 'user_absent' as any });
    const p2 = mem('p2', { projectId: 'proj', chatId: '' as any, witnessedContext: 'user_absent' as any });
    const p3 = mem('p3', { projectId: undefined as any, chatId: 'chat-a', witnessedContext: undefined as any });
    planCase(
      'shared-values',
      [
        outcome({ aboutCharacterId: null, members: [p1, p2], digests: [dg(['p1', 'p2', 'ghost'])] }),
        outcome({ aboutCharacterId: HOLDER, members: [p1, p3], digests: [dg(['p1', 'p3'])] }),
      ],
      idx(p1, p2, p3),
    );
  }

  // ── reference-time arms for a create's lastReinforcedAt ───────────────────
  {
    const t1 = mem('t1', { createdAt: '2026-08-01T00:00:00.000Z', lastReinforcedAt: 'garbage' as any });
    const t2 = mem('t2', { createdAt: '2026-08-03T00:00:00.000Z', lastReinforcedAt: '' as any });
    const t3 = mem('t3', { createdAt: '2026-08-02T00:00:00.000Z', lastReinforcedAt: '2026-08-10T00:00:00Z' as any });
    const t4 = mem('t4', { createdAt: 'not a date', lastReinforcedAt: '2026-01-01T00:00:00.000Z' as any });
    planCase('reference-times', [outcome({ members: [t1, t2, t3, t4], digests: [dg(['t1', 't2', 't3', 't4'])] })], idx(t1, t2, t3, t4));
    planCase('reference-unparsable-only', [outcome({ members: [t4], digests: [dg(['t4'])] })], idx(t4));
  }

  // ── an empty run and a dry-run shape (no embeddings) ──────────────────────
  planCase('empty', [], []);
  planCase('standalone-only', [outcome({ members: [mem('only')], keepStandalone: ['only', 'only'] })], idx(mem('only')));

  // ── exported helpers ──────────────────────────────────────────────────────
  for (const values of [[], [null], ['a', 'a'], ['a', 'b'], ['', ''], ['a', undefined], [undefined, 'a'], ['a', null], ['x']] as Array<Array<string | null | undefined>>) {
    emit({ case: 'sharedValue', values: values.map((v) => (v === undefined ? null : v)), out: sharedValue(values) });
  }
  for (const values of [[], [null], ['user_present'], ['user_absent', 'user_absent'], ['user_absent', 'user_present'], [null, 'user_present'], ['user_absent', null], ['', '']] as any[]) {
    emit({ case: 'deriveWitnessedContext', values, out: deriveWitnessedContext(values) });
  }
  for (const [model, imps] of [
    [0.95, [0.5, 0.8]],
    [0.1, [0.5, 0.8]],
    [0.1, []],
    [1.5, []],
    [0.6, [0.3]],
    [0.1, [0.1, 0.15]],
    [0.99, [0.9, 1.0]],
  ] as Array<[number, number[]]>) {
    const rows = imps.map((r, i) => mem(`c${i}`, { reinforcedImportance: r }));
    emit({ case: 'clampDigestImportance', model, rows, out: clampDigestImportance(model, rows) });
  }
  for (const [model, members, kind] of [
    [['Maps', 'maps', 'past'], [['present', 'scope: narrow', 'trivia']], 'semantic'],
    [[], [['future', 'banter'], ['moment', 'banter'], ['future']], 'semantic'],
    [[], [['future'], ['moment']], 'semantic'],
    [[], [['future'], ['moment']], 'episodic'],
    [['scope:wide', 'scope: Narrow', 'HISTORY', 'Information'], [], 'semantic'],
    [['  ', 'one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight', 'nine', 'ONE'], [], 'semantic'],
  ] as Array<[string[], string[][], 'semantic' | 'episodic']>) {
    emit({ case: 'normalizeDigestKeywords', model, members, kind, out: normalizeDigestKeywords(model, members, kind) });
  }
  process.exit(0);
}

try {
  main();
} catch (err) {
  process.stderr.write(`consolidation-plan oracle failed: ${(err as Error)?.stack ?? err}\n`);
  process.exit(1);
}
