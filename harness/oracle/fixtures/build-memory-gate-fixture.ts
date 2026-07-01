/**
 * Tier-3 fixture builder for the memory gate (v4 createMemoryWithGate).
 *
 * Bakes the SEED state both differential sides start from: for every scenario
 * with pre-existing memories it seeds one `memories` row per seed (pinned id +
 * `seedTimestamp`) and the matching per-character vector index (`saveMeta` +
 * `addEntry`, embedding = the seed's unit vector as a Float32Array), through v4's
 * REAL `MemoriesRepository` / `VectorIndicesRepository`. Both the jest oracle and
 * the Rust test then copy THIS fixture and run only the gate — which mints new
 * ids/timestamps — so the seed rows are byte-identical on both sides and only the
 * gate's own effect differs.
 *
 * Seed `memories.embedding` is left NULL (the gate reads the `vector_entries`
 * store, never `memories.embedding`, for its search); the vector lives in
 * `vector_entries`. Characters with no seed (`insert_empty`,
 * `skip_embedding_failed`) get nothing — their index is created (or not) by the
 * gate.
 *
 * Run from the v4 server checkout under Node (matches v4's real native binding):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-memory-gate-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-memory-gate-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedMemory {
  id: string;
  content: string;
  summary: string;
  vector: number[];
}
interface Scenario {
  name: string;
  characterId: string;
  seedMemories: SeedMemory[];
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  scenarios: Scenario[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'memory-gate-tier3.json'), 'utf8')
  ) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-memory-gate-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { MemoriesRepository } = await import('@/lib/database/repositories/memories.repository');
  const { VectorIndicesRepository } = await import(
    '@/lib/database/repositories/vector-indices.repository'
  );

  await initializeDatabase();
  const memories = new MemoriesRepository();
  const vectors = new VectorIndicesRepository();

  let seededMemories = 0;
  let seededVectors = 0;
  for (const scenario of spec.scenarios) {
    for (const seed of scenario.seedMemories) {
      await memories.create(
        {
          characterId: scenario.characterId,
          aboutCharacterId: null,
          chatId: null,
          projectId: null,
          content: seed.content,
          summary: seed.summary,
          keywords: [],
          tags: [],
          importance: 0.5,
          embedding: null,
          source: 'AUTO',
          witnessedContext: null,
          sourceMessageId: null,
          lastAccessedAt: null,
          reinforcementCount: 1,
          lastReinforcedAt: null,
          relatedMemoryIds: [],
          reinforcedImportance: 0.5,
        } as never,
        {
          id: seed.id,
          createdAt: spec.seedTimestamp,
          updatedAt: spec.seedTimestamp,
        }
      );
      seededMemories++;

      // Seed the per-character vector index: metadata + one entry per seed.
      await vectors.saveMeta(scenario.characterId, seed.vector.length);
      await vectors.addEntry({
        id: seed.id,
        characterId: scenario.characterId,
        embedding: new Float32Array(seed.vector),
      });
      seededVectors++;
    }
  }

  await closeDatabase();
  process.stderr.write(
    `built memory-gate fixture: ${out} (${seededMemories} seed memories, ${seededVectors} seed vectors)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-gate fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
