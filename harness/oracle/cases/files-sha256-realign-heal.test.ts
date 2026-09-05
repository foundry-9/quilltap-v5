/**
 * @jest-environment node
 *
 * P4.D152 realign ORACLE — v4's REAL `realign-file-entry-sha256-v1` migration
 * (`migrations/scripts/realign-file-entry-sha256.ts`, v4 `0b0617fee`, bug 117)
 * plus its REAL ledger write (`migrations/state.ts` `recordCompletedMigration`),
 * driven over the shared spec
 * `harness/oracle/fixtures/files-sha256-realign-heal.json`.
 *
 * The migration reads TWO databases: the MAIN partition through
 * `migrations/lib/database-utils` (mocked to a shared in-memory DB, exactly as
 * the P4.D145 collapse oracle does) and the MOUNT-INDEX partition, which it
 * opens ITSELF with `new Database(getMountIndexDatabasePath())`. So this oracle
 * has to do two extra things the single-partition heal oracles do not:
 *
 *   1. **Un-mock `better-sqlite3`.** `jest.config.ts` maps it to
 *      `__mocks__/better-sqlite3.ts`, whose `prepare().get()` returns
 *      `undefined` — under that mock EVERY row would read as an orphaned blob
 *      and the corpus would measure nothing. The real driver is loaded by
 *      absolute path (v4's own integration-test trick) and handed back through
 *      `jest.mock`.
 *   2. **Point `getMountIndexDatabasePath()` at a real temp file** and drop
 *      `ENCRYPTION_MASTER_PEPPER` for the duration, so the migration's
 *      `PRAGMA key` branch is not taken and both sides read a plain file. The
 *      cipher is boot infrastructure, not part of this comparand.
 *
 * Each row dumps:
 *   - `completedBefore` / `shouldRun` / the `MigrationResult` (id, success,
 *     itemsAffected, message — the four-count sentence is a comparand);
 *   - the whole post-pass `files` table (id, sha256, storageKey, updatedAt)
 *     ordered by id — so realigned rows, skipped rows AND the untouched
 *     `updatedAt` of an agreeing row are all comparands;
 *   - `warns`: every `logger.warn` the pass emitted, in order, as
 *     `{message, fileId, blobId, storageKey}` — the malformed-key and
 *     missing-blob arms are log-only on the state, so without this they would
 *     be invisible;
 *   - `shouldRunAfter` and a SECOND `run()` (idempotence), plus the files table
 *     after it;
 *   - the `migrations_state` / `migrations_metadata` rows v4's runner writes.
 *     ⚠ v5 writes NO row on a pass that realigned zero (see
 *     `db::files_sha256_realign_heal`'s header) while v4's runner writes one
 *     whenever `shouldRun()` was true — a RECORDED DIVERGENCE the Rust side
 *     asserts in both directions rather than a match.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores
 * .claude/ paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-files-sha256-realign-heal-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/files-sha256-realign-heal.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/files-sha256-realign-heal.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-files-sha256-realign-heal.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "files-sha256-realign-heal\.test\.ts$"
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

/** The migration constructs its own mount handle — give it the REAL driver. */
jest.mock('better-sqlite3', () => {
  const real = loadDriver();
  return { __esModule: true, default: real, Database: real };
});

interface WarnCall {
  message: string;
  context: unknown;
}
const warnCalls: WarnCall[] = [];
const infoCalls: WarnCall[] = [];

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
  // v4 `0506517d3` collapsed the migration's inline mount-index opener onto
  // this module's `openMountIndexDbIfPresent` (→ `openEncryptedSqlite`); a mock
  // without it made `run()` abort with "is not a function" at the `d883a5ee1`
  // unification. This mirrors what the inline opener did in this venue: the
  // case has already dropped `ENCRYPTION_MASTER_PEPPER`, so no key pragma.
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
  sha256: string;
  storageKey: string | null;
}
interface SpecBlob {
  id: string;
  sha256: string;
}
interface Scenario {
  name: string;
  files: SpecFile[];
  blobs: SpecBlob[];
  generate?: { count: number };
  /** Plant the ledger row first — the cross-app "v4 already ran it" shape. */
  preCompleted?: boolean;
}
interface Spec {
  userId: string;
  createdAt: string;
  nowIso: string;
  scenarios: Scenario[];
}

/**
 * The generated past-the-first-batch rows. Ids are zero-padded so the keyset
 * walk's `ORDER BY id` is the same total order on both sides, and every row is
 * drifted so the batch boundary is measurable in the counts.
 */
function expand(s: Scenario): { files: SpecFile[]; blobs: SpecBlob[] } {
  const files = [...s.files];
  const blobs = [...s.blobs];
  if (s.generate) {
    for (let i = 0; i < s.generate.count; i++) {
      const n = String(i).padStart(4, '0');
      blobs.push({ id: `gb-${n}`, sha256: `stored-${n}` });
      files.push({ id: `gf-${n}`, sha256: `input-${n}`, storageKey: `mount-blob:mp-1:gb-${n}` });
    }
  }
  return { files, blobs };
}

/**
 * The MAIN partition, at the migration's own vintage: only the columns the pass
 * reads and writes, which is also all v4's own test builds.
 */
