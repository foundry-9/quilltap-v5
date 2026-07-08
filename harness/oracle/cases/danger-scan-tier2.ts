/**
 * Tier-2 oracle case — the scheduled danger-classification scan (P4.1d, v4
 * `lib/background-jobs/scheduled-danger-scan.ts` `runScheduledDangerScan`).
 *
 * Copies the pre-seeded fixture, drives v4's REAL `runScheduledDangerScan`
 * (reads + enqueues only — no model call, so this is a plain tsx real-DB
 * differential), then emits:
 *   - a `result` line ({ usersProcessed, chatsEnqueued });
 *   - a `precheck` line (the all-users-OFF gate: `anyEnabled` — the sweep's
 *     `scheduleDangerScan` pre-check condition, recomputed here the way v4
 *     computes it);
 *   - a `jobs` dump (every `background_jobs` row, ordered by payload — the
 *     minted ids/timestamps are normalized IN THE HARNESS on both sides).
 *
 * The v4 job-processor host is neutralized (`childCrashed: true` pre-seeded
 * into the HMR-global state) so `ensureProcessorRunning()` inside `enqueueJob`
 * never forks a child that would claim the enqueued jobs before the dump.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_DANGER_SCAN=/tmp/qt-danger-scan.db \
 *     $N/node --import tsx $V5/harness/oracle/cases/danger-scan-tier2.ts > /tmp/oracle-danger-scan.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'danger-scan.json'), 'utf8'),
  ) as Spec;

  const fixture = process.env.QT_FIXTURE_DANGER_SCAN;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_DANGER_SCAN must point at the seed fixture .db');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-danger-scan-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'danger-scan.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // Neutralize the job-processor host BEFORE any service import: enqueueJob's
  // ensureProcessorRunning() would otherwise fork a child that claims the
  // enqueued jobs before the dump (the enclave-lifecycle latch precedent).
  (globalThis as Record<string, unknown>).__quilltapJobHost = {
    child: null,
    spawning: false,
    shuttingDown: false,
    childCrashed: true,
    restartTimestamps: [],
    signalHandlersInstalled: true,
    invalidationListeners: new Set(),
  };

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { resolveDangerousContentSettings } = await import(
    '@/lib/services/dangerous-content/resolver.service'
  );
  const { runScheduledDangerScan } = await import(
    '@/lib/background-jobs/scheduled-danger-scan'
  );

  await initializeDatabase();

  // The scheduleDangerScan pre-check condition, recomputed v4's way.
  const allChatSettings = await getRepositories().chatSettings.findAll();
  const anyEnabled = allChatSettings.some((settings) => {
    const { settings: dangerSettings } = resolveDangerousContentSettings(settings);
    return dangerSettings.mode !== 'OFF';
  });

  const result = await runScheduledDangerScan();

  const jobs = (await rawQuery<Array<Record<string, unknown>>>(
    'SELECT * FROM background_jobs ORDER BY payload ASC',
    [],
  )) ?? [];

  process.stdout.write(JSON.stringify({ kind: 'precheck', anyEnabled }) + '\n');
  process.stdout.write(JSON.stringify({ kind: 'result', ...result }) + '\n');
  process.stdout.write(JSON.stringify({ kind: 'jobs', rows: jobs }) + '\n');

  await closeDatabase();
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`danger-scan oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
