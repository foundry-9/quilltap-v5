/**
 * Tier-2 oracle case #2 — the `tags` repo (Phase-2, repo #2 after `folders`).
 *
 * Proves what state v4 leaves the database in after a fixed create + update +
 * delete sequence, so the Rust port can be diffed against it (structural DB
 * diff). Widens the tier-2 surface past `folders` (all strings): a boolean
 * column (`quickHide` -> INTEGER 0/1), a nullable JSON-object column
 * (`visualStyle` -> compact JSON, schema field order), the `nameLower`
 * derivation, and the `delete` op.
 *
 * Flow (mirrors folders-tier2.ts):
 *   1. copy the SEED-ONLY fixture (built by build-tags-fixture.ts) to a fresh
 *      working copy — so this run never mutates the shared starting state;
 *   2. open the copy through v4's real `initializeDatabase()` (writable
 *      ChaCha20 path) and run the op sequence from `tags-tier2.json` via the
 *      real `TagsRepository` (ids + timestamps pinned -> deterministic);
 *   3. close, then dump the `tags` table canonically (RAW on-disk cells via
 *      `rawQuery`: quickHide is the integer 0/1, visualStyle the JSON string)
 *      and emit it as one NDJSON row for the Rust harness to diff.
 *
 * NORMALIZATION SPEC: none. Every id and timestamp is pinned on both sides, so
 * the dumps must match outright.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`), AFTER
 * building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TAGS=/tmp/qt-tags-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/tags-tier2.ts \
 *     > /tmp/oracle-tags.ndjson
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
  userId: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'tags-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_TAGS;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_TAGS must point at the seed fixture from build-tags-fixture.ts'
    );
  }

  // Work on a fresh copy so the shared seed fixture stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-tags-oracle-'));
  // v4 nests working files under `<dataDir>/data/` (instance lock, sibling DBs).
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'tags-work.db');
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
  const { TagsRepository } = await import(
    '@/lib/database/repositories/tags.repository'
  );

  await initializeDatabase();
  const repo = new TagsRepository();

  for (const op of spec.ops) {
    if (op.kind === 'create') {
      await repo.create(op.data as never, op.options);
    } else if (op.kind === 'update') {
      await repo.update(op.id as string, op.data as never);
    } else {
      await repo.delete(op.id as string);
    }
  }

  // Read RAW on-disk state through v4's own connected backend (so no npm
  // package is resolved from the v5 tree). table_info gives schema column order;
  // SELECT * gives the persisted rows (booleans as 0/1, JSON columns as text).
  const columns = (
    (await rawQuery('PRAGMA table_info(tags)')) as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM tags')) as Array<
    Record<string, unknown>
  >;

  await closeDatabase();

  const dump = canonicalizeRows({
    table: 'tags',
    columns,
    rawRows,
    orderBy: 'id',
  });

  process.stdout.write(JSON.stringify({ case: 'tags-tier2', ...dump }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`tags-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
