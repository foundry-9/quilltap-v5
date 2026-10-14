/**
 * Oracle case: the recall ranking (P4.D291 — v4 `lib/memory/scoring/rank.ts`,
 * NEW in `584d28164`, pinned at `83334b9a5`).
 *
 * Drives the REAL `rankForRecall`, `rankByBlend` and `reserveBackgroundSlots`
 * over a fixed corpus and emits NDJSON. Every row carries its INPUT (the
 * candidates, the recall context with its ring as an array, an optional
 * tuning INPUT resolved on both sides through `resolveRecallTuning`), so the
 * Rust side (`recall_rank_equivalence`) recomputes from the same bytes.
 *
 * Covered: the blend, the multipliers (scope / temporal / context /
 * participant / ring / window / fresh / cap / gate), `extraFired` labels, the
 * cross-project exclusion, the stable sort (ties, a NaN salience), and R6's
 * reservation arithmetic (fewer / more qualifying rows than the reserve, a
 * zero fraction, a head larger than the pool).
 *
 * IMPORTANT — imports the actual app code. Run from inside the v4 checkout (or
 * a worktree pinned at the round's pin) so `@/` resolves:
 *
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/recall-rank.ts \
 *     > /tmp/oracle-recall-rank.ndjson
 */

import { rankForRecall, rankByBlend, reserveBackgroundSlots, type RankInput } from '@/lib/memory/scoring/rank'
import { resolveRecallTuning, type RecallTuningInput } from '@/lib/memory/recall/tuning'
import type { RecallContext } from '@/lib/memory/recall/tags'
import type { RecallCandidate } from '@/lib/memory/recall/types'
import type { Memory } from '@/lib/schemas/types'

const NOW = Date.parse('2026-06-15T12:00:00.000Z')
const DAY = 86_400_000
const iso = (msAgo: number) => new Date(NOW - msAgo).toISOString()

let n = 0
function mem(over: Partial<Memory> & { id?: string }): Memory {
  n++
  return {
    id: over.id ?? `a0000000-0000-4000-8000-${String(n).padStart(12, '0')}`,
    characterId: 'c0000000-0000-4000-8000-000000000001',
    content: 'content',
    summary: 'summary',
    keywords: [],
    tags: [],
    importance: 0.5,
    source: 'AUTO',
    kind: 'semantic',
    entities: [],
    reinforcementCount: 1,
    relatedMemoryIds: [],
    tier: 'hot',
    consolidatedFrom: [],
    createdAt: iso(10 * DAY),
    updatedAt: iso(10 * DAY),
    ...over,
  } as Memory
}

interface CtxInput {
  currentProjectId?: string | null
  scopePolicy?: 'down-weight' | 'exclude'
  presentAboutCharacterIds?: string[]
  turnContext?: string | null
  turnTemporal?: string | null
  turnRetrospective?: boolean
  occurredWithin?: { from: string; to: string } | null
  expandRelated?: boolean
  recentlyWhisperedIds?: string[]
  currentChatId?: string
  nowMs?: number
  tuningInput?: RecallTuningInput
  presentParticipantNames?: string[]
}

function toCtx(c: CtxInput): RecallContext {
  const ctx = {
    currentProjectId: c.currentProjectId ?? null,
    scopePolicy: c.scopePolicy ?? 'down-weight',
    presentAboutCharacterIds: c.presentAboutCharacterIds ?? [],
    turnContext: c.turnContext ?? null,
    turnTemporal: c.turnTemporal ?? null,
    expandRelated: c.expandRelated ?? false,
  } as RecallContext
  if (c.turnRetrospective !== undefined) ctx.turnRetrospective = c.turnRetrospective
  if (c.occurredWithin !== undefined) ctx.occurredWithin = c.occurredWithin
  if (c.recentlyWhisperedIds !== undefined) ctx.recentlyWhisperedIds = new Set(c.recentlyWhisperedIds)
  if (c.currentChatId !== undefined) ctx.currentChatId = c.currentChatId
  if (c.nowMs !== undefined) ctx.nowMs = c.nowMs
  if (c.tuningInput !== undefined) ctx.tuning = resolveRecallTuning(c.tuningInput)
  if (c.presentParticipantNames !== undefined) ctx.presentParticipantNames = c.presentParticipantNames
  return ctx
}

function inputJson(c: RankInput) {
  return {
    memory: c.memory,
    score: c.score,
    rawScore: c.rawScore,
    salience: Number.isNaN(c.salience) ? 'NaN' : c.salience,
    usedEmbedding: c.usedEmbedding,
  }
}

function candJson(c: RecallCandidate) {
  return {
    id: c.memory.id,
    score: c.score,
    usedEmbedding: c.usedEmbedding,
    salience: Number.isNaN(c.salience) ? 'NaN' : c.salience,
    recallAdjustment: c.recallAdjustment
      ? {
          multiplier: c.recallAdjustment.multiplier,
          fired: c.recallAdjustment.fired,
          blendedBefore: Number.isNaN(c.recallAdjustment.blendedBefore) ? 'NaN' : c.recallAdjustment.blendedBefore,
          blendedAfter: Number.isNaN(c.recallAdjustment.blendedAfter) ? 'NaN' : c.recallAdjustment.blendedAfter,
        }
      : null,
  }
}

const out: unknown[] = []
const ri = (memory: Memory, score: number, salience: number, rawScore = score, usedEmbedding = true): RankInput =>
  ({ memory, score, rawScore, salience, usedEmbedding })

