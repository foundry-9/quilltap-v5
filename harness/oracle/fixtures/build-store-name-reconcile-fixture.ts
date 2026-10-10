/**
 * Tier-2 fixture builder for `store_name_reconcile_equivalence` (P4.D274 unit
 * 4): the EMPTY base pair every scenario starts from — a main DB and a
 * mount-index DB, both keyed with the spec's test pepper, built by v4's REAL
 * initialization at the pin (`initializeDatabase`, then the two repositories'
 * first access, which runs `ensureCollection` / `onTableEnsured` — so the
 * mount index carries `idx_doc_mount_points_name_nocase` exactly as v4
 * creates it). Never committed: built per run under the lane's own /tmp dir
 * (the scenario rows are planted by the case and the Rust test alike, from
 * `store-name-reconcile-spec.json`).
 *
 * Run from the pinned v4 worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SNR_DIR=/tmp/qt-store-name-reconcile $N/npx tsx $V5W/harness/oracle/fixtures/build-store-name-reconcile-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'store-name-reconcile-spec.json'), 'utf8')) as {
    testPepperBase64: string;
  };
  const outDir = process.env.QT_FIXTURE_SNR_DIR;
  if (!outDir) throw new Error('QT_FIXTURE_SNR_DIR is required (see header)');
  mkdirSync(outDir, { recursive: true });

  const scratch = mkdtempSync(join(tmpdir(), 'qt-snr-build-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const main = join(scratch, 'data', 'main.db');
  const mount = join(scratch, 'data', 'mount.db');

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = main;
  process.env.SQLITE_MOUNT_INDEX_PATH = mount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();
  const repos = getRepositories();
  await repos.docMountPoints.findAll();
  await repos.characters.findAllRaw();
  closeMountIndexSQLiteClient();
  await closeDatabase();

  for (const [src, name] of [
    [main, 'base-main.db'],
    [mount, 'base-mount.db'],
  ] as const) {
    if (!existsSync(src)) throw new Error(`v4 did not create ${src}`);
    copyFileSync(src, join(outDir, name));
  }
  process.stdout.write(`built store-name-reconcile base pair in ${outDir}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`build-store-name-reconcile-fixture failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
