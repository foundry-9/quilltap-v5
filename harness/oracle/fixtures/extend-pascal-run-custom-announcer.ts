/**
 * P4.D319 (v4 `a05c6f755`) — widen the COMMITTED `pascal-run-custom-{main,mount}.db`
 * pair IN PLACE with the `announced_dresser` definition (`announceAsCharacter:
 * true`, a `{{character}}` message and chip, one trigger phrase) on the Trigger
 * Shelf, through v4's REAL `storeMountFile`.
 *
 * ## Why a widen and not a rebuild
 *
 * The same reason as `extend-pascal-run-custom-triggers.ts` (P4.D305): a fresh
 * build at the pin comes out a generateDDL vintage BEHIND the committed bytes
 * and re-mints every character-vault id, which moves every family that reads
 * the pair (memory note `rebuilt-fixture-pair-vintage`). The definition is
 * additive, so it is added on the committed file. The builder carries the same
 * definition (through the shared `pascal-run-custom-announcer.ts`) so a future
 * full rebuild keeps it.
 *
 * The union re-runs this AFTER P4.D316's extender on main's committed pair
 * (§R.10 (u) — a binary pair cannot be 3-way merged).
 *
 * Idempotent: a pair whose Trigger Shelf already links
 * `Tools/announced_dresser.tool.json` is left alone.
 *
 * Run (Node 24) from a worktree pinned at the round's pin, on COPIES staged
 * under the lane's scratch, then copy the two files back:
 *   cd /tmp/qt-v4-pin-<lane>-e8e17b2d9
 *   QT_FIXTURE_CI_MAIN=<staged>/pascal-run-custom-main.db \
 *   QT_FIXTURE_CI_MOUNT=<staged>/pascal-run-custom-mount.db \
 *     npx tsx <V5W>/harness/oracle/fixtures/extend-pascal-run-custom-announcer.ts
 */

import { readFileSync, existsSync, rmSync, mkdtempSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { tmpdir } from 'node:os';

import { SHELF_MP } from './pascal-run-custom-triggers';
import { addAnnouncedDresser, ANNOUNCED_DRESSER_PATH } from './pascal-run-custom-announcer';

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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-pascal-extend-announcer-'));
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

  if (await repos.docMountFileLinks.findByMountPointAndPath(SHELF_MP, ANNOUNCED_DRESSER_PATH)) {
    process.stderr.write('pascal-run-custom pair already carries announced_dresser — unchanged\n');
  } else {
    await addAnnouncedDresser(async (mountPointId, relativePath, body) => {
      await storeMountFile({
        mountPointId,
        relativePath,
        data: Buffer.from(JSON.stringify(body, null, 2), 'utf8'),
        originalMimeType: 'application/json',
        transcodeImages: false,
        extractText: false,
        enqueueEmbedding: false,
        force: true,
      } as never);
    });
    process.stderr.write(`widened pascal-run-custom pair with announced_dresser: main=${mainPath}\n`);
  }

  // Let v4's un-awaited post-write tails (the mount point's `refreshStats`)
  // land BEFORE the connections close (memory note
  // `oracle-async-tails-and-jest-mocks`).
  await new Promise((r) => setTimeout(r, 500));
  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`pascal-run-custom announcer widen failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
