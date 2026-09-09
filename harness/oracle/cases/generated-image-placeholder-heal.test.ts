/**
 * @jest-environment node
 *
 * P4.D175 placeholder-clear ORACLE — v4's REAL
 * `clear-generated-image-placeholder-descriptions-v1` migration
 * (`migrations/scripts/clear-generated-image-placeholder-descriptions.ts`, v4
 * `78b381a96`, bug 132) plus its REAL ledger write (`migrations/state.ts`
 * `recordCompletedMigration`), driven over the shared spec
 * `harness/oracle/fixtures/generated-image-placeholder-heal.json`.
 *
 * The same two-partition shape as the P4.D152 realign oracle, and the same two
 * tricks it needs (see that file's header for the reasoning):
 *
 *   1. **Un-mock `better-sqlite3`.** `jest.config.ts` maps it to a stub whose
 *      `prepare().get()` returns `undefined`; under that mock every COUNT would
 *      read as zero and the corpus would measure nothing.
 *   2. **Point `getMountIndexDatabasePath()` at a real temp file** and drop
 *      `ENCRYPTION_MASTER_PEPPER`, so both sides read a plain file and the
 *      cipher stays out of the comparand.
 *
 * Each row dumps:
 *   - `completedBefore` / `shouldRun` / the `MigrationResult` (id, success,
 *     itemsAffected, message — v4's pluralised sentence, including the
 *     `(mount index not inspected)` parenthetical, is a comparand);
 *   - the whole post-pass `files` table (id, source, description, updatedAt)
 *     and `doc_mount_file_links` table (id, originalMimeType, description),
 *     both ordered by id — so cleared rows, SURVIVING rows and the untouched
 *     `updatedAt` of an unmatched row are all comparands;
 *   - `info`: the `Cleared placeholder descriptions from generated images`
 *     line and its three counts, which are the only place `linksSkipped` is
 *     observable;
 *   - `warns`: every `logger.warn`, in order — the two skip arms are SILENT in
 *     v4 (its warn lives in the catch, not the else), and an empty array here
 *     is what says so;
 *   - `shouldRunAfter` and a SECOND `run()` (idempotence), plus both tables
 *     after it;
 *   - the `migrations_state` / `migrations_metadata` rows the runner writes.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores
 * .claude/ paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-generated-image-placeholder-heal-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/generated-image-placeholder-heal.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/generated-image-placeholder-heal.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-generated-image-placeholder-heal.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "generated-image-placeholder-heal\.test\.ts$"
 */

import { describe, it, expect, jest } from '@jest/globals';
import * as fs from 'fs';
import * as os from 'os';
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
let mountDbPath = '';

/** The migration opens its own mount handle — give it the REAL driver. */
jest.mock('better-sqlite3', () => {
  const real = loadDriver();
  return { __esModule: true, default: real, Database: real };
});

interface LogCall {
  message: string;
  context: unknown;
}
const warnCalls: LogCall[] = [];
const infoCalls: LogCall[] = [];

