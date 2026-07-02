/**
 * Tier-3 fixture builder for the memory gate's watermark auto-housekeeping
 * check (v4 `maybeEnqueueHousekeeping` / `enqueueMemoryHousekeeping`).
 *
 * Bakes the MAIN-db seed state both sides start from: two `chat_settings`
 * rows (auto-housekeeping enabled with a per-character override / disabled),
 * per-character `memories` ballast sized so each scenario's single INSERT
 * lands exactly at (or below) the watermark AFTER the write, and two pinned
 * `background_jobs` rows — a COMPLETED sweep with a FUTURE `updatedAt` (always
 * inside the 15-minute throttle window, wall-clock-proof) and a PENDING
 * housekeeping job (the in-flight dedupe target). No vaults, no vectors —
 * candidates carry `aboutCharacterId: null` and every store starts empty.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-memory-watermark.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-memory-watermark-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  chatSettings: Array<{
    id: string;
    userId: string;
    autoHousekeepingSettings: Record<string, unknown>;
  }>;
  seedMemories: Array<{ id: string; characterId: string; content: string; summary: string }>;
  seedJobs: Array<{
    id: string;
    userId: string;
    type: string;
    status: string;
    payload: Record<string, unknown>;
    createdAt: string;
    updatedAt: string;
  }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'memory-watermark-tier3.json'), 'utf8')
  ) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) throw new Error('QT_FIXTURE_OUT must point at the fixture .db to write');
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-memory-watermark-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { VectorIndicesRepository } = await import(
    '@/lib/database/repositories/vector-indices.repository'
  );

  await initializeDatabase();
  const repos = getRepositories();

  // Materialize the vector tables (v4 ensureCollections them lazily on first
  // repository use; the Rust store expects them to exist). A read suffices.
  await new VectorIndicesRepository().findMetaByCharacterId(
    '00000000-0000-4000-8000-000000000000'
  );

  for (const cs of spec.chatSettings) {
    await repos.chatSettings.create(
      {
        userId: cs.userId,
        autoHousekeepingSettings: cs.autoHousekeepingSettings,
      } as never,
      { id: cs.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  for (const seed of spec.seedMemories) {
    await repos.memories.create(
      {
        characterId: seed.characterId,
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
      { id: seed.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  for (const job of spec.seedJobs) {
    await repos.backgroundJobs.create(
      {
        userId: job.userId,
        type: job.type,
        status: job.status,
        payload: job.payload,
        priority: 0,
        attempts: 0,
        maxAttempts: 1,
        lastError: null,
        scheduledAt: job.createdAt,
        startedAt: null,
        completedAt: null,
      } as never,
      { id: job.id, createdAt: job.createdAt, updatedAt: job.updatedAt }
    );
  }

  await closeDatabase();
  process.stderr.write(
    `built memory-watermark fixture: ${out} (${spec.chatSettings.length} settings rows, ` +
      `${spec.seedMemories.length} seed memories, ${spec.seedJobs.length} seed jobs)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-watermark fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
