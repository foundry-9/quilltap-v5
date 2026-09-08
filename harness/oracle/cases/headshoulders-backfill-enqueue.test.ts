/**
 * @jest-environment node
 *
 * P4.82 TIER-2 ORACLE for the one-time head-and-shoulders backfill SCAN and the
 * dedupe-on-enqueue queue helper: drives v4's REAL `enqueueHeadShouldersBackfill`
 * (`lib/startup/enqueue-headshoulders-backfill.ts`, which calls the REAL
 * `enqueueCharacterHeadShouldersBackfill` in `queue-service.ts`) over a FRESH
 * copy of the committed `headshoulders-{main,mount}.db` pair per case, and dumps
 * what it wrote.
 *
 * The fixture's eleven characters cover every classification arm in one scan:
 * a vault-less character, one whose prompt is already filled, one whose prompt is
 * WHITESPACE-only (treated as absent → enqueued), one with no seed at all, one
 * whose only seed is whitespace (`hasSeed` is NOT trimmed here, so it IS
 * enqueued — the asymmetry with the handler, which trims and then returns
 * silently), an eligible one, and one that already has a PENDING backfill job so
 * the helper's dedupe answers `{isNew:false}` and the scan counts it SKIPPED.
 *
 * Three cases:
 *   - `fresh_scan` — no flag: the full scan, seven enqueues, the flag written;
 *   - `second_run` — the same instance run TWICE in one case: the second call
 *     must find the flag and do nothing at all (no scan line, no new rows);
 *   - `flag_preset` — the flag pre-set to `'true'` BEFORE the first call, which
 *     is the cross-app shape that matters most: v4 has already run this scan on
 *     every instance it has booted, so v5 must scan nothing there.
 *
 * Dumped per case: the result struct; the `background_jobs` rows (minted ids and
 * timestamps placeholdered, everything else verbatim — `type`, `payload`,
 * `priority`, `maxAttempts`, `status`); the `instance_settings` flag row; and the
 * captured log lines with their bags.
 *
 * Seams mocked, and why:
 *   - the background-jobs processor — off, so it cannot claim the rows the scan
 *     enqueues and race the dump (the P4.6y lesson);
 *   - `@/lib/logger` — captured, since the scan's five sentences are the port;
 *   - nothing else. The vault overlay, the repositories and the queue service all
 *     run REAL.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-hs-enq-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/headshoulders-backfill-enqueue.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/headshoulders.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_HS_MAIN=$V5W/crates/quilltap-web/tests/fixtures/headshoulders-main.db \
 *   QT_FIXTURE_HS_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/headshoulders-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-headshoulders-enqueue.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- headshoulders-backfill-enqueue
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  frozenNowMs: number;
  characters: Array<{ id: string; pendingBackfillJobId?: string }>;
}

interface LogLine {
  level: string;
  message: string;
  context: Record<string, unknown> | null;
}

/** The scan's own five sentences (its child logger carries no prefix). */
const SCAN_SENTENCES = new Set([
  'Head-and-shoulders backfill scanning',
  'Failed to enqueue head-and-shoulders backfill',
  'Head-and-shoulders backfill enqueue complete',
  'Failed to read head-and-shoulders backfill flag; treating as not-yet-run',
  'Failed to record head-and-shoulders backfill flag',
]);

