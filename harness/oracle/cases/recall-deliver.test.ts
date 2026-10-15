/**
 * @jest-environment node
 *
 * Oracle case: recall delivery (P4.D291 — v4 `lib/memory/recall/deliver.ts`,
 * `584d28164`, pinned at `83334b9a5`).
 *
 * Drives the REAL `survivingLines`, `traceLines`, `ringFromDebugLog`,
 * `withRecallLines` and `deliverRecall`. Only the repository factory is
 * replaced — by RECORDERS: `memories.updateAccessTimeBulk(characterId, ids)`
 * and `chats.update(chatId, patch)` record their arguments (a `failRing` case
 * makes `chats.update` throw), so each delivery row carries exactly what v4
 * would write: the stamped ids and the ring payload. The Rust side
 * (`recall_deliver_equivalence`) runs v5's `deliver_recall` over a REAL
 * provisioned scratch DB and compares those writes with what landed (the
 * stamped rows' `lastAccessedAt`, the chat's `commonplaceRecallHistory`).
 *
 * Rows: `surviving` (verbatim, case / whitespace, the whole-line rule, the
 * 3-word floor, the 0.6 share, the Unicode strip, the compressed arm),
 * `traceLines`, `deliver` (the record, the counts, the ring incl. the retro
 * signature delivered / not delivered / already-trimmed, persistRing off, a
 * failed ring write, compression losses), `ring` (`ringFromDebugLog`) and
 * `withRecall`.
 *
 * Stage the case OUTSIDE any `.claude/` path (v4's jest ignores it):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-recall-deliver-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/recall-deliver.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   TZ=UTC QT_ORACLE_OUT=/tmp/oracle-recall-deliver.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 --roots "$PWD" --roots "$TMPO/cases" -- recall-deliver
 */

interface Line { memoryId: string; line: string; body: string }

const L = (memoryId: string, line: string, body: string): Line => ({ memoryId, line, body })

const SURVIVING: Array<{ id: string; lines: Line[]; text: string; compressed?: boolean }> = [
  { id: 'empty-text', lines: [L('m1', '- likes tea', 'likes tea')], text: '' },
  { id: 'verbatim', lines: [L('m1', '- likes tea', 'likes tea'), L('m2', '- hates rain', 'hates rain')], text: 'x\n- likes tea\ny' },
  { id: 'case-whitespace', lines: [L('m1', '- Likes   TEA\tnow', 'likes tea')], text: 'header\n-   likes tea now  \n' },
  { id: 'body-alone-not-enough', lines: [L('m1', '[yesterday] likes tea', 'likes tea')], text: 'Someone likes tea a lot.' },
  { id: 'compressed-overlap', compressed: true,
    lines: [L('m1', '- Amy walked the harbour wall at dawn', 'Amy walked the harbour wall at dawn')],
    text: 'Summary: at dawn Amy walked along the harbour.' },
  { id: 'compressed-below-share', compressed: true,
    lines: [L('m1', '- Amy walked the harbour wall at dawn', 'Amy walked the harbour wall at dawn')],
    text: 'Amy was at the harbour.' },
  { id: 'compressed-two-words', compressed: true,
    lines: [L('m1', '- likes tea', 'likes tea')], text: 'she likes her tea' },
  { id: 'compressed-across-lines-no', compressed: true,
    lines: [L('m1', '- Amy walked harbour wall dawn', 'Amy walked harbour wall dawn')],
    text: 'Amy walked\nharbour wall dawn' },
  { id: 'unicode-strip', compressed: true,
    lines: [L('m1', '- Zoë’s café — naïve résumé, coöperation!', 'Zoë’s café — naïve résumé, coöperation!')],
    text: 'zoë’s café; naïve: résumé (coöperation)' },
  { id: 'apostrophe-hyphen-kept', compressed: true,
    lines: [L('m1', "- the well-known o'clock bell rang", "the well-known o'clock bell rang")],
    text: "THE well-known O'CLOCK bell" },
  { id: 'duplicate-words', compressed: true,
    lines: [L('m1', '- tea tea tea tea biscuits garden', 'tea tea tea tea biscuits garden')],
    text: 'tea biscuits garden' },
  { id: 'uncompressed-no-overlap', lines: [L('m1', '- Amy walked the harbour wall at dawn', 'Amy walked the harbour wall at dawn')],
    text: 'Summary: at dawn Amy walked the harbour wall.' },
]

