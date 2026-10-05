/**
 * Oracle case — v4's REAL `impersonation-voice-mode-v1` migration (P4.D251,
 * v4 `07b8f0209`) run against THREE starting shapes of `chat_settings`, so
 * v5's boot ensure (`db::chat_settings_impersonation_voice_mode_repair`) can
 * be diffed against what v4 itself does to an existing instance.
 *
 * One pin, the TARGET. The base shape is derived from v4's OWN generateDDL
 * rather than transcribed (the P4.D249 `PERMANENT_LINE` idiom): the case lets
 * v4's real `ensureCollection('chat_settings', ChatSettingsSchema)` create the
 * table in a scratch database, reads the CREATE back from `sqlite_master`, and
 * removes exactly the `"impersonationVoiceMode"` line — the pre-`686954937`
 * statement byte for byte, because that commit's D23 re-dump put the OLD
 * column at the SAME position and `07b8f0209`'s replaced it in place. The
 * retired column is then added back the way a real instance got it: through
 * the add-field migration's exact ALTER (`INTEGER DEFAULT 0`, appended).
 *
 *   (A) the old column present, the new absent — a v4 4.10-dev instance that
 *       has not yet booted at `07b8f0209` (live Friday's shape, §R.13); rows
 *       1 / 0 / NULL / 1, so the backfill count is 4 and the `'ask'` count 2.
 *   (B) NEITHER column — an instance older than the toggle; v4 runs BOTH
 *       migrations in registration order (`dependsOn`), v5 runs its ONE
 *       ensure. The final shape must agree: the mode appended alone.
 *   (C) BOTH columns present, the old one LAST — the §R.13 ping-pong shape
 *       (v4 migrated, then a v5 still carrying the P4.D179 ensure re-added
 *       the old column). v4's re-runnable `shouldRun` fires; only rows whose
 *       mode `IS NULL OR = 'off'` are backfilled; a row already `'always'` or
 *       `'ask'` is left alone; the old column is dropped.
 *
 * Every mode lists BOTH migrations in v4's registered order; the add-field one
 * answers `not needed` wherever the old column already exists, which the Rust
 * side asserts from the report. Each base file is written under
 * `QT_FIXTURE_OUT_DIR` as `voice-mode-<a|b|c>.db` (encrypted with the
 * chat-settings spec's test pepper); v4's migrations run on a COPY. The Rust
 * side runs v5's ensure on its own copy of the same file.
 *
 * Output (three NDJSON lines, one per mode): the migration report, `PRAGMA
 * table_info`, the table's `sqlite_master.sql`, and every row's
 * `(id, impersonationVoiceMode)`.
 *
 * Run from the v4 checkout (TARGET-pinned) under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT_DIR=/tmp/qt-voice-mode-ensure \
 *     $N/npx tsx $V5W/harness/oracle/cases/chat-settings-voice-mode-ensure.ts \
 *     > /tmp/oracle-voice-mode-ensure.ndjson
 */

import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
} from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { runV4Migrations } from '../lib/v4-migrations';

const MODE_LINE = `  "impersonationVoiceMode" TEXT DEFAULT 'off',\n`;
/** v4 `add-impersonation-voice-rewrite-field.ts:64`, verbatim. */
const ADD_OLD = `ALTER TABLE "chat_settings" ADD COLUMN "impersonationVoiceRewrite" INTEGER DEFAULT 0`;
/** v4 `impersonation-voice-mode.ts:72` through `addColumnIfMissing`, verbatim. */
const ADD_MODE = `ALTER TABLE "chat_settings" ADD COLUMN "impersonationVoiceMode" TEXT DEFAULT 'off'`;

const MIGRATIONS = [
  {
    file: 'migrations/scripts/add-impersonation-voice-rewrite-field.ts',
    exportName: 'addImpersonationVoiceRewriteFieldMigration',
  },
  {
    file: 'migrations/scripts/impersonation-voice-mode.ts',
    exportName: 'impersonationVoiceModeMigration',
  },
];

