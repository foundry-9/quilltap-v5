/**
 * @jest-environment node
 *
 * P4.106 item 8 — the bug-158 heal ORACLE: v4's REAL
 * `clear-scenario-seeded-chat-summaries-v1` migration (`migrations/scripts/
 * clear-scenario-seeded-chat-summaries.ts`, `da9c4f34f`) plus its REAL ledger
 * (`migrations/state.ts`), driven over the shared spec
 * `harness/oracle/fixtures/scenario-seeded-summary-heal.json`. The shape is the
 * `chat-activity-heal.test.ts` precedent's, scenario for scenario.
 *
 * Both sides build the same migration-vintage reduced `chats` table from the
 * spec and follow the RUNNER's exact sequence: `loadMigrationState()` →
 * `isMigrationCompleted()` → (if not completed) `shouldRun()` → (only if true)
 * `run()` → `recordCompletedMigration()`. The case dumps every `chats` row
 * (`updatedAt` as `<bumped>` when the pass moved it), the `migrations_state`
 * rows, the `migrations_metadata` rows and the `MigrationResult`.
 *
 * Four paths, each named in the row's `path`:
 *   - `ran` — the seeded rows cleared, the ledger row written; a second
 *     `shouldRun()` proves the rewrite is its own fixed point and a reloaded
 *     `isMigrationCompleted()` proves the runner skips a re-run.
 *   - `no-drift` — nothing seeded: the runner writes NO ledger row. `run()` is
 *     called on the untouched DB purely to pin the no-op sentence (it makes no
 *     UPDATE — `toClear === 0`).
 *   - `not-applicable` — the `chats` table lacks `scenarioText`
 *     (`chatsTableUsable()` false): no run, nothing stamped, no sentence.
 *   - `already-completed` — a prior ledger row for the migration is planted
 *     through the REAL `recordCompletedMigration` first; the runner skips before
 *     `shouldRun()`, so the still-seeded rows stay seeded.
 *
 * Harnessing follows v4's own integration tests (real driver by absolute path
 * past the jest `better-sqlite3` mock; the `migrations/lib` logger/progress/
 * database-utils mocks over one shared in-memory DB — with `executeSQLite`/
 * `querySQLite` added so `state.ts` runs against the same DB).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores
 * .claude/ paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-scenario-seeded-summary-heal-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/scenario-seeded-summary-heal.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/scenario-seeded-summary-heal.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-scenario-seeded-summary-heal.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "scenario-seeded-summary-heal\.test\.ts$"
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

const MIGRATION_ID = 'clear-scenario-seeded-chat-summaries-v1';
const SEED_UPDATED_AT = '2026-12-31T00:00:00.000Z';

interface SpecChat {
  id: string;
  contextSummary: string | null;
  scenarioText: string | null;
}
interface Scenario {
  name: string;
  priorLedger?: boolean;
  omitScenarioColumn?: boolean;
  chats: SpecChat[];
}

/** The migration-vintage reduced `chats` table. */
function buildScenario(s: Scenario): DatabaseInstance {
  const db = new Database(':memory:');
  if (s.omitScenarioColumn) {
    db.exec(`
      CREATE TABLE chats (
        id TEXT PRIMARY KEY,
        contextSummary TEXT,
        createdAt TEXT NOT NULL,
        updatedAt TEXT NOT NULL
      );
    `);
    const ins = db.prepare(
      'INSERT INTO chats (id, contextSummary, createdAt, updatedAt) VALUES (?, ?, ?, ?)'
    );
    for (const c of s.chats) {
      ins.run(c.id, c.contextSummary, '2026-01-01T00:00:00.000Z', SEED_UPDATED_AT);
    }
  } else {
    db.exec(`
      CREATE TABLE chats (
        id TEXT PRIMARY KEY,
        contextSummary TEXT,
        scenarioText TEXT,
        createdAt TEXT NOT NULL,
        updatedAt TEXT NOT NULL
      );
    `);
    const ins = db.prepare(
      'INSERT INTO chats (id, contextSummary, scenarioText, createdAt, updatedAt) VALUES (?, ?, ?, ?, ?)'
    );
    for (const c of s.chats) {
      ins.run(c.id, c.contextSummary, c.scenarioText, '2026-01-01T00:00:00.000Z', SEED_UPDATED_AT);
    }
  }
  return db;
}

