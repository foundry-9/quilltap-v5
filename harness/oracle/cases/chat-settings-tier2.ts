/**
 * Tier-2 oracle case — the `chat_settings` repo (Phase-2), a plain main-DB
 * `AbstractBaseRepository` with the WIDEST JSON-object surface yet (~33 columns,
 * ~15 nested typed-struct JSON columns).
 *
 * Proves what state v4 leaves the database in after a fixed create / update /
 * delete sequence, so the Rust port can be diffed against it (structural DB
 * diff). Banks: two UUID TEXT (id, userId); one enum TEXT (avatarDisplayMode) +
 * one plain-string TEXT (avatarDisplayStyle); a record/map JSON column
 * (tagStyles, kept {}); ~15 nested typed-struct JSON columns (serialized in
 * SCHEMA field order by v4's Zod .parse → JSON.stringify); five nullable
 * UUID/string TEXT columns; five boolean columns; and the FIRST INTEGER-affinity
 * number column (sidebarWidth: .min(256).max(512), both bounds integer →
 * INTEGER, vs the prior min-only/bare REAL number columns).
 *
 * Flow (mirrors connection-profiles-tier2.ts, no expectThrow — no conflict/guard
 * behavior in scope; create/update/delete delegate straight to the base repo):
 *   1. copy the SEED-ONLY fixture to a fresh working copy;
 *   2. open the copy through v4's real `initializeDatabase()` and run the op
 *      sequence from `chat-settings-tier2.json` via the real
 *      `ChatSettingsRepository`;
 *   3. close, then dump the table canonically (RAW on-disk cells via `rawQuery`:
 *      booleans as 0/1, the INTEGER sidebarWidth as a bare integer, the JSON
 *      columns as their JSON text, nulls explicit) and emit it as one NDJSON row.
 *
 * NORMALIZATION SPEC: none. Every id and timestamp is pinned on both sides.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`), AFTER
 * building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHAT_SETTINGS=/tmp/qt-chat-settings-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chat-settings-tier2.ts \
 *     > /tmp/oracle-chat-settings.ndjson
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
  const specPath = join(here, '..', 'fixtures', 'chat-settings-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_CHAT_SETTINGS;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_CHAT_SETTINGS must point at the seed fixture from build-chat-settings-fixture.ts'
    );
  }

  // Work on a fresh copy so the shared seed fixture stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-chat-settings-oracle-'));
  // v4 nests working files under `<dataDir>/data/` (instance lock, sibling DBs).
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'chat-settings-work.db');
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
  const { ChatSettingsRepository } = await import(
    '@/lib/database/repositories/chat-settings.repository'
  );

  await initializeDatabase();
  const repo = new ChatSettingsRepository();

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
  // schema column order; SELECT * gives the persisted rows (booleans as 0/1, the
  // INTEGER sidebarWidth as an integer, JSON columns as text, nulls explicit).
  const columns = (
    (await rawQuery('PRAGMA table_info(chat_settings)')) as Array<{
      name: string;
    }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM chat_settings')) as Array<
    Record<string, unknown>
  >;

  await closeDatabase();

  const dump = canonicalizeRows({
    table: 'chat_settings',
    columns,
    rawRows,
    orderBy: 'id',
  });

  process.stdout.write(
    JSON.stringify({ case: 'chat-settings-tier2', ...dump }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-settings-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
