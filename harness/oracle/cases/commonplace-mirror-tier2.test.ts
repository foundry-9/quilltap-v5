/**
 * @jest-environment node
 *
 * Tier-2 ORACLE for the Commonplace mirror written from committed digests
 * (P4.D288, C1-7 / L-3: v4 `mirrorCommonplaceForSubjects`,
 * `lib/memory/commonplace-mirror.ts:111-200`, with bug 208's emptied-bucket
 * removal in the bridge, at the `83334b9a5` pin), driven over a COPY of the
 * consolidation fixture pair with `QUILLTAP_JOB_CHILD` unset.
 *
 * The calls (`harness/oracle/fixtures/commonplace-mirror-calls.json`) run in
 * order on ONE copy: planted digests mirrored (a subject listed twice, self +
 * other), the unchanged re-run, an empty subject list, a subject that no
 * longer exists, a missing holder, a bucket emptied (its file REMOVED) and
 * emptied again, the 200-digest read cap over 205 planted digests, and two
 * retirements whose announcement refreshes the removed digests' bucket (the
 * second removes the file). Comparands per call: the answer and every captured
 * line (the mirror's `module: 'memory:commonplace-mirror'` lines, the
 * retirement's refresh line, the bridge's lines). At the end: the
 * `Commonplace/` links + files + documents, folders and chunks by SQL
 * projections with no ids or clocks.
 *
 * Run (Node 24, from the round's pin — the CASES run from a /tmp mirror
 * because jest ignores .claude/ paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=/tmp/qt-consolidation-oracle
 *   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-consolidation-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-consolidation-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-consolidation-fixture.ts
 *   rm -f /tmp/qt-commonplace-mirror.ndjson
 *   QT_FIXTURE_CONSOLIDATION_MAIN=/tmp/qt-consolidation-main.db \
 *   QT_FIXTURE_CONSOLIDATION_MOUNT=/tmp/qt-consolidation-mount.db \
 *   QT_ORACLE_OUT=/tmp/qt-commonplace-mirror.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$M/cases" -- commonplace-mirror-tier2
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

const CAPTURED = new Set([
  // lib/memory/commonplace-mirror.ts (module: memory:commonplace-mirror)
  'Commonplace mirror skipped: holder not found',
  'Commonplace mirror skips a bucket whose subject no longer exists',
  'Commonplace mirror written from committed digests',
  'Commonplace mirror failed (the digests are committed regardless)',
  // lib/memory/retire.ts — the announcement's refresh
  'Refreshed Commonplace mirror after retiring a digest',
  // the bridge
  'Skipping Commonplace mirror: holder not found',
  'Skipping Commonplace mirror for archived character',
  'Skipping Commonplace mirror: holder has no character vault',
  'Wrote Commonplace digest mirror',
  'Failed to write a Commonplace digest mirror',
  'Removed Commonplace mirror for an emptied bucket',
  'Failed to remove an emptied Commonplace mirror',
  'Commonplace mirror failed',
  'Commonplace mirror complete',
  'Document absent from database-backed store',
  'Failed to resolve character vault store',
]);

/** A logger's own context minus the root logger's `service` / `environment`
 * (v5 renders neither) — a child's `module` stays, first, as v4 merges it. */
function ownContext(ctx: Record<string, unknown> | undefined): Record<string, unknown> {
  const { service: _s, environment: _e, ...rest } = ctx ?? {};
  return rest;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'consolidation-tier3.json'), 'utf8'));
  const callsSpec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'commonplace-mirror-calls.json'), 'utf8'),
  ) as {
    calls: Array<Record<string, unknown>>;
    projections: Array<{ name: string; sql: string }>;
  };

  const fixtureMain = process.env.QT_FIXTURE_CONSOLIDATION_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_CONSOLIDATION_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_CONSOLIDATION_MAIN / _MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-commonplace-mirror-oracle-'));
  try {
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const workMain = join(scratch, 'main.db');
    const workMount = join(scratch, 'mount.db');
    copyFileSync(fixtureMain, workMain);
    copyFileSync(fixtureMount, workMount);

    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.SQLITE_PATH = workMain;
    process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
    process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'llm-logs.db');
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.QUILLTAP_JOB_CHILD;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';
    (globalThis as Record<string, unknown>).__quilltapJobHost = { shuttingDown: true };

    jest.resetModules();
    const cipherDriverPath = require('node:path').join(
      process.cwd(),
      'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
    );
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
    jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
    jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
      jest.requireActual('@/lib/file-storage/character-vault-bridge'),
    );
    jest.doMock('@/lib/file-storage/manager', () => jest.requireActual('@/lib/file-storage/manager'));

    const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
    const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
    const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    const { mirrorCommonplaceForSubjects } = await import('@/lib/memory/commonplace-mirror');
    const { retireMemories } = await import('@/lib/memory/retire');
    const { Logger } = await import('@/lib/logger');

    let sink: Array<{ level: string; message: string; context: unknown }> | null = null;
    for (const level of ['error', 'warn', 'info', 'debug'] as const) {
      const proto = Logger.prototype as unknown as Record<string, (...a: unknown[]) => void>;
      const original = proto[level];
      proto[level] = function (
        this: { context?: Record<string, unknown> },
        message: string,
        context?: Record<string, unknown>,
        ...rest: unknown[]
      ) {
        if (sink && typeof message === 'string' && CAPTURED.has(message)) {
          sink.push({
            level,
            message,
            context: JSON.parse(JSON.stringify({ ...ownContext(this.context), ...(context ?? {}) })),
          });
        }
        return original.call(this, message, context, ...rest);
      };
    }

    await initializeDatabase();
    const lines: string[] = [];
    for (const call of callsSpec.calls) {
      sink = [];
      let result: unknown = null;
      switch (call.kind) {
        case 'sql': {
          // The raw handle's `exec`: `rawQuery` reads a `WITH …` statement as
          // a query and refuses the CTE insert.
          const maindb = getRawDatabase();
          if (!maindb) throw new Error('main DB handle unavailable');
          for (const sql of call.sql as string[]) maindb.exec(sql);
          break;
        }
        case 'mirror':
          result = await mirrorCommonplaceForSubjects(
            call.holderId as string,
            call.subjects as string[],
            call.updatedAt as string,
          );
          break;
        case 'retire':
          result = (await retireMemories(call.ids as string[], { reason: 'item-delete' })).retired;
          break;
        default:
          throw new Error(`unknown call kind ${String(call.kind)}`);
      }
      lines.push(JSON.stringify({ kind: 'call', name: call.name, result, lines: sink }));
      sink = null;
    }

    const midb = getRawMountIndexDatabase();
    if (!midb) throw new Error('mount-index DB handle unavailable');
    for (const { name, sql } of callsSpec.projections) {
      const rows = midb.prepare(sql).raw(true).all();
      lines.push(JSON.stringify({ kind: 'projection', name, rows }));
    }

    closeMountIndexSQLiteClient();
    await closeDatabase();
    fs.writeFileSync(outPath, lines.join('\n') + '\n');
    process.stderr.write(`commonplace mirror oracle wrote ${outPath}\n`);
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

test('commonplace mirror tier-2 oracle', async () => {
  await main();
});
