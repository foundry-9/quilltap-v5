/**
 * The Inventory widen of the COMMITTED `pascal-run-custom-{main,mount}.db`
 * pair, IN PLACE, through v4's REAL code: the Inventory columns (C1's
 * `plantInventoryColumns`), then each lane's share of objects, definitions and
 * carried sets.
 *
 * This file is P4.D328's (the Inventory tools' share, `pascal-run-custom-
 * inventory.ts`). P4.D329's branch carries the same skeleton with only the
 * columns and ITS share; the union keeps this file, adds P4.D329's ONE import
 * (`addPascalInventoryShelf` from `pascal-run-custom-inventory-pascal.ts`) and
 * ONE call beside this file's, and re-runs the extenders in pick order on
 * main's committed pair (a binary pair cannot be 3-way merged — P4.D319's
 * precedent).
 *
 * Why a widen and not a rebuild: `extend-pascal-run-custom-announcer.ts`.
 * Idempotent: each share leaves an already-widened pair alone (this one keys
 * on Bertie's vault already holding `INV_LANTERN_ID`; the columns plant skips
 * a column that exists).
 *
 * Run (Node 24) from the worktree pinned at `322a5488e`, on COPIES staged
 * under the lane's scratch, then copy the two files back:
 *   cd /tmp/qt-v4-pin-p4d328-322a5488e
 *   QT_FIXTURE_CI_MAIN=/tmp/p4d328/prc/pascal-run-custom-main.db \
 *   QT_FIXTURE_CI_MOUNT=/tmp/p4d328/prc/pascal-run-custom-mount.db \
 *     npx tsx <V5W>/harness/oracle/fixtures/extend-pascal-run-custom-inventory.ts
 */

import { readFileSync, existsSync, rmSync, mkdtempSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { tmpdir } from 'node:os';

import { plantInventoryColumns } from '../lib/v4-migrations';
import { addInventoryToolObjects } from './pascal-run-custom-inventory';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'pascal-run-custom.json'), 'utf8')) as Spec;

  const mainPath = process.env.QT_FIXTURE_CI_MAIN;
  const mountPath = process.env.QT_FIXTURE_CI_MOUNT;
  if (!mainPath || !mountPath || !existsSync(mainPath) || !existsSync(mountPath)) {
    throw new Error('QT_FIXTURE_CI_MAIN and QT_FIXTURE_CI_MOUNT must point at the staged .db files');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-pascal-extend-inventory-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainPath;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountPath;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );

  await initializeDatabase();
  await plantInventoryColumns();
  const repos = getRepositories();

  const added = await addInventoryToolObjects({ repos, userId: spec.userId, ts: spec.seedTimestamp });
  process.stderr.write(
    added
      ? `widened pascal-run-custom pair with the Inventory tools' objects: main=${mainPath}\n`
      : "pascal-run-custom pair already carries the Inventory tools' objects — unchanged\n",
  );

  // v4's un-awaited post-write tails land before the connections close.
  await new Promise((r) => setTimeout(r, 500));
  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`pascal-run-custom inventory widen failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
