/**
 * Tier-2 SEED fixture builder — the chats messages WRITE path (Phase-2, the chats
 * repo — sub-unit 4a: addMessage / addMessages).
 *
 * Creates the spec chats via v4's REAL `repos.chats.create` (ids + timestamps
 * pinned to the sentinel) and materializes the empty `chat_messages` table (by
 * forcing `ensureCollection('chat_messages', ChatMessageRowSchema)` via a
 * `getMessageCount` read), so the Rust port — which does NOT own DDL — can INSERT
 * into the same table when it runs the op sequence against a copy. The op
 * sequence itself is NOT run here; both the oracle and the Rust test run it on a
 * fresh copy of this seed.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-chatsmsg-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chats-messages-fixture.ts
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
  const specPath = join(here, 'chats-messages-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the seed fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chatsmsg-fixture-build-'));
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
  // Force the chat_messages table into existence (empty) so the Rust port can
  // INSERT into it — getMessageCount runs getMessages → ensureMessagesCollection.
  await repo.getMessageCount(spec.chats[0].id);

  await closeDatabase();
  process.stderr.write(`built chats messages seed fixture: ${out} (${spec.chats.length} chats)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats messages fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
