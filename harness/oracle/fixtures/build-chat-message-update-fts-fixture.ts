/**
 * Tier-2 SEED fixture builder — `updateMessage` UNDER the FTS triggers (P4.105).
 *
 * A SIBLING of `build-chats-messages-ops-fixture.ts` rather than a variant of
 * it: that family's fixture is deliberately trigger-free (its dump is
 * `chat_messages` + `chats` only, and it stays the neutrality leg for this
 * port), so the triggered venue gets its own spec and its own file. The FTS
 * arm follows `build-chats-search-fixture.ts`'s `QT_FIXTURE_FTS=1` precedent:
 * v4's REAL `ensureChatMessageFtsSchema` + `rebuildChatMessageFtsIndex`, so
 * both sides then write under the same five objects.
 *
 * Order matters, and each step is there for an op:
 *
 * 1. seed every message through v4's REAL `repos.chats.addMessages` (no
 *    triggers yet — `initializeDatabase()` does not run the migration runner);
 * 2. `ensureChatMessageFtsSchema` + `rebuildChatMessageFtsIndex` — the rebuild
 *    walks eligible rows in rowid order, so the map's `ftsId`s are 1..N over
 *    the eligible rows and the LAST seeded row holds the max of both the
 *    `chat_messages` rowid space and the `ftsId` space. That is what makes a
 *    DELETE + re-INSERT of any EARLIER row mint a NEW id, instead of quietly
 *    reusing the max.
 *
 * Run (Node 24, from the PINNED v4 worktree — drift-ledger §5.1):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-chatmsgupdatefts-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chat-message-update-fts-fixture.ts
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
  const specPath = join(here, 'chat-message-update-fts.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the seed fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chatmsgupdatefts-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { ensureChatMessageFtsSchema, rebuildChatMessageFtsIndex } = await import(
    '@/lib/database/backends/sqlite/chat-message-fts'
  );

  await initializeDatabase();

  const repo = new ChatsRepository();
  for (const c of spec.chats) {
    const { id, createdAt, updatedAt, ...data } = c;
    await repo.create(data as never, { id, createdAt, updatedAt });
  }
  // `chats.transcriptVersion` arrives by MIGRATION only (see the ops builder's
  // P4.D183 note) — added before the seed so `updateMessage`'s announce bump
  // lands on a real column on both sides.
  const chatCols = ((await rawQuery(`PRAGMA table_info(chats)`)) as Array<{ name: string }>).map(
    (c) => c.name,
  );
  if (!chatCols.includes('transcriptVersion')) {
    await rawQuery(`ALTER TABLE "chats" ADD COLUMN "transcriptVersion" INTEGER DEFAULT 0`);
  }

  // 1) seed
  for (const seed of spec.seedMessages) {
    await repo.addMessages(seed.chatId, seed.messages as never);
  }

  const db = getRawDatabase();
  if (!db) throw new Error('there is no raw SQLite database');

  // 2) v4's REAL index
  ensureChatMessageFtsSchema(db);
  const { indexed } = rebuildChatMessageFtsIndex(db);

  await closeDatabase();
  process.stderr.write(
    `built chat-message-update-fts seed fixture: ${out} (${spec.chats.length} chat, ${indexed} messages indexed)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-message-update-fts fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
