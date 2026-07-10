/**
 * Read-differential ORACLE for the preset-scenario resolvers (P4.4 unit 2,
 * sub-unit 1).
 *
 * Opens the pre-seeded main + mount-index fixtures and drives v4's REAL
 * `resolveGeneralScenarioBody` / `resolveProjectScenarioBody`
 * (lib/mount-index/{general,project}-scenarios.ts) over a path matrix, emitting
 * each resolved body (or null). The Rust port
 * (db::scenarios::resolve_{general,project}_scenario_body) reads the SAME
 * fixtures and must produce the same result exactly (no normalization).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SCEN_MAIN=/tmp/qt-scen-main.db QT_FIXTURE_SCEN_MOUNT=/tmp/qt-scen-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/cases/scenario-resolvers.ts > /tmp/oracle-scen.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync, existsSync, mkdtempSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  generalMountPointId: string;
  projectMountPointId: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'scenario-resolvers.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_SCEN_MAIN;
  const mountFixture = process.env.QT_FIXTURE_SCEN_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_SCEN_MAIN and QT_FIXTURE_SCEN_MOUNT must point at the seeded fixtures');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-scen-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'scen-main-work.db');
  const mountWork = join(scratch, 'scen-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { resolveGeneralScenarioBody } = await import('@/lib/mount-index/general-scenarios');
  const { resolveProjectScenarioBody } = await import('@/lib/mount-index/project-scenarios');

  await initializeDatabase();

  const P = spec.projectMountPointId;

  const rows: Array<{ id: string; body: string | null }> = [];
  const gen = async (id: string, path: string) =>
    rows.push({ id, body: await resolveGeneralScenarioBody(path) });
  const proj = async (id: string, path: string) =>
    rows.push({ id, body: await resolveProjectScenarioBody(P, path) });

  await gen('general_bare', 'welcome');
  await gen('general_full', 'Scenarios/welcome.md');
  await gen('general_nomd', 'Scenarios/welcome');
  await gen('general_leading_slash', '/welcome');
  await gen('general_missing', 'nonexistent');
  await gen('general_empty', 'empty');
  await proj('project_full', 'Scenarios/tavern.md');
  await proj('project_bare', 'tavern');
  await proj('project_missing', 'nope');

  await closeDatabase();

  for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`scenario-resolvers oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
