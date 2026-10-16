/**
 * Tier-2 oracle case — the two 4.11 `chat_settings` boot ensures (P4.D304,
 * `db::chat_settings_4_11_repair`) against v4's REAL migrations
 * `add-custom-tool-triggers-field-v1` (`362426c8f`) and
 * `add-composer-slash-commands-field-v1` (`4877fc2da`), through
 * `harness/oracle/lib/v4-migrations.ts` (the direct `shouldRun()` / `run()`
 * arm — no ledger handshake is under test: neither side stamps, R-B).
 *
 * Seven shapes, each base file built HERE at the pin from v4's own generateDDL
 * (the CREATE `ensureCollection` makes, read back from `sqlite_master`) and
 * written to `QT_FIXTURE_OUT_DIR/cs411-<mode>.db`, each with two rows (one at
 * every default, one with `customTools = 0`):
 *   T-A  the `"customToolTriggers" INTEGER DEFAULT 0,` line REMOVED → the
 *        triggers migration RUNS and appends the column;
 *   T-B  the pin's CREATE as is → `not needed`;
 *   T-C  T-A with the migration's ADD applied by hand → `not needed`;
 *   S-A / S-B / S-C  the same three for `"composerSlashCommands" INTEGER
 *        DEFAULT 1,` and the slash migration;
 *   D    BOTH lines removed → v4 runs BOTH in registration order (triggers,
 *        then slash), so `sqlite_master.sql` ends with the two clauses in that
 *        order.
 * Each line is asserted present exactly once before its removal. Comparands:
 * `PRAGMA table_info` (order included), `sqlite_master.sql`, and each row's
 * two raw cells (the DEFAULT back-fill — `0` / `1`, never NULL). v4's report is
 * emitted per mode; the Rust side asserts `+RAN` only on the A modes and D.
 *
 * Run from the pinned v4 worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   rm -rf /tmp/qt-cs411-ensure
 *   QT_FIXTURE_OUT_DIR=/tmp/qt-cs411-ensure \
 *     $N/npx tsx $V5W/harness/oracle/cases/chat-settings-4-11-columns-ensure.ts \
 *     > /tmp/oracle-cs411-ensure.ndjson
 */

import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { runV4Migrations } from '../lib/v4-migrations';

const TRIGGERS_LINE = `  "customToolTriggers" INTEGER DEFAULT 0,\n`;
const SLASH_LINE = `  "composerSlashCommands" INTEGER DEFAULT 1,\n`;
/** v4 `add-custom-tool-triggers-field.ts:52` through `addColumnIfMissing`. */
const TRIGGERS_ADD = `ALTER TABLE "chat_settings" ADD COLUMN "customToolTriggers" INTEGER DEFAULT 0`;
/** v4 `add-composer-slash-commands-field.ts:47` through `addColumnIfMissing`. */
const SLASH_ADD = `ALTER TABLE "chat_settings" ADD COLUMN "composerSlashCommands" INTEGER DEFAULT 1`;
// NOTE: no `-v1` in either file name (unlike the ids).
const TRIGGERS = {
  file: 'migrations/scripts/add-custom-tool-triggers-field.ts',
  exportName: 'addCustomToolTriggersFieldMigration',
};
const SLASH = {
  file: 'migrations/scripts/add-composer-slash-commands-field.ts',
  exportName: 'addComposerSlashCommandsFieldMigration',
};

type Mode = 'T-A' | 'T-B' | 'T-C' | 'S-A' | 'S-B' | 'S-C' | 'D';

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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cs411-ensure-ddl-'));
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
  for (const line of [TRIGGERS_LINE, SLASH_LINE]) {
    if (current.split(line).length !== 2) {
      throw new Error(`expected exactly one ${JSON.stringify(line)} in v4's DDL:\n${current}`);
    }
  }
  const withoutTriggers = current.replace(TRIGGERS_LINE, '');
  const withoutSlash = current.replace(SLASH_LINE, '');
  const withoutBoth = withoutTriggers.replace(SLASH_LINE, '');

  const plan: Record<Mode, { ddl: string; add?: string; migrations: Array<typeof TRIGGERS> }> = {
    'T-A': { ddl: withoutTriggers, migrations: [TRIGGERS] },
    'T-B': { ddl: current, migrations: [TRIGGERS] },
    'T-C': { ddl: withoutTriggers, add: TRIGGERS_ADD, migrations: [TRIGGERS] },
    'S-A': { ddl: withoutSlash, migrations: [SLASH] },
    'S-B': { ddl: current, migrations: [SLASH] },
    'S-C': { ddl: withoutSlash, add: SLASH_ADD, migrations: [SLASH] },
    D: { ddl: withoutBoth, migrations: [TRIGGERS, SLASH] },
  };

  const Database = createRequire(process.cwd() + '/')('better-sqlite3');
  const ts = '2026-01-01T00:00:00.000Z';
  for (const mode of Object.keys(plan) as Mode[]) {
    const { ddl, add, migrations } = plan[mode];
    const out = join(outDir, `cs411-${mode}.db`);
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
    const base = new Database(out);
    base.pragma(keyPragma);
    base.pragma('journal_mode = TRUNCATE');
    base.exec(ddl);
    if (add) base.exec(add);
    base
      .prepare('INSERT INTO chat_settings (id, userId, createdAt, updatedAt) VALUES (?, ?, ?, ?)')
      .run('s1', 'u1', ts, ts);
    base
      .prepare(
        'INSERT INTO chat_settings (id, userId, customTools, createdAt, updatedAt) VALUES (?, ?, 0, ?, ?)',
      )
      .run('s2', 'u2', ts, ts);
    base.close();

    const workDir = mkdtempSync(join(tmpdir(), `qt-cs411-ensure-${mode}-`));
    const work = join(workDir, 'work.db');
    copyFileSync(out, work);
    const report = await runV4Migrations({
      dbPath: work,
      pepperBase64: spec.testPepperBase64,
      migrations,
    });
    const db = new Database(work, { readonly: true });
    db.pragma(keyPragma);
    const tableInfo = db.prepare('PRAGMA table_info("chat_settings")').all();
    const sql = (
      db.prepare("SELECT sql FROM sqlite_master WHERE name = 'chat_settings'").get() as { sql: string }
    ).sql;
    const rows = db
      .prepare(
        'SELECT id, "customToolTriggers" AS triggers, "composerSlashCommands" AS slash FROM chat_settings ORDER BY id',
      )
      .all();
    db.close();
    rmSync(workDir, { recursive: true, force: true });
    process.stdout.write(
      JSON.stringify({ case: 'chat-settings-4-11-columns-ensure', mode, report, tableInfo, sql, rows }) +
        '\n',
    );
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-settings-4-11-columns-ensure oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