function dumpChats(s: Scenario): unknown[] {
  const cols = s.omitScenarioColumn
    ? 'id, contextSummary, updatedAt'
    : 'id, contextSummary, scenarioText, updatedAt';
  const rows = testDb.prepare(`SELECT ${cols} FROM chats ORDER BY id`).all() as Array<
    Record<string, unknown>
  >;
  return rows.map((r) => ({
    ...r,
    updatedAt: r.updatedAt === SEED_UPDATED_AT ? SEED_UPDATED_AT : '<bumped>',
  }));
}

describe('scenario-seeded-summary-heal oracle', () => {
  it('runs the real migration + ledger over each scenario and dumps the result', async () => {
    const out = process.env.QT_ORACLE_OUT;
    if (!out) throw new Error('set QT_ORACLE_OUT');

    const spec = JSON.parse(
      fs.readFileSync(
        path.join(__dirname, '..', 'fixtures', 'scenario-seeded-summary-heal.json'),
        'utf8'
      )
    ) as { scenarios: Scenario[] };

    const { clearScenarioSeededChatSummariesMigration: migration } = await import(
      '@/migrations/scripts/clear-scenario-seeded-chat-summaries'
    );
    const { loadMigrationState, isMigrationCompleted, recordCompletedMigration } = await import(
      '@/migrations/state'
    );

    const lines: string[] = [];
    for (const scenario of spec.scenarios) {
      testDb = buildScenario(scenario);

      if (scenario.priorLedger) {
        // A completed row written by the REAL ledger writer, as either app's
        // earlier boot would have left it.
        const prior = await loadMigrationState();
        await recordCompletedMigration(prior, {
          id: MIGRATION_ID,
          success: true,
          itemsAffected: 3,
          message: 'Cleared the scenario standing in as a summary on 3 conversations',
          durationMs: 0,
          timestamp: '2026-09-20T00:00:00.000Z',
        });
      }

      // The runner's exact sequence.
      let state = await loadMigrationState();
      const completedBefore = isMigrationCompleted(state, MIGRATION_ID);

      let pathTaken: string;
      let shouldRun: boolean | null = null;
      let result: Record<string, unknown> | null = null;
      let shouldRunAfter: boolean | null = null;
      let skippedOnRerun: boolean | null = null;
      let noDriftRunMessage: string | null = null;

      if (completedBefore) {
        pathTaken = 'already-completed';
      } else {
        shouldRun = await migration.shouldRun();
        if (shouldRun) {
          pathTaken = 'ran';
          const r = await migration.run();
          expect(r.success).toBe(true);
          result = { id: r.id, success: r.success, itemsAffected: r.itemsAffected, message: r.message };
          state = await recordCompletedMigration(state, r);
          shouldRunAfter = await migration.shouldRun();
          const reloaded = await loadMigrationState();
          skippedOnRerun = isMigrationCompleted(reloaded, MIGRATION_ID);
        } else if (scenario.omitScenarioColumn) {
          // chatsTableUsable() is false — `run()` would address a column that
          // does not exist; the runner never reaches it.
          pathTaken = 'not-applicable';
        } else {
          pathTaken = 'no-drift';
          // v4's runner writes NOTHING here. `run()` is called only to pin the
          // no-op branch's sentence — `toClear === 0`, so it makes no UPDATE.
          const r = await migration.run();
          noDriftRunMessage = r.message;
        }
      }

      const chats = dumpChats(scenario);
      const ledgerExists =
        (testDb
          .prepare(`SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?`)
          .get('migrations_state') as unknown) !== undefined;
      const ledger = ledgerExists
        ? testDb.prepare('SELECT * FROM migrations_state ORDER BY id').all()
        : [];
      const metadata = ledgerExists
        ? testDb.prepare('SELECT * FROM migrations_metadata ORDER BY key').all()
        : [];

      lines.push(
        JSON.stringify({
          scenario: scenario.name,
          path: pathTaken,
          completedBefore,
          shouldRun,
          shouldRunAfter,
          skippedOnRerun,
          noDriftRunMessage,
          result,
          chats,
          ledger,
          metadata,
        })
      );
      testDb.close();
    }

    fs.writeFileSync(out, lines.join('\n') + '\n');
  });
});
