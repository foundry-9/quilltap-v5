/**
 * Tier-2 oracle case — the stale-chat asset collapse (W4.8, v4
 * `lib/background-jobs/maintenance/collapse-stale-chat-assets.ts`).
 *
 * Copies the pre-seeded two-DB fixture, drives v4's REAL `collapseStaleChatAssets`
 * with a PINNED `now` (spec.now), then emits:
 *   - a `summary` line (the returned `StaleChatCollapseSummary`);
 *   - a `files` dump (main DB, the surviving `files` rows: id + source/category
 *     + linkedTo, ordered by id) — the load-bearing state (which files the sweep
 *     deleted vs kept);
 *   - a `mount` dump (the album `doc_mount_file_links` row — untouched, proving
 *     the sweep touches only `files`).
 * The Rust port reads a COPY of the SAME fixture and runs its own
 * `collapse_stale_chat_assets`, diffing each line.
 *
 * The sweep deletes through `deleteFileCompletely`; the seeded files carry a NULL
 * `storageKey`, so v4's `fileStorageManager.deleteFile` is skipped (a no-op-warn)
 * — matching the Rust FsSeam exactly. No LLM, no minting → ZERO normalization
 * (every id/timestamp is pinned by the builder).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MAINT_MAIN=/tmp/qt-maint-main.db \
 *   QT_FIXTURE_MAINT_MOUNT=/tmp/qt-maint-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/cases/maintenance-sweep-tier2.ts > /tmp/oracle-maintenance-sweep.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  now: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'maintenance-sweep.json'), 'utf8'),
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_MAINT_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_MAINT_MOUNT;
  if (!fixtureMain || !fixtureMount || !existsSync(fixtureMain) || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_MAINT_MAIN and QT_FIXTURE_MAINT_MOUNT must point at the seed fixtures');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-maint-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'maint-main.db');
  const workMount = join(scratch, 'maint-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { collapseStaleChatAssets } = await import(
    '@/lib/background-jobs/maintenance/collapse-stale-chat-assets'
  );

  await initializeDatabase();

  const summary = await collapseStaleChatAssets(new Date(spec.now).getTime());

  // Dump the surviving `files` rows (main DB) ordered by id.
  const files = (await rawQuery<Array<Record<string, unknown>>>(
    'SELECT id, source, category, linkedTo FROM files ORDER BY id ASC',
    [],
  )) ?? [];

  // Dump the album link (mount-index) — untouched by the sweep.
  const mountDb = getRawMountIndexDatabase();
  const links = mountDb
    ? (mountDb
        .prepare('SELECT id, relativePath, mountPointId FROM doc_mount_file_links ORDER BY id ASC')
        .all() as Array<Record<string, unknown>>)
    : [];

  process.stdout.write(JSON.stringify({ kind: 'summary', summary }) + '\n');
  process.stdout.write(JSON.stringify({ kind: 'files', rows: files }) + '\n');
  process.stdout.write(JSON.stringify({ kind: 'mount', rows: links }) + '\n');

  await closeMountIndexSQLiteClient();
  await closeDatabase();
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`maintenance-sweep oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