const TRACE = [
  { stage: 'query', detail: { distilled: 'ran', textLength: 12, embedded: true, embeddingCalls: 1, probes: 0, anchors: 0, window: null, retrospective: false } },
  { stage: 'gather', label: 'head', detail: { strategy: 'vector', vector: 3, probes: 0, probeAdded: 0, anchors: ['Lighthouse Point'], anchorAdded: 1, literal: false, pool: 4 } },
  { stage: 'score', label: 'head', detail: { floor: 0.3, floored: 1, bestCosine: 0.812, window: 'none', excluded: 0, ranked: 3 } },
  { stage: 'assemble', label: 'retrospective', detail: { suppressed: true, signature: '∅|amy' } },
  { stage: 'gather', label: '', detail: { strategy: 'text', reason: 'no embedding', probes: 1, anchors: [], pool: 0 } },
]

const ARCH = [L('a1', '- archive one', 'archive one'), L('a2', '- archive two', 'archive two')]
const HEAD = [L('h1', '[yesterday] head one', 'head one'), L('h2', '[today] head two', 'head two'), L('h3', '[today] head three', 'head three')]
const INTER = [L('i1', '- About Amy: [today] inter one', 'inter one')]
const RELEVANT = `## Memory Anchors\n${ARCH.map(l => l.line).join('\n')}\n\nMost relevant memories for this turn:\n${HEAD.map(l => l.line).join('\n')}`
const INTER_SECTION = `## Memories About Other Characters\n${INTER[0].line}`
const RETRO = '### Relevant Past Conversations\n\n#### The fair (`c0000000-0000-4000-8000-000000000001`)'

const DELIVER: Array<{
  id: string
  lines: Array<[string, Line[]]>
  sections: Record<string, string>
  retroSignature?: string | null
  trace?: unknown[]
  deliveredText: string
  history?: unknown
  persistRing?: boolean
  failRing?: boolean
}> = [
  { id: 'all-delivered', lines: [['archive', ARCH], ['head', HEAD], ['interCharacter', INTER]],
    sections: { relevant: RELEVANT, interCharacter: INTER_SECTION },
    trace: TRACE, deliveredText: `${RELEVANT}\n\n---\n\n${INTER_SECTION}`, history: { turns: [['x'], ['y']] } },
  { id: 'nothing-rendered', lines: [], sections: {}, deliveredText: 'Current state only' },
  { id: 'empty-pools-kept-in-order', lines: [['archive', []], ['head', HEAD.slice(0, 1)]],
    sections: { relevant: `Most relevant memories for this turn:\n${HEAD[0].line}` },
    deliveredText: `Most relevant memories for this turn:\n${HEAD[0].line}` },
  { id: 'head-lost-to-compression', lines: [['archive', ARCH], ['head', HEAD]],
    sections: { relevant: RELEVANT }, deliveredText: `${ARCH.map(l => l.line).join('\n')}\nhead two, roughly` },
  { id: 'retro-signature-delivered', lines: [['head', HEAD.slice(0, 1)]],
    sections: { relevant: `Most relevant memories for this turn:\n${HEAD[0].line}`, retrospective: `  ${RETRO}  ` },
    retroSignature: '2026-06-01T00:00:00.000Z→2026-06-07T23:59:59.999Z|amy',
    deliveredText: `Most relevant memories for this turn:\n${HEAD[0].line}\n\n---\n\n${RETRO}`,
    history: { turns: [['a'], ['b'], ['c']], retroSignatures: ['s1', 's2', 's3'] } },
  { id: 'retro-only-signature', lines: [], sections: { retrospective: RETRO }, retroSignature: '∅|lighthouse point',
    deliveredText: `Current state\n\n${RETRO}` },
  { id: 'retro-not-delivered', lines: [['head', HEAD.slice(0, 2)]],
    sections: { relevant: `Most relevant memories for this turn:\n${HEAD.slice(0, 2).map(l => l.line).join('\n')}`, retrospective: RETRO },
    retroSignature: '∅|bo', deliveredText: `Most relevant memories for this turn:\n${HEAD.slice(0, 2).map(l => l.line).join('\n')}` },
  { id: 'persist-ring-off', lines: [['head', HEAD]], sections: { relevant: `Most relevant memories for this turn:\n${HEAD.map(l => l.line).join('\n')}` },
    deliveredText: `Most relevant memories for this turn:\n${HEAD.map(l => l.line).join('\n')}`, persistRing: false },
  { id: 'ring-write-fails', lines: [['head', HEAD.slice(0, 1)]], sections: { relevant: `Most relevant memories for this turn:\n${HEAD[0].line}` },
    deliveredText: `Most relevant memories for this turn:\n${HEAD[0].line}`, failRing: true },
  { id: 'inter-only', lines: [['interCharacter', INTER]], sections: { interCharacter: INTER_SECTION },
    deliveredText: `x\n\n${INTER_SECTION}` },
  { id: 'malformed-history', lines: [['head', HEAD.slice(0, 1)]], sections: { relevant: `Most relevant memories for this turn:\n${HEAD[0].line}` },
    deliveredText: `Most relevant memories for this turn:\n${HEAD[0].line}`, history: { turns: 'nope', retroSignatures: [1, 's'] } },
]

