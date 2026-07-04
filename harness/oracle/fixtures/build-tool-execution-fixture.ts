/**
 * Tier-2 SEED fixture builder — W4.1c sub-unit c.1 (v4 saveToolMessages).
 *
 * Creates the spec chat (via v4's REAL `repos.chats.create`, id + timestamps
 * pinned to the sentinel) and the spec GENERATED image file (via
 * `repos.files.create`, likewise pinned), and materializes the empty
 * `chat_messages` table (via a `getMessageCount` read) so the Rust port — which
 * does NOT own DDL — can INSERT into the same tables when it runs the calls
 * against a copy. The saveToolMessages calls themselves are NOT run here; both the
 * oracle and the Rust test run them on a fresh copy of this seed.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-toolexec-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-tool-execution-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  chat: Record<string, unknown> & { id: string; createdAt: string; updatedAt: string };
  file: Record<string, unknown> & { id: string; createdAt: string; updatedAt: string };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'tool-execution-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the seed fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-toolexec-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();
  const repos = getRepositories();

  {
    const { id, createdAt, updatedAt, ...data } = spec.chat;
    await repos.chats.create(data as never, { id, createdAt, updatedAt });
  }
  {
    const { id, createdAt, updatedAt, ...data } = spec.file;
    await repos.files.create(data as never, { id, createdAt, updatedAt });
  }
  // Force the chat_messages table into existence (empty) so the Rust port can
  // INSERT into it — getMessageCount runs getMessages → ensureMessagesCollection.
  await repos.chats.getMessageCount(spec.chat.id);

  await closeDatabase();
  process.stderr.write(`built tool-execution seed fixture: ${out}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`tool-execution fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
