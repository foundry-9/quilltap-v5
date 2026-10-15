/**
 * Oracle case: the recall renderers (P4.D291 — v4 `lib/memory/recall/render.ts`,
 * `584d28164`, pinned at `83334b9a5`).
 *
 * Drives the REAL `renderArchive`, `renderHead`, `renderRelevant`,
 * `renderInterCharacter`, `formatMemorySubjectPrefix`, `humanKeywords`,
 * `isTargetingKeyword` and `formatKeywordsTag` over a fixed corpus mirroring
 * `__tests__/unit/lib/memory/recall/render.test.ts` and growing it: the model
 * text with no id / number / multiplier / targeting tag, `history` / `present`
 * / `scope:  Narrow` collisions, a GOOGLE row and a default row on the same
 * text near a budget edge (R-T: 3.8 vs 3.5 chars per token), narrative times,
 * subject prefixes (self, named, unnamed, blank name), digest provenance, an
 * empty body, the head's blend-vs-salience ordering (incl. a NaN salience and
 * the 0.05 near-tie), the inter-character interleave and its per-character
 * break, `Unknown` names.
 *
 * Every row carries its INPUT, so the Rust side (`recall_render_equivalence`)
 * recomputes from the same bytes and byte-compares the whole section.
 * `Date` is frozen at `NOW` (renderRelevant dates its lines on the wall clock).
 *
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/recall-render.ts \
 *     > /tmp/oracle-recall-render.ndjson
 */

const NOW = Date.parse('2026-06-15T12:00:00.000Z')
const RealDate = Date
;(globalThis as { Date: DateConstructor }).Date = class extends RealDate {
  constructor(...args: unknown[]) {
    if (args.length === 0) super(NOW)
    // @ts-expect-error variadic forwarding
    else super(...args)
  }
  static now(): number {
    return NOW
  }
} as DateConstructor

