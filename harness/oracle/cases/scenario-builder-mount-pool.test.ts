/**
 * @jest-environment node
 *
 * ORACLE for the Scenario Builder mount pool (P4.D217 — v4 `d1c06cd9d`,
 * `lib/scenario-builder/mount-pool.ts`), ported to
 * quilltap_core::services::scenario_builder::mount_pool.
 *
 * Drives v4's REAL `resolveScenarioBuilderMountPool` over REAL repositories on a
 * REAL two-partition fixture: the doc-opacity fixture (built by its OWN,
 * unchanged builder — `build-doc-opacity-fixture.ts`, P4.D216's) plus the plants
 * in `scenario-builder-mount-pool.json`, applied IN ORDER on a per-run COPY
 * through v4's own `rawQuery`. v4's `mount-pool.test.ts` mocks every repository
 * and `getGeneralMountPointId`, so it is a source of CASE NAMES only — its eight
 * shapes are this family's arms, planted rather than mocked.
 *
 * The whole DB stack is doMocked to the REAL modules (past jest.setup's global
 * mocks) plus the real better-sqlite3-multiple-ciphers cipher binding (the
 * doc-opacity recipe). The `ScenarioBuilderMountPool` service logger is wrapped
 * to RECORD its lines (and only its lines) per arm.
 *
 * Emits ONE NDJSON line per arm: { arm, pool, logs, settingsLogs } in step
 * order (`settingsLogs`, P4.113: v4's `[InstanceSettings]` WARN + the backend's
 * `Raw query failed` ERROR, off the plain logger).
 *
 * Run (Node 24, from the v4 checkout or a PINNED worktree). STAGE this case
 * OUTSIDE `.claude/` — v4's jest ignores those paths, so `--roots` into a
 * worktree matches ZERO tests and leaves the previous NDJSON in place. The
 * fixture pair is MINTED by the opacity builder — rebuild, regenerate, then
 * `cargo test` against that SAME build, in that order.
 *   N=~/.nvm/versions/node/v24.13.1/bin ; W=<this worktree>
 *   STAGE=/tmp/qt-oracle-stage-sb-pool
 *   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
 *   cp $W/harness/oracle/cases/scenario-builder-mount-pool.test.ts $STAGE/harness/oracle/cases/
 *   cp $W/harness/oracle/fixtures/scenario-builder-mount-pool.json $STAGE/harness/oracle/fixtures/
 *   cd ~/source/quilltap-server
 *   rm -f /tmp/qt-sbpool-main.db /tmp/qt-sbpool-mount.db
 *   QT_FIXTURE_DOPA_MAIN=/tmp/qt-sbpool-main.db QT_FIXTURE_DOPA_MOUNT=/tmp/qt-sbpool-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-doc-opacity-fixture.ts
 *   QT_FIXTURE_SBPOOL_MAIN=/tmp/qt-sbpool-main.db QT_FIXTURE_SBPOOL_MOUNT=/tmp/qt-sbpool-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-scenario-builder-mount-pool.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=240000 \
 *       --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "scenario-builder-mount-pool\.test\.ts$"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Statement {
  sql: string;
  params: unknown[];
}
type Step =
  | { kind: 'plant'; name: string; statements: Statement[] }
  | {
      kind: 'arm';
      name: string;
      userId: string;
      projectId: string | null;
      characterIds: string[];
    };
interface Spec {
  testPepperBase64: string;
  leilaniId: string;
  steps: Step[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'scenario-builder-mount-pool.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_SBPOOL_MAIN;
  const mountFixture = process.env.QT_FIXTURE_SBPOOL_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_SBPOOL_MAIN and QT_FIXTURE_SBPOOL_MOUNT must point at the doc-opacity builder output',
    );
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-sbpool-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories'),
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));

  // Record the pool's own logger, and only it.
  const logLines: Array<{ level: string; message: string; context: unknown }> = [];
  jest.doMock('@/lib/logging/create-logger', () => {
    const actual = jest.requireActual('@/lib/logging/create-logger');
    return {
      __esModule: true,
      ...actual,
      createServiceLogger: (serviceName: string) => {
        if (serviceName !== 'ScenarioBuilderMountPool') return actual.createServiceLogger(serviceName);
        const rec = (level: string) => (message: string, context?: unknown) => {
          logLines.push({ level, message, context: context ?? null });
        };
        return { debug: rec('debug'), info: rec('info'), warn: rec('warn'), error: rec('error') };
      },
    };
  });

  const work = mkdtempSync(join(scratch, 'run-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { resolveScenarioBuilderMountPool } = await import('@/lib/scenario-builder/mount-pool');

  // P4.113: v4's `readSetting` WARNs `[InstanceSettings] Failed to read
  // setting` on the plain `@/lib/logger` singleton (not the pool's service
  // logger), and v4's backend logs ERROR `Raw query failed` before the
  // rethrow — recorded off the `Logger` prototype (singleton and children
  // alike, before the level check) of THIS registry generation (imported
  // after `resetModules`, so it is the one the modules under test resolve),
  // filtered to those two lines, per arm.
  const settingsLines: Array<Record<string, unknown>> = [];
  const { Logger } = await import('@/lib/logger');
  for (const level of ['error', 'warn'] as const) {
    const original = Logger.prototype[level];
    Logger.prototype[level] = function (
      this: unknown,
      message: string,
      context?: Record<string, unknown>,
      ...rest: unknown[]
    ) {
      if (message.startsWith('[InstanceSettings]') || message === 'Raw query failed') {
        settingsLines.push({
          level,
          message,
          key: context?.key ?? null,
          query: context?.query ?? null,
          hasError: typeof context?.error === 'string' && context.error.length > 0,
        });
      }
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }

  await initializeDatabase();
  const repos = getRepositories();
  const leilani = await repos.characters.findByIdRaw(spec.leilaniId);
  const subs: Record<string, string> = {
    '{{leilaniVaultId}}': leilani?.characterDocumentMountPointId as string,
  };
  const sub = (v: unknown): unknown => (typeof v === 'string' && subs[v] !== undefined ? subs[v] : v);

  const outLines: string[] = [];
  try {
    for (const step of spec.steps) {
      if (step.kind === 'plant') {
        for (const st of step.statements) await rawQuery(st.sql, st.params.map(sub));
        continue;
      }
      logLines.splice(0);
      settingsLines.splice(0);
      const pool = await resolveScenarioBuilderMountPool({
        userId: step.userId,
        projectId: step.projectId,
        characterIds: step.characterIds,
      });
      outLines.push(
        JSON.stringify({
          arm: step.name,
          pool,
          logs: logLines.splice(0),
          settingsLogs: settingsLines.splice(0),
        }),
      );
    }
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }

  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`scenario-builder-mount-pool oracle wrote ${outPath} (${outLines.length} arms)\n`);
}

test('scenario-builder mount-pool oracle', async () => {
  await main();
});
