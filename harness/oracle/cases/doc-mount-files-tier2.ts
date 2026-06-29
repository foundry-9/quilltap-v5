/**
 * Tier-2 oracle case — the `doc_mount_files` repo (the NARROWEST mount-index
 * sibling-DB repo of Phase 2: a required sha256 TEXT, one REAL-int column, two
 * enum TEXT columns; no JSON, no boolean, no nullable columns).
 *
 * Proves what state v4 leaves the MOUNT-INDEX database in after a fixed
 * create + update + delete sequence, so the Rust port can be diffed against it.
 *
 * ── THE SIBLING-DB MACHINERY (mirrors the doc_mount_points slice) ─────────────
 * The data lives in `quilltap-mount-index.db`, NOT the main DB. So:
 *   - SQLITE_MOUNT_INDEX_PATH points at the working COPY of the fixture (seed +
 *     ops applied here); SQLITE_PATH points at a fresh throwaway main DB;
 *   - ops run through the REAL `DocMountFilesRepository` (overridden
 *     getCollection routes to the mount-index DB);
 *   - the raw read-back is done through the mount-index handle directly
 *     (`getRawMountIndexDatabase()`), NOT `rawQuery` — `rawQuery` targets the
 *     MAIN backend, so it would read the wrong (empty) database. The handle is a
 *     better-sqlite3 connection, so `pragma('table_info')` gives schema column
 *     order and `prepare('SELECT *').all()` gives the persisted rows.
 *
 * NORMALIZATION SPEC: none. Every id and timestamp is pinned on both sides, so
 * the dumps must match outright — no id remap, no timestamp placeholder.
 *
 * Run from the v4 server checkout under Node 24, AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_DOC_MOUNT_FILES=/tmp/qt-dmf-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/doc-mount-files-tier2.ts \
 *     > /tmp/oracle-dmf.ndjson
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
  const specPath = join(here, '..', 'fixtures', 'doc-mount-files-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_DOC_MOUNT_FILES;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_DOC_MOUNT_FILES must point at the seed fixture from build-doc-mount-files-fixture.ts'
    );
  }

  // Work on a fresh copy of the mount-index fixture so the shared seed stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-dmf-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'dmf-mount-index-work.db');
  copyFileSync(fixture, work);

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db'); // fresh throwaway main DB
  process.env.SQLITE_MOUNT_INDEX_PATH = work; // the working copy we mutate + read
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE; // journal_mode = TRUNCATE on both DBs
  process.env.LOG_LEVEL = 'error'; // keep the NDJSON pipe clean

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { DocMountFilesRepository } = await import(
    '@/lib/database/repositories/doc-mount-files.repository'
  );

  await initializeDatabase();
  const repo = new DocMountFilesRepository();

  for (const op of spec.ops) {
    if (op.kind === 'create') {
      await repo.create(op.data as never, op.options);
    } else if (op.kind === 'update') {
      await repo.update(op.id as string, op.data as never);
    } else {
      await repo.delete(op.id as string);
    }
  }

  // Read RAW on-disk state through the MOUNT-INDEX handle directly (rawQuery
  // targets the MAIN backend, which would be the wrong DB). table_info gives
  // schema column order; SELECT * gives the persisted rows.
  const midb = getRawMountIndexDatabase();
  if (!midb) {
    throw new Error('mount-index DB handle unavailable (degraded open?)');
  }
  const columns = (
    midb.pragma('table_info(doc_mount_files)') as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = midb
    .prepare('SELECT * FROM doc_mount_files')
    .all() as Array<Record<string, unknown>>;

  closeMountIndexSQLiteClient();
  await closeDatabase();

  const dump = canonicalizeRows({
    table: 'doc_mount_files',
    columns,
    rawRows,
    orderBy: 'id',
  });

  process.stdout.write(
    JSON.stringify({ case: 'doc-mount-files-tier2', ...dump }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`doc-mount-files-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
