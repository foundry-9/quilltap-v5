/**
 * Tier-2 oracle case — the `connection_profiles.maxImagesPerRequest` boot
 * ensure (P4.D362, `db::connection_profiles_max_images_repair`) against v4's
 * REAL migration `add-profile-max-images-per-request-v1` (v4 `d439f6dc9` — the
 * TWENTY-THIRD D23 move), through v4's REAL `MigrationRunner` NARROWED to it
 * (`runV4Migrations({ runner: true })`; v4 stamps, v5 never does).
 *
 * Five shapes, each base built HERE at the pin from v4's own generateDDL (the
 * connections repository's first access; the `"maxImagesPerRequest" INTEGER
 * DEFAULT 4,` line asserted present exactly once), written to
 * `QT_FIXTURE_OUT_DIR/max-images-<mode>.db` with three profile rows:
 *   a  BASELINE — the line removed → v4 RUNS: appended + its backfill
 *      (`backfilled` measured in the report);
 *   b  NO TABLE → `not needed`;
 *   c  MIGRATED — (a) after one v4 run (ledger dropped) → `not needed`;
 *   d  GENERATEDDL-CURRENT → `not needed`, untouched;
 *   e  PLANTED NULL — (c) with `p2`'s cell set NULL → `not needed` on BOTH
 *      (the column gate; the NULL stays NULL). Then v4's REAL
 *      `connections.findById('p2')` / `findAll()` on a copy measure how a
 *      NULL cell reads (R-L7): the value it hydrates (or the row dropped).
 * Comparands: the report, `PRAGMA table_info`, `sqlite_master.sql`, every
 * row's `quote()` / `typeof` cap, the `migrations_state` ids, and (e)'s read.
 *
 * Run from the pinned v4 worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   rm -rf /tmp/qt-max-images-ensure /tmp/oracle-max-images-ensure.ndjson
 *   QT_FIXTURE_OUT_DIR=/tmp/qt-max-images-ensure \
 *     $N/npx tsx $V5W/harness/oracle/cases/connection-profiles-max-images-ensure.ts \
 *     > /tmp/oracle-max-images-ensure.ndjson
 */

import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { PIN_624_MIGRATIONS, runV4Migrations } from '../lib/v4-migrations';
import { PEPPER, cells, insertRow, ledger, openDb, strip, sweep, tableInfo, tableSql } from '../lib/keystone-ensure-kit';

const LINE = `  "maxImagesPerRequest" INTEGER DEFAULT 4,\n`;
const MIGRATION = PIN_624_MIGRATIONS.filter((m) => m.exportName === 'addProfileMaxImagesPerRequestMigration');
export const USER_ID = '6f1c3a52-0000-4000-8000-0000000000aa';
export const PROFILE_IDS = [
  '6f1c3a52-0000-4000-8000-0000000000b1',
  '6f1c3a52-0000-4000-8000-0000000000b2',
  '6f1c3a52-0000-4000-8000-0000000000b3',
];

type Mode = 'a' | 'b' | 'c' | 'd' | 'e';

