/**
 * Tier-2 oracle case — the keystone boot ensure (P4.D341,
 * `db::memory_keystone_repair`) against v4's REAL migration
 * `add-memory-keystone-v1` (v4 `8cb584eff` — the SEVENTEENTH D23 move), run
 * through v4's REAL `MigrationRunner` NARROWED to it (`runV4Migrations({
 * runner: true })` — the ledger is a comparand: v4 STAMPS the id when it runs,
 * v5 never does, `UNSTAMPED_COLUMN_ENSURES`).
 *
 * Five shapes, each base file built HERE at the pin from v4's own generateDDL
 * (`ensureCollection('memories', MemorySchema)` read back from `sqlite_master`;
 * the removed line asserted present exactly once), written to
 * `QT_FIXTURE_OUT_DIR/keystone-<mode>.db` with three memory rows:
 *   a  BASELINE — the `"keystone" INTEGER DEFAULT 0,` line removed, no index →
 *      v4 RUNS: appends `INTEGER NOT NULL DEFAULT 0`, creates the index;
 *   b  NO TABLE → `not needed`;
 *   c  ALREADY MIGRATED — (a) after one v4 run (ledger dropped) → `not needed`;
 *   d  GENERATEDDL-CURRENT, the index ABSENT → v4 RUNS and creates the index
 *      ONLY (`!hasIndex()` — the arm R-4 exists for);
 *   e  GENERATEDDL-CURRENT + the index → `not needed`.
 * After the run, wherever the column exists, the case plants `keystone = 1` on
 * `m2` (the Rust side does the same on its copy). Comparands per mode: the
 * report, `PRAGMA table_info` (order + text), `sqlite_master.sql` of the table
 * and of `idx_memories_keystone` (`null` when absent), every row's `quote()`d
 * `keystone` + storage class, the `migrations_state` ids, and on (a) / (d)
 * `EXPLAIN QUERY PLAN` of the frozen archive's keystone read
 * (`SELECT id FROM memories WHERE characterId = ? AND keystone = 1`) — the
 * partial index USED, not only present.
 *
 * Run from the pinned v4 worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   rm -rf /tmp/qt-memory-keystone-ensure /tmp/oracle-memory-keystone-ensure.ndjson
 *   QT_FIXTURE_OUT_DIR=/tmp/qt-memory-keystone-ensure \
 *     $N/npx tsx $V5W/harness/oracle/cases/memory-keystone-ensure.ts \
 *     > /tmp/oracle-memory-keystone-ensure.ndjson
 */

import { copyFileSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { KEYSTONE_236_MIGRATIONS, runV4Migrations } from '../lib/v4-migrations';
import {
  PEPPER,
  cells,
  indexSql,
  insertRow,
  ledger,
  openDb,
  strip,
  sweep,
  tableInfo,
  tableSql,
} from '../lib/keystone-ensure-kit';

const KEYSTONE_LINE = `  "keystone" INTEGER DEFAULT 0,\n`;
const INDEX = 'idx_memories_keystone';
/** v4's statement (`add-memory-keystone-v1.ts:60-62`), for mode (e)'s base. */
const INDEX_SQL = `CREATE INDEX IF NOT EXISTS "idx_memories_keystone" ON "memories" ("characterId", "aboutCharacterId") WHERE "keystone" = 1`;
export const EQP_SQL = `EXPLAIN QUERY PLAN SELECT id FROM memories WHERE characterId = 'k1' AND keystone = 1`;
const MIGRATION = KEYSTONE_236_MIGRATIONS.filter((m) => m.exportName === 'addMemoryKeystoneMigration');

type Mode = 'a' | 'b' | 'c' | 'd' | 'e';

async function main(): Promise<void> {
  const outDir = process.env.QT_FIXTURE_OUT_DIR;
  if (!outDir) throw new Error('QT_FIXTURE_OUT_DIR must name the dir for the base files');
  if (MIGRATION.length !== 1) throw new Error('KEYSTONE_236_MIGRATIONS lacks the keystone ref');
  mkdirSync(outDir, { recursive: true });
  process.env.LOG_LEVEL = 'error';

  const scratch = mkdtempSync(join(tmpdir(), 'qt-keystone-ensure-ddl-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.SQLITE_PATH = join(scratch, 'ddl.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { MemorySchema } = await import('@/lib/schemas/memory.types');
  await initializeDatabase();
  await ensureCollection('memories', MemorySchema);
  const current = (
    (await rawQuery(`SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'memories'`)) as Array<{
      sql: string;
    }>
  )[0].sql;
  // generateDDL's own indexes on the table (createdAt etc.) ride every base.
  const genIndexes = (
    (await rawQuery(
      `SELECT sql FROM sqlite_master WHERE type = 'index' AND tbl_name = 'memories' AND sql IS NOT NULL`,
    )) as Array<{ sql: string }>
  ).map((r) => r.sql);
  await closeDatabase();
  const baseline = strip(current, [KEYSTONE_LINE]);

  const seed = (db: ReturnType<typeof openDb>) => {
    for (const [id, about] of [
      ['m1', null],
      ['m2', 'k2'],
      ['m3', 'k2'],
    ]) {
      insertRow(db, 'memories', {
        id,
        characterId: 'k1',
        aboutCharacterId: about,
        content: `c-${id}`,
        summary: `s-${id}`,
        createdAt: '2026-01-01T00:00:00.000Z',
        updatedAt: '2026-01-01T00:00:00.000Z',
      });
    }
  };
  const runOnCopy = async (base: string) => {
    const workDir = mkdtempSync(join(tmpdir(), 'qt-keystone-ensure-'));
    const work = join(workDir, 'work.db');
    copyFileSync(base, work);
    const report = await runV4Migrations({ dbPath: work, pepperBase64: PEPPER, migrations: MIGRATION, runner: true });
    return { workDir, work, report };
  };

  for (const mode of ['a', 'b', 'c', 'd', 'e'] as Mode[]) {
    const out = join(outDir, `keystone-${mode}.db`);
    sweep(out);
    const base = openDb(out);
    base.pragma('journal_mode = TRUNCATE');
    if (mode === 'b') {
      base.exec('CREATE TABLE "unrelated" ("id" TEXT PRIMARY KEY NOT NULL)');
    } else {
      base.exec(mode === 'a' || mode === 'c' ? baseline : current);
      for (const s of genIndexes) base.exec(s);
      if (mode === 'e') base.exec(INDEX_SQL);
      seed(base);
    }
    base.close();
    if (mode === 'c') {
      const first = await runOnCopy(out);
      if (!first.report[0]?.startsWith('+RAN')) throw new Error(`(c) setup did not run: ${first.report}`);
      const migrated = openDb(first.work);
      migrated.exec('DROP TABLE IF EXISTS migrations_state; DROP TABLE IF EXISTS migrations_metadata');
      migrated.close();
      sweep(out);
      copyFileSync(first.work, out);
      rmSync(first.workDir, { recursive: true, force: true });
    }

    const { workDir, work, report } = await runOnCopy(out);
    const db = openDb(work);
    const hasColumn =
      !!tableSql(db, 'memories') &&
      (db.prepare(`PRAGMA table_info("memories")`).all() as Array<{ name: string }>).some((c) => c.name === 'keystone');
    if (hasColumn) db.prepare(`UPDATE memories SET keystone = 1 WHERE id = 'm2'`).run();
    const eqp =
      mode === 'a' || mode === 'd'
        ? (db.prepare(EQP_SQL).all() as Array<{ detail: string }>).map((r) => r.detail)
        : null;
    const row = {
      case: 'memory-keystone-ensure',
      mode,
      report,
      tableInfo: tableInfo(db, 'memories'),
      sql: tableSql(db, 'memories'),
      indexSql: indexSql(db, INDEX),
      rows: cells(db, 'memories', ['keystone']),
      ledger: ledger(db),
      eqp,
    };
    db.close();
    rmSync(workDir, { recursive: true, force: true });
    process.stdout.write(JSON.stringify(row) + '\n');
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-keystone-ensure oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
