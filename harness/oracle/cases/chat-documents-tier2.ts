/**
 * Tier-2 oracle case — the `chat_documents` repo (Phase-2).
 *
 * Proves what state v4 leaves the database in after a fixed create / update /
 * delete sequence, so the Rust port can be diffed against it (structural DB
 * diff). Banks a plain (non-Taggable) AbstractBaseRepository with an
 * all-text-plus-one-boolean shape: two required TEXT strings (chatId, filePath),
 * an enum TEXT column (scope), two nullable TEXT columns (mountPoint,
 * displayTitle), and one boolean (isActive -> INTEGER 0/1).
 *
 * Flow (mirrors image-profiles-tier2.ts):
 *   1. copy the SEED-ONLY fixture to a fresh working copy;
 *   2. open the copy through v4's real `initializeDatabase()` and run the op
 *      sequence from `chat-documents-tier2.json` via the real
 *      `ChatDocumentsRepository`;
 *   3. close, then dump the table canonically (RAW on-disk cells via `rawQuery`:
 *      isActive as 0/1, nulls explicit) and emit it as one NDJSON row.
 *
 * NORMALIZATION SPEC: none. Every id and timestamp is pinned on both sides.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`), AFTER
 * building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CD=/tmp/qt-cd-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chat-documents-tier2.ts \
 *     > /tmp/oracle-cd.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  copyFileSync,
  existsSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind: 'create' | 'update' | 'delete';
  id?: string;
  data?: Record<string, unknown>;
  options?: { id: string; createdAt: string; updatedAt: string };
}

interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'chat-documents-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_CD;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_CD must point at the seed fixture from build-chat-documents-fixture.ts'
    );
  }

  // Work on a fresh copy so the shared seed fixture stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-cd-oracle-'));
  // v4 nests working files under `<dataDir>/data/` (instance lock, sibling DBs).
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'cd-work.db');
  copyFileSync(fixture, work);

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE; // writable path uses journal_mode = TRUNCATE
  // Keep stdout clean for the NDJSON: v4's console logger sends INFO to stdout.
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { ChatDocumentsRepository } = await import(
    '@/lib/database/repositories/chat-documents.repository'
  );

  await initializeDatabase();
  const repo = new ChatDocumentsRepository();

  for (const op of spec.ops) {
    if (op.kind === 'create') {
      await repo.create(op.data as never, op.options);
    } else if (op.kind === 'update') {
      await repo.update(op.id as string, op.data as never);
    } else {
      await repo.delete(op.id as string);
    }
  }

  // Read RAW on-disk state through v4's own connected backend. table_info gives
  // schema column order; SELECT * gives the persisted rows (isActive as 0/1,
  // nulls explicit).
  const columns = (
    (await rawQuery('PRAGMA table_info(chat_documents)')) as Array<{
      name: string;
    }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM chat_documents')) as Array<
    Record<string, unknown>
  >;

  await closeDatabase();

  const dump = canonicalizeRows({
    table: 'chat_documents',
    columns,
    rawRows,
    orderBy: 'id',
  });

  process.stdout.write(
    JSON.stringify({ case: 'chat-documents-tier2', ...dump }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-documents-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