function buildMain(files: SpecFile[], spec: Spec): DatabaseInstance {
  const db = new Database(':memory:');
  db.exec(`
    CREATE TABLE "files" (
      "id" TEXT PRIMARY KEY,
      "userId" TEXT NOT NULL,
      "sha256" TEXT NOT NULL,
      "storageKey" TEXT,
      "createdAt" TEXT NOT NULL,
      "updatedAt" TEXT NOT NULL
    );
  `);
  const insert = db.prepare(
    'INSERT INTO files (id, userId, sha256, storageKey, createdAt, updatedAt) VALUES (?, ?, ?, ?, ?, ?)'
  );
  for (const f of files) {
    insert.run(f.id, spec.userId, f.sha256, f.storageKey, spec.createdAt, spec.createdAt);
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
  ).run(
    'realign-file-entry-sha256-v1',
    spec.nowIso,
    '4.9.0',
    0,
    'planted by the other app'
  );
}

/** The MOUNT partition on disk — the migration opens this path itself. */
function buildMount(blobs: SpecBlob[], dir: string): string {
  const p = path.join(dir, 'quilltap-mount-index.db');
  const db = new Database(p);
  db.exec(`
    CREATE TABLE "doc_mount_blobs" (
      "id" TEXT PRIMARY KEY,
      "sha256" TEXT NOT NULL
    );
  `);
  const insert = db.prepare('INSERT INTO doc_mount_blobs (id, sha256) VALUES (?, ?)');
  for (const b of blobs) insert.run(b.id, b.sha256);
  db.close();
  return p;
}

function dumpFiles(db: DatabaseInstance) {
  return db
    .prepare('SELECT id, sha256, storageKey, updatedAt FROM files ORDER BY id')
    .all() as Array<Record<string, unknown>>;
}

function tableExists(db: DatabaseInstance, name: string): boolean {
  return (
    (db.prepare(`SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?`).get(name) as
      | unknown
      | undefined) !== undefined
  );
}

/** The warn shape both sides compare: message + the three id fields v4 carries. */
function shapeWarns(calls: WarnCall[]) {
  return calls.map((c) => {
    const ctx = (c.context ?? {}) as Record<string, unknown>;
    return {
      message: c.message,
      fileId: ctx.fileId ?? null,
      blobId: ctx.blobId ?? null,
      storageKey: ctx.storageKey ?? null,
    };
  });
}

describe('files-sha256-realign-heal oracle', () => {
  it('runs the real migration + ledger over each scenario and dumps the result', async () => {
    const out = process.env.QT_ORACLE_OUT;
    if (!out) throw new Error('set QT_ORACLE_OUT');

    const spec = JSON.parse(
      fs.readFileSync(
        path.join(__dirname, '..', 'fixtures', 'files-sha256-realign-heal.json'),
        'utf8'
      )
    ) as Spec;

    // The migration keys the mount DB when a pepper is present; both sides read
    // a plain file here (see the header).
    const savedPepper = process.env.ENCRYPTION_MASTER_PEPPER;
    delete process.env.ENCRYPTION_MASTER_PEPPER;

    const { realignFileEntrySha256Migration } = await import(
      '@/migrations/scripts/realign-file-entry-sha256'
    );
    const { loadMigrationState, isMigrationCompleted, recordCompletedMigration } = await import(
      '@/migrations/state'
    );

    const lines: string[] = [];
    try {
      for (const scenario of spec.scenarios) {
        const { files, blobs } = expand(scenario);
        const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'qt-realign-'));
        mountDbPath = buildMount(blobs, dir);
        testDb = buildMain(files, spec);
        if (scenario.preCompleted) plantLedgerRow(testDb, spec);
        warnCalls.length = 0;
        infoCalls.length = 0;

        let state = await loadMigrationState();
        const completedBefore = isMigrationCompleted(state, 'realign-file-entry-sha256-v1');
        // v4's runner checks the ledger FIRST and never reaches shouldRun().
        const shouldRun = completedBefore
          ? false
          : await realignFileEntrySha256Migration.shouldRun();

        let result: Record<string, unknown> | null = null;
        let warns: unknown[] = [];
        let summary: unknown = null;
        let shouldRunAfter: boolean | null = null;
        let secondRun: Record<string, unknown> | null = null;
        let filesAfterSecondRun: unknown = null;

        if (shouldRun) {
          const r = await realignFileEntrySha256Migration.run();
          expect(r.success).toBe(true);
          result = {
            id: r.id,
            success: r.success,
            itemsAffected: r.itemsAffected,
            message: r.message,
          };
          warns = shapeWarns(warnCalls);
          const info = infoCalls.find((c) =>
            c.message.startsWith('Scanned ')
          );
          summary = info ? { message: info.message, context: info.context } : null;
          state = await recordCompletedMigration(state, r);

          shouldRunAfter = await realignFileEntrySha256Migration.shouldRun();
          warnCalls.length = 0;
          const r2 = await realignFileEntrySha256Migration.run();
          secondRun = { itemsAffected: r2.itemsAffected, message: r2.message };
          filesAfterSecondRun = dumpFiles(testDb);
        }

        const files_ = dumpFiles(testDb);
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
            summary,
            files: files_,
            shouldRunAfter,
            secondRun,
            filesAfterSecondRun,
            ledger,
            metadata,
          })
        );
        testDb.close();
        fs.rmSync(dir, { recursive: true, force: true });
      }
    } finally {
      if (savedPepper !== undefined) process.env.ENCRYPTION_MASTER_PEPPER = savedPepper;
    }

    fs.writeFileSync(out, lines.join('\n') + '\n');
  });
});
