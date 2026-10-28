/**
 * @jest-environment node
 *
 * Oracle case: the recall probe fixture (P4.174 / C-3 — P4.D291 Tier 2 item
 * 24). v4's REAL `searchRecall` (`lib/memory/recall/search.ts`) over v4's OWN
 * probe set (`__tests__/fixtures/recall-probes/probe-set.json`, read at the
 * pin AT RUN TIME — never vendored) through v4's own `probeWorld` seams
 * (`harness.ts`: the in-memory index, the repository reads, the canned
 * embedding table), exactly as `probe-fixture.test.ts` wires them.
 *
 * One NDJSON row per turn (A, B, C, F1, F2, M, O2, O3, O4): the candidates
 * `{id, score, salience, blendedAfter, fired, usedEmbedding}` that v4's CURRENT
 * arithmetic returns (the fixture's own `expected` rows, captured pre-Phase E,
 * are NOT the comparand), plus the turn's `gold` and the head size so the
 * Rust side can re-derive gold-at-head. `rust: recall_probe_fixture_equivalence`
 * plants the same corpus on a scratch DB and runs v5's `search_recall`.
 *
 * Stage the case OUTSIDE any `.claude/` path (v4's jest ignores it):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-recall-probe-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/recall-probe-fixture.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   TZ=UTC QT_ORACLE_OUT=/tmp/oracle-recall-probe.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 --roots "$PWD" --roots "$TMPO/cases" -- recall-probe-fixture
 */

jest.mock('@/lib/repositories/factory', () => ({ getRepositories: jest.fn() }))
jest.mock('@/lib/embedding/embedding-service', () => ({
  generateEmbeddingForUser: jest.fn(),
  EmbeddingError: class EmbeddingError extends Error {},
  cosineSimilarity: jest.fn(),
}))
jest.mock('@/lib/memory/index-store', () => ({
  memoryIndex: { search: jest.fn(), shape: jest.fn() },
  embeddingTextFor: jest.fn(),
}))

import { searchRecall } from '@/lib/memory/recall/search'
import { getRepositories } from '@/lib/repositories/factory'
import { generateEmbeddingForUser, cosineSimilarity } from '@/lib/embedding/embedding-service'
import { memoryIndex } from '@/lib/memory/index-store'

test('recall-probe-fixture oracle', async () => {
  const outPath = process.env.QT_ORACLE_OUT
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write')
  const fs = await import('fs')
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const harness = require(`${process.cwd()}/__tests__/fixtures/recall-probes/harness`)
  const fixture = harness.loadProbeFixture()
  const world = harness.probeWorld(fixture)

  ;(getRepositories as jest.Mock).mockReturnValue({ memories: world.memories })
  ;(generateEmbeddingForUser as jest.Mock).mockImplementation((text: string) => world.embed(text))
  // The stand-in index scores with a NORMALISING cosine; the real index store
  // (and v5's `CharacterVectorStore`) score with v4's `cosineSimilarity` — a
  // plain dot product over unit vectors. The probe set's vectors are unit only
  // to float32 rounding (~4e-7), so the index here ranks with the REAL one.
  const realCosine = jest.requireActual('@/lib/embedding/embedding-service').cosineSimilarity as (a: ArrayLike<number>, b: ArrayLike<number>) => number
  ;(cosineSimilarity as jest.Mock).mockImplementation(realCosine)
  ;(memoryIndex.search as jest.Mock).mockImplementation(
    async (_c: string, vector: Float32Array, opts: { limit: number; hotOnly: boolean; exclude?: ReadonlySet<string> }) =>
      world.rows
        .filter((r: any) => r.embedding.length === vector.length)
        .filter((r: any) => !opts.hotOnly || r.tier !== 'cold')
        .filter((r: any) => !opts.exclude || !opts.exclude.has(r.id))
        .map((r: any) => ({ id: r.id, score: realCosine(vector, r.embedding), metadata: { memoryId: r.id, characterId: r.characterId, tier: r.tier } }))
        .sort((a: any, b: any) => b.score - a.score)
        .slice(0, opts.limit),
  )
  ;(memoryIndex.shape as jest.Mock).mockImplementation(world.index.shape)

  const lines: string[] = []
  for (const turn of fixture.turns) {
    const probes = []
    for (const text of turn.extraProbes) probes.push({ text, embedding: await world.embed(text) })
    const query = {
      text: turn.query,
      embedding: (await world.embed(turn.query)) as never,
      probes: probes as never,
      signals: null,
      anchors: turn.entityAnchors,
      window: turn.occurredWithin,
      retrospective: turn.recallContext.turnRetrospective,
    }
    const { candidates } = await searchRecall(fixture.characterId, query, {
      limit: turn.limit,
      minImportance: turn.minImportance,
      headSize: turn.headSize,
      nowMs: turn.weightClockMs,
      recallContext: {
        ...turn.recallContext,
        recentlyWhisperedIds: new Set(turn.recallContext.recentlyWhisperedIds),
      } as never,
    })
    lines.push(
      JSON.stringify({
        slot: turn.slot,
        headSize: turn.headSize,
        gold: turn.gold,
        candidates: candidates.map((c: any) => ({
          id: c.memory.id,
          score: c.score,
          salience: c.salience,
          blendedAfter: c.recallAdjustment?.blendedAfter ?? null,
          fired: c.recallAdjustment?.fired ?? [],
          usedEmbedding: c.usedEmbedding,
        })),
      }),
    )
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n')
  process.stderr.write(`recall-probe-fixture oracle wrote ${outPath} (${lines.length} rows)\n`)
}, 180000)