async function main() {
  const r = await import('@/lib/memory/recall/render')
  const DAY = 86_400_000
  const iso = (msAgo: number) => new RealDate(NOW - msAgo).toISOString()
  const SELF = '00000000-0000-4000-8000-0000000000c1'
  const AMY = '00000000-0000-4000-8000-00000000a001'
  const BO = '00000000-0000-4000-8000-00000000b002'
  const GHOST = '00000000-0000-4000-8000-00000000f00f'
  const BLANK = '00000000-0000-4000-8000-00000000e00e'

  const mem = (id: string, over: Record<string, unknown> = {}) => ({
    id,
    characterId: SELF,
    content: `content of ${id}`,
    summary: `summary of ${id}`,
    keywords: [],
    tags: [],
    importance: 0.7,
    reinforcedImportance: 0.7,
    source: 'AUTO',
    reinforcementCount: 1,
    relatedMemoryIds: [],
    createdAt: iso(3 * DAY),
    updatedAt: iso(3 * DAY),
    ...over,
  })
  const cand = (m: Record<string, unknown>, score: number, salience: number, adj?: Record<string, unknown>) =>
    ({ memory: m, score, usedEmbedding: true, salience, ...(adj ? { recallAdjustment: adj } : {}) })

  const subjects = {
    self: { selfCharacterId: SELF, names: {} as Record<string, string> },
    named: { selfCharacterId: SELF, names: { [AMY]: 'Amy', [BO]: '  Bo  ', [BLANK]: '   ' } },
  }
  const toSubject = (s: { selfCharacterId: string; names: Record<string, string> }) =>
    ({ selfCharacterId: s.selfCharacterId, characterNames: new Map(Object.entries(s.names)) })

  const memories = [
    mem('a1000000-0000-4000-8000-000000000001', { keywords: ['tea', 'past', 'scope: narrow', 'history'] }),
    mem('a1000000-0000-4000-8000-000000000002', { aboutCharacterId: AMY, keywords: ['  garden ', '', 'Present', 'scope:  Narrow', 'scope: sideways'] }),
    mem('a1000000-0000-4000-8000-000000000003', { aboutCharacterId: GHOST, narrativeTime: '  the winter of the long frost ' }),
    mem('a1000000-0000-4000-8000-000000000004', { aboutCharacterId: SELF, source: 'CONSOLIDATED', consolidatedFrom: ['x', 'y'], keywords: ['banter', 'lanterns'] }),
    mem('a1000000-0000-4000-8000-000000000005', { source: 'CONSOLIDATED', consolidatedFrom: ['x'] }),
    mem('a1000000-0000-4000-8000-000000000006', { summary: '   ', content: '  only the content  ' }),
    mem('a1000000-0000-4000-8000-000000000007', { summary: '', content: '' }),
    mem('a1000000-0000-4000-8000-000000000008', { aboutCharacterId: BLANK, occurredAt: iso(9 * DAY), lastReinforcedAt: iso(1 * DAY) }),
    mem('a1000000-0000-4000-8000-000000000009', { aboutCharacterId: BO, createdAt: iso(40 * DAY), keywords: [7, 'moment', 'trivia', 'harbour'] }),
    mem('a1000000-0000-4000-8000-000000000010', { createdAt: 'not a date' }),
  ]

  const out: unknown[] = []
  const section = (s: Record<string, any>) => ({
    content: s.content,
    tokenCount: s.tokenCount,
    memoriesUsed: s.memoriesUsed,
    lines: s.lines,
    debugMemories: JSON.parse(JSON.stringify(s.debugMemories)),
  })

  // ---- keywords ----------------------------------------------------------------
  const kwCases: unknown[][] = [
    [], ['tea'], [' tea ', '', '  '], ['past', 'moment', 'present', 'future'],
    ['history', 'banter', 'trivia', 'information', 'philosophy', 'relationships', 'mannerisms'],
    ['scope: narrow', 'scope:wide', 'scope:  Narrow', 'SCOPE: WIDE', 'scope: sideways', 'scope:'],
    ['History', ' PAST ', 'Tea Party'], [7, null, 'ok'],
  ]
  for (const kws of kwCases) {
    out.push({
      kind: 'keywords',
      input: kws,
      human: r.humanKeywords(kws as string[]),
      tag: r.formatKeywordsTag(kws as string[]),
      targeting: (kws as unknown[]).filter((k): k is string => typeof k === 'string').map(k => [k, r.isTargetingKeyword(k)]),
    })
  }
  out.push({ kind: 'keywordsNull', human: r.humanKeywords(null), tag: r.formatKeywordsTag(undefined) })

  // ---- prefixes ----------------------------------------------------------------
  for (const about of [null, '', SELF, AMY, BO, GHOST, BLANK]) {
    out.push({ kind: 'prefix', about, prefix: r.formatMemorySubjectPrefix(about, toSubject(subjects.named)) })
  }

  // ---- archive -----------------------------------------------------------------
  for (const provider of [undefined, 'OPENAI', 'GOOGLE']) {
    for (const budget of [0, 20, 60, 61, 62, 63, 64, 65, 70, 90, 120, 1000]) {
      for (const subj of ['self', 'named'] as const) {
        out.push({
          kind: 'archive',
          input: { memories, maxTokens: budget, provider: provider ?? null, subject: subjects[subj] },
          output: section(r.renderArchive(memories as never, budget, provider as never, toSubject(subjects[subj]))),
        })
      }
    }
  }

  // ---- head ----------------------------------------------------------------------
  const adj = (after: number) => ({ multiplier: 1.1, fired: ['narrow✓'], blendedBefore: after / 1.1, blendedAfter: after })
  const headPools: Record<string, unknown[]> = {
    blend: memories.map((m, i) => cand(m, 0.5 + i / 50, 0.3 + (i % 3) / 10, adj(0.4 + ((i * 7) % 10) / 20))),
    salience: memories.map((m, i) => cand(m, 0.9 - i / 40, [0.6, 0.62, 0.3, 0.64, NaN, 0.66, 0.2, 0.7, 0.55, 0.5][i])),
    mixed: memories.map((m, i) => cand(m, 0.7, 0.5, i % 2 === 0 ? adj(0.5 + i / 30) : undefined)),
  }
  for (const [poolName, pool] of Object.entries(headPools)) {
    for (const provider of [undefined, 'GOOGLE']) {
      for (const [maxTokens, maxEntries] of [[200, 5], [1200, 15], [70, 15], [100, 3], [1000, 0]]) {
        out.push({
          kind: 'head',
          input: {
            candidates: pool.map(c => ({ ...(c as object), salience: Number.isNaN((c as { salience: number }).salience) ? 'NaN' : (c as { salience: number }).salience })),
            provider: provider ?? null,
            subject: subjects.named,
            maxTokens,
            maxEntries,
            nowMs: NOW - 2 * DAY,
          },
          output: section(r.renderHead(pool as never, provider as never, toSubject(subjects.named), { maxTokens, maxEntries, nowMs: NOW - 2 * DAY })),
        })
      }
    }
  }

  // ---- relevant ------------------------------------------------------------------
  for (const provider of [undefined, 'GOOGLE']) {
    for (const budget of [0, 40, 80, 120, 1200]) {
      const pool = headPools.salience
      out.push({
        kind: 'relevant',
        input: {
          candidates: pool.map(c => ({ ...(c as object), salience: Number.isNaN((c as { salience: number }).salience) ? 'NaN' : (c as { salience: number }).salience })),
          provider: provider ?? null,
          subject: subjects.named,
          maxTokens: budget,
          nowMs: NOW,
        },
        output: section(r.renderRelevant(pool as never, budget, provider as never, toSubject(subjects.named))),
      })
    }
  }

  // ---- inter-character -------------------------------------------------------------
  const inter = [
    mem('b1000000-0000-4000-8000-000000000001', { aboutCharacterId: AMY, reinforcedImportance: 0.4 }),
    mem('b1000000-0000-4000-8000-000000000002', { aboutCharacterId: AMY, source: 'CONSOLIDATED', consolidatedFrom: ['p', 'q', 'r'], reinforcedImportance: 0.2 }),
    mem('b1000000-0000-4000-8000-000000000003', { aboutCharacterId: BO, reinforcedImportance: 0.9, keywords: ['harbour', 'past'] }),
    mem('b1000000-0000-4000-8000-000000000004', { aboutCharacterId: AMY, reinforcedImportance: 0.9, createdAt: iso(60 * DAY) }),
    mem('b1000000-0000-4000-8000-000000000005', { aboutCharacterId: GHOST, content: '  ', summary: 'ghost summary' }),
    mem('b1000000-0000-4000-8000-000000000006', { reinforcedImportance: 0.9 }),
    mem('b1000000-0000-4000-8000-000000000007', { aboutCharacterId: BO, reinforcedImportance: 0.5, createdAt: 'bad' }),
  ]
  const rel = [
    cand(mem('c1000000-0000-4000-8000-000000000001', { aboutCharacterId: BO }), 0.6, 0.3),
    cand(mem('c1000000-0000-4000-8000-000000000002', { aboutCharacterId: AMY }), 0.9, 0.3),
    cand(mem('c1000000-0000-4000-8000-000000000003', { aboutCharacterId: AMY }), 0.7, 0.3),
    cand(inter[0], 0.8, 0.3),
    cand(mem('c1000000-0000-4000-8000-000000000004', {}), 0.99, 0.3),
  ]
  const namesMap = { [AMY]: 'Amy', [BO]: '', [BLANK]: 'Blank' }
  for (const provider of [undefined, 'GOOGLE']) {
    for (const budget of [0, 40, 70, 100, 140, 1000]) {
      for (const [imp, relv, label] of [[inter, rel, 'both'], [inter, [], 'importance'], [[], rel, 'relevance'], [[], [], 'none']] as const) {
        out.push({
          kind: 'inter',
          id: `${provider ?? 'default'}/${budget}/${label}`,
          input: { importance: imp, names: namesMap, maxTokens: budget, provider: provider ?? null, relevance: relv, nowMs: NOW },
          output: section(r.renderInterCharacter(imp as never, new Map(Object.entries(namesMap)), budget, provider as never, relv as never, NOW)),
        })
      }
    }
  }

  out.push({ kind: 'meta', nowMs: NOW })
  for (const row of out) process.stdout.write(JSON.stringify(row) + '\n')
}

main().catch(e => {
  process.stderr.write(String(e?.stack ?? e) + '\n')
  process.exit(1)
})
