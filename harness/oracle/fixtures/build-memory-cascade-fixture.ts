/**
 * Tier-2 fixture builder — the seed state for the memory-service cascade-delete
 * family (v4 `lib/memory/memory-service.ts` `deleteMemoryWithVector` /
 * `deleteMemoriesBySourceMessageWithVectors` /
 * `deleteMemoriesBySourceMessagesWithVectors` /
 * `deleteMemoriesByChatIdWithVectors`).
 *
 * Seeds, through v4's REAL repositories:
 *   - one `memories` row per seed (pinned id + `seedTimestamp`, with the
 *     chatId / sourceMessageId / relatedMemoryIds the cascades read), and
 *   - the per-character vector stores: a `vector_indices` metadata row per
 *     character in `seedMetaCharacterIds` (including characters with NO entries —
 *     the untouched-store sentinel proof) plus one `vector_entries` row per seed
 *     that carries a vector (`vector: null` seeds get no entry — the `hasVector`
 *     counting proof).
 *
 * `saveMeta` / `addEntry` mint their timestamps, so after seeding the builder pins
 * every `vector_indices.createdAt/updatedAt` and `vector_entries.createdAt` to the
 * sentinel via rawQuery — the differential then distinguishes a flush-time
 * metadata bump (minted, collapsed to `<ts>`) from an untouched store (sentinel
 * preserved).
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
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { MemoriesRepository } = await import(
    '@/lib/database/repositories/memories.repository'
  );
  const { VectorIndicesRepository } = await import(
    '@/lib/database/repositories/vector-indices.repository'
  );

  await initializeDatabase();
  const memories = new MemoriesRepository();
  const vectors = new VectorIndicesRepository();

  // Metadata rows first — including entry-less characters (their stores must
  // exist so an untouched sweep leaves a provable sentinel).
  for (const characterId of spec.seedMetaCharacterIds) {
    await vectors.saveMeta(characterId, spec.dimensions);
  }

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
        embedding: null,
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
    if (seed.vector) {
      await vectors.addEntry({
        id: seed.id,
        characterId: seed.characterId,
        embedding: new Float32Array(seed.vector),
      });
      seededEntries++;
    }
  }

  // Pin the seed-minted vector timestamps to the sentinel (see header).
  await rawQuery('UPDATE vector_indices SET createdAt = ?, updatedAt = ?', [
    spec.seedTimestamp,
    spec.seedTimestamp,
  ]);
  await rawQuery('UPDATE vector_entries SET createdAt = ?', [spec.seedTimestamp]);

  await closeDatabase();
  process.stderr.write(
    `built memory-cascade fixture: ${out} (${spec.seedMemories.length} memories, ` +
      `${spec.seedMetaCharacterIds.length} metas, ${seededEntries} entries)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-cascade fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