jest.mock('@/migrations/lib/logger', () => ({
  logger: {
    info: (message: string, context: unknown) => {
      infoCalls.push({ message, context });
    },
    debug: jest.fn(),
    warn: (message: string, context: unknown) => {
      warnCalls.push({ message, context });
    },
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
  openMountIndexDbIfPresent: (opts?: { foreignKeys?: boolean }) => {
    if (!mountDbPath || !fs.existsSync(mountDbPath)) return null;
    const Driver = loadDriver();
    const db = new Driver(mountDbPath);
    db.pragma('journal_mode = WAL');
    if (opts?.foreignKeys) db.pragma('foreign_keys = ON');
    db.pragma('busy_timeout = 5000');
    return db;
  },
}));

jest.mock('@/lib/paths', () => {
  const actual = jest.requireActual('@/lib/paths') as Record<string, unknown>;
  return {
    __esModule: true,
    ...actual,
    getMountIndexDatabasePath: () => mountDbPath,
  };
});

interface SpecFile {
  id: string;
  source: string;
  description: string | null;
}
interface SpecLink {
  id: string;
  originalMimeType: string | null;
  description: string;
}
interface Scenario {
  name: string;
  files: SpecFile[];
  links: SpecLink[];
  /** No mount partition on disk at all. */
  noMount?: boolean;
  /** A links table that predates `originalMimeType`. */
  linksTableMissingColumn?: boolean;
  /** Plant the ledger row first — the cross-app "v4 already ran it" shape. */
  preCompleted?: boolean;
}
interface Spec {
  userId: string;
  createdAt: string;
  nowIso: string;
  scenarios: Scenario[];
}

const MIGRATION_ID = 'clear-generated-image-placeholder-descriptions-v1';

/**
 * The MAIN partition, at the migration's own vintage: only the columns the pass
 * reads and writes.
 */
function buildMain(files: SpecFile[], spec: Spec): DatabaseInstance {
  const db = new Database(':memory:');
  db.exec(`
    CREATE TABLE "files" (
      "id" TEXT PRIMARY KEY,
      "userId" TEXT NOT NULL,
      "source" TEXT NOT NULL,
      "description" TEXT,
      "createdAt" TEXT NOT NULL,
      "updatedAt" TEXT NOT NULL
    );
  `);
  const insert = db.prepare(
    'INSERT INTO files (id, userId, source, description, createdAt, updatedAt) VALUES (?, ?, ?, ?, ?, ?)'
  );
  for (const f of files) {
    insert.run(f.id, spec.userId, f.source, f.description, spec.createdAt, spec.createdAt);
  }
  return db;
}

/** v4 `migrations/state.ts`'s own tables + the completed row it writes. */
function plantLedgerRow(db: DatabaseInstance, spec: Spec): void {
  db.exec(`
    CREATE TABLE IF NOT EXISTS "migrations_state" (
      "id" TEXT PRIMARY KEY,
      "completedAt" TEXT NOT NULL,
      "quilltapVersion" TEXT NOT NULL,
      "itemsAffected" INTEGER NOT NULL DEFAULT 0,
      "message" TEXT
    );
    CREATE TABLE IF NOT EXISTS "migrations_metadata" (
      "key" TEXT PRIMARY KEY,
      "value" TEXT NOT NULL
    );
  `);
  db.prepare(
    'INSERT INTO migrations_state (id, completedAt, quilltapVersion, itemsAffected, message) VALUES (?, ?, ?, ?, ?)'
  ).run(MIGRATION_ID, spec.nowIso, '4.10.0', 0, 'planted by the other app');
}

/** The MOUNT partition on disk — the migration opens this path itself. */
function buildMount(scenario: Scenario, dir: string): string {
  const p = path.join(dir, 'quilltap-mount-index.db');
  const db = new Database(p);
  db.exec(
    scenario.linksTableMissingColumn
      ? `CREATE TABLE "doc_mount_file_links" ("id" TEXT PRIMARY KEY, "description" TEXT NOT NULL DEFAULT '');`
      : `CREATE TABLE "doc_mount_file_links" (
           "id" TEXT PRIMARY KEY,
           "originalMimeType" TEXT,
           "description" TEXT NOT NULL DEFAULT ''
         );`
  );
  if (!scenario.linksTableMissingColumn) {
    const insert = db.prepare(
      'INSERT INTO doc_mount_file_links (id, originalMimeType, description) VALUES (?, ?, ?)'
    );
    for (const l of scenario.links) insert.run(l.id, l.originalMimeType, l.description);
  }
  db.close();
  return p;
}

function dumpFiles(db: DatabaseInstance) {
  return db
    .prepare('SELECT id, source, description, updatedAt FROM files ORDER BY id')
    .all() as Array<Record<string, unknown>>;
}

function dumpLinks(): Array<Record<string, unknown>> {
  if (!mountDbPath || !fs.existsSync(mountDbPath)) return [];
  const db = new Database(mountDbPath);
  try {
    const cols = (db.prepare(`PRAGMA table_info("doc_mount_file_links")`).all() as Array<{
      name: string;
    }>).map((c) => c.name);
    if (!cols.includes('originalMimeType')) {
      return db
        .prepare('SELECT id, description FROM doc_mount_file_links ORDER BY id')
        .all() as Array<Record<string, unknown>>;
    }
    return db
      .prepare('SELECT id, originalMimeType, description FROM doc_mount_file_links ORDER BY id')
      .all() as Array<Record<string, unknown>>;
  } finally {
    db.close();
  }
}

function tableExists(db: DatabaseInstance, name: string): boolean {
  return (
    (db.prepare(`SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?`).get(name) as
      | unknown
      | undefined) !== undefined
  );
}

function shapeLogs(calls: LogCall[]) {
  return calls.map((c) => {
    const ctx = (c.context ?? {}) as Record<string, unknown>;
    return {
      message: c.message,
      filesCleared: ctx.filesCleared ?? null,
      linksCleared: ctx.linksCleared ?? null,
      linksSkipped: ctx.linksSkipped ?? null,
    };
  });
}

describe('generated-image-placeholder-heal oracle', () => {
  it('runs the real migration + ledger over each scenario and dumps the result', async () => {
    const out = process.env.QT_ORACLE_OUT;
    if (!out) throw new Error('set QT_ORACLE_OUT');

    const spec = JSON.parse(
      fs.readFileSync(
        path.join(__dirname, '..', 'fixtures', 'generated-image-placeholder-heal.json'),
        'utf8'
      )
    ) as Spec;

    const savedPepper = process.env.ENCRYPTION_MASTER_PEPPER;
    delete process.env.ENCRYPTION_MASTER_PEPPER;

    const { clearGeneratedImagePlaceholderDescriptionsMigration } = await import(
      '@/migrations/scripts/clear-generated-image-placeholder-descriptions'
    );
    const { loadMigrationState, isMigrationCompleted, recordCompletedMigration } = await import(
      '@/migrations/state'
    );

    const lines: string[] = [];
    try {
      for (const scenario of spec.scenarios) {
        const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'qt-placeholder-'));
        mountDbPath = scenario.noMount ? '' : buildMount(scenario, dir);
        testDb = buildMain(scenario.files, spec);
        if (scenario.preCompleted) plantLedgerRow(testDb, spec);
        warnCalls.length = 0;
        infoCalls.length = 0;

        let state = await loadMigrationState();
        const completedBefore = isMigrationCompleted(state, MIGRATION_ID);
        // v4's runner checks the ledger FIRST and never reaches shouldRun().
        const shouldRun = completedBefore
          ? false
          : await clearGeneratedImagePlaceholderDescriptionsMigration.shouldRun();

        let result: Record<string, unknown> | null = null;
        let warns: unknown[] = [];
        let info: unknown[] = [];
        let shouldRunAfter: boolean | null = null;
        let secondRun: Record<string, unknown> | null = null;
        let filesAfterSecondRun: unknown = null;
        let linksAfterSecondRun: unknown = null;

        if (shouldRun) {
          warnCalls.length = 0;
          infoCalls.length = 0;
          const r = await clearGeneratedImagePlaceholderDescriptionsMigration.run();
          expect(r.success).toBe(true);
          result = {
            id: r.id,
            success: r.success,
            itemsAffected: r.itemsAffected,
            message: r.message,
          };
          warns = shapeLogs(warnCalls);
          info = shapeLogs(infoCalls);
          state = await recordCompletedMigration(state, r);

          shouldRunAfter = await clearGeneratedImagePlaceholderDescriptionsMigration.shouldRun();
          const r2 = await clearGeneratedImagePlaceholderDescriptionsMigration.run();
          secondRun = { itemsAffected: r2.itemsAffected, message: r2.message };
          filesAfterSecondRun = dumpFiles(testDb);
          linksAfterSecondRun = dumpLinks();
        }

        const files_ = dumpFiles(testDb);
        const links_ = dumpLinks();
        const ledgerExists = tableExists(testDb, 'migrations_state');
        const ledger = ledgerExists
          ? (testDb
              .prepare('SELECT id, itemsAffected, message FROM migrations_state ORDER BY id')
              .all() as Array<Record<string, unknown>>)
          : [];
        const metadata = ledgerExists
          ? (testDb.prepare('SELECT key FROM migrations_metadata ORDER BY key').all() as Array<
              Record<string, unknown>
            >)
          : [];

        lines.push(
          JSON.stringify({
            scenario: scenario.name,
            completedBefore,
            shouldRun,
            result,
            warns,
            info,
            files: files_,
            links: links_,
            shouldRunAfter,
            secondRun,
            filesAfterSecondRun,
            linksAfterSecondRun,
            ledger,
            metadata,
          })
        );
        testDb.close();
        mountDbPath = '';
        fs.rmSync(dir, { recursive: true, force: true });
      }
    } finally {
      if (savedPepper !== undefined) process.env.ENCRYPTION_MASTER_PEPPER = savedPepper;
    }

    fs.writeFileSync(out, lines.join('\n') + '\n');
  });
});
