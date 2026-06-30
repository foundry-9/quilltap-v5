/**
 * Tier-2 oracle case — the `chats` repo (Phase-2, the conversation capstone,
 * sub-unit 1: slim-row marshaling).
 *
 * Proves what state v4 leaves the `chats` table in after a fixed
 * create / update / delete sequence, so the Rust port can be diffed against it
 * (structural DB diff). Exercises `create` (rich + minimal, the ~96-column
 * surface incl. the typed `participants` array column + the JSON-array / open-JSON
 * / number-affinity / enum / nullable columns) and `update` (the
 * updatedAt-PRESERVED branch and the explicit-updatedAt branch) and `delete`.
 *
 * Flow (mirrors memories-tier2.ts):
 *   1. copy the SEED (empty-table) fixture to a fresh working copy;
 *   2. open it through v4's real `initializeDatabase()` and run the op sequence
 *      from `chats-tier2.json` via the real `ChatsRepository`;
 *   3. close, then dump `chats` canonically (RAW on-disk cells via `rawQuery`)
 *      and emit it as one NDJSON row.
 *
 * NORMALIZATION SPEC: none. `update` never mints `updatedAt` (it preserves the
 * existing value unless the caller passes one), so every id + timestamp is
 * pinned on both sides — zero normalization.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHATS=/tmp/qt-chats-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-tier2.ts > /tmp/oracle-chats.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
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
  const specPath = join(here, '..', 'fixtures', 'chats-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_CHATS;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_CHATS must point at the seed fixture from build-chats-fixture.ts');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chats-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'chats-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');

  await initializeDatabase();
  const repo = new ChatsRepository();

  for (const op of spec.ops) {
    if (op.kind === 'create') {
      await repo.create(op.data as never, op.options);
    } else if (op.kind === 'update') {
      await repo.update(op.id as string, op.data as never);
    } else {
      // syncVaults defaults to true; the fixture has no provisioned vaults, so
      // the summary sweep is a no-op (and is deferred in the Rust port).
      await repo.delete(op.id as string);
    }
  }

  const columns = (
    (await rawQuery('PRAGMA table_info(chats)')) as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM chats')) as Array<Record<string, unknown>>;

  await closeDatabase();

  const dump = canonicalizeRows({ table: 'chats', columns, rawRows, orderBy: 'id' });
  process.stdout.write(JSON.stringify({ case: 'chats-tier2', ...dump }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
