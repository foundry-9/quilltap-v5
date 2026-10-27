/**
 * Tier-2 oracle case — the terminal-geometry boot ensure (P4.D337,
 * `db::terminal_sessions_geometry_repair`) against v4's REAL migration
 * `add-terminal-session-geometry-v1` (bug 255, v4 `a6292f169` — the FIFTEENTH
 * D23 move), run through v4's REAL `MigrationRunner` NARROWED to it
 * (`runV4Migrations({ runner: true })` — the ledger's silence is part of the
 * comparand: v4's runner STAMPS the id when it runs, v5 never does —
 * `UNSTAMPED_COLUMN_ENSURES`).
 *
 * Five shapes, each base file built HERE at the pin from v4's own generateDDL
 * (the CREATE `ensureCollection('terminal_sessions', TerminalSessionSchema)`
 * makes, read back from `sqlite_master`; each removed line asserted present
 * exactly once first), written to `QT_FIXTURE_OUT_DIR/geom-<mode>.db` with two
 * rows (one live, one exited with `exitCode 137`):
 *   a  BASELINE — both `"cols" REAL,` / `"rows" REAL,` lines removed → v4 RUNS
 *      and appends both (`INTEGER DEFAULT NULL`, `cols` then `rows`);
 *   b  NO TABLE → `not needed`; neither side creates it;
 *   c  ALREADY MIGRATED — (a)'s file after one v4 run → `not needed`;
 *   d  GENERATEDDL-CURRENT — the pin's CREATE as is (REAL, schema order) →
 *      `not needed`, untouched;
 *   e  HALF — only `"rows" REAL,` removed → v4 runs and appends `rows` alone
 *      (`addColumnIfMissing` skips `cols`).
 * Comparands per mode: the migration report, `PRAGMA table_info` (order
 * included), `sqlite_master.sql` (`null` when absent), every row's `(id, cols,
 * rows)` as `quote()`d cells (NULL — no backfill), the `migrations_state` ids,
 * and on (a) and (d) the STORAGE CLASS after a write: v4's REAL
 * `TerminalSessionsRepository.create` writes one more row with `cols: 80, rows:
 * 24`, and the case reports `typeof("cols")` / `typeof("rows")` and the dumped
 * values — `integer` on the migration's appended column, `real` on the fresh
 * REAL one.
 *
 * Run from the pinned v4 worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   rm -rf /tmp/qt-ts-geom-ensure
 *   QT_FIXTURE_OUT_DIR=/tmp/qt-ts-geom-ensure \
 *     $N/npx tsx $V5W/harness/oracle/cases/terminal-sessions-geometry-ensure.ts \
 *     > /tmp/oracle-ts-geom-ensure.ndjson
 */

import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { TERMINAL_GEOMETRY_MIGRATIONS, runV4Migrations } from '../lib/v4-migrations';

const COLS_LINE = `  "cols" REAL,\n`;
const ROWS_LINE = `  "rows" REAL,\n`;

type Mode = 'a' | 'b' | 'c' | 'd' | 'e';

