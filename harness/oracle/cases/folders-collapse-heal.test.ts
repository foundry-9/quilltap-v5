/**
 * @jest-environment node
 *
 * P4.D145 collapse ORACLE — v4's REAL `collapse-duplicate-folders-v1` migration
 * (`migrations/scripts/collapse-duplicate-folders.ts`, v4 `a5df98b3f`, bug 114)
 * plus its REAL ledger write (`migrations/state.ts` `recordCompletedMigration`),
 * driven over the shared spec
 * `harness/oracle/fixtures/folders-collapse-heal.json`.
 *
 * Both sides build the same migration-vintage `folders` table — v4's own
 * `__tests__/unit/migrations/collapse-duplicate-folders.test.ts` DDL, with NO
 * unique index, which is also the shape v4's `generateDDL` emits (an expression
 * index is inexpressible there, so neither app's fresh surface carries it) — and
 * follow the RUNNER's sequence: `loadMigrationState()` -> `isMigrationCompleted()`
 * -> `shouldRun()` -> `run()` -> `recordCompletedMigration()`.
 *
 * Each row dumps:
 *   - the whole post-pass `folders` table (id, userId, projectId, path,
 *     parentFolderId, createdAt, updatedAt) ordered by id — so the survivor
 *     choice, the repoint and the deletes are all comparands;
 *   - `indexSql`, the `sqlite_master` text of the created index. This is the
 *     cross-app once-only marker: v4's `shouldRun()` is `!indexExists()`, so a
 *     byte-identical index in a v5-collapsed database is exactly what makes a
 *     later v4 boot skip;
 *   - `shouldRunAfter` and a SECOND `run()` (v4's own no-op-on-re-run case);
 *   - `probes`: inserts attempted after the pass, each recorded as threw/not —
 *     v4's "the index then rejects a duplicate insert" case, generalized;
 *   - the `migrations_state` / `migrations_metadata` rows v4's runner writes.
 *     v5 writes NEITHER (its guard is the index, never the ledger), so the
 *     harness records that as a deliberate divergence rather than a match.
 *
 * Harnessing follows v4's own integration test (the real driver by absolute path
 * past the jest `better-sqlite3` mock; the `migrations/lib` logger / progress /
 * database-utils mocks over one shared in-memory DB, with `executeSQLite` /
 * `querySQLite` added so `state.ts` runs against the same DB).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores
 * .claude/ paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-folders-collapse-heal-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/folders-collapse-heal.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/folders-collapse-heal.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-folders-collapse-heal.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "folders-collapse-heal\.test\.ts$"
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

interface SpecFolder {
  id: string;
  userId?: string;
  projectId: string | null;
  path: string;
  parentFolderId?: string | null;
  createdAt: string;
}
interface SpecProbe {
  id: string;
  userId?: string;
  projectId: string | null;
  path: string;
}
interface Scenario {
  name: string;
  folders: SpecFolder[];
  probes: SpecProbe[];
}

/** v4's own test derivation: the path's last segment, or 'Root'. */
function folderName(p: string): string {
  return p.replace(/\/$/, '').split('/').pop() || 'Root';
}

/** v4's integration-test schema: the migration-vintage table, no unique index. */
function buildScenario(s: Scenario, defaultUser: string): DatabaseInstance {
  const db = new Database(':memory:');
  db.exec(`
    CREATE TABLE "folders" (
      "id" TEXT PRIMARY KEY,
      "userId" TEXT NOT NULL,
      "path" TEXT NOT NULL,
      "name" TEXT NOT NULL,
      "parentFolderId" TEXT,
      "projectId" TEXT,
      "createdAt" TEXT NOT NULL,
      "updatedAt" TEXT NOT NULL
    );
  `);
  const insert = db.prepare(
    'INSERT INTO folders (id, userId, path, name, parentFolderId, projectId, createdAt, updatedAt) ' +
      'VALUES (?, ?, ?, ?, ?, ?, ?, ?)'
  );
  for (const row of s.folders) {
    insert.run(
      row.id,
      row.userId ?? defaultUser,
      row.path,
      folderName(row.path),
      row.parentFolderId ?? null,
      row.projectId,
      row.createdAt,
      row.createdAt
    );
  }
  return db;
}

