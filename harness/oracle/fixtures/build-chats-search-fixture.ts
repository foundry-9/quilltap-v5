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
 * ## The two venues (P4.D204)
 *
 * `f45a517a9` gives global search an FTS5 index, and `initializeDatabase()`
 * does NOT run the migration runner — so a fixture built this way has no FTS
 * objects unless the builder creates them itself. Both states are real and both
 * are tested, so this builder makes EITHER, chosen by `QT_FIXTURE_FTS`:
 *
 * - unset — the fixture has no index, and every query goes down v4's `LIKE`
 *   fallback (including the runtime fallback v4 takes when an FTS query throws
 *   `no such table`). This is the state a v4 instance is in before its
 *   migration runs, and the state a v5 instance is in before its boot
 *   reconciler runs.
 * - `1` — the builder calls v4's REAL `ensureChatMessageFtsSchema` and
 *   `rebuildChatMessageFtsIndex` after seeding, so the index is populated and
 *   the token-semantics queries (`walk` vs `sidewalk`, `cafe` vs `café`) take
 *   the indexed path.
 *
 * Run (Node 24, from the v4 checkout) — BOTH fixtures:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-chsearch-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chats-search-fixture.ts
 *   QT_FIXTURE_FTS=1 QT_FIXTURE_OUT=/tmp/qt-chsearch-fixture-fts.db \
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

  // P4.D204: the FTS venue. v4's REAL ensure + rebuild, run AFTER seeding so
  // the index is built from the rows the repository actually wrote (including
  // the compressed ones — the rebuild reads through `qt_text`).
  let indexed: number | null = null;
  if (process.env.QT_FIXTURE_FTS === '1') {
    const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
    const { ensureChatMessageFtsSchema, rebuildChatMessageFtsIndex } = await import(
      '@/lib/database/backends/sqlite/chat-message-fts'
    );
    const db = getRawDatabase();
    if (!db) throw new Error('QT_FIXTURE_FTS=1 but there is no raw SQLite database');
    ensureChatMessageFtsSchema(db);
    indexed = rebuildChatMessageFtsIndex(db).indexed;
  }

  await closeDatabase();
  process.stderr.write(
    `built chats search seed fixture: ${out} (${spec.chats.length} chats` +
      (indexed === null ? ', no FTS index' : `, ${indexed} messages indexed`) +
      ')\n',
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats search fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
