/**
 * @jest-environment node
 *
 * P4.D97 heal ORACLE — v4's REAL `retire-prefill-on-thinking-profiles-v1`
 * migration (`migrations/scripts/retire-prefill-on-thinking-profiles.ts`,
 * `97d2fcb5`) plus its REAL ledger write
 * (`migrations/state.ts` `recordCompletedMigration`), driven over the shared
 * row spec `harness/oracle/fixtures/thinking-prefill-heal.json`.
 *
 * Both sides build the same migration-vintage `connection_profiles` table
 * (v4's own integration test schema) from the spec; v4 runs
 * `loadMigrationState()` → `run()` → `recordCompletedMigration()` — the exact
 * runner sequence — and this case dumps the profile rows, the
 * `migrations_state` row, the `migrations_metadata` rows, and the
 * `MigrationResult`, so the v5 heal (which must interoperate with this very
 * ledger row in BOTH directions) can be diffed cell-for-cell. A second
 * `isMigrationCompleted` read after the record proves the runner would skip a
 * re-run — the once-only property the v5 heal leans on.
 *
 * Harnessing follows v4's own
 * `__tests__/unit/lib/database/migration/retire-prefill-on-thinking-profiles.integration.test.ts`
 * (real driver by absolute path past the jest `better-sqlite3` mock; the
 * `migrations/lib` logger/progress/database-utils mocks over one shared
 * in-memory DB — with `executeSQLite`/`querySQLite` added so `state.ts` runs
 * against the same DB).
 *
 * Run (Node 24, from the pinned v4 worktree — cp to a /tmp mirror; jest
 * ignores .claude/ paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-thinking-heal-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/thinking-prefill-heal.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/thinking-prefill-heal.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-thinking-prefill-heal.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "thinking-prefill-heal\.test\.ts$"
 */

import { describe, it, expect, jest } from '@jest/globals';
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

jest.mock('@/migrations/lib/logger', () => ({
  logger: {
    info: jest.fn(),
    debug: jest.fn(),
    warn: jest.fn(),
    error: jest.fn(),
    child: jest.fn().mockReturnValue({
      info: jest.fn(),
      debug: jest.fn(),
      warn: jest.fn(),
      error: jest.fn(),
    }),
  },
}));

jest.mock('@/migrations/lib/progress', () => ({
  reportProgress: jest.fn(),
}));

// One mock serves BOTH consumers: the migration script's table/column gates
// and state.ts's ledger reads/writes — all over the one in-memory DB, so the
// ledger row lands in the same database the heal edited, exactly as a real
// SQLite-backend run does.
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
  modelName: string | null;
  parameters: string | null;
  prefill: number | null;
}

describe('thinking-prefill-heal oracle', () => {
  it('runs the real migration + ledger and dumps the result', async () => {
    const out = process.env.QT_ORACLE_OUT;
    if (!out) throw new Error('set QT_ORACLE_OUT');

    const spec = JSON.parse(
      fs.readFileSync(path.join(__dirname, '..', 'fixtures', 'thinking-prefill-heal.json'), 'utf8')
    ) as { rows: SpecRow[] };

    testDb = new Database(':memory:');
    // v4's own integration-test schema: the migration-vintage reduced table.
    testDb.exec(`
      CREATE TABLE connection_profiles (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        provider TEXT NOT NULL,
        modelName TEXT,
        parameters TEXT,
        multiCharacterPrefill INTEGER DEFAULT 1
      )
    `);
    const ins = testDb.prepare(
      `INSERT INTO connection_profiles
         (id, name, provider, modelName, parameters, multiCharacterPrefill)
       VALUES (?, ?, ?, ?, ?, ?)`
    );
    for (const r of spec.rows) {
      ins.run(r.id, `${r.provider} profile`, r.provider, r.modelName, r.parameters, r.prefill);
    }

    const { retirePrefillOnThinkingProfilesMigration } = await import(
      '@/migrations/scripts/retire-prefill-on-thinking-profiles'
    );
    const { loadMigrationState, isMigrationCompleted, recordCompletedMigration } = await import(
      '@/migrations/state'
    );

    // The runner's exact sequence.
    let state = await loadMigrationState();
    expect(isMigrationCompleted(state, 'retire-prefill-on-thinking-profiles-v1')).toBe(false);
    expect(await retirePrefillOnThinkingProfilesMigration.shouldRun()).toBe(true);
    const result = await retirePrefillOnThinkingProfilesMigration.run();
    expect(result.success).toBe(true);
    state = await recordCompletedMigration(state, result);

    // The once-only property: a re-run would be skipped at the completed check.
    const reloaded = await loadMigrationState();
    const skippedOnRerun = isMigrationCompleted(reloaded, 'retire-prefill-on-thinking-profiles-v1');

    const profiles = testDb
      .prepare('SELECT * FROM connection_profiles ORDER BY id')
      .all();
    const ledger = testDb.prepare('SELECT * FROM migrations_state ORDER BY id').all();
    const metadata = testDb.prepare('SELECT * FROM migrations_metadata ORDER BY key').all();

    fs.writeFileSync(
      out,
      JSON.stringify({
        case: 'thinking-prefill-heal',
        result: {
          id: result.id,
          success: result.success,
          itemsAffected: result.itemsAffected,
          message: result.message,
        },
        skippedOnRerun,
        profiles,
        ledger,
        metadata,
      }) + '\n'
    );
    testDb.close();
  });
});