/** The row the storage-class check writes through each side's real create. */
export const WRITE_ROW = {
  id: '00000000-0000-4000-8000-0000000000c3',
  chatId: '00000000-0000-4000-8000-0000000000cc',
  label: null,
  shell: '/bin/zsh',
  cwd: '/tmp',
  startedAt: '2026-01-02T00:00:00.000Z',
  exitedAt: null,
  exitCode: null,
  transcriptPath: null,
  cols: 80,
  rows: 24,
  createdAt: '2026-01-02T00:00:00.000Z',
  updatedAt: '2026-01-02T00:00:00.000Z',
};

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'terminal-sessions-tier2.json'), 'utf8'),
  ) as { testPepperBase64: string };
  const keyPragma = `key = "x'${Buffer.from(spec.testPepperBase64, 'base64').toString('hex')}'"`;
  const outDir = process.env.QT_FIXTURE_OUT_DIR;
  if (!outDir) throw new Error('QT_FIXTURE_OUT_DIR must name the dir for the base files');
  mkdirSync(outDir, { recursive: true });
  process.env.LOG_LEVEL = 'error';

  // 1. v4's own generateDDL for the table, from a scratch database.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-ts-geom-ddl-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'ddl.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { TerminalSessionSchema } = await import('@/lib/schemas/terminal.types');
  await initializeDatabase();
  await ensureCollection('terminal_sessions', TerminalSessionSchema);
  const current = (
    (await rawQuery(
      "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'terminal_sessions'",
    )) as Array<{ sql: string }>
  )[0].sql;
  await closeDatabase();
  for (const line of [COLS_LINE, ROWS_LINE]) {
    if (current.split(line).length !== 2) {
      throw new Error(`expected exactly one ${JSON.stringify(line)} in v4's DDL:\n${current}`);
    }
  }
  const baseline = current.replace(COLS_LINE, '').replace(ROWS_LINE, '');
  const half = current.replace(ROWS_LINE, '');

  const Database = createRequire(process.cwd() + '/')('better-sqlite3');
  const fresh = (path: string) => {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      if (existsSync(path + suffix)) rmSync(path + suffix);
    }
  };
  const seed = (db: { prepare: (s: string) => { run: (...a: unknown[]) => void } }) => {
    const insert = db.prepare(
      `INSERT INTO terminal_sessions (id, chatId, label, shell, cwd, startedAt, exitedAt,
         exitCode, transcriptPath, createdAt, updatedAt)
       VALUES (?, '00000000-0000-4000-8000-0000000000cc', NULL, '/bin/bash', '/home', '2026-01-01T00:00:00.000Z', ?, ?,
         NULL, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')`,
    );
    insert.run('00000000-0000-4000-8000-0000000000c1', null, null);
    insert.run('00000000-0000-4000-8000-0000000000c2', '2026-01-01T01:00:00.000Z', 137);
  };
  const runOnCopy = async (base: string) => {
    const workDir = mkdtempSync(join(tmpdir(), 'qt-ts-geom-'));
    mkdirSync(join(workDir, 'data'), { recursive: true });
    const work = join(workDir, 'work.db');
    copyFileSync(base, work);
    const report = await runV4Migrations({
      dbPath: work,
      pepperBase64: spec.testPepperBase64,
      migrations: TERMINAL_GEOMETRY_MIGRATIONS,
      runner: true,
    });
    return { workDir, work, report };
  };

  for (const mode of ['a', 'b', 'c', 'd', 'e'] as Mode[]) {
    const out = join(outDir, `geom-${mode}.db`);
    fresh(out);
    const base = new Database(out);
    base.pragma(keyPragma);
    base.pragma('journal_mode = TRUNCATE');
    if (mode === 'b') {
      base.exec('CREATE TABLE "unrelated" ("id" TEXT PRIMARY KEY NOT NULL)');
    } else {
      base.exec(mode === 'd' ? current : mode === 'e' ? half : baseline);
      seed(base);
    }
    base.close();
    if (mode === 'c') {
      const first = await runOnCopy(out);
      if (!first.report[0]?.startsWith('+RAN')) {
        throw new Error(`(c) setup: v4's first run did not run: ${first.report}`);
      }
      // (c)'s base is the migrated file WITHOUT the ledger row the first run
      // wrote, so the second run is asked by the column gate, not the ledger.
      const migrated = new Database(first.work);
      migrated.pragma(keyPragma);
      migrated.exec('DROP TABLE IF EXISTS migrations_state');
      migrated.close();
      fresh(out);
      copyFileSync(first.work, out);
      rmSync(first.workDir, { recursive: true, force: true });
    }

    const { workDir, work, report } = await runOnCopy(out);

    // The storage class after a write, through v4's REAL repository create.
    if (mode === 'a' || mode === 'd') {
      process.env.SQLITE_PATH = work;
      process.env.QUILLTAP_DATA_DIR = workDir;
      await initializeDatabase();
      const { TerminalSessionsRepository } = await import(
        '@/lib/database/repositories/terminal-sessions.repository'
      );
      const { id, createdAt, updatedAt, ...data } = WRITE_ROW;
      await new TerminalSessionsRepository().create(data as never, { id, createdAt, updatedAt });
      await closeDatabase();
    }

    const db = new Database(work, { readonly: true });
    db.pragma(keyPragma);
    const sqlRow = db
      .prepare("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'terminal_sessions'")
      .get() as { sql: string } | undefined;
    const tableInfo = sqlRow ? db.prepare('PRAGMA table_info("terminal_sessions")').all() : [];
    const names = new Set((tableInfo as Array<{ name: string }>).map((c) => c.name));
    const cell = (c: string) => (names.has(c) ? `quote("${c}")` : `'absent'`);
    const rows = sqlRow
      ? db
          .prepare(
            `SELECT id, ${cell('cols')} AS cols, ${cell('rows')} AS rows,
               ${names.has('cols') ? 'typeof("cols")' : "'absent'"} AS colsType,
               ${names.has('rows') ? 'typeof("rows")' : "'absent'"} AS rowsType
             FROM terminal_sessions ORDER BY id`,
          )
          .all()
      : [];
    const ledger = db
      .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'migrations_state'")
      .get()
      ? (db.prepare('SELECT id FROM migrations_state ORDER BY id').all() as Array<{ id: string }>).map(
          (r) => r.id,
        )
      : [];
    db.close();
    rmSync(workDir, { recursive: true, force: true });

    process.stdout.write(
      JSON.stringify({
        case: 'terminal-sessions-geometry-ensure',
        mode,
        report,
        tableInfo,
        sql: sqlRow ? sqlRow.sql : null,
        rows,
        ledger,
      }) + '\n',
    );
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`terminal-sessions-geometry-ensure oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
