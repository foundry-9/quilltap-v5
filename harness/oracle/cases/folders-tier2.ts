/**
 * Tier-2 oracle case #1 — the `folders` repo (Phase-2 on-ramp pilot).
 *
 * Proves what state v4 leaves the database in after a fixed `create` + `update`
 * sequence, so the Rust port can be diffed against it (structural DB diff).
 *
 * Flow (mirrors the tier-1 cases, made stateful):
 *   1. copy the SEED-ONLY fixture (built by build-folders-fixture.ts) to a fresh
 *      working copy — so this run never mutates the shared starting state;
 *   2. open the copy through v4's real `initializeDatabase()` (writable
 *      ChaCha20 path) and run the op sequence from `folders-tier2.json` via the
 *      real `FoldersRepository` (ids + timestamps pinned -> deterministic);
 *   3. close, then dump the `folders` table canonically and emit it as one
 *      NDJSON row for the Rust harness to diff field-by-field.
 *
 * NORMALIZATION SPEC: none. Every id and timestamp is pinned on both sides
 * (CreateOptions on create; an explicit `updatedAt` in the update patch), so the
 * dumps must match outright — no id remap, no timestamp placeholder. This is the
 * strongest tier-2 form; the remap/placeholder machinery (phase-2-onramp.md,
 * normalization classes) is reserved for later repos that cannot take injected
 * ids/clocks.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`), AFTER
 * building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_FOLDERS=/tmp/qt-folders-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/folders-tier2.ts \
 *     > /tmp/oracle-folders.ndjson
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
  kind: 'create' | 'update';
  id?: string;
  data: Record<string, unknown>;
  options?: { id: string; createdAt: string; updatedAt: string };
}

interface Spec {
  testPepperBase64: string;
  userId: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'folders-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_FOLDERS;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_FOLDERS must point at the seed fixture from build-folders-fixture.ts'
    );
  }

  // Work on a fresh copy so the shared seed fixture stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-folders-oracle-'));
  // v4 nests working files under `<dataDir>/data/` (instance lock, sibling DBs).
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'folders-work.db');
  copyFileSync(fixture, work);

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE; // writable path uses journal_mode = TRUNCATE
  // Keep stdout clean for the NDJSON: v4's console logger sends INFO to stdout.
  // ERROR-only (-> stderr via console.error) leaves the pipe uncontaminated.
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { FoldersRepository } = await import(
    '@/lib/database/repositories/folders.repository'
  );

  await initializeDatabase();
  const repo = new FoldersRepository();

  for (const op of spec.ops) {
    if (op.kind === 'create') {
      await repo.create(op.data as never, op.options);
    } else {
      await repo.update(op.id as string, op.data as never);
    }
  }

  // Read RAW on-disk state through v4's own connected backend (so no npm
  // package is resolved from the v5 tree). table_info gives schema column order;
  // SELECT * gives the persisted rows.
  const columns = (
    (await rawQuery('PRAGMA table_info(folders)')) as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM folders')) as Array<
    Record<string, unknown>
  >;

  await closeDatabase();

  const dump = canonicalizeRows({
    table: 'folders',
    columns,
    rawRows,
    orderBy: 'id',
  });

  process.stdout.write(JSON.stringify({ case: 'folders-tier2', ...dump }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`folders-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
