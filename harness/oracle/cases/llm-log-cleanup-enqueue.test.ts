/**
 * @jest-environment node
 *
 * Tier-2 ORACLE for the LLM-log cleanup **enqueuer** (P4.24 — v4
 * `runScheduledCleanup`, `lib/background-jobs/scheduled-cleanup.ts`).
 *
 * v5 has shipped `run_scheduled_cleanup` since P4.1d and **nothing has ever
 * driven it differentially** — which is exactly how dogfood finding #41 shipped
 * (`retentionDays` serialized `7.0` into a payload column both apps read, because
 * the `f64` went straight to `serde_json` instead of through `js_number_to_json`).
 * This case closes that hole: it drives v4's REAL enqueuer over the committed
 * main fixture and dumps `background_jobs` **including the payload bytes**, so
 * the JSON rendering of the retention number is part of the diff.
 *
 * Nothing is mocked except the job-processor host, which is neutralized before
 * any service import (`__quilltapJobHost.childCrashed`) so `enqueueJob`'s
 * `ensureProcessorRunning()` cannot fork a child that claims the rows before the
 * dump — the danger-scan precedent. The clock is pinned the same way the handler
 * case pins it (`Date` only), so the minted timestamps are stable; the Rust side
 * mints real ones and the harness placeholders them.
 *
 * The corpus takes every branch of v4's `enabled && retentionDays > 0` filter:
 * `windowed` / `fractional` / `bystander` enqueue, `keepForever` fails the `> 0`
 * test, `disabled` / `disabledOverride` fail `enabled`, `noSettings` and
 * `noSettingsOverride` own no `chat_settings` row so `findAll` never sees them,
 * and **`nullBag`** — whose cell is SQL NULL — is the one v4 resolves through
 * `LLMLoggingSettingsSchema.default({...})`.
 *
 * TZ is irrelevant here (no calendar arithmetic on this path) and is pinned to
 * UTC only so the run is reproducible.
 *
 * ⚠️ The enqueuer INSERTS, so the case works on a /tmp copy of the committed
 * fixture and never touches the committed files.
 *
 * Run from the v4 server checkout under Node 24 (the /tmp mirror dodges jest's
 * `/.claude/` testPathIgnorePatterns):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<the v5 worktree>
 *   cd ~/source/quilltap-server
 *   mkdir -p /tmp/qt-llc-oracle/cases /tmp/qt-llc-oracle/fixtures
 *   cp $V5/harness/oracle/cases/llm-log-cleanup-enqueue.test.ts /tmp/qt-llc-oracle/cases/
 *   cp $V5/harness/oracle/fixtures/llm-log-cleanup.json         /tmp/qt-llc-oracle/fixtures/
 *   TZ=UTC \
 *   QT_FIXTURE_LLC_MAIN=$V5/crates/quilltap-web/tests/fixtures/llm-log-cleanup-main.db \
 *   QT_FIXTURE_LLC_LOGS=$V5/crates/quilltap-web/tests/fixtures/llm-log-cleanup-llmlogs.db \
 *   QT_ORACLE_OUT=/tmp/oracle-llm-log-cleanup-enqueue.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots /tmp/qt-llc-oracle/cases -- llm-log-cleanup-enqueue
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

interface Spec {
  testPepperBase64: string;
  nowIso: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'llm-log-cleanup.json'), 'utf8'),
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_LLC_MAIN;
  const fixtureLogs = process.env.QT_FIXTURE_LLC_LOGS;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureLogs || !existsSync(fixtureLogs)) {
    throw new Error('QT_FIXTURE_LLC_MAIN / QT_FIXTURE_LLC_LOGS must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-llc-enq-'));
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
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const { runScheduledCleanup } = await import('@/lib/background-jobs/scheduled-cleanup');

  await initializeDatabase();

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

  const summary = await runScheduledCleanup();

  jest.useRealTimers();

  const lines: string[] = [];
  lines.push(
    JSON.stringify({
      kind: 'summary',
      usersProcessed: summary.usersProcessed,
      jobsEnqueued: summary.jobsEnqueued,
    }),
  );

  // The FULL background_jobs dump — payload bytes included, which is the point.
  const columns = (
    (await rawQuery('PRAGMA table_info(background_jobs)')) as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM background_jobs')) as Array<
    Record<string, unknown>
  >;
  const rows = rawRows
    .map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
      return out;
    })
    .sort((a, b) => {
      const av = `${String(a.userId ?? '')}|${String(a.id ?? '')}`;
      const bv = `${String(b.userId ?? '')}|${String(b.id ?? '')}`;
      return av < bv ? -1 : av > bv ? 1 : 0;
    });
  lines.push(JSON.stringify({ kind: 'table', table: 'background_jobs', columns, rows }));

  closeLLMLogsSQLiteClient();
  await closeDatabase();

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`llm-log-cleanup enqueue oracle wrote ${outPath}\n`);
}

test('llm-log-cleanup enqueue tier-2 oracle', async () => {
  await main();
});
