/**
 * Read-differential fixture builder — the shared starting state for v4's
 * `ChatsRepository` findBy* queries (Phase-2, the chats repo — sub-unit 2, read).
 *
 * Runs v4's REAL `repos.chats.create(data, { id, createdAt, updatedAt })` for each
 * spec chat (ids + timestamps pinned, so the fixture is fully deterministic). Both
 * the oracle and the Rust port (db::chats_read) then READ a COPY of this SAME baked
 * fixture, so every cell is identical on both sides — the hydrated query results
 * compare exactly, with NO normalization (nothing is mutated, so no minted
 * timestamp ever appears).
 *
 * `chats` is a single MAIN-db table; the slim row is the only thing written
 * (`create` writes nothing to `chat_messages` on SQLite). Each chat's defaults +
 * dropped optionals are materialized by v4's own `ChatMetadataBaseSchema.parse`
 * inside `_create`, so the stored columns are exactly what the read path marshals
 * back.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHATSREAD=/tmp/qt-chatsread.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chats-read-fixture.ts
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
  const specPath = join(here, 'chats-read-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_CHATSREAD;
  if (!out) {
    throw new Error('QT_FIXTURE_CHATSREAD must point at the fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chatsread-fixture-build-'));
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
  process.stderr.write(`built chats read fixture: ${out} (${spec.chats.length} rows)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats read fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
