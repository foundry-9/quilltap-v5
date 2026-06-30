/**
 * Tier-2 SEED fixture builder — the chats messages mutation path (Phase-2, the
 * chats repo — sub-unit 4b: updateMessage / deleteMessagesByIds / clearMessages).
 *
 * Creates the spec chats and seeds each with its messages via v4's REAL
 * `repos.chats.create` + `.addMessages` (ids + message createdAt pinned). The
 * chat-metadata `lastMessageAt`/`updatedAt` that `addMessages` mints are baked
 * into THIS fixture once and read identically by both the oracle and the Rust
 * test — and the 4b ops never mint a new chat timestamp (updateMessage touches no
 * metadata; delete/clear preserve `updatedAt`), so the whole differential needs
 * ZERO normalization.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-chatsmsgops-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chats-messages-ops-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  chats: Array<Record<string, unknown> & { id: string; createdAt: string; updatedAt: string }>;
  seedMessages: Array<{ chatId: string; messages: Array<Record<string, unknown>> }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'chats-messages-ops-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the seed fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chatsmsgops-fixture-build-'));
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
  for (const seed of spec.seedMessages) {
    await repo.addMessages(seed.chatId, seed.messages as never);
  }

  await closeDatabase();
  process.stderr.write(`built chats messages ops seed fixture: ${out} (${spec.chats.length} chats)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats messages ops fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