async function main(): Promise<void> {
  const outDir = process.env.QT_FIXTURE_OUT_DIR;
  if (!outDir) throw new Error('QT_FIXTURE_OUT_DIR must name the dir for the base files');
  if (MIGRATION.length !== 1) throw new Error('PIN_624_MIGRATIONS lacks the max-images ref');
  mkdirSync(outDir, { recursive: true });
  process.env.LOG_LEVEL = 'error';

  const scratch = mkdtempSync(join(tmpdir(), 'qt-max-images-ddl-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.SQLITE_PATH = join(scratch, 'ddl.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  const manager = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  await manager.initializeDatabase();
  await getRepositories().connections.count();
  const current = ((await manager.rawQuery(
    `SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'connection_profiles'`,
  )) as Array<{ sql: string }>)[0].sql;
  await manager.closeDatabase();
  const baseline = strip(current, [LINE]);

  const seed = (db: ReturnType<typeof openDb>) => {
    PROFILE_IDS.forEach((id, i) => {
      insertRow(db, 'connection_profiles', {
        id,
        userId: USER_ID,
        name: `p${i + 1}`,
        provider: 'OPENAI',
        modelName: 'gpt-5',
        parameters: '{}',
        tags: '[]',
        createdAt: '2026-01-01T00:00:00.000Z',
        updatedAt: '2026-01-01T00:00:00.000Z',
      });
    });
  };
  const runOnCopy = async (base: string) => {
    const workDir = mkdtempSync(join(tmpdir(), 'qt-max-images-ensure-'));
    const work = join(workDir, 'work.db');
    copyFileSync(base, work);
    const report = await runV4Migrations({ dbPath: work, pepperBase64: PEPPER, migrations: MIGRATION, runner: true });
    return { workDir, work, report };
  };

  /** v4's REAL reads over a copy (R-L7). */
  const readLeg = async (work: string) => {
    const legDir = mkdtempSync(join(tmpdir(), 'qt-max-images-leg-'));
    mkdirSync(join(legDir, 'data'), { recursive: true });
    const leg = join(legDir, 'leg.db');
    copyFileSync(work, leg);
    // A fresh child process: v4's manager singleton goes stale across a
    // close + re-init (every row then fails Zod).
    const child = spawnSync(process.execPath, [...process.execArgv, process.argv[1]], {
      env: { ...process.env, QT_MI_LEG: leg, QT_MI_LEG_DIR: legDir },
      encoding: 'utf8',
    });
    if (child.status !== 0) throw new Error(`read leg failed: ${child.stderr}`);
    const { byId, all } = JSON.parse(child.stdout.trim().split('\n').pop() as string) as {
      byId: Record<string, unknown> | null;
      all: Array<Record<string, unknown>>;
    };
    rmSync(legDir, { recursive: true, force: true });
    const cap = (p: Record<string, unknown> | null | undefined) =>
      p == null ? '<row absent>' : p.maxImagesPerRequest === undefined ? '<key absent>' : p.maxImagesPerRequest;
    return {
      findById: cap(byId),
      findAll: PROFILE_IDS.map((id) => cap(all.find((p) => p.id === id))),
    };
  };

  for (const mode of ['a', 'b', 'c', 'd', 'e'] as Mode[]) {
    const out = join(outDir, `max-images-${mode}.db`);
    sweep(out);
    const base = openDb(out);
    base.pragma('journal_mode = TRUNCATE');
    if (mode === 'b') {
      base.exec('CREATE TABLE "unrelated" ("id" TEXT PRIMARY KEY NOT NULL)');
    } else {
      base.exec(mode === 'd' ? current : baseline);
      seed(base);
    }
    base.close();
    if (mode === 'c' || mode === 'e') {
      const first = await runOnCopy(out);
      if (!first.report[0]?.startsWith('+RAN')) throw new Error(`(${mode}) setup did not run: ${first.report}`);
      const migrated = openDb(first.work);
      migrated.exec('DROP TABLE IF EXISTS migrations_state; DROP TABLE IF EXISTS migrations_metadata');
      if (mode === 'e') {
        migrated.prepare(`UPDATE connection_profiles SET "maxImagesPerRequest" = NULL WHERE id = ?`).run(PROFILE_IDS[1]);
      }
      migrated.close();
      sweep(out);
      copyFileSync(first.work, out);
      rmSync(first.workDir, { recursive: true, force: true });
    }

    const { workDir, work, report } = await runOnCopy(out);
    const db = openDb(work);
    const row = {
      case: 'connection-profiles-max-images-ensure',
      mode,
      report,
      tableInfo: tableInfo(db, 'connection_profiles'),
      sql: tableSql(db, 'connection_profiles'),
      rows: cells(db, 'connection_profiles', ['maxImagesPerRequest']),
      ledger: ledger(db),
    };
    db.close();
    const read = mode === 'e' || mode === 'c' ? await readLeg(work) : null;
    rmSync(workDir, { recursive: true, force: true });
    process.stdout.write(JSON.stringify({ ...row, read }) + '\n');
  }
  process.exit(0);
}

/** The read leg's child: v4's REAL `findById` + `findAll`. */
async function legChild(leg: string, legDir: string): Promise<void> {
  process.env.LOG_LEVEL = 'error';
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.SQLITE_PATH = leg;
  process.env.QUILLTAP_DATA_DIR = legDir;
  delete process.env.SQLITE_WAL_MODE;
  const manager = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  await manager.initializeDatabase();
  const repos = getRepositories();
  const byId = await repos.connections.findById(PROFILE_IDS[1]);
  const all = await repos.connections.findAll();
  await manager.closeDatabase();
  process.stdout.write(JSON.stringify({ byId, all }) + '\n');
  process.exit(0);
}

if (process.env.QT_MI_LEG) {
  legChild(process.env.QT_MI_LEG, process.env.QT_MI_LEG_DIR as string).catch((err) => {
    process.stderr.write(`read leg child failed: ${err?.stack ?? err}\n`);
    process.exit(1);
  });
} else
main().catch((err) => {
  process.stderr.write(`connection-profiles-max-images-ensure oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
