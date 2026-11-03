/**
 * Oracle case: the recall renderers (P4.D291 — v4 `lib/memory/recall/render.ts`,
 * `584d28164`, pinned at `83334b9a5`; P4.D342 — `a5efdbb97`'s
 * `renderInnerCircle` + `innerCircleTrimOrder` and `8cb584eff`'s keystone
 * body rule, pinned at `236c04e35`).
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
    // P4.D342 (v4 `8cb584eff`): keystones render in FULL in the archive and the
    // head — content first, both trimmed; an empty or blank content falls back
    // to the (trimmed) summary.
    mem('a1000000-0000-4000-8000-000000000011', { keystone: true, aboutCharacterId: AMY, content: 'Amy is engaged to Charlie, and the wedding is in June.', summary: 'amy engaged' }),
    mem('a1000000-0000-4000-8000-000000000012', { keystone: true, content: '', summary: '  keystone summary only  ' }),
    mem('a1000000-0000-4000-8000-000000000013', { keystone: true, content: '   ', summary: 'blank-content keystone', keystoneNote: 'x' }),
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
    salience: memories.map((m, i) => cand(m, 0.9 - i / 40, [0.6, 0.62, 0.3, 0.64, NaN, 0.66, 0.2, 0.7, 0.55, 0.5, 0.95, 0.94, 0.93][i])),
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

  // ---- inner circle (P4.D342 — v4 `a5efdbb97` + `8cb584eff`) ------------------
  // `renderInnerCircle` and `innerCircleTrimOrder`: full text (content first,
  // both trimmed), NO age tag (the rows are planted old), subject prefixes, the
  // budget cut in trim order — keystones last, the loosest tier first, a
  // member's later rows before anyone's first — the rest in CIRCLE order, the
  // walk BREAKING at the first row that does not fit.
  const types = await import('@/lib/memory/recall/types')
  const ent = (m: Record<string, unknown>, characterId: string, tier: number, rank: number, order: number, keystone = false) =>
    ({ memory: m, characterId, tier, rank, order, keystone })
  const old = { createdAt: iso(400 * DAY), updatedAt: iso(400 * DAY) }
  const circles: Record<string, unknown[]> = {
    tiers: [
      ent(mem('d1000000-0000-4000-8000-000000000001', { ...old, aboutCharacterId: AMY, content: 'Amy is my sister.' }), AMY, 0, 0, 0),
      ent(mem('d1000000-0000-4000-8000-000000000002', { ...old, aboutCharacterId: AMY, content: 'Amy keeps bees on the roof of the east wing.' }), AMY, 0, 1, 0),
      ent(mem('d1000000-0000-4000-8000-000000000003', { ...old, aboutCharacterId: BO, source: 'CONSOLIDATED', consolidatedFrom: ['p', 'q'], content: '  Bo sails the Kittiwake.  ', keywords: ['harbour'] }), BO, 1, 0, 1),
      ent(mem('d1000000-0000-4000-8000-000000000004', { ...old, aboutCharacterId: BO, content: '' , summary: '  bo summary only  ' }), BO, 1, 1, 1),
      ent(mem('d1000000-0000-4000-8000-000000000005', { ...old, aboutCharacterId: GHOST, content: 'The quiet one from the club reads in the library.' }), GHOST, 2, 0, 2),
      ent(mem('d1000000-0000-4000-8000-000000000006', { ...old, aboutCharacterId: GHOST, content: '   ', summary: '' }), GHOST, 2, 1, 2),
    ],
    keystones: [
      ent(mem('d2000000-0000-4000-8000-000000000001', { ...old, aboutCharacterId: AMY, content: 'Amy is expecting in the spring.' }), AMY, -1, 0, 0, true),
      ent(mem('d2000000-0000-4000-8000-000000000002', { ...old, aboutCharacterId: BO, content: 'Bo is my oldest friend.' }), BO, 0, 0, 0),
      ent(mem('d2000000-0000-4000-8000-000000000003', { ...old, aboutCharacterId: GHOST, content: 'The quiet one is my godchild, and I promised to look after them.', keystone: true }), GHOST, 2, 0, 1, true),
      ent(mem('d2000000-0000-4000-8000-000000000004', { ...old, aboutCharacterId: BO, content: 'Bo mends nets on Sundays.' }), BO, 0, 1, 0),
      ent(mem('d2000000-0000-4000-8000-000000000005', { ...old, aboutCharacterId: GHOST, content: 'The quiet one plays the oboe.' }), GHOST, 2, 1, 1),
    ],
  }
  for (const [label, entries] of Object.entries(circles)) {
    out.push({ kind: 'trimOrder', id: label, input: entries, order: types.innerCircleTrimOrder(entries as never).map(e => (e as { memory: { id: string } }).memory.id) })
    for (const provider of [undefined, 'OPENAI', 'GOOGLE']) {
      for (const budget of [0, 8, 20, 40, 60, 61, 62, 90, 1000]) {
        out.push({
          kind: 'innerCircle',
          id: `${label}/${provider ?? 'default'}/${budget}`,
          input: { entries, maxTokens: budget, provider: provider ?? null, subject: subjects.named },
          output: section(r.renderInnerCircle(entries as never, budget, provider as never, toSubject(subjects.named))),
        })
      }
    }
  }
  out.push({ kind: 'innerCircle', id: 'empty', input: { entries: [], maxTokens: 1000, provider: null, subject: subjects.named }, output: section(r.renderInnerCircle([], 1000, undefined, toSubject(subjects.named))) })

  out.push({ kind: 'meta', nowMs: NOW })
  for (const row of out) process.stdout.write(JSON.stringify(row) + '\n')
}

main().catch(e => {
  process.stderr.write(String(e?.stack ?? e) + '\n')
  process.exit(1)
})
