/**
 * @jest-environment node
 *
 * ORACLE for v4's JSON-setting reads (P4.113 Tier 2 — v4 `d1c06cd9d`,
 * `lib/instance-settings/index.ts:130-145` `readJsonSetting`), ported to
 * quilltap_core::db::instance_settings.
 *
 * Drives v4's REAL five getters — `getMemoryRecallSettings`,
 * `getDataRetentionSettings`, `getBrahmaConsoleSettings`, `getTabooSettings`,
 * `getMemoryExtractionLimits` — over a REAL (fresh, test-pepper) database: per
 * case the `instance_settings` row for `key` is deleted, then (unless `raw` is
 * null) written with the case's raw text through v4's own `rawQuery`, then the
 * getter runs. Records the returned value and every WARN v4 logged during the
 * getter (off the `Logger` prototype, singleton and children alike, before the
 * level check): its message and whether a non-empty `error` field rode with it
 * (compared by PRESENCE — V8's `JSON.parse` / Zod wording is not v5's).
 *
 * The DB stack is doMocked to the REAL modules (past jest.setup's global
 * mocks) plus the real better-sqlite3-multiple-ciphers cipher binding (the
 * scenario-builder-mount-pool recipe).
 *
 * Emits ONE NDJSON line per case: { name, key, value, warns }.
 *
 * Run (Node 24, from the v4 checkout or a PINNED worktree). STAGE this case
 * OUTSIDE `.claude/` — v4's jest ignores those paths.
 *   N=~/.nvm/versions/node/v24.13.1/bin ; W=<this worktree>
 *   STAGE=/tmp/qt-oracle-stage-is-json-warns
 *   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
 *   cp $W/harness/oracle/cases/instance-settings-json-warns.test.ts $STAGE/harness/oracle/cases/
 *   cp $W/harness/oracle/fixtures/instance-settings-json-warns.json $STAGE/harness/oracle/fixtures/
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-instance-settings-json-warns.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=240000 \
 *       --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "instance-settings-json-warns\.test\.ts$"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Case {
  name: string;
  key: string;
  raw: string | null;
}

/** The throwaway test pepper (32 bytes, base64) — never a real instance's. */
const TEST_PEPPER = '3q2+796tvu3erb7t3q2+796tvu3erb7t3q2+796tvu0=';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'instance-settings-json-warns.json'), 'utf8'),
  ) as { cases: Case[] };
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-is-json-warns-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;
  process.env.QUILLTAP_DATA_DIR = scratch;
  process.env.SQLITE_PATH = join(scratch, 'data', 'quilltap.db');
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/instance-settings', () => jest.requireActual('@/lib/instance-settings'));

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const settings = await import('@/lib/instance-settings');
  const { Logger } = await import('@/lib/logger');

  let sink: Array<Record<string, unknown>> | null = null;
  const original = Logger.prototype.warn;
  Logger.prototype.warn = function (
    this: unknown,
    message: string,
    context?: Record<string, unknown>,
    ...rest: unknown[]
  ) {
    sink?.push({
      message,
      hasError: typeof context?.error === 'string' && context.error.length > 0,
    });
    return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
  } as never;

  const getters: Record<string, () => Promise<unknown>> = {
    memoryRecall: settings.getMemoryRecallSettings,
    dataRetention: settings.getDataRetentionSettings,
    brahmaConsole: settings.getBrahmaConsoleSettings,
    taboo: settings.getTabooSettings,
    memoryExtractionLimits: settings.getMemoryExtractionLimits,
  };

  await initializeDatabase();
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  const outLines: string[] = [];
  try {
    for (const c of spec.cases) {
      const getter = getters[c.key];
      if (!getter) throw new Error(`no getter for key ${c.key}`);
      await rawQuery('DELETE FROM "instance_settings" WHERE "key" = ?', [c.key]);
      if (c.raw !== null) {
        await rawQuery('INSERT INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
          c.key,
          c.raw,
        ]);
      }
      sink = [];
      const value = await getter();
      outLines.push(JSON.stringify({ name: c.name, key: c.key, value, warns: sink }));
      sink = null;
    }
  } finally {
    Logger.prototype.warn = original;
    await closeDatabase();
    rmSync(scratch, { recursive: true, force: true });
  }

  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`instance-settings-json-warns oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('instance-settings json-warns oracle', async () => {
  await main();
});
