/**
 * @jest-environment node
 *
 * Tier-2 ORACLE for the `LLM_LOG_CLEANUP` job handler (P4.24 — v4
 * `handleLLMLogCleanup`, `lib/background-jobs/handlers/llm-log-cleanup.ts`).
 *
 * Drives v4's REAL handler through v4's REAL queue claim/mark machinery over
 * the committed two-database fixture. **Nothing about the handler, the
 * repositories, or the queue is mocked.** Only two things are pinned:
 *
 *   - **The clock.** `jest.useFakeTimers` with everything except `Date` in
 *     `doNotFake`, so `new Date()` inside `cleanupOldLogs` is exactly
 *     `spec.nowIso` and nothing else about the runtime changes. The Rust side
 *     passes the same instant explicitly (core reads no ambient clock).
 *   - **The job-processor host.** `__quilltapJobHost.childCrashed` is pre-seeded
 *     before any service import so `enqueueJob`'s `ensureProcessorRunning()`
 *     cannot fork a child that races the claim loop (the danger-scan
 *     precedent). Nothing in this case enqueues, but the queue module reaches
 *     for it on import.
 *
 * **The timezone is the point of this family.** v4's cutoff is
 * `cutoffDate.setDate(cutoffDate.getDate() - retentionDays)` — LOCAL calendar-day
 * arithmetic — so it agrees with `now - N*86400000` under UTC and differs by an
 * hour across a DST transition. This case therefore runs TWICE, once per
 * `spec.timezoneLegs` entry, with `TZ` in the process environment (v4 reads the
 * ambient zone) and the leg recorded in every row. A UTC-only family would be
 * structurally blind to the whole bug class (the P4.d26 lesson).
 *
 * The drive is a claim loop, identical on the Rust side: claim the next due job,
 * run the handler, `markCompleted` on return / `markFailed(message)` on throw.
 * Each processed job is recorded as `kind:"processed"` — the Rust side asserts
 * the same sequence element-for-element, which pins the claim ORDER too. Then
 * both `background_jobs` (main) and `llm_logs` (the llm-logs partition) are
 * dumped in full.
 *
 * ⚠️ The handler DELETES, so the case works on /tmp copies of the committed
 * fixture and never touches the committed files.
 *
 * ⚠️ **A bare `TZ=` on the command line is NOT enough any more.** Since v4
 * `f7f1a956` its `jest.config.ts` assigns `process.env.TZ = 'UTC'` before Jest
 * forks its workers, clobbering it — and this case would then refuse to run at
 * all (it throws on a TZ/leg mismatch, which is why it never silently
 * mis-recorded). The zone must be applied from
 * `--globalSetup <v5>/harness/oracle/lib/jest-zone-globalsetup.cjs`, the one
 * hook that runs in the MAIN process after that config and before the fork; an
 * in-worker assignment is inert, because `jest-environment-node` gives the test
 * a deep COPY of `process`. This file then PROVES the zone took, at a winter
 * and a summer instant, against an independently computed offset.
 *
 * Run from the v4 server checkout under Node 24, ONCE PER LEG (the /tmp mirror
 * dodges jest's `/.claude/` testPathIgnorePatterns):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<the v5 worktree>
 *   cd ~/source/quilltap-server
 *   mkdir -p /tmp/qt-llc-oracle/cases /tmp/qt-llc-oracle/fixtures
 *   cp $V5/harness/oracle/cases/llm-log-cleanup-jobs.test.ts /tmp/qt-llc-oracle/cases/
 *   cp $V5/harness/oracle/fixtures/llm-log-cleanup.json      /tmp/qt-llc-oracle/fixtures/
 *   for LEG in UTC America/Chicago; do
 *     SLUG=$(echo "$LEG" | tr '/' '-')
 *     QT_LLC_LEG=$LEG \
 *     GS=$V5/harness/oracle/lib/jest-zone-globalsetup.cjs \
 *     QT_FIXTURE_LLC_MAIN=$V5/crates/quilltap-web/tests/fixtures/llm-log-cleanup-main.db \
 *     QT_FIXTURE_LLC_LOGS=$V5/crates/quilltap-web/tests/fixtures/llm-log-cleanup-llmlogs.db \
 *     QT_ORACLE_OUT=/tmp/oracle-llm-log-cleanup-$SLUG.ndjson \
 *       $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *         --globalSetup "$GS" \
 *         --roots "$PWD" --roots /tmp/qt-llc-oracle/cases -- llm-log-cleanup-jobs
 *   done
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';

// ── The zone GUARD (see the header) ──────────────────────────────────────────
// This file cannot SET the zone: `jest-environment-node` hands the test a deep
// copy of `process`, so an in-worker `process.env.TZ = …` writes to a sandbox
// object libuv never reads. The pin has to come from
// `--globalSetup <v5>/harness/oracle/lib/jest-zone-globalsetup.cjs`, which runs
// in the main process after v4's config assigns UTC and before the workers
// fork. What this file does is PROVE the pin took.
const LLC_LEG = process.env.QT_LLC_LEG ?? 'UTC';

/** The offset (minutes WEST of UTC, `getTimezoneOffset`'s sign) `zone` has at
 *  `at` — computed with an EXPLICIT `timeZone`, so it is independent of
 *  whatever the process default happens to be. */
