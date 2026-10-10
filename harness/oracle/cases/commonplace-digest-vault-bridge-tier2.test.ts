/**
 * @jest-environment node
 *
 * Tier-2 ORACLE for the Commonplace digest vault bridge (P4.D271, v4
 * `writeCommonplaceDigestsToVault`, `lib/file-storage/commonplace-digest-vault-
 * bridge.ts`, byte-identical at the `01a83539d` pin), driven over a COPY of the
 * consolidation fixture pair with `QUILLTAP_JOB_CHILD` unset (the in-process
 * arm; the host-RPC arm is NO-PORT).
 *
 * The calls run in order on ONE copy (each later call sees the earlier ones'
 * files): self + other files and an empty one; the unchanged re-run (a new
 * `updatedAt` only); a changed digest; a subject named `Self`; a per-file
 * failure (the fixture's mount-index trigger plant refuses any document whose
 * content carries `FAILWRITE`); a missing holder; an archived holder; a vault
 * whose store type is not `character`; a FILESYSTEM vault; a vaultless
 * character; a dangling vault pointer; the stale pre-existing file overwritten;
 * an empty file list; a list of only empty files.
 *
 * Comparands per call: the result object and the captured lines (the bridge's
 * own, `Document absent from database-backed store`, `Failed to resolve
 * character vault store`; context in v4's key order). At the end: the
 * `Commonplace/` links + files + documents, folders and chunks, by SQL
 * projections with no ids or clocks.
 *
 * Run (Node 24, from the v4 checkout or the round's pin — the CASES run from
 * a /tmp mirror because jest ignores .claude/ paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=/tmp/qt-consolidation-oracle
 *   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-consolidation-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-consolidation-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-consolidation-fixture.ts
 *   rm -f /tmp/qt-consolidation-bridge.ndjson
 *   QT_FIXTURE_CONSOLIDATION_MAIN=/tmp/qt-consolidation-main.db \
 *   QT_FIXTURE_CONSOLIDATION_MOUNT=/tmp/qt-consolidation-mount.db \
 *   QT_ORACLE_OUT=/tmp/qt-consolidation-bridge.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$M/cases" -- commonplace-digest-vault-bridge-tier2
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

const BRIDGE_CALLS_FILE = 'commonplace-bridge-calls.json';

const CAPTURED = new Set([
  'Skipping Commonplace mirror: holder not found',
  'Skipping Commonplace mirror for archived character',
  'Skipping Commonplace mirror: holder has no character vault',
  'Wrote Commonplace digest mirror',
  'Failed to write a Commonplace digest mirror',
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
  const callsSpec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', BRIDGE_CALLS_FILE), 'utf8')) as {
    calls: Array<{ name: string; input: unknown }>;
    projections: Array<{ name: string; sql: string }>;
  };
  const calls = callsSpec.calls;

  const fixtureMain = process.env.QT_FIXTURE_CONSOLIDATION_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_CONSOLIDATION_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_CONSOLIDATION_MAIN / _MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-consolidation-bridge-oracle-'));
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

    jest.resetModules();
    const cipherDriverPath = require('node:path').join(
      process.cwd(),
      'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
    );
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
    jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
    // jest.setup mocks the vault resolver to a fake `mock-vault-mount`; the
    // bridge must resolve the REAL vault.
    jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
      jest.requireActual('@/lib/file-storage/character-vault-bridge'),
    );
    jest.doMock('@/lib/file-storage/manager', () => jest.requireActual('@/lib/file-storage/manager'));

    const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
    const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    const { writeCommonplaceDigestsToVault } = await import('@/lib/file-storage/commonplace-digest-vault-bridge');
    const { Logger } = await import('@/lib/logger');

    let sink: Array<{ level: string; message: string; context: unknown }> | null = null;
    for (const level of ['error', 'warn', 'info', 'debug'] as const) {
      const proto = Logger.prototype as unknown as Record<string, (...a: unknown[]) => void>;
      const original = proto[level];
      proto[level] = function (this: { context?: Record<string, unknown> }, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
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
    for (const call of calls) {
      sink = [];
      const result = await writeCommonplaceDigestsToVault(call.input as never);
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
    process.stderr.write(`commonplace bridge oracle wrote ${outPath}\n`);
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

test('commonplace digest vault bridge tier-2 oracle', async () => {
  await main();
});
