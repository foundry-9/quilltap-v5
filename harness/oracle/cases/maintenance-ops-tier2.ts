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
  utimesSync,
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
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
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
  // P4.D355: `debug`, so step 6b's six lines reach the spliced sink below
  // (only those six are kept; nothing is written to stdout by the logger).
  process.env.LOG_LEVEL = 'debug';

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

  // P4.D355 (v4 `85c002822`, bug 267): step 6b's orphan-transcript corpus,
  // mirroring v4's own `terminal-transcript-orphan-sweep.test.ts`. Step 6 runs
  // first (its reaped rows' files are gone by 6b), so: the SURVIVING row d4's
  // transcript (named by id → scanned, spared silently), an ORPHAN 48 h
  // unwritten (deleted), an orphan written 5 min ago (spared by the 1 h
  // grace), and a shell-init artifact (not a `.log` → not even scanned).
  const nowMs = Date.now();
  const orphanCorpus: Array<[string, number]> = [
    ['a0000000-0000-4000-8000-0000000000d4.log', 48 * 3600 * 1000],
    ['b0000000-0000-4000-8000-0000000000e1.log', 48 * 3600 * 1000],
    ['b0000000-0000-4000-8000-0000000000e2.log', 5 * 60 * 1000],
    ['init-b0000000-0000-4000-8000-0000000000e1.bashrc', 48 * 3600 * 1000],
  ];
  for (const [name, ageMs] of orphanCorpus) {
    const p = join(transcriptsDir, name);
    writeFileSync(p, `transcript ${name}\n`);
    const t = new Date(nowMs - ageMs);
    utimesSync(p, t, t);
  }

  // Bug 43: the orphan-thumbnail sweep corpus, on disk under
  // `<QUILLTAP_DATA_DIR>/files/_thumbnails/` (the local backend's base). A LIVE
  // entry (its fileId f1000000… has a `files` row → survives), an ORPHAN
  // (f2000000… has none → deleted), and a GARBAGE name (unparseable → skipped).
  const thumbsDir = join(scratch, 'files', '_thumbnails');
  mkdirSync(thumbsDir, { recursive: true });
  writeFileSync(
    join(thumbsDir, 'f1000000-0000-4000-8000-000000000001_150.webp'),
    'live-thumbnail',
  );
  writeFileSync(
    join(thumbsDir, 'f2000000-0000-4000-8000-000000000002_300.webp'),
    'orphan-thumbnail',
  );
  writeFileSync(join(thumbsDir, 'bad-name.webp'), 'garbage');

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { runScheduledMaintenance } = await import(
    '@/lib/background-jobs/scheduled-maintenance'
  );
  const { getLastMaintenanceSweepAt } = await import('@/lib/instance-settings');

  // P4.D355: splice a sink into the singleton logger's shared transports
  // (children included) and keep step 6b's six lines — `{level, message,
  // fields}` with the child bindings dropped and `filePath` / `transcriptsDir`
  // reduced to their basenames (each side's scratch dir differs).
  const SWEEP_LINES = new Set([
    'No terminal transcript directory; nothing to sweep',
    'Sparing transcript of a live PTY with no row',
    'Sparing recently written orphan transcript',
    'Removed orphaned terminal transcript',
    'Failed to remove orphaned terminal transcript',
    'Swept orphaned terminal transcripts',
  ]);
  const sweepLogs: Array<Record<string, unknown>> = [];
  const { logger } = await import('@/lib/logger');
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown> };
      if (!SWEEP_LINES.has(data.message)) return;
      const { service: _s, module: _m, environment: _e, ...fields } = (data.context ?? {}) as Record<
        string,
        unknown
      >;
      for (const k of ['filePath', 'transcriptsDir']) {
        if (typeof fields[k] === 'string') fields[k] = (fields[k] as string).split('/').pop();
      }
      sweepLogs.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
    },
  });

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
      // P4.D355: the step-6b corpus — which files remain.
      remaining: orphanCorpus.map(([name]) => name).filter((n) => existsSync(join(transcriptsDir, n))),
    }) + '\n',
  );
  process.stdout.write(JSON.stringify({ kind: 'sweep_logs', lines: sweepLogs }) + '\n');

  await closeMountIndexSQLiteClient();
  await closeDatabase();
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`maintenance-ops oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
