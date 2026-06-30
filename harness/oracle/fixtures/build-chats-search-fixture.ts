/**
 * Tier-2/read SEED fixture builder — the chats search & replace ops (Phase-2,
 * the chats repo — sub-unit 6: countMessagesWithText / findMessagesWithText /
 * searchMessagesGlobal / replaceInMessages).
 *
 * Creates the spec chats and seeds each with its messages (a mix of message /
 * context-summary / system events, with PINNED message createdAt values so the
 * `searchMessagesGlobal` createdAt-DESC ordering is unambiguous) via v4's REAL
 * `repos.chats.create` + `.addMessages`. The three read methods produce VALUES
 * and `replaceInMessages` mutates `chat_messages` WITHOUT touching any timestamp,
 * so both the oracle and the Rust test READ/RUN over the SAME baked fixture and
 * the differential needs ZERO normalization.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-chsearch-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chats-search-fixture.ts
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
  const specPath = join(here, 'chats-search.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the seed fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chsearch-fixture-build-'));
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
  process.stderr.write(`built chats search seed fixture: ${out} (${spec.chats.length} chats)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats search fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