function zoneOffsetMinutes(zone: string, at: Date): number {
  const parts = Object.fromEntries(
    new Intl.DateTimeFormat('en-US', {
      timeZone: zone,
      hour12: false,
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    })
      .formatToParts(at)
      .map((p) => [p.type, p.value])
  ) as Record<string, string>;
  const asUtc = Date.UTC(
    Number(parts.year),
    Number(parts.month) - 1,
    Number(parts.day),
    Number(parts.hour) % 24,
    Number(parts.minute),
    Number(parts.second)
  );
  return (at.getTime() - asUtc) / 60000;
}

for (const instant of ['2026-01-15T12:00:00Z', '2026-07-15T12:00:00Z']) {
  const at = new Date(instant);
  const want = zoneOffsetMinutes(LLC_LEG, at);
  if (at.getTimezoneOffset() !== want) {
    throw new Error(
      `zone pin did not take: process zone reports ${at.getTimezoneOffset()} at ` +
        `${instant}, ${LLC_LEG} is ${want}. Pass ` +
        '--globalSetup <v5>/harness/oracle/lib/jest-zone-globalsetup.cjs; v4 ' +
        'jest.config.ts pins UTC and a bare TZ= on the command line is clobbered.'
    );
  }
}
import { tmpdir } from 'node:os';

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}
function canonicalizeRows(opts: {
  table: string;
  columns: string[];
  rawRows: Array<Record<string, unknown>>;
  orderBy?: string;
}): { table: string; columns: string[]; rows: Array<Record<string, unknown>> } {
  const { table, columns, rawRows, orderBy = 'id' } = opts;
  const rows = rawRows
    .map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
      return out;
    })
    .sort((a, b) => {
      const av = String(a[orderBy] ?? '');
      const bv = String(b[orderBy] ?? '');
      return av < bv ? -1 : av > bv ? 1 : 0;
    });
  return { table, columns, rows };
}