const CASES: Array<{ name: string; presetFlag?: string; runs: number }> = [
  { name: 'fresh_scan', runs: 1 },
  { name: 'second_run', runs: 2 },
  { name: 'flag_preset', presetFlag: 'true', runs: 1 },
];

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'headshoulders.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_HS_MAIN;
  const mountFixture = process.env.QT_FIXTURE_HS_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_HS_MAIN and QT_FIXTURE_HS_MOUNT must point at the seeded fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );

  // The job ids the fixture pins; anything else the scan mints is placeholdered.
  const seededJobIds = new Set(
    spec.characters.map((c) => c.pendingBackfillJobId).filter(Boolean) as string[],
  );

  const lines: string[] = [];
  const RealDate = Date;

  for (const c of CASES) {
    const scratch = mkdtempSync(join(tmpdir(), 'qt-hs-enq-oracle-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const mainWork = join(scratch, 'hs-main.db');
    const mountWork = join(scratch, 'hs-mount.db');
    copyFileSync(mainFixture, mainWork);
    copyFileSync(mountFixture, mountWork);

    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    const logLines: LogLine[] = [];

    jest.resetModules();
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () =>
      jest.requireActual('@/lib/database/repositories'),
    );
    jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
    jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
      jest.requireActual('@/lib/file-storage/character-vault-bridge'),
    );
    jest.doMock('@/lib/logger', () => {
      const record =
        (level: LogLine['level']) =>
        (message: string, context?: Record<string, unknown>) => {
          if (
            typeof message === 'string' &&
            (SCAN_SENTENCES.has(message) || message.startsWith('[HeadShouldersBackfill]'))
          ) {
            logLines.push({ level, message, context: context ?? null });
          }
        };
      const mk = (): Record<string, unknown> => ({
        info: record('info'),
        warn: record('warn'),
        debug: record('debug'),
        error: record('error'),
        child: () => mk(),
      });
      return { __esModule: true, logger: mk() };
    });
    jest.doMock('@/lib/background-jobs/processor', () => {
      const actual = jest.requireActual('@/lib/background-jobs/processor');
      return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
    });

    const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
    const { closeMountIndexSQLiteClient } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');

    await initializeDatabase();

    const frozen = spec.frozenNowMs;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    global.Date = class extends RealDate {
      constructor(...a: unknown[]) {
        if (a.length === 0) super(frozen);
        // @ts-expect-error forward variadic args
        else super(...a);
      }
      static now(): number {
        return frozen;
      }
    } as unknown as DateConstructor;

    try {
      const db = getRawDatabase();
      if (!db) throw new Error('main DB handle unavailable');
      if (c.presetFlag !== undefined) {
        db.prepare(
          `INSERT INTO "instance_settings" ("key", "value") VALUES (?, ?)
             ON CONFLICT("key") DO UPDATE SET "value" = excluded."value"`,
        ).run('headshoulders_backfill_enqueued_v1', c.presetFlag);
      }

      const { enqueueHeadShouldersBackfill } = await import(
        '@/lib/startup/enqueue-headshoulders-backfill'
      );

      const results: unknown[] = [];
      for (let i = 0; i < c.runs; i++) {
        results.push(await enqueueHeadShouldersBackfill());
      }

      const jobs = (
        db
          .prepare(
            `SELECT "id", "userId", "type", "status", "payload", "priority", "maxAttempts", "attempts"
               FROM "background_jobs"`,
          )
          .all() as Array<Record<string, unknown>>
      )
        .map((r) => ({
          id: seededJobIds.has(String(r.id)) ? String(r.id) : '<minted>',
          userId: r.userId,
          type: r.type,
          status: r.status,
          payload: r.payload,
          priority: r.priority,
          maxAttempts: r.maxAttempts,
          attempts: r.attempts,
        }))
        .sort((a, b) => (JSON.stringify(a) < JSON.stringify(b) ? -1 : 1));

      const flag =
        (
          db
            .prepare(`SELECT "value" FROM "instance_settings" WHERE "key" = ?`)
            .get('headshoulders_backfill_enqueued_v1') as { value: string } | undefined
        )?.value ?? null;

      lines.push(
        JSON.stringify({ name: c.name, results, jobs, flag, log: logLines }),
      );
    } finally {
      global.Date = RealDate;
      await new Promise((resolve) => setTimeout(resolve, 50));
      await closeDatabase();
      closeMountIndexSQLiteClient();
      rmSync(scratch, { recursive: true, force: true });
    }
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`headshoulders-enqueue oracle wrote ${outPath} (${lines.length} lines)\n`);
}

test('headshoulders-backfill enqueue oracle', async () => {
  await main();
});
