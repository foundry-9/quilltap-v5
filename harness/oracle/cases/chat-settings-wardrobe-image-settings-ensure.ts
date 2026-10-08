/**
 * Tier-2 oracle case — the `chat_settings.wardrobeImageSettings` boot ensure
 * (P4.D255 Tier 2 item 20) against v4's REAL `add-wardrobe-image-settings-
 * field-v1` (`7c8572869`), through `harness/oracle/lib/v4-migrations.ts`.
 *
 * Three shapes, each base file built HERE at the pin from v4's own generateDDL
 * and written to `QT_FIXTURE_OUT_DIR/wardrobe-image-<mode>.db`:
 *   (A) the pre-round table — the pin's generateDDL with the
 *       `"wardrobeImageSettings" TEXT,` line REMOVED mechanically (the
 *       `PERMANENT_LINE` idiom) — with two rows → the migration RUNS and
 *       appends `TEXT DEFAULT '{"imageProfileId":null}'`; both rows read the
 *       one-key default;
 *   (B) the pin's generateDDL (schema-order nullable TEXT, no default) → `not
 *       needed`;
 *   (C) (A) already migrated (the migration's statement applied) → `not needed`.
 * Comparands: `PRAGMA table_info` (order included), `sqlite_master.sql`, and
 * each row's raw cell. v4's report is asserted per mode (one RUN, two not
 * needed), so no mode can go vacuous.
 *
 * Run from the pinned v4 worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   rm -rf /tmp/qt-wardrobe-image-ensure
 *   QT_FIXTURE_OUT_DIR=/tmp/qt-wardrobe-image-ensure \
 *     $N/npx tsx $V5W/harness/oracle/cases/chat-settings-wardrobe-image-settings-ensure.ts \
 *     > /tmp/oracle-wardrobe-image-ensure.ndjson
 */

import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { runV4Migrations } from '../lib/v4-migrations';

const COLUMN_LINE = `  "wardrobeImageSettings" TEXT,\n`;
/** v4 `add-wardrobe-image-settings-field-v1.ts:50-55` through `addColumnIfMissing`. */
const ADD = `ALTER TABLE "chat_settings" ADD COLUMN "wardrobeImageSettings" TEXT DEFAULT '{"imageProfileId":null}'`;
const MIGRATIONS = [
  {
    file: 'migrations/scripts/add-wardrobe-image-settings-field-v1.ts',
    exportName: 'addWardrobeImageSettingsFieldMigration',
  },
];

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'chat-settings-tier2.json'), 'utf8'),
  ) as { testPepperBase64: string };
  const keyPragma = `key = "x'${Buffer.from(spec.testPepperBase64, 'base64').toString('hex')}'"`;
  const outDir = process.env.QT_FIXTURE_OUT_DIR;
  if (!outDir) throw new Error('QT_FIXTURE_OUT_DIR must name the dir for the base files');
  mkdirSync(outDir, { recursive: true });
  process.env.LOG_LEVEL = 'error';

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wardrobe-image-ensure-ddl-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'ddl.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { ChatSettingsSchema } = await import('@/lib/schemas/settings.types');
  await initializeDatabase();
  await ensureCollection('chat_settings', ChatSettingsSchema);
  const current = (
    (await rawQuery("SELECT sql FROM sqlite_master WHERE name = 'chat_settings'")) as Array<{ sql: string }>
  )[0].sql;
  await closeDatabase();
  if (current.split(COLUMN_LINE).length !== 2) {
    throw new Error(`expected exactly one wardrobeImageSettings line in v4's DDL:\n${current}`);
  }
  const preRound = current.replace(COLUMN_LINE, '');

  const Database = createRequire(process.cwd() + '/')('better-sqlite3');
  const ts = '2026-01-01T00:00:00.000Z';
  for (const mode of ['A', 'B', 'C'] as const) {
    const out = join(outDir, `wardrobe-image-${mode}.db`);
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
    const base = new Database(out);
    base.pragma(keyPragma);
    base.pragma('journal_mode = TRUNCATE');
    base.exec(mode === 'B' ? current : preRound);
    if (mode === 'C') base.exec(ADD);
    for (const id of ['s1', 's2']) {
      base
        .prepare('INSERT INTO chat_settings (id, userId, createdAt, updatedAt) VALUES (?, ?, ?, ?)')
        .run(id, 'u1', ts, ts);
    }
    base.close();

    const workDir = mkdtempSync(join(tmpdir(), `qt-wardrobe-image-ensure-${mode}-`));
    const work = join(workDir, 'work.db');
    copyFileSync(out, work);
    const report = await runV4Migrations({
      dbPath: work,
      pepperBase64: spec.testPepperBase64,
      migrations: MIGRATIONS,
    });
    const db = new Database(work, { readonly: true });
    db.pragma(keyPragma);
    const tableInfo = db.prepare('PRAGMA table_info("chat_settings")').all();
    const sql = (
      db.prepare("SELECT sql FROM sqlite_master WHERE name = 'chat_settings'").get() as { sql: string }
    ).sql;
    const rows = db
      .prepare('SELECT id, "wardrobeImageSettings" AS cell FROM chat_settings ORDER BY id')
      .all();
    db.close();
    rmSync(workDir, { recursive: true, force: true });
    process.stdout.write(
      JSON.stringify({ case: 'chat-settings-wardrobe-image-ensure', mode, report, tableInfo, sql, rows }) +
        '\n',
    );
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-settings-wardrobe-image-ensure oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