const RINGS: Array<string[] | null | undefined> = [
  null, undefined, [], ['[Recall] deliver: nothing'],
  ['[Recall] query: {}', '[Recall:ring] a b  c', '[Recall:ring] z'],
  ['[Recall:ring]'], ['x', '[Recall:ring]   '], ['[Recall:ringx] q'],
]

const WITH: Array<[string[] | null | undefined, string[]]> = [
  [null, ['extraction line']],
  [undefined, []],
  [['[Recall] a', 'old extraction', '[Recall:ring] h1', '[Recall]x', 'Recall'], ['new 1', 'new 2']],
]

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write')
  const fs = await import('fs')
  const lines: string[] = []

  let stampCalls: Array<{ characterId: string; ids: string[] }> = []
  let ringWrites: Array<{ chatId: string; patch: unknown }> = []
  let failRing = false
  jest.resetModules()
  jest.doMock('@/lib/repositories/factory', () => ({
    __esModule: true,
    getRepositories: () => ({
      memories: {
        updateAccessTimeBulk: async (characterId: string, ids: string[]) => {
          stampCalls.push({ characterId, ids: [...ids] })
          return ids.length
        },
      },
      chats: {
        update: async (chatId: string, patch: unknown) => {
          if (failRing) throw new Error('database is locked')
          ringWrites.push({ chatId, patch: JSON.parse(JSON.stringify(patch)) })
          return null
        },
      },
    }),
  }))
  const { logger } = await import('@/lib/logger')
  let logs: Array<{ level: string; message: string; fields: unknown }> = []
  for (const level of ['warn', 'debug'] as const) {
    const prior = (logger as unknown as Record<string, (...a: unknown[]) => unknown>)[level]
    ;(logger as unknown as Record<string, unknown>)[level] = (message: string, fields?: unknown) => {
      if (typeof message === 'string' && message.startsWith('[Recall]')) {
        logs.push(JSON.parse(JSON.stringify({ level, message, fields: fields ?? {} })))
      }
      return prior.call(logger, message, fields)
    }
  }
  const d = await import('@/lib/memory/recall/deliver')

  for (const c of SURVIVING) {
    const kept = d.survivingLines(c.lines, c.text, { compressed: c.compressed === true })
    lines.push(JSON.stringify({ kind: 'surviving', id: c.id, input: c, kept: kept.map(l => l.memoryId) }))
  }
  lines.push(JSON.stringify({ kind: 'traceLines', input: TRACE, output: d.traceLines(TRACE as never) }))

  for (const c of DELIVER) {
    stampCalls = []
    ringWrites = []
    logs = []
    failRing = c.failRing === true
    const bundle = {
      lines: Object.fromEntries(c.lines),
      sections: c.sections,
      retroSignature: c.retroSignature ?? null,
      trace: c.trace ?? [],
    }
    const delivery = await d.deliverRecall(bundle as never, {
      characterId: 'c0000000-0000-4000-8000-000000000001',
      chat: { id: 'cc000000-0000-4000-8000-000000000001', commonplaceRecallHistory: c.history },
      deliveredText: c.deliveredText,
      ...(c.persistRing === false ? { persistRing: false } : {}),
    })
    await new Promise(resolve => setImmediate(resolve))
    lines.push(JSON.stringify({
      kind: 'deliver',
      id: c.id,
      input: c,
      delivered: Object.entries(delivery.delivered),
      lost: delivery.lost,
      debugLog: delivery.debugLog,
      stampCalls,
      ringWrites,
      logs,
    }))
  }

  for (const ring of RINGS) {
    lines.push(JSON.stringify({ kind: 'ring', input: ring ?? null, undefinedInput: ring === undefined, output: d.ringFromDebugLog(ring) }))
  }
  for (const [existing, add] of WITH) {
    lines.push(JSON.stringify({ kind: 'withRecall', existing: existing ?? null, add, output: d.withRecallLines(existing, add) }))
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n')
  process.stderr.write(`recall-deliver oracle wrote ${outPath} (${lines.length} rows)\n`)
}

test('recall-deliver oracle', async () => {
  await main()
}, 180000)
