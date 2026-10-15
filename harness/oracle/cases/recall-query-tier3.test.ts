/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for recall's first stage (P4.D291): v4's REAL `buildRecallQuery`
 * (`lib/memory/recall/query.ts`, `584d28164`, pinned at `83334b9a5`) — and,
 * through it, the REAL distiller `extractMemorySearchKeywords` with bug 212's
 * memory-calendar TODAY line, `resolveReferenceWindow` scan and
 * `normaliseTimeRange` (`c2795733f`).
 *
 * Mocked, and nothing else:
 * - `generateEmbeddingForUser` — answers from the case's canned table (a text
 *   with no entry, or one marked `{fail}`, throws an `Error` with the canned
 *   message); every call's text is recorded in order;
 * - `executeCheapLLMTask` — CAPTURES the prompt and runs v4's REAL parse
 *   callback over the case's canned text (`fail` → `{ success: false }`);
 * - `refreshMemoryTimeZone` — the case's zone (the instance's memory calendar).
 * `Date` is frozen at `$nowMs`.
 *
 * Mirrors `__tests__/unit/lib/memory/recall/build-recall-query.test.ts`'s seven
 * cases and grows them: probe failure, main failure with probes, an explicit
 * probe list, `skipEmbedding`, a real distil in a second zone (the TODAY line
 * and a date-only range normalised to LOCAL day bounds), a day reference
 * overriding the model's range, a narrative timeline, an unparsable distil.
 *
 * Emits one NDJSON line per case: `{ name, calls: [{ query, trace }], embedCalls,
 * distillPrompt, logs }` — each query in v4's shape (embedding as
 * `{vector, provider}` or null; probes as `{text, embedded}`).
 *
 * Stage the case OUTSIDE any `.claude/` path (v4's jest ignores it); see the
 * `recall_query_tier3_equivalence.rs` header for the recipe.
 */

interface EmbedSpec {
  vector?: number[]
  provider?: string
  fail?: string
}
interface CallSpec {
  text: string
  signals?: Record<string, unknown> | null
  probes?: string[]
  skipEmbedding?: boolean
  embeddingProfileId?: string
  distill?: {
    messages: Array<{ role: string; content: string }>
    characterName: string
    chatId?: string
    characterId?: string
    clock?: { nowIso: string; timelineMode: string }
    latency?: 'interactive' | 'background'
  }
}
interface CaseSpec {
  name: string
  zone: string
  embeddings: Record<string, EmbedSpec>
  distill?: { kind: 'json' | 'fail'; text?: string }
  /** One memo shared across the calls when true. */
  sharedMemo?: boolean
  calls: CallSpec[]
}

const NOW = Date.parse('2026-06-17T03:30:00.000Z')

const CASES: CaseSpec[] = [
  {
    name: 'ordinary_turn_embeds_once',
    zone: 'UTC',
    embeddings: { 'They talk of the fair.': { vector: [0.6, 0.8, 0], provider: 'OPENAI' } },
    calls: [{ text: 'the recent window', signals: { keywords: ['x'], paraphrase: 'They talk of the fair.', retrospective: false, timeRange: null, entities: [] } }],
  },
  {
    name: 'keywords_when_no_paraphrase',
    zone: 'UTC',
    embeddings: { 'fair lantern': { vector: [1, 0, 0], provider: 'OPENAI' } },
    calls: [{ text: 'window', signals: { keywords: ['fair', 'lantern'], retrospective: false, timeRange: null, entities: [] } }],
  },
  {
    name: 'retrospective_probes_embedded_once_each',
    zone: 'UTC',
    embeddings: {
      'They remember the fair.': { vector: [1, 0, 0], provider: 'OPENAI' },
      'Lighthouse Point': { vector: [0, 1, 0], provider: 'OPENAI' },
      'They remember the fair. (around 2026-06-01 to 2026-06-07)': { vector: [0, 0, 1], provider: 'OPENAI' },
    },
    calls: [{
      text: 'w',
      signals: {
        keywords: ['fair'],
        paraphrase: 'They remember the fair.',
        retrospective: true,
        timeRange: { from: '2026-06-01T00:00:00.000Z', to: '2026-06-07T23:59:59.999Z' },
        entities: [' Lighthouse Point ', '  '],
      },
    }],
  },
  {
    name: 'provider_failure_keeps_text',
    zone: 'UTC',
    embeddings: { 'the window': { fail: 'provider down' } },
    calls: [{ text: 'the window' }],
  },
  {
    name: 'main_failure_probes_unembedded',
    zone: 'UTC',
    embeddings: { 'They remember.': { fail: 'provider down' } },
    calls: [{
      text: 'w',
      signals: { keywords: [], paraphrase: 'They remember.', retrospective: true, timeRange: null, entities: ['Amy', 'Bo'] },
    }],
  },
  {
    name: 'probe_failure_skips_probe',
    zone: 'UTC',
    embeddings: {
      'They remember.': { vector: [1, 0], provider: 'BUILTIN' },
      'Amy Bo': { fail: 'probe down' },
    },
    calls: [{
      text: 'w',
      signals: { keywords: [], paraphrase: 'They remember.', retrospective: true, timeRange: null, entities: ['Amy', 'Bo'] },
    }],
  },
  {
    name: 'explicit_probes_override',
    zone: 'UTC',
    embeddings: {
      'a question': { vector: [1, 0], provider: 'OPENAI' },
      'probe one': { vector: [0, 1], provider: 'OPENAI' },
      'probe two': { vector: [1, 1], provider: 'OPENAI' },
    },
    calls: [{ text: '  a question  ', probes: [' probe one ', '', 'probe two', 'probe three'] }],
  },
  {
    name: 'skip_embedding',
    zone: 'UTC',
    embeddings: {},
    calls: [{ text: 'no vector please', skipEmbedding: true, probes: ['p'] }],
  },
  {
    name: 'memo_reused_across_calls',
    zone: 'UTC',
    sharedMemo: true,
    embeddings: { 'same words': { vector: [0.5, 0.5], provider: 'OPENAI' } },
    calls: [
      { text: 'same words', embeddingProfileId: 'prof-1' },
      { text: 'same words', embeddingProfileId: 'prof-1' },
      { text: 'same words' },
    ],
  },
  {
    name: 'distil_fails_falls_back',
    zone: 'UTC',
    distill: { kind: 'fail' },
    embeddings: { 'the recent window': { vector: [1, 0], provider: 'OPENAI' } },
    calls: [{
      text: 'the recent window',
      distill: {
        messages: [{ role: 'user', content: 'hi' }],
        characterName: 'Friday',
        clock: { nowIso: '2026-06-17T03:30:00.000Z', timelineMode: 'realtime' },
        latency: 'interactive',
      },
    }],
  },
  {
    name: 'distil_runs_second_zone',
    zone: 'America/Chicago',
    distill: {
      kind: 'json',
      text: '{"keywords":["fair","lantern"],"temporal":"past","context":"history","paraphrase":"They recall the lantern fair.","retrospective":true,"timeRange":{"from":"2026-06-10","to":"2026-06-12"},"entities":["Lighthouse Point"]}',
    },
    embeddings: {
      'They recall the lantern fair.': { vector: [1, 0, 0], provider: 'OPENAI' },
      'Lighthouse Point': { vector: [0, 1, 0], provider: 'OPENAI' },
      'They recall the lantern fair. (around 2026-06-10 to 2026-06-13)': { vector: [0, 0, 1], provider: 'OPENAI' },
    },
    calls: [{
      text: 'the recent window',
      distill: {
        messages: [
          { role: 'user', content: 'Do you remember the lantern fair?' },
          { role: 'assistant', content: 'Vaguely — tell me more.' },
          { role: 'user', content: 'At Lighthouse Point, the week before last.' },
        ],
        characterName: 'Friday',
        chatId: 'cc000000-0000-4000-8000-000000000001',
        characterId: 'c0000000-0000-4000-8000-000000000001',
        clock: { nowIso: '2026-06-17T03:30:00.000Z', timelineMode: 'realtime' },
        latency: 'interactive',
      },
    }],
  },
  {
    name: 'day_reference_overrides_range',
    zone: 'America/Chicago',
    distill: {
      kind: 'json',
      text: '{"keywords":["mission"],"paraphrase":"How the mission went.","retrospective":false,"timeRange":{"from":"2026-06-01","to":"2026-06-02"},"entities":[]}',
    },
    embeddings: {},
    calls: [{
      text: 'window',
      skipEmbedding: true,
      distill: {
        messages: [{ role: 'user', content: 'How did the mission go yesterday?' }],
        characterName: 'Friday',
        clock: { nowIso: '2026-06-17T03:30:00.000Z', timelineMode: 'realtime' },
      },
    }],
  },
  {
    name: 'narrative_timeline_no_scan',
    zone: 'America/Chicago',
    distill: {
      kind: 'json',
      text: '{"keywords":["mission"],"paraphrase":"How the mission went.","retrospective":false,"timeRange":null,"entities":[]}',
    },
    embeddings: {},
    calls: [{
      text: 'window',
      skipEmbedding: true,
      distill: {
        messages: [{ role: 'user', content: 'How did the mission go yesterday?' }],
        characterName: 'Friday',
        clock: { nowIso: '2026-06-17T03:30:00.000Z', timelineMode: 'narrative' },
      },
    }],
  },
  {
    name: 'distil_unparsable',
    zone: 'UTC',
    distill: { kind: 'json', text: 'not json at all' },
    embeddings: { 'the recent window': { vector: [1, 0], provider: 'OPENAI' } },
    calls: [{
      text: 'the recent window',
      distill: {
        messages: [{ role: 'user', content: 'hi' }],
        characterName: 'Friday',
        clock: { nowIso: '2026-06-17T03:30:00.000Z', timelineMode: 'realtime' },
      },
    }],
  },
  {
    name: 'empty_query_no_distil_no_embed',
    zone: 'UTC',
    distill: { kind: 'json', text: '{"keywords":["never"]}' },
    embeddings: {},
    calls: [{
      text: '   ',
      distill: { messages: [{ role: 'user', content: 'hi' }], characterName: 'Friday' },
    }],
  },
]

function queryJson(q: Record<string, any>) {
  return {
    text: q.text,
    embedding: q.embedding ? { vector: Array.from(q.embedding.embedding as Float32Array), provider: q.embedding.provider } : null,
    probes: (q.probes as Array<{ text: string; embedding: unknown }>).map(p => ({ text: p.text, embedded: !!p.embedding })),
    signals: q.signals ?? null,
    anchors: q.anchors,
    window: q.window,
    retrospective: q.retrospective,
    embeddingError: q.embeddingError ?? null,
  }
}

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write')
  const fs = await import('fs')

  const RealDate = Date
  const FakeDate = class extends RealDate {
    constructor(...args: unknown[]) {
      if (args.length === 0) super(NOW)
      // @ts-expect-error variadic forwarding
      else super(...args)
    }
    static now(): number {
      return NOW
    }
  } as DateConstructor
  ;(global as { Date: DateConstructor }).Date = FakeDate

  const lines: string[] = []
  for (const c of CASES) {
    jest.resetModules()
    const embedCalls: Array<{ text: string; profile: string | null }> = []
    jest.doMock('@/lib/embedding/embedding-service', () => ({
      __esModule: true,
      generateEmbeddingForUser: jest.fn(async (text: string, _u: string, profile?: string) => {
        embedCalls.push({ text, profile: profile ?? null })
        const spec = c.embeddings[text]
        if (!spec) throw new Error('no canned embedding')
        if (spec.fail) throw new Error(spec.fail)
        return { embedding: new Float32Array(spec.vector!), provider: spec.provider ?? 'OPENAI', model: 'm', dimensions: spec.vector!.length }
      }),
    }))
    let distillPrompt: Array<{ role: string; content: string }> | null = null
    jest.doMock('@/lib/memory/cheap-llm-tasks/core-execution', () => ({
      __esModule: true,
      executeCheapLLMTask: jest.fn(async (_sel: unknown, messages: unknown, _uid: unknown, parse: unknown) => {
        distillPrompt = (messages as Array<{ role: string; content: string }>).map(m => ({ role: m.role, content: m.content }))
        if (!c.distill || c.distill.kind === 'fail') return { success: false, error: 'canned failure' } as never
        return { success: true, result: (parse as (s: string) => unknown)(c.distill.text ?? '') } as never
      }),
    }))
    jest.doMock('@/lib/memory/time/calendar-refresh', () => ({
      __esModule: true,
      refreshMemoryTimeZone: jest.fn(async () => c.zone),
    }))

    const { logger } = await import('@/lib/logger')
    const logs: Array<{ level: string; message: string; fields: unknown }> = []
    for (const level of ['warn', 'debug'] as const) {
      const prior = (logger as unknown as Record<string, (...a: unknown[]) => unknown>)[level]
      ;(logger as unknown as Record<string, unknown>)[level] = (message: string, fields?: unknown) => {
        if (typeof message === 'string' && message.startsWith('[Recall]')) {
          logs.push(JSON.parse(JSON.stringify({ level, message, fields: fields ?? {} })))
        }
        return prior.call(logger, message, fields)
      }
    }

    const { buildRecallQuery } = await import('@/lib/memory/recall/query')
    const memo = c.sharedMemo ? new Map() : undefined
    const calls: unknown[] = []
    for (const call of c.calls) {
      const trace: unknown[] = []
      const q = await buildRecallQuery(
        {
          userId: 'u1',
          text: call.text,
          ...(call.embeddingProfileId ? { embeddingProfileId: call.embeddingProfileId } : {}),
          ...(call.signals !== undefined ? { signals: call.signals as never } : {}),
          ...(call.probes ? { probes: call.probes } : {}),
          ...(call.skipEmbedding ? { skipEmbedding: true } : {}),
          ...(memo ? { embeddingMemo: memo } : {}),
          ...(call.distill
            ? { distill: { ...call.distill, selection: { provider: 'OPENAI', modelName: 'cheap-model' } as never } }
            : {}),
        } as never,
        trace as never,
      )
      calls.push({ query: queryJson(q as never), trace })
    }
    lines.push(JSON.stringify({ name: c.name, spec: c, calls, embedCalls, distillPrompt, logs }))
  }
  ;(global as { Date: DateConstructor }).Date = RealDate
  fs.writeFileSync(outPath, lines.join('\n') + '\n')
  process.stderr.write(`recall-query oracle wrote ${outPath} (${lines.length} cases)\n`)
}

test('recall-query tier-3 oracle', async () => {
  await main()
}, 180000)
