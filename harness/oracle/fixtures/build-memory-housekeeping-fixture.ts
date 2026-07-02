/**
 * Tier-2 fixture builder — the seed state for the housekeeping sweep (v4
 * `lib/memory/housekeeping.ts` `runHousekeeping` / `needsHousekeeping`).
 *
 * Seeds, through v4's REAL repositories, a per-character memory corpus carrying
 * the fields the sweep reads (importance / reinforcedImportance / source /
 * pinned createdAt / lastAccessedAt / lastReinforcedAt / reinforcementCount /
 * relatedMemoryIds) plus the per-character vector stores (metadata rows +
 * entries only for seeds with a vector). Vector-table timestamps are pinned to
 * the sentinel afterwards (saveMeta/addEntry mint), exactly like the cascade
 * fixture.
 *
 * ⏳ CORPUS FRESHNESS: housekeeping decisions depend on wall-clock age, so the
 * "recent" seed dates (2026-06-xx) age. The banked outcomes hold until roughly
 * 2026-12 (when `a1000005`'s age crosses the 6-month window). The differential
 * compares oracle-vs-Rust (not spec-pinned expectations), so both sides stay in
 * agreement even after that — but the header comments describing per-row
 * outcomes, and the Rust test's sanity row-counts, assume fresh dates. When
 * regenerating after 2026-12, refresh the recent dates in
 * memory-housekeeping-tier2.json first.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   PATH=$N:$PATH QT_FIXTURE_OUT=/tmp/qt-mem-housekeeping-fixture.db \
 *     npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-memory-housekeeping-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedMemory {
  id: string;
  characterId: string;
  importance: number;
  reinforcedImportance: number;
  source: 'AUTO' | 'MANUAL';
  createdAt: string;
  lastAccessedAt: string | null;
  lastReinforcedAt: string | null;
  reinforcementCount: number;
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
    readFileSync(join(here, 'memory-housekeeping-tier2.json'), 'utf8')
  ) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mem-housekeeping-build-'));
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

  for (const characterId of spec.seedMetaCharacterIds) {
    await vectors.saveMeta(characterId, spec.dimensions);
  }

  let seededEntries = 0;
  for (const seed of spec.seedMemories) {
    await memories.create(
      {
        characterId: seed.characterId,
        aboutCharacterId: null,
        chatId: null,
        projectId: null,
        content: `content ${seed.id}`,
        summary: `summary ${seed.id}`,
        keywords: [],
        tags: [],
        importance: seed.importance,
        embedding: null,
        source: seed.source,
        witnessedContext: null,
        sourceMessageId: null,
        lastAccessedAt: seed.lastAccessedAt,
        reinforcementCount: seed.reinforcementCount,
        lastReinforcedAt: seed.lastReinforcedAt,
        relatedMemoryIds: seed.relatedMemoryIds,
        reinforcedImportance: seed.reinforcedImportance,
      } as never,
      { id: seed.id, createdAt: seed.createdAt, updatedAt: spec.seedTimestamp }
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

  // Pin the seed-minted vector timestamps to the sentinel (see the cascade
  // builder for the rationale).
  await rawQuery('UPDATE vector_indices SET createdAt = ?, updatedAt = ?', [
    spec.seedTimestamp,
    spec.seedTimestamp,
  ]);
  await rawQuery('UPDATE vector_entries SET createdAt = ?', [spec.seedTimestamp]);

  await closeDatabase();
  process.stderr.write(
    `built memory-housekeeping fixture: ${out} (${spec.seedMemories.length} memories, ` +
      `${spec.seedMetaCharacterIds.length} metas, ${seededEntries} entries)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-housekeeping fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
