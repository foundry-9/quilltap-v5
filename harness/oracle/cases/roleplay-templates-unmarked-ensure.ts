/**
 * Tier-2 oracle case — the roleplay-template unmarked-text boot ensure
 * (P4.D341, `db::roleplay_templates_unmarked_repair`) against v4's REAL
 * migration `add-roleplay-template-unmarked-fields-v1` (v4 `778b97b2b` — the
 * TWENTY-FIRST D23 move), through v4's REAL `MigrationRunner` narrowed to it.
 *
 * Five shapes, each base built HERE at the pin from v4's own generateDDL
 * (`ensureCollection('roleplay_templates', RoleplayTemplateSchema)`; each
 * removed line asserted present exactly once), written to
 * `QT_FIXTURE_OUT_DIR/rt-unmarked-<mode>.db`, each with the two pre-round
 * built-ins as a pre-round instance stores them (`Standard` with a BARE `*`
 * narration, `Quilltap RP` with the pair as JSON text) and one user template:
 *   a  BASELINE — both lines removed → v4 appends `unmarkedStyle` (`TEXT`) then
 *      `markdownAsterisks` (`INTEGER DEFAULT 1`);
 *   b  NO TABLE → `not needed`;
 *   c  ALREADY MIGRATED — (a) after one v4 run (ledger dropped) → `not needed`;
 *   d  GENERATEDDL-CURRENT → `not needed`, untouched;
 *   e  HALF — only `"markdownAsterisks" INTEGER DEFAULT 1,` removed → v4
 *      appends it alone.
 * Comparands (taken right after the migration): the report, `PRAGMA
 * table_info`, `sqlite_master.sql`, every row's `quote()`d `unmarkedStyle` /
 * `markdownAsterisks` + storage class, the `migrations_state` ids. Then on (a)
 * and (d) v4's REAL seeder (`RoleplayTemplatesRepository.seedBuiltInTemplates()`
 * — the first-boot rewrite under the ensure) runs once and `seeded` reports
 * every BUILT-IN row by name: `narrationDelimiters` / `unmarkedStyle` /
 * `markdownAsterisks` `quote()`d + storage class (Novel is inserted with a
 * minted id + timestamps, so rows are keyed by name, never id). The Rust side
 * runs v5's `seed_built_in_templates` on its copy and compares the same.
 *
 * Run from the pinned v4 worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   rm -rf /tmp/qt-rt-unmarked-ensure /tmp/oracle-rt-unmarked-ensure.ndjson
 *   QT_FIXTURE_OUT_DIR=/tmp/qt-rt-unmarked-ensure \
 *     $N/npx tsx $V5W/harness/oracle/cases/roleplay-templates-unmarked-ensure.ts \
 *     > /tmp/oracle-rt-unmarked-ensure.ndjson
 */

import { copyFileSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { KEYSTONE_236_MIGRATIONS, runV4Migrations } from '../lib/v4-migrations';
import {
  PEPPER,
  cells,
  insertRow,
  ledger,
  openDb,
  strip,
  sweep,
  tableInfo,
  tableSql,
} from '../lib/keystone-ensure-kit';

const STYLE_LINE = `  "unmarkedStyle" TEXT,\n`;
const ASTERISKS_LINE = `  "markdownAsterisks" INTEGER DEFAULT 1,\n`;
const MIGRATION = KEYSTONE_236_MIGRATIONS.filter((m) => m.exportName === 'addRoleplayTemplateUnmarkedFieldsMigration');
export const SEEDED_SQL = `SELECT name, quote("narrationDelimiters") AS narrationDelimiters,
    typeof("narrationDelimiters") AS narrationDelimitersType,
    quote("unmarkedStyle") AS unmarkedStyle, typeof("unmarkedStyle") AS unmarkedStyleType,
    quote("markdownAsterisks") AS markdownAsterisks, typeof("markdownAsterisks") AS markdownAsterisksType
  FROM roleplay_templates WHERE "isBuiltIn" = 1 ORDER BY name`;

type Mode = 'a' | 'b' | 'c' | 'd' | 'e';

async function main(): Promise<void> {
  const outDir = process.env.QT_FIXTURE_OUT_DIR;
  if (!outDir) throw new Error('QT_FIXTURE_OUT_DIR must name the dir for the base files');
  if (MIGRATION.length !== 1) throw new Error('KEYSTONE_236_MIGRATIONS lacks the unmarked ref');
  mkdirSync(outDir, { recursive: true });
  process.env.LOG_LEVEL = 'error';

  const scratch = mkdtempSync(join(tmpdir(), 'qt-rt-unmarked-ddl-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.SQLITE_PATH = join(scratch, 'ddl.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { RoleplayTemplateSchema } = await import('@/lib/schemas/template.types');
  await initializeDatabase();
  await ensureCollection('roleplay_templates', RoleplayTemplateSchema);
  const current = (
    (await rawQuery(`SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'roleplay_templates'`)) as Array<{
      sql: string;
    }>
  )[0].sql;
  await closeDatabase();
  const baseline = strip(current, [STYLE_LINE, ASTERISKS_LINE]);
  const half = strip(current, [ASTERISKS_LINE]);

  const runOnCopy = async (base: string) => {
    const workDir = mkdtempSync(join(tmpdir(), 'qt-rt-unmarked-'));
    mkdirSync(join(workDir, 'data'), { recursive: true });
    const work = join(workDir, 'work.db');
    copyFileSync(base, work);
    const report = await runV4Migrations({ dbPath: work, pepperBase64: PEPPER, migrations: MIGRATION, runner: true });
    return { workDir, work, report };
  };

  const t0 = '2026-01-01T00:00:00.000Z';
  for (const mode of ['a', 'b', 'c', 'd', 'e'] as Mode[]) {
    const out = join(outDir, `rt-unmarked-${mode}.db`);
    sweep(out);
    const base = openDb(out);
    base.pragma('journal_mode = TRUNCATE');
    if (mode === 'b') {
      base.exec('CREATE TABLE "unrelated" ("id" TEXT PRIMARY KEY NOT NULL)');
    } else {
      base.exec(mode === 'a' || mode === 'c' ? baseline : mode === 'e' ? half : current);
      const row = (id: string, name: string, builtIn: number, narration: string) =>
        insertRow(base, 'roleplay_templates', {
          id,
          name,
          systemPrompt: `prompt-${id}`,
          isBuiltIn: builtIn,
          narrationDelimiters: narration,
          createdAt: t0,
          updatedAt: t0,
        });
      row('00000000-0000-4000-8000-0000000000b1', 'Standard', 1, '*');
      row('00000000-0000-4000-8000-0000000000b2', 'Quilltap RP', 1, '["[","]"]');
      row('00000000-0000-4000-8000-0000000000b3', 'House Style', 0, '*');
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
    const db = openDb(work, true);
    const row: Record<string, unknown> = {
      case: 'roleplay-templates-unmarked-ensure',
      mode,
      report,
      tableInfo: tableInfo(db, 'roleplay_templates'),
      sql: tableSql(db, 'roleplay_templates'),
      rows: cells(db, 'roleplay_templates', ['unmarkedStyle', 'markdownAsterisks']),
      ledger: ledger(db),
      seeded: null,
    };
    db.close();
    if (mode === 'a' || mode === 'd') {
      process.env.SQLITE_PATH = work;
      process.env.QUILLTAP_DATA_DIR = workDir;
      await initializeDatabase();
      const { RoleplayTemplatesRepository } = await import(
        '@/lib/database/repositories/roleplay-templates.repository'
      );
      await new RoleplayTemplatesRepository().seedBuiltInTemplates();
      await closeDatabase();
      const after = openDb(work, true);
      row.seeded = after.prepare(SEEDED_SQL).all();
      after.close();
    }
    rmSync(workDir, { recursive: true, force: true });
    process.stdout.write(JSON.stringify(row) + '\n');
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`roleplay-templates-unmarked-ensure oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
