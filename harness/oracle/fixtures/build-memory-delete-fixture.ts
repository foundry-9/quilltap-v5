/**
 * Tier-2 fixture builder — a pre-seeded `memories` GRAPH for the deletion
 * chokepoint (v4 `lib/memory/memory-gate.ts`
 * `deleteMemoryWithUnlink` / `deleteMemoriesWithUnlinkBatch`).
 *
 * Runs v4's REAL `repos.memories.create(data, { id, createdAt, updatedAt })` for
 * each seed row (ids + timestamps pinned from `memory-delete-tier2.json`), so the
 * baked graph is fully deterministic. Both the oracle (`memory-delete-tier2.ts`)
 * and the Rust port then run the DELETE ops on a COPY of this SAME fixture, so the
 * seeded ids + createdAt match exactly and only the neighbour-unlink `updatedAt`
 * mints diverge (sentinel-normalized).
 *
 * The seed only carries `id` / `characterId` / `relatedMemoryIds` (the columns the
 * unlink scan reads); the builder fills the remaining required fields with fixed
 * defaults so `MemorySchema` validation passes.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-mem-delete-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-memory-delete-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedRow {
  id: string;
  characterId: string;
  relatedMemoryIds: string[];
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  seed: SeedRow[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'memory-delete-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mem-delete-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { MemoriesRepository } = await import(
    '@/lib/database/repositories/memories.repository'
  );
  const { MemorySchema } = await import('@/lib/schemas/types');

  await initializeDatabase();
  await ensureCollection('memories', MemorySchema);

  const repo = new MemoriesRepository();
  for (const row of spec.seed) {
    await repo.create(
      {
        characterId: row.characterId,
        content: `content ${row.id}`,
        summary: `summary ${row.id}`,
        keywords: [],
        tags: [],
        importance: 0.5,
        source: 'AUTO',
        reinforcementCount: 1,
        reinforcedImportance: 0.5,
        relatedMemoryIds: row.relatedMemoryIds,
      } as never,
      { id: row.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp },
    );
  }

  await closeDatabase();
  process.stderr.write(`built memory-delete fixture: ${out} (${spec.seed.length} rows)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-delete fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