// ---- the pools ---------------------------------------------------------------
const P1 = 'p0000000-0000-4000-8000-000000000001'
const P2 = 'p0000000-0000-4000-8000-000000000002'
const OTHER = 'c0000000-0000-4000-8000-0000000000aa'
const CHAT = 'cc000000-0000-4000-8000-000000000001'

const tagged = [
  ri(mem({ keywords: ['tea', 'past'] }), 0.62, 0.40),
  ri(mem({ keywords: ['moment'] }), 0.55, 0.45),
  ri(mem({ keywords: ['scope: narrow'], projectId: P1 }), 0.58, 0.35),
  ri(mem({ keywords: ['scope: narrow'], projectId: P2 }), 0.70, 0.50),
  ri(mem({ keywords: ['relationships'], aboutCharacterId: OTHER }), 0.48, 0.30),
  ri(mem({ keywords: ['history', 'present'] }), 0.44, 0.60),
  ri(mem({ keywords: [], occurredAt: iso(2 * DAY) }), 0.51, 0.20),
  ri(mem({ keywords: [], createdAt: iso(6 * 3_600_000), chatId: 'cc000000-0000-4000-8000-0000000000ff' }), 0.40, 0.25),
  ri(mem({ keywords: [], createdAt: iso(6 * 3_600_000), chatId: CHAT }), 0.40, 0.25),
  ri(mem({ keywords: ['future'] }), 0.66, 0.10, 0.30),
]
const ringId = tagged[1].memory.id

const ties = [
  ri(mem({}), 0.5, 0.5),
  ri(mem({}), 0.5, 0.5),
  ri(mem({}), 0.6, 0.3),
  ri(mem({}), 0.5, 0.5),
  ri(mem({ createdAt: 'not a date' }), 0.9, NaN),
  ri(mem({}), 0.45, 0.62),
]

const contexts: Array<[string, CtxInput]> = [
  ['bare', {}],
  ['project-down-weight', { currentProjectId: P1 }],
  ['project-exclude', { currentProjectId: P1, scopePolicy: 'exclude' }],
  ['context-relationships', { turnContext: 'relationships', presentAboutCharacterIds: [OTHER] }],
  ['retrospective', { turnRetrospective: true, recentlyWhisperedIds: [ringId] }],
  ['ring', { recentlyWhisperedIds: [ringId] }],
  ['window', { occurredWithin: { from: iso(3 * DAY), to: iso(1 * DAY) } }],
  ['fresh', { nowMs: NOW, currentChatId: CHAT }],
  ['gate-off', { tuningInput: { boostGateAbs: null, boostGateMargin: null } as RecallTuningInput, currentProjectId: P1 }],
  ['legacy-cap4', {
    tuningInput: { boostGateAbs: 0, boostGateMargin: 0, boostCap: 4, specificAnchors: false } as RecallTuningInput,
    turnContext: 'relationships',
    presentAboutCharacterIds: [OTHER],
    nowMs: NOW,
    currentChatId: CHAT,
  }],
  ['everything', {
    currentProjectId: P1,
    turnContext: 'history',
    turnRetrospective: true,
    presentAboutCharacterIds: [OTHER],
    occurredWithin: { from: iso(3 * DAY), to: iso(1 * DAY) },
    recentlyWhisperedIds: [ringId],
    nowMs: NOW,
    currentChatId: CHAT,
  }],
]

for (const [name, ctxIn] of contexts) {
  for (const [poolName, pool] of [['tagged', tagged], ['ties', ties]] as const) {
    for (const [bestName, best] of [['best', 0.7], ['low-best', 0.45]] as const) {
      for (const extra of [[], ['related↗'], ['bg↺', 'x']]) {
        const ranked = rankForRecall(pool, toCtx(ctxIn), best, extra)
        out.push({
          kind: 'rankForRecall',
          id: `${name}/${poolName}/${bestName}/${extra.join('+') || 'none'}`,
          input: { candidates: pool.map(inputJson), context: ctxIn, bestCosine: best, extraFired: extra },
          output: ranked.map(candJson),
        })
      }
    }
  }
}

for (const [poolName, pool] of [['tagged', tagged], ['ties', ties]] as const) {
  out.push({
    kind: 'rankByBlend',
    id: poolName,
    input: { candidates: pool.map(inputJson) },
    output: rankByBlend(pool).map(candJson),
  })
}

// ---- R6 ------------------------------------------------------------------------
const bctx = toCtx({ nowMs: NOW, currentChatId: CHAT })
const rankedPool = rankForRecall(tagged, bctx, 0.7)
const backgroundPool = rankForRecall(ties.slice(0, 4), bctx, 0.7, ['bg↺'])
const reserveCases: Array<[string, number, number, number]> = [
  // [id, headSize, fraction, backgroundCount]
  ['quarter-of-8', 8, 0.25, 4],
  ['half-of-8', 8, 0.5, 4],
  ['half-of-8-one-bg', 8, 0.5, 1],
  ['zero-fraction', 8, 0, 4],
  ['no-background', 8, 0.5, 0],
  ['head-larger-than-pool', 20, 0.5, 4],
  ['odd-head', 5, 0.5, 4],
  ['head-one', 1, 0.5, 4],
]
for (const [id, headSize, fraction, bgCount] of reserveCases) {
  const bg = backgroundPool.slice(0, bgCount)
  out.push({
    kind: 'reserve',
    id,
    input: {
      ranked: rankedPool.map(candJson),
      background: bg.map(candJson),
      headSize,
      fraction,
    },
    output: reserveBackgroundSlots(rankedPool, bg, headSize, fraction).map(c => c.memory.id),
  })
}

out.push({ kind: 'meta', nowMs: NOW })
for (const row of out) process.stdout.write(JSON.stringify(row) + '\n')