function dumpFolders(db: DatabaseInstance) {
  return db
    .prepare(
      'SELECT id, userId, projectId, path, parentFolderId, createdAt, updatedAt ' +
        'FROM folders ORDER BY id'
    )
    .all() as Array<Record<string, unknown>>;
}

function indexSql(db: DatabaseInstance): string | null {
  const row = db
    .prepare(`SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?`)
    .get('idx_folders_userId_projectId_path') as { sql: string } | undefined;
  return row ? row.sql : null;
}

function tableExists(db: DatabaseInstance, name: string): boolean {
  return (
    (db.prepare(`SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?`).get(name) as
      | unknown
      | undefined) !== undefined
  );
}

describe('folders-collapse-heal oracle', () => {
  it('runs the real migration + ledger over each scenario and dumps the result', async () => {
    const out = process.env.QT_ORACLE_OUT;
    if (!out) throw new Error('set QT_ORACLE_OUT');

    const spec = JSON.parse(
      fs.readFileSync(path.join(__dirname, '..', 'fixtures', 'folders-collapse-heal.json'), 'utf8')
    ) as { userId: string; scenarios: Scenario[] };

    const { collapseDuplicateFoldersMigration } = await import(
      '@/migrations/scripts/collapse-duplicate-folders'
    );
    const { loadMigrationState, isMigrationCompleted, recordCompletedMigration } = await import(
      '@/migrations/state'
    );

    const lines: string[] = [];
    for (const scenario of spec.scenarios) {
      testDb = buildScenario(scenario, spec.userId);

      let state = await loadMigrationState();
      const completedBefore = isMigrationCompleted(state, 'collapse-duplicate-folders-v1');
      const shouldRun = await collapseDuplicateFoldersMigration.shouldRun();
      expect(shouldRun).toBe(true); // no index yet, in every scenario

      const r = await collapseDuplicateFoldersMigration.run();
      expect(r.success).toBe(true);
      const result = {
        id: r.id,
        success: r.success,
        itemsAffected: r.itemsAffected,
        message: r.message,
      };
      state = await recordCompletedMigration(state, r);

      const folders = dumpFolders(testDb);
      const createdIndexSql = indexSql(testDb);
      const shouldRunAfter = await collapseDuplicateFoldersMigration.shouldRun();

      // v4's own "is a no-op on a second run" case, run for every scenario.
      const r2 = await collapseDuplicateFoldersMigration.run();
      const secondRun = { itemsAffected: r2.itemsAffected, message: r2.message };
      const foldersAfterSecondRun = dumpFolders(testDb);

      // v4's "the index then rejects a duplicate insert" case, generalized: each
      // probe is an INSERT attempted against the freshly-indexed table.
      const probeInsert = testDb.prepare(
        'INSERT INTO folders (id, userId, path, name, parentFolderId, projectId, createdAt, updatedAt) ' +
          "VALUES (?, ?, ?, ?, NULL, ?, '2026-05-01T00:00:00.000Z', '2026-05-01T00:00:00.000Z')"
      );
      const probes = scenario.probes.map((p) => {
        try {
          probeInsert.run(
            p.id,
            p.userId ?? spec.userId,
            p.path,
            folderName(p.path),
            p.projectId
          );
          return { id: p.id, threw: false, message: null as string | null };
        } catch (error) {
          return {
            id: p.id,
            threw: true,
            message: error instanceof Error ? error.message : String(error),
          };
        }
      });

      const ledgerExists = tableExists(testDb, 'migrations_state');
      const ledger = ledgerExists
        ? (testDb.prepare('SELECT * FROM migrations_state ORDER BY id').all() as Array<
            Record<string, unknown>
          >)
        : [];
      const metadata = ledgerExists
        ? (testDb.prepare('SELECT * FROM migrations_metadata ORDER BY key').all() as Array<
            Record<string, unknown>
          >)
        : [];

      lines.push(
        JSON.stringify({
          scenario: scenario.name,
          completedBefore,
          shouldRun,
          result,
          folders,
          indexSql: createdIndexSql,
          shouldRunAfter,
          secondRun,
          foldersAfterSecondRun,
          probes,
          ledger,
          metadata,
        })
      );
      testDb.close();
    }

    fs.writeFileSync(out, lines.join('\n') + '\n');
  });
});
