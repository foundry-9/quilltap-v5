/**
 * P4.D329 (v4 `e1872c94f`) — widen the COMMITTED `workbench-{main,mount}.db`
 * pair IN PLACE with `Tools/kappa_inventory.tool.json` (a `$inventory` whole
 * roll) on the General store, through v4's REAL `storeMountFile`, so the
 * library lists one `rollForm: "inventory"` entry.
 *
 * ## Why a widen and not a rebuild
 *
 * The `extend-pascal-run-custom-announcer.ts` reason: a fresh build at the pin
 * comes out a generateDDL vintage behind the committed bytes and re-mints ids,
 * moving every family that reads the pair (memory note
 * `rebuilt-fixture-pair-vintage`). The definition is additive. The builder
 * carries the same definition (`workbench-inventory-roll.ts`).
 *
 * Idempotent: a pair whose General store already links the path is left alone.
 *
 * Run (Node 24) from the worktree pinned at `322a5488e`, on COPIES staged under
 * `/tmp/p4d329/`, then copy the two files back:
 *   cd /tmp/qt-v4-pin-p4d329-322a5488e
 *   QT_FIXTURE_CI_MAIN=/tmp/p4d329/wb/workbench-main.db \
 *   QT_FIXTURE_CI_MOUNT=/tmp/p4d329/wb/workbench-mount.db \
 *     npx tsx <V5W>/harness/oracle/fixtures/extend-workbench-inventory-roll.ts
 */

import { readFileSync, existsSync, rmSync, mkdtempSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { tmpdir } from 'node:os';

import { INVENTORY_ROLL_PATH, KAPPA_INVENTORY } from './workbench-inventory-roll';

interface Spec {
  testPepperBase64: string;
  generalMountPointId: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'workbench.json'), 'utf8')) as Spec;

  const mainPath = process.env.QT_FIXTURE_CI_MAIN;
  const mountPath = process.env.QT_FIXTURE_CI_MOUNT;
  if (!mainPath || !mountPath || !existsSync(mainPath) || !existsSync(mountPath)) {
    throw new Error('QT_FIXTURE_CI_MAIN and QT_FIXTURE_CI_MOUNT must point at the staged .db files');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-workbench-extend-inventory-'));
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
  const { storeMountFile } = await import('@/lib/mount-index/store-file');

  await initializeDatabase();
  const repos = getRepositories() as unknown as {
    docMountFileLinks: { findByMountPointAndPath: (mp: string, p: string) => Promise<unknown> };
  };

  if (await repos.docMountFileLinks.findByMountPointAndPath(spec.generalMountPointId, INVENTORY_ROLL_PATH)) {
    process.stderr.write('workbench pair already carries kappa_inventory — unchanged\n');
  } else {
    await storeMountFile({
      mountPointId: spec.generalMountPointId,
      relativePath: INVENTORY_ROLL_PATH,
      data: Buffer.from(JSON.stringify(KAPPA_INVENTORY, null, 2), 'utf8'),
      originalMimeType: 'application/json',
      transcodeImages: false,
      extractText: false,
      enqueueEmbedding: false,
      force: true,
    } as never);
    process.stderr.write(`widened workbench pair with kappa_inventory: main=${mainPath}\n`);
  }

  // v4's un-awaited post-write tails land before the connections close.
  await new Promise((r) => setTimeout(r, 500));
  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`workbench inventory-roll widen failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
