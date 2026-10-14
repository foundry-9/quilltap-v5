/**
 * Tier-2 fixture builder — the seed state for the memory-service cascade-delete
 * family (v4 `lib/memory/memory-service.ts` `deleteMemoryWithVector` /
 * `deleteMemoriesBySourceMessageWithVectors` /
 * `deleteMemoriesBySourceMessagesWithVectors` /
 * `deleteMemoriesByChatIdWithVectors`).
 *
 * Seeds, through v4's REAL repository, one `memories` row per seed (pinned
 * id + `seedTimestamp`, with the chatId / sourceMessageId / relatedMemoryIds
 * the cascades read). P4.D288 (v4 `af48f6cc2`, the `83334b9a5` pin): the
 * `vector_indices` / `vector_entries` mirror is gone — a seed's vector is the
 * row's own `embedding` (v4's `create` stores the quantized blob); a `vector:
 * null` seed has none (the loaded-index counting proof). `seedMetaCharacterIds`
 * and `dimensions` are kept in the committed spec, read by nothing.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-mem-cascade-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-memory-cascade-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedMemory {
  id: string;
  characterId: string;
  chatId: string | null;
  sourceMessageId: string | null;
  relatedMemoryIds: string[];
  vector: number[] | null;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  dimensions: number;
  seedMemories: SeedMemory[];
  seedMetaCharacterIds: string[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'memory-cascade-tier2.json'), 'utf8')
  ) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mem-cascade-build-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { MemoriesRepository } = await import(
    '@/lib/database/repositories/memories.repository'
  );
  await initializeDatabase();
  const memories = new MemoriesRepository();

  let seededEntries = 0;
  for (const seed of spec.seedMemories) {
    await memories.create(
      {
        characterId: seed.characterId,
        aboutCharacterId: null,
        chatId: seed.chatId,
        projectId: null,
        content: `content ${seed.id}`,
        summary: `summary ${seed.id}`,
        keywords: [],
        tags: [],
        importance: 0.5,
        embedding: seed.vector ? new Float32Array(seed.vector) : null,
        source: 'AUTO',
        witnessedContext: null,
        sourceMessageId: seed.sourceMessageId,
        lastAccessedAt: null,
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: seed.relatedMemoryIds,
        reinforcedImportance: 0.5,
      } as never,
      { id: seed.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
    if (seed.vector) seededEntries++;
  }

  await closeDatabase();
  process.stderr.write(
    `built memory-cascade fixture: ${out} (${spec.seedMemories.length} memories, ` +
      `${seededEntries} with a vector)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-cascade fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
