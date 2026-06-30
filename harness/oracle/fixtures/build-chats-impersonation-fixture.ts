/**
 * Tier-2 SEED fixture builder — the chats impersonation ops (Phase-2, the chats
 * repo — sub-unit 6: addImpersonation / removeImpersonation /
 * getImpersonatedParticipantIds / setActiveTypingParticipant /
 * updateAllLLMPauseTurnCount).
 *
 * Creates the spec chats (each with pinned participants — pinned participant
 * ids + createdAt/updatedAt) via v4's REAL `repos.chats.create` (ids + chat
 * createdAt/updatedAt pinned). The impersonation ops mint NO ids and NO
 * timestamps — they only rewrite `impersonatingParticipantIds` /
 * `activeTypingParticipantId` / `allLLMPauseTurnCount`, and `update` preserves
 * the chat's `updatedAt` — so EVERY timestamp stays at the pinned seed sentinel
 * and the whole `chats` dump is diffed exactly (ZERO normalization).
 *
 * Run (Node 24, from the v4 checkout):
 *   export PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-chimp-fixture.db \
 *     npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chats-impersonation-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  chats: Array<Record<string, unknown> & { id: string; createdAt: string; updatedAt: string }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'chats-impersonation-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the seed fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chimp-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');

  await initializeDatabase();

  const repo = new ChatsRepository();
  for (const c of spec.chats) {
    const { id, createdAt, updatedAt, ...data } = c;
    await repo.create(data as never, { id, createdAt, updatedAt });
  }

  await closeDatabase();
  process.stderr.write(`built chats impersonation seed fixture: ${out} (${spec.chats.length} chats)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats impersonation fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