interface Spec {
  testPepperBase64: string;
  nowIso: string;
  timezoneLegs: string[];
  jobs: Array<{ id: string; name: string }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'llm-log-cleanup.json'), 'utf8'),
  ) as Spec;

  const leg = process.env.QT_LLC_LEG;
  if (!leg) throw new Error('QT_LLC_LEG must name the timezone leg');
  if (!spec.timezoneLegs.includes(leg)) {
    throw new Error(`QT_LLC_LEG=${leg} is not one of the corpus legs`);
  }
  // The module-level pin above already re-applied and PROVED the zone; this is
  // the belt to its braces.
  if (process.env.TZ !== leg) {
    throw new Error(`TZ=${process.env.TZ} does not match QT_LLC_LEG=${leg}`);
  }

  const fixtureMain = process.env.QT_FIXTURE_LLC_MAIN;
  const fixtureLogs = process.env.QT_FIXTURE_LLC_LOGS;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureLogs || !existsSync(fixtureLogs)) {
    throw new Error('QT_FIXTURE_LLC_MAIN / QT_FIXTURE_LLC_LOGS must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  // Copies — the handler deletes rows, and the committed fixture stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-llc-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'llc-main.db');
  const workLogs = join(scratch, 'llc-llmlogs.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureLogs, workLogs);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_LLM_LOGS_PATH = workLogs;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // Neutralize the job-processor host BEFORE any service import (the
  // danger-scan precedent): the queue module reaches for it on load.
  (globalThis as Record<string, unknown>).__quilltapJobHost = {
    child: null,
    spawning: false,
    shuttingDown: false,
    childCrashed: true,
  };

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/background-jobs/processor', () => ({
    __esModule: true,
    getProcessorStatus: () => ({ running: false }),
    startProcessor: () => {},
    stopProcessor: () => {},
    wakeProcessor: () => {},
    ensureProcessorRunning: () => {},
  }));

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRawLLMLogsDatabase, closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const { handleLLMLogCleanup } = await import('@/lib/background-jobs/handlers/llm-log-cleanup');
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();
  const repos = getRepositories();

  // Only `Date` is faked; every timer API stays real so the sync DB driver and
  // the async repo wrappers behave exactly as they do in production.
  jest.useFakeTimers({
    now: new Date(spec.nowIso),
    doNotFake: [
      'hrtime',
      'nextTick',
      'performance',
      'queueMicrotask',
      'requestAnimationFrame',
      'cancelAnimationFrame',
      'requestIdleCallback',
      'cancelIdleCallback',
      'setImmediate',
      'clearImmediate',
      'setInterval',
      'clearInterval',
      'setTimeout',
      'clearTimeout',
    ],
  });

  const lines: string[] = [];
  lines.push(JSON.stringify({ kind: 'leg', timezone: leg, nowIso: spec.nowIso }));

  // The claim loop (identical on the Rust side). A FAILED retry-eligible job has
  // its `scheduledAt` REWOUND by raw SQL on both sides so the backoff's
  // wall-clock wait never enters the differential.
  const REWIND_SQL =
    'UPDATE "background_jobs" SET "scheduledAt" = \'1999-01-01T00:00:00.000Z\' ' +
    'WHERE "status" = \'FAILED\' AND "attempts" < "maxAttempts"';
  let guard = 0;
  for (;;) {
    if (++guard > 200) throw new Error('claim loop failed to converge');
    const job = await repos.backgroundJobs.claimNextJob();
    if (!job) {
      const rewound = ((await rawQuery(REWIND_SQL)) as unknown as { changes?: number })?.changes;
      if (!rewound) break;
      continue;
    }
    let error: string | null = null;
    try {
      await handleLLMLogCleanup(job);
      await repos.backgroundJobs.markCompleted(job.id);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      await repos.backgroundJobs.markFailed(job.id, error);
    }
    lines.push(
      JSON.stringify({
        kind: 'processed',
        jobId: job.id,
        outcome: error === null ? 'completed' : 'failed',
        error,
      }),
    );
  }

  jest.useRealTimers();

  const dumpMain = async (table: string, orderBy: string) => {
    const columns = ((await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>).map(
      (c) => c.name,
    );
    const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };
  const dumpLogs = (table: string, orderBy: string) => {
    const ldb = getRawLLMLogsDatabase();
    if (!ldb) throw new Error('llm-logs DB handle unavailable');
    const columns = (ldb.prepare(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>).map(
      (c) => c.name,
    );
    const rawRows = ldb.prepare(`SELECT * FROM ${table}`).all() as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };

  lines.push(JSON.stringify({ kind: 'table', ...(await dumpMain('background_jobs', 'id')) }));
  lines.push(JSON.stringify({ kind: 'table', ...dumpLogs('llm_logs', 'id') }));

  closeLLMLogsSQLiteClient();
  await closeDatabase();

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`llm-log-cleanup (${leg}) oracle wrote ${outPath}\n`);
}

test('llm-log-cleanup jobs tier-2 oracle', async () => {
  await main();
});
