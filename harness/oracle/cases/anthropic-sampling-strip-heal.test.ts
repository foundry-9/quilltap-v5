/**
 * @jest-environment node
 *
 * P4.D362 strip-heal ORACLE — v4's REAL `strip-anthropic-sampling-parameters-v1`
 * migration (`migrations/scripts/strip-anthropic-sampling-parameters-v1.ts`,
 * `544c7ab1e`) plus its REAL ledger (`migrations/state.ts`
 * `loadMigrationState` / `isMigrationCompleted` / `recordCompletedMigration`),
 * driven over the shared row spec
 * `harness/oracle/fixtures/anthropic-sampling-strip-heal.json`. The
 * `thinking-prefill-heal.test.ts` recipe whole: the real driver past the jest
 * `better-sqlite3` mock, the `migrations/lib` logger / progress /
 * database-utils mocks over one in-memory DB per mode.
 *
 * Modes (the runner's exact sequence each time — completed check FIRST, then
 * `shouldRun`, then `run` + `recordCompletedMigration`):
 *   (a) `unstamped`       — the spec table, no ledger → v4 RUNS and stamps;
 *   (b) `already-stamped` — (a), then a NEW Anthropic row carrying
 *                           `temperature` planted AFTER the stamp, then the
 *                           runner again → skipped, the row untouched;
 *   (c) `no-parameters`   — a table without `parameters` → `shouldRun` false,
 *                           nothing stamped;
 *   (d) `no-table`        — no `connection_profiles` → `shouldRun` false.
 * Each mode dumps every `parameters` cell `quote()`d, the `migrations_state`
 * and `migrations_metadata` rows, the `MigrationResult`, the logger calls
 * (level, message, meta), and a second `isMigrationCompleted` read.
 *
 * Run (Node 24, from the pinned v4 worktree — cp to a /tmp mirror; jest
 * ignores .claude/ paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-anthropic-strip-heal-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/anthropic-sampling-strip-heal.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/anthropic-sampling-strip-heal.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-anthropic-sampling-strip-heal.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "anthropic-sampling-strip-heal\.test\.ts$"
 */

import { describe, it, jest } from '@jest/globals';
import * as fs from 'fs';
import path from 'path';

function loadDriver() {
  try {
    return require(path.join(
      process.cwd(),
      'packages',
      'quilltap',
      'node_modules',
      'better-sqlite3-multiple-ciphers'
    ));
  } catch {
    return require(path.join(process.cwd(), 'node_modules', 'better-sqlite3'));
  }
}
const Database = loadDriver();
type DatabaseInstance = ReturnType<typeof Database>;

let testDb: DatabaseInstance = null as unknown as DatabaseInstance;
const logs: Array<{ level: string; message: string; meta: unknown }> = [];

jest.mock('@/migrations/lib/logger', () => {
  const rec = (level: string) => (message: string, meta?: unknown) => {
    logs.push({ level, message, meta: meta ?? null });
  };
  const l = { info: rec('info'), debug: rec('debug'), warn: rec('warn'), error: rec('error') };
  return { logger: { ...l, child: () => l } };
});

jest.mock('@/migrations/lib/progress', () => ({
  reportProgress: () => undefined,
}));

jest.mock('@/migrations/lib/database-utils', () => ({
  isSQLiteBackend: () => true,
  sqliteTableExists: (name: string) =>
    (
      testDb
        .prepare(`SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?`)
        .get(name) as unknown
    ) !== undefined,
  getSQLiteDatabase: () => testDb,
  getSQLiteTableColumns: (name: string) =>
    testDb.prepare(`PRAGMA table_info(${name})`).all() as Array<{ name: string }>,
  executeSQLite: (sql: string) => {
    testDb.exec(sql);
  },
  querySQLite: (sql: string) => testDb.prepare(sql).all(),
}));

interface SpecRow {
  id: string;
  provider: string;
  parameters: string | null;
}

const ID = 'strip-anthropic-sampling-parameters-v1';