type Mode = 'a' | 'b' | 'c';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'chat-settings-tier2.json'), 'utf8')
  ) as { testPepperBase64: string };
  const keyPragma = `key = "x'${Buffer.from(spec.testPepperBase64, 'base64').toString('hex')}'"`;

  const outDir = process.env.QT_FIXTURE_OUT_DIR;
  if (!outDir) throw new Error('QT_FIXTURE_OUT_DIR must name the directory for the three base .db files');
  mkdirSync(outDir, { recursive: true });

  // v4's migration logger writes to STDOUT; keep the NDJSON lines alone there.
  process.env.LOG_LEVEL = 'error';

  // 1. v4's own generateDDL for the table, from a scratch database.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-voice-mode-ensure-ddl-'));
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
  const ddlRows = (await rawQuery(
    "SELECT sql FROM sqlite_master WHERE name = 'chat_settings'"
  )) as Array<{ sql: string }>;
  await closeDatabase();
  const current = ddlRows[0].sql;
  if (current.split(MODE_LINE).length !== 2) {
    throw new Error(`expected exactly one impersonationVoiceMode line in v4's DDL:\n${current}`);
  }
  if (current.includes('impersonationVoiceRewrite')) {
    throw new Error(`v4's generateDDL still names the retired column:\n${current}`);
  }
  const baseDdl = current.replace(MODE_LINE, '');

  const Database = createRequire(process.cwd() + '/')('better-sqlite3');
  const ts = '2026-01-01T00:00:00.000Z';

  for (const mode of ['a', 'b', 'c'] as Mode[]) {
    const out = join(outDir, `voice-mode-${mode}.db`);
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
    // 2. The base-shape file for this mode.
    const base = new Database(out);
    base.pragma(keyPragma);
    base.pragma('journal_mode = TRUNCATE');
    base.exec(baseDdl);
    const row = (id: string, cols: string, vals: string, params: unknown[]) =>
      base
        .prepare(
          `INSERT INTO chat_settings (id, userId, createdAt, updatedAt${cols}) VALUES (?, 'u1', ?, ?${vals})`
        )
        .run(id, ts, ts, ...params);
    if (mode === 'a') {
      base.exec(ADD_OLD);
      row('s1', ', impersonationVoiceRewrite', ', ?', [1]);
      row('s2', ', impersonationVoiceRewrite', ', ?', [0]);
      row('s3', ', impersonationVoiceRewrite', ', ?', [null]);
      row('s4', ', impersonationVoiceRewrite', ', ?', [1]);
    } else if (mode === 'b') {
      row('s1', '', '', []);
      row('s2', '', '', []);
    } else {
      // The ping-pong shape: v4 added the mode (appended), then the P4.D179
      // ensure re-added the old column (appended after it).
      base.exec(ADD_MODE);
      base.exec(ADD_OLD);
      row('s1', ', impersonationVoiceRewrite, impersonationVoiceMode', ', ?, ?', [1, 'always']);
      row('s2', ', impersonationVoiceRewrite, impersonationVoiceMode', ', ?, ?', [1, 'off']);
      row('s3', ', impersonationVoiceRewrite, impersonationVoiceMode', ', ?, ?', [0, null]);
      row('s4', ', impersonationVoiceRewrite, impersonationVoiceMode', ', ?, ?', [1, 'ask']);
    }
    base.close();

    // 3. v4's migrations on a COPY.
    const work = join(mkdtempSync(join(tmpdir(), `qt-voice-mode-ensure-${mode}-`)), 'work.db');
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
      db.prepare("SELECT sql FROM sqlite_master WHERE name = 'chat_settings'").get() as {
        sql: string;
      }
    ).sql;
    const rows = db
      .prepare('SELECT id, impersonationVoiceMode FROM chat_settings ORDER BY id')
      .all();
    db.close();

    process.stdout.write(
      JSON.stringify({ case: 'chat-settings-voice-mode-ensure', mode, report, tableInfo, sql, rows }) + '\n'
    );
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-settings-voice-mode-ensure oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
