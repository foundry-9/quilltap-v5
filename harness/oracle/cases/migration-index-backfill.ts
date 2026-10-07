/**
 * Oracle case — the migration-index-family boot backfill (P4.160), the v4 side
 * of `migration_index_backfill_equivalence.rs`'s arm B, plus the order's two
 * cross-compat measurements over a backfilled v5 instance.
 *
 * ARM B — v4's REAL `add-connection-profile-unique-name-index-v1` migration
 * (`migrations/scripts/add-connection-profile-unique-name-index.ts`), run
 * through `harness/oracle/lib/v4-migrations.ts` against FIVE starting shapes of
 * `connection_profiles`. It is the one migration-family script v4 WOULD run on
 * a pre-P4.153 v5 instance (its `shouldRun` is the index's absence), so v5's
 * backfill ports it rather than inventing a dedupe.
 *
 * Each base file holds the table EXACTLY as a v5-provisioned instance holds it
 * — `fresh_schema.json`'s committed `connection_profiles` statements, read
 * from the v5 tree, never transcribed — plus the mode's planted rows. v4's
 * migration runs on a COPY; the Rust side derives a pre-round instance, plants
 * the SAME statements (emitted here as `plants`) and boots the real Host.
 *
 *   clash        — one user's `Alpha` / ` alpha ` / `ALPHA ` / `\tAlpha`, a
 *                  second user's `Alpha` (the index is per user), a createdAt
 *                  tie broken by id, and `Éclair` / `éclair` (JS's
 *                  `toLowerCase` folds them; SQLite's ASCII `lower` does not);
 *   unicode-trim — names padded with U+FEFF (JS `trim` strips it, Rust's
 *                  `str::trim` does not), U+0085 (the reverse) and U+3000
 *                  (both strip);
 *   no-clash     — clean names, nothing renamed;
 *   padded-only  — ONE name with surrounding spaces: renamed with no clash
 *                  (`uniqueName !== profile.name`, v4's trim quirk);
 *   indexed      — the UNIQUE index already present: `not needed`.
 *
 * Output per mode (one NDJSON line): the runner's report, the plants, the
 * index's `sqlite_master.sql` (`null` when absent), every row's `(id, userId,
 * name, updatedAt)`, and every line v4's migration logger was asked to write
 * (a `MigrationLogger.prototype` spy — `{level, message, meta, keys}`, `keys`
 * keeping v4's field order; the real method still runs, quiet at
 * `LOG_LEVEL=error`).
 *
 * MEASUREMENTS (the order's items 9 and 10) — with `QT_V5_BACKFILLED_DIR` set
 * to the backfilled v5 instance the Rust arm A wrote (`QT_V5_BACKFILLED_OUT`),
 * one more line: v4's REAL `MigrationRunner` over a COPY of it — every
 * migration that RAN (id + message), and every index the run added or changed
 * per partition. Not compared by the Rust side; recorded in the P4.160 survey.
 *
 * Run (Node 24, from the pinned v4 worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT_DIR=/tmp/qt-index-backfill \
 *     $N/npx tsx $V5W/harness/oracle/cases/migration-index-backfill.ts \
 *     > /tmp/oracle-index-backfill.ndjson
 */

import { copyFileSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { runV4Migrations } from '../lib/v4-migrations';

const PEPPER = '3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=';
const INDEX = 'idx_connection_profiles_userId_name';
const MIGRATIONS = [
  {
    file: 'migrations/scripts/add-connection-profile-unique-name-index.ts',
    exportName: 'addConnectionProfileUniqueNameIndexMigration',
  },
];

const U1 = 'ffffffff-ffff-ffff-ffff-ffffffffffff';
const U2 = '22222222-2222-4222-8222-222222222222';

type Row = [id: string, userId: string, name: string, createdAt: string];

const day = (n: number) => `2026-01-${String(n).padStart(2, '0')}T00:00:00.000Z`;

const MODES: Record<string, Row[]> = {
  clash: [
    ['p01', U1, 'Alpha', day(1)],
    ['p02', U1, ' alpha ', day(2)],
    ['p03', U1, 'ALPHA ', day(3)],
    ['p04', U2, 'Alpha', day(4)],
    ['p05', U1, '\tAlpha', day(5)],
    ['p06', U1, 'Éclair', day(6)],
    ['p07', U1, 'éclair', day(7)],
    // A createdAt tie: `id ASC` decides who keeps the name.
    ['p09', U1, 'Tie', day(8)],
    ['p08', U1, 'tie', day(8)],
  ],
  'unicode-trim': [
    ['q01', U1, 'Gamma﻿', day(1)],
    ['q02', U1, 'Delta\u0085', day(2)],
    ['q03', U1, '　Ideo', day(3)],
  ],
  'no-clash': [
    ['r01', U1, 'One', day(1)],
    ['r02', U1, 'Two', day(2)],
    ['r03', U2, 'One', day(3)],
  ],
  'padded-only': [['s01', U1, ' Solo ', day(1)]],
  indexed: [['t01', U1, 'Already', day(1)]],
};

const sqlString = (s: string) => `'${s.replace(/'/g, "''")}'`;

function plantSql(rows: Row[]): string[] {
  return rows.map(
    ([id, userId, name, createdAt]) =>
      'INSERT INTO connection_profiles (id, userId, name, provider, modelName, createdAt, updatedAt) ' +
      `VALUES (${sqlString(id)}, ${sqlString(userId)}, ${sqlString(name)}, 'OPENAI', 'm', ` +
      `${sqlString(createdAt)}, ${sqlString(createdAt)})`,
  );
}

interface LogLine {
  level: string;
  message: string;
  meta: Record<string, unknown> | null;
  keys: string[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const v5Root = join(here, '..', '..', '..');
  const fresh = JSON.parse(
    readFileSync(
      join(v5Root, 'crates/quilltap-core/src/services/provisioning/fresh_schema.json'),
      'utf8',
    ),
  ) as { main: string[] };
  const migrationIndexes = JSON.parse(
    readFileSync(
      join(v5Root, 'crates/quilltap-core/src/services/provisioning/migration_indexes.json'),
      'utf8',
    ),
  ) as { main: string[] };
  const tableDdl = fresh.main.filter(
    (s) => s.startsWith('CREATE TABLE "connection_profiles" (') || / ON "connection_profiles" \(/.test(s),
  );
  if (!tableDdl[0]?.startsWith('CREATE TABLE')) throw new Error('fresh_schema.json: no connection_profiles table');
  const uniqueSql = migrationIndexes.main.find((s) => s.includes(`"${INDEX}"`));
  if (!uniqueSql) throw new Error(`migration_indexes.json: no ${INDEX}`);

  const outDir = process.env.QT_FIXTURE_OUT_DIR;
  if (!outDir) throw new Error('QT_FIXTURE_OUT_DIR must name the directory for the base .db files');
  mkdirSync(outDir, { recursive: true });

  // Quiet: the migration logger writes to STDOUT, where the NDJSON lines go.
  process.env.LOG_LEVEL = 'error';
  const v4Root = process.cwd();
  const loggerMod = await import(pathToFileURL(join(v4Root, 'migrations/lib/logger.ts')).href);
  const captured: LogLine[] = [];
  {
    const proto = Object.getPrototypeOf(loggerMod.logger) as Record<
      string,
      (m: string, meta?: Record<string, unknown>) => void
    >;
    for (const level of ['debug', 'info', 'warn', 'error']) {
      const real = proto[level];
      proto[level] = function (this: unknown, message: string, meta?: Record<string, unknown>) {
        captured.push({ level, message, meta: meta ?? null, keys: Object.keys(meta ?? {}) });
        return real.call(this, message, meta);
      };
    }
  }

  const Database = createRequire(v4Root + '/')('better-sqlite3');
  const keyPragma = `key = "x'${Buffer.from(PEPPER, 'base64').toString('hex')}'"`;
  const clear = (path: string) => {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      if (existsSync(path + suffix)) rmSync(path + suffix);
    }
  };

  for (const [mode, rows] of Object.entries(MODES)) {
    const plants = plantSql(rows);
    if (mode === 'indexed') plants.unshift(uniqueSql);
    const base = join(outDir, `index-backfill-${mode}.db`);
    clear(base);
    {
      const db = new Database(base);
      db.pragma(keyPragma);
      db.pragma('journal_mode = TRUNCATE');
      for (const sql of tableDdl) db.exec(sql);
      for (const sql of plants) db.exec(sql);
      db.close();
    }

    const workDir = mkdtempSync(join(tmpdir(), 'qt-index-backfill-'));
    const work = join(workDir, 'work.db');
    copyFileSync(base, work);
    captured.length = 0;
    const report = await runV4Migrations({ dbPath: work, pepperBase64: PEPPER, migrations: MIGRATIONS });
    // The migration's own lines (its `context`); the harness's database
    // utils log `Creating SQLite data directory` for the scratch lock dir.
    const logs = captured.filter(
      (l) => l.meta?.context === 'migration.add-connection-profile-unique-name-index',
    );

    const db = new Database(work, { readonly: true });
    db.pragma(keyPragma);
    const indexRow = db
      .prepare("SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?")
      .get(INDEX) as { sql: string } | undefined;
    const out = db
      .prepare('SELECT id, userId, name, updatedAt FROM connection_profiles ORDER BY id')
      .all();
    db.close();
    rmSync(workDir, { recursive: true, force: true });

    process.stdout.write(
      JSON.stringify({
        case: 'migration-index-backfill',
        mode,
        report,
        plants: plants.filter((s) => s !== uniqueSql),
        indexPlanted: mode === 'indexed',
        indexSql: indexRow ? indexRow.sql : null,
        rows: out,
        seeded: Object.fromEntries(rows.map(([id, , , createdAt]) => [id, createdAt])),
        logs,
      }) + '\n',
    );
  }

  const backfilled = process.env.QT_V5_BACKFILLED_DIR;
  if (backfilled) await measureRunnerOnBackfilled(backfilled, Database, keyPragma);
  process.exit(0);
}

/**
 * Items 9 + 10: v4's REAL `MigrationRunner` over a COPY of the backfilled v5
 * instance (the `migrations-first.ts` env block pointed at the copy).
 */
async function measureRunnerOnBackfilled(dir: string, Database: any, keyPragma: string): Promise<void> {
  const scratch = mkdtempSync(join(tmpdir(), 'qt-runner-on-backfilled-'));
  const data = join(scratch, 'data');
  cpSync(dir, data, { recursive: true });
  const files = {
    main: join(data, 'quilltap.db'),
    mountIndex: join(data, 'quilltap-mount-index.db'),
    llmLogs: join(data, 'quilltap-llm-logs.db'),
  };
  const indexes = () =>
    Object.fromEntries(
      Object.entries(files).map(([p, f]) => {
        const db = new Database(f, { readonly: true });
        db.pragma(keyPragma);
        const rows = db
          .prepare("SELECT name, sql FROM sqlite_master WHERE type = 'index' AND sql IS NOT NULL")
          .all() as { name: string; sql: string }[];
        db.close();
        return [p, new Map(rows.map((r) => [r.name, r.sql]))];
      }),
    ) as Record<string, Map<string, string>>;
  const before = indexes();

  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.QUILLTAP_DATA_DIR = scratch;
  process.env.SQLITE_PATH = files.main;
  process.env.SQLITE_MOUNT_INDEX_PATH = files.mountIndex;
  process.env.SQLITE_LLM_LOGS_PATH = files.llmLogs;
  delete process.env.SQLITE_WAL_MODE;
  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  await initializeDatabase();
  const { MigrationRunner } = await import('@/migrations');
  const result = await new MigrationRunner().runMigrations();
  await closeDatabase();

  const after = indexes();
  const changes: string[] = [];
  for (const p of Object.keys(files)) {
    for (const [name, sql] of after[p]) {
      if (!before[p].has(name)) changes.push(`${p}: +${name} ${sql}`);
      else if (before[p].get(name) !== sql) changes.push(`${p}: ~${name} ${sql}`);
    }
    for (const name of before[p].keys()) if (!after[p].has(name)) changes.push(`${p}: -${name}`);
  }
  process.stdout.write(
    JSON.stringify({
      case: 'migration-index-backfill',
      mode: 'runner-on-backfilled',
      success: result.success,
      migrationsRun: result.migrationsRun,
      migrationsSkipped: result.migrationsSkipped,
      failed: result.failed ?? [],
      deferred: result.deferred ?? [],
      ran: result.results.map((r: { id: string; success: boolean; message: string }) => ({
        id: r.id,
        success: r.success,
        message: r.message,
      })),
      indexChanges: changes,
    }) + '\n',
  );
  rmSync(scratch, { recursive: true, force: true });
}

main().catch((err) => {
  process.stderr.write(`migration-index-backfill oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