describe('anthropic-sampling-strip-heal oracle', () => {
  it('runs the real migration + ledger over every mode', async () => {
    const out = process.env.QT_ORACLE_OUT;
    if (!out) throw new Error('set QT_ORACLE_OUT');
    const spec = JSON.parse(
      fs.readFileSync(
        path.join(__dirname, '..', 'fixtures', 'anthropic-sampling-strip-heal.json'),
        'utf8'
      )
    ) as { rows: SpecRow[] };

    const { stripAnthropicSamplingParametersMigration: m } = await import(
      '@/migrations/scripts/strip-anthropic-sampling-parameters-v1'
    );
    const { loadMigrationState, isMigrationCompleted, recordCompletedMigration } = await import(
      '@/migrations/state'
    );

    async function runner() {
      let state = await loadMigrationState();
      if (isMigrationCompleted(state, ID)) return { ran: false, skipped: 'completed', result: null };
      if (!(await m.shouldRun())) return { ran: false, skipped: 'shouldRun', result: null };
      const result = await m.run();
      if (!result.success) throw new Error(result.message);
      state = await recordCompletedMigration(state, result);
      return {
        ran: true,
        skipped: null,
        result: {
          id: result.id,
          success: result.success,
          itemsAffected: result.itemsAffected,
          message: result.message,
        },
      };
    }

    function seedTable(withParameters: boolean) {
      testDb.exec(
        withParameters
          ? 'CREATE TABLE connection_profiles (id TEXT PRIMARY KEY, name TEXT NOT NULL, provider TEXT NOT NULL, parameters TEXT)'
          : 'CREATE TABLE connection_profiles (id TEXT PRIMARY KEY, name TEXT NOT NULL, provider TEXT NOT NULL)'
      );
      for (const r of spec.rows) {
        if (withParameters) {
          testDb
            .prepare('INSERT INTO connection_profiles (id, name, provider, parameters) VALUES (?, ?, ?, ?)')
            .run(r.id, `${r.provider} profile`, r.provider, r.parameters);
        } else {
          testDb
            .prepare('INSERT INTO connection_profiles (id, name, provider) VALUES (?, ?, ?)')
            .run(r.id, `${r.provider} profile`, r.provider);
        }
      }
    }

    function tableExists(name: string) {
      return testDb.prepare(`SELECT 1 FROM sqlite_master WHERE type='table' AND name=?`).get(name) !== undefined;
    }

    function dump(mode: string, run: unknown, extra: Record<string, unknown> = {}) {
      const hasParams =
        tableExists('connection_profiles') &&
        (testDb.prepare('PRAGMA table_info(connection_profiles)').all() as Array<{ name: string }>).some(
          (c) => c.name === 'parameters'
        );
      const profiles = hasParams
        ? testDb.prepare('SELECT id, quote(parameters) AS parameters FROM connection_profiles ORDER BY rowid').all()
        : [];
      const ledger = tableExists('migrations_state')
        ? testDb.prepare('SELECT id, itemsAffected, message FROM migrations_state ORDER BY id').all()
        : [];
      const metadataKeys = tableExists('migrations_metadata')
        ? testDb.prepare('SELECT key FROM migrations_metadata ORDER BY key').all()
        : [];
      return {
        case: 'anthropic-sampling-strip-heal',
        mode,
        run,
        profiles,
        ledger,
        metadataKeys,
        ledgerTables: tableExists('migrations_state') && tableExists('migrations_metadata'),
        logs: logs.splice(0),
        ...extra,
      };
    }

    const lines: unknown[] = [];

    // (a) unstamped
    testDb = new Database(':memory:');
    seedTable(true);
    logs.splice(0);
    {
      const run = await runner();
      const skippedOnRerun = isMigrationCompleted(await loadMigrationState(), ID);
      lines.push(dump('unstamped', run, { skippedOnRerun }));
    }
    testDb.close();

    // (b) already stamped by v4, then a plant after the stamp
    testDb = new Database(':memory:');
    seedTable(true);
    await runner();
    testDb
      .prepare('INSERT INTO connection_profiles (id, name, provider, parameters) VALUES (?, ?, ?, ?)')
      .run('anth-after-stamp', 'late', 'ANTHROPIC', '{"temperature":0.4}');
    logs.splice(0);
    lines.push(dump('already-stamped', await runner()));
    testDb.close();

    // (c) no parameters column
    testDb = new Database(':memory:');
    seedTable(false);
    logs.splice(0);
    lines.push(dump('no-parameters', await runner()));
    testDb.close();

    // (d) no table
    testDb = new Database(':memory:');
    logs.splice(0);
    lines.push(dump('no-table', await runner()));
    testDb.close();

    fs.writeFileSync(out, lines.map((l) => JSON.stringify(l)).join('\n') + '\n');
  });
});
