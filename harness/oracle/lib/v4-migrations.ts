/**
 * Run v4's REAL migration modules against one encrypted SQLite file (P4.D225 —
 * the migrator's module-running arm, and the per-run fixture builders' widen).
 *
 * Why the real modules and not the ADD-COLUMN twin: the chain after P4.D225
 * needs migrations the twin cannot express (#75's and #76's JavaScript
 * backfills, #76's DROP). Driving v4's own `shouldRun()` / `run()` is the only
 * shape that carries all of them, and for an ALTER-only migration (P4.D225's
 * `add-chat-refusal-ledger-v1`) it is simply the same statement from its
 * source.
 *
 * Mechanics that bite:
 *
 * - v4's migration connection (`migrations/lib/database-utils.ts`
 *   `getSQLiteDatabase`) reads `SQLITE_PATH` + `ENCRYPTION_MASTER_PEPPER` from
 *   the environment, takes the instance lock under the DATA dir, and opens the
 *   file with `journal_mode = WAL` — which is PERSISTED in the file header. A
 *   fixture must come back in the journal mode it went in with, so the mode is
 *   read first and restored after (and any `-wal`/`-shm` residue removed).
 * - The connection is a module-level singleton: `closeSQLite()` before and
 *   after, and the utils module is imported by the SAME absolute URL the
 *   migration modules resolve, so both see one instance.
 * - The environment is saved and restored, so a builder that set its own
 *   `SQLITE_PATH` for `initializeDatabase` is left as it was.
 *
 * Run with cwd = the v4 checkout (or a pinned worktree): every module path is
 * resolved against it, and `better-sqlite3` is required from it (v4 aliases it
 * to the sqleet/ChaCha20 build).
 */

import { existsSync, mkdtempSync, rmSync, statSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

export interface V4MigrationRef {
  /** Path under the v4 root, e.g. `migrations/scripts/add-chat-refusal-ledger.ts`. */
  file: string;
  /** The exported `Migration` object's name. */
  exportName: string;
}

export interface RunV4MigrationsOptions {
  dbPath: string;
  pepperBase64: string;
  migrations: V4MigrationRef[];
  /** Report what `shouldRun()` says; run nothing. */
  reportOnly?: boolean;
  /**
   * A migration whose FILE is absent at this pin is skipped and reported
   * rather than thrown — how a builder stays runnable at a baseline pin that
   * predates the migration (and so predates the columns, as v4 there does).
   */
  allowMissing?: boolean;
}

const ENV_KEYS = ['SQLITE_PATH', 'ENCRYPTION_MASTER_PEPPER', 'QUILLTAP_DATA_DIR'] as const;

function keyHex(pepperBase64: string): string {
  return Buffer.from(pepperBase64, 'base64').toString('hex');
}

function readJournalMode(Database: any, dbPath: string, pepperBase64: string): string {
  const db = new Database(dbPath, { readonly: true });
  try {
    db.pragma(`key = "x'${keyHex(pepperBase64)}'"`);
    return String(db.pragma('journal_mode', { simple: true }));
  } finally {
    db.close();
  }
}

export async function runV4Migrations(opts: RunV4MigrationsOptions): Promise<string[]> {
  const v4Root = process.cwd();
  const requireFromV4 = createRequire(v4Root + '/');
  const Database = requireFromV4('better-sqlite3');
  const report: string[] = [];

  const utilsPath = join(v4Root, 'migrations/lib/database-utils.ts');
  if (!existsSync(utilsPath)) throw new Error(`not a v4 checkout (no ${utilsPath})`);

  const savedEnv = Object.fromEntries(ENV_KEYS.map((k) => [k, process.env[k]]));
  const scratch = mkdtempSync(join(tmpdir(), 'qt-v4-migrations-'));
  const originalMode = readJournalMode(Database, opts.dbPath, opts.pepperBase64);

  process.env.SQLITE_PATH = opts.dbPath;
  process.env.ENCRYPTION_MASTER_PEPPER = opts.pepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;

  const utils = await import(pathToFileURL(utilsPath).href);
  utils.closeSQLite();
  try {
    for (const ref of opts.migrations) {
      const file = join(v4Root, ref.file);
      if (!existsSync(file)) {
        if (opts.allowMissing) {
          report.push(`${ref.file} absent at this pin (skipped)`);
          continue;
        }
        throw new Error(`migration module not found at this pin: ${ref.file}`);
      }
      const mod = await import(pathToFileURL(file).href);
      const migration = mod[ref.exportName];
      if (!migration || typeof migration.shouldRun !== 'function') {
        throw new Error(`${ref.file} exports no migration named ${ref.exportName}`);
      }
      const needed = await migration.shouldRun();
      if (!needed) {
        report.push(`${migration.id} not needed`);
        continue;
      }
      if (opts.reportOnly) {
        report.push(`WOULD RUN ${migration.id}`);
        continue;
      }
      const result = await migration.run();
      if (!result?.success) {
        throw new Error(`${migration.id} failed: ${result?.error ?? JSON.stringify(result)}`);
      }
      report.push(`+RAN ${migration.id} (${result.message})`);
    }
  } finally {
    utils.closeSQLite();
    for (const k of ENV_KEYS) {
      if (savedEnv[k] === undefined) delete process.env[k];
      else process.env[k] = savedEnv[k] as string;
    }
    rmSync(scratch, { recursive: true, force: true });
  }

  // Put the header's journal mode back and sweep WAL residue — in BOTH modes.
  // `reportOnly` writes no migration, but `shouldRun()` still opens v4's
  // migration connection, which sets `journal_mode = WAL` in the header: a dry
  // run over a committed fixture left it byte-changed until P4.D226 measured it
  // (41 of 41 copies differed after a `--report-only` pass).
  {
    const db = new Database(opts.dbPath);
    try {
      db.pragma(`key = "x'${keyHex(opts.pepperBase64)}'"`);
      const now = String(db.pragma('journal_mode', { simple: true }));
      if (now !== originalMode) db.pragma(`journal_mode = ${originalMode}`);
    } finally {
      db.close();
    }
    for (const suffix of ['-wal', '-shm']) {
      if (existsSync(opts.dbPath + suffix)) rmSync(opts.dbPath + suffix);
    }
    // An EMPTY rollback journal is residue, not state (a non-empty one would
    // be a hot journal and is left for SQLite to recover).
    const journal = opts.dbPath + '-journal';
    if (existsSync(journal) && statSync(journal).size === 0) rmSync(journal);
  }
  return report;
}

/**
 * v4 `49059fb14` (#74): the Concierge refusal ledger's two `chats` columns.
 * Absent at any pin before it — builders pass `allowMissing: true` so they stay
 * runnable at the baseline (where v4 has no such columns either).
 */
export const ADD_CHAT_REFUSAL_LEDGER: V4MigrationRef = {
  file: 'migrations/scripts/add-chat-refusal-ledger.ts',
  exportName: 'addChatRefusalLedgerMigration',
};
