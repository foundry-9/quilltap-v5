/**
 * Tier-2 oracle case — the whole daily maintenance pass (P4.1d, v4
 * `lib/background-jobs/scheduled-maintenance.ts` `runScheduledMaintenance`),
 * proving BOTH newly-ported repo ops (`docMountFileLinks.sweepOrphanedFiles`,
 * `terminalSessions.cleanupClosedSessions`) inside v4's real four-sweep
 * orchestration + the `lastMaintenanceSweepAt` stamp.
 *
 * Copies the pre-seeded two-DB fixture, creates the transcript files (the
 * default `<logs>/terminals/<id>.log` for t-old-default, and the pinned
 * explicit path for t-old-explicit), drives v4's REAL
 * `runScheduledMaintenance()` (wall-clock cutoffs — the fixture's day-keyed
 * rows are robust to minutes of skew), then emits:
 *   - a `summary` line (the MaintenanceSweepSummary);
 *   - a `jobs` dump (id + status, ordered by id — reap survivors);
 *   - a `terminals` dump (ids, ordered);
 *   - a `mount_files` dump (ids, ordered — the orphan-sweep survivors);
 *   - a `stamp` line ({ present, parseable } — the value is wall-clock);
 *   - a `transcripts` line (which transcript files remain).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MAINT_OPS_MAIN=/tmp/qt-maint-ops-main.db \
 *   QT_FIXTURE_MAINT_OPS_MOUNT=/tmp/qt-maint-ops-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/cases/maintenance-ops-tier2.ts > /tmp/oracle-maintenance-ops.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  copyFileSync,
  writeFileSync,
  existsSync,
  rmSync,
} from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  explicitTranscriptPath: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'maintenance-ops.json'), 'utf8'),
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_MAINT_OPS_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_MAINT_OPS_MOUNT;
  if (!fixtureMain || !fixtureMount || !existsSync(fixtureMain) || !existsSync(fixtureMount)) {
    throw new Error(
      'QT_FIXTURE_MAINT_OPS_MAIN and QT_FIXTURE_MAINT_OPS_MOUNT must point at the seed fixtures',
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-maint-ops-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'maint-ops-main.db');
  const workMount = join(scratch, 'maint-ops-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // The transcript files: the default-path one (t-old-default) and the pinned
  // explicit-path one (t-old-explicit). t-old-nofile gets NO file (ENOENT).
  const transcriptsDir = join(scratch, 'logs', 'terminals');
  mkdirSync(transcriptsDir, { recursive: true });
  const defaultTranscript = join(
    transcriptsDir,
    'a0000000-0000-4000-8000-0000000000d1.log',
  );
  writeFileSync(defaultTranscript, 'old default-path transcript\n');
  if (existsSync(spec.explicitTranscriptPath)) rmSync(spec.explicitTranscriptPath);
  writeFileSync(spec.explicitTranscriptPath, 'old explicit-path transcript\n');

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { runScheduledMaintenance } = await import(
    '@/lib/background-jobs/scheduled-maintenance'
  );
  const { getLastMaintenanceSweepAt } = await import('@/lib/instance-settings');

  await initializeDatabase();

  const summary = await runScheduledMaintenance();

  const jobs = (await rawQuery<Array<Record<string, unknown>>>(
    'SELECT id, status FROM background_jobs ORDER BY id ASC',
    [],
  )) ?? [];
  const terminals = (await rawQuery<Array<Record<string, unknown>>>(
    'SELECT id FROM terminal_sessions ORDER BY id ASC',
    [],
  )) ?? [];
  const mountDb = getRawMountIndexDatabase();
  const mountFiles = mountDb
    ? (mountDb
        .prepare('SELECT id FROM doc_mount_files ORDER BY id ASC')
        .all() as Array<Record<string, unknown>>)
    : [];
  const stamp = await getLastMaintenanceSweepAt();

  process.stdout.write(JSON.stringify({ kind: 'summary', summary }) + '\n');
  process.stdout.write(JSON.stringify({ kind: 'jobs', rows: jobs }) + '\n');
  process.stdout.write(JSON.stringify({ kind: 'terminals', rows: terminals }) + '\n');
  process.stdout.write(JSON.stringify({ kind: 'mount_files', rows: mountFiles }) + '\n');
  process.stdout.write(
    JSON.stringify({
      kind: 'stamp',
      present: stamp !== null,
      parseable: stamp !== null && !Number.isNaN(stamp.getTime()),
    }) + '\n',
  );
  process.stdout.write(
    JSON.stringify({
      kind: 'transcripts',
      defaultGone: !existsSync(defaultTranscript),
      explicitGone: !existsSync(spec.explicitTranscriptPath),
    }) + '\n',
  );

  await closeMountIndexSQLiteClient();
  await closeDatabase();
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`maintenance-ops oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
