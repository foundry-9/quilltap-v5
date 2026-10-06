/**
 * Oracle case — v4's REAL `add-chat-informs-permanent-v1` migration (P4.D249,
 * v4 `52d6e7ecd`) run against FOUR starting shapes of `chat_informs` (P4.151
 * B3 — the multi-mode template P4.D251 built for the voice-mode ensure), so
 * v5's boot ensure (`db::chat_informs_permanent_repair`) can be diffed against
 * what v4 itself does to an existing instance in every arm of its `shouldRun`.
 *
 * One pin, the TARGET. The baseline shape is derived from v4's OWN
 * generateDDL rather than transcribed: the case lets v4's real
 * `ensureCollection('chat_informs', ChatInformSchema)` create the table in a
 * scratch database, reads the CREATE back from `sqlite_master`, and removes
 * exactly the `"permanent"` line — which is the pre-`52d6e7ecd` statement byte
 * for byte (the D23 re-dump #4 moved that one line and nothing else).
 *
 *   (a) BASELINE — the pre-`52d6e7ecd` table, three seed rows (one consumed);
 *       v4 RUNS the migration (the ALTER appends `INTEGER NOT NULL DEFAULT 0`).
 *   (b) NO TABLE — the file has no `chat_informs` at all (an instance older
 *       than informs); v4's `shouldRun` table gate answers `not needed` and
 *       neither side creates the table.
 *   (c) ALREADY MIGRATED — (a)'s file after one v4 run, migrated again; v4's
 *       column gate answers `not needed` and nothing moves.
 *   (d) GENERATEDDL-CURRENT — the table as a fresh v4 creates it (the
 *       schema-order nullable `"permanent" INTEGER DEFAULT 0` line kept), with
 *       ONE row's `permanent` set to NULL after the seed (P4.157 R-D — the
 *       nullable column's one shape no insert produces); v4 answers `not
 *       needed`, the table is left alone, and the NULL is what it reads back.
 *
 * Each base file is written under `QT_FIXTURE_OUT_DIR` as
 * `inform-ensure-<a|b|c|d>.db` (encrypted with the chat-informs spec's test
 * pepper); v4's migration runs on a COPY. The Rust side runs v5's ensure on its
 * own copy of the same file.
 *
 * Output (four NDJSON lines, one per mode): the migration report, `PRAGMA
 * table_info`, the table's `sqlite_master.sql` (`null` when absent), and every
 * row's `(id, permanent)`.
 *
 * Run from the v4 checkout (TARGET-pinned) under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT_DIR=/tmp/qt-inform-ensure \
 *     $N/npx tsx $V5W/harness/oracle/cases/chat-informs-permanent-ensure.ts \
 *     > /tmp/oracle-inform-ensure.ndjson
 */

import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { runV4Migrations } from '../lib/v4-migrations';

const PERMANENT_LINE = '  "permanent" INTEGER DEFAULT 0,\n';

const MIGRATIONS = [
  {
    file: 'migrations/scripts/add-chat-informs-permanent.ts',
    exportName: 'addChatInformsPermanentMigration',
  },
];

type Mode = 'a' | 'b' | 'c' | 'd';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'chat-informs-tier2.json'), 'utf8')
  ) as { testPepperBase64: string };
  const keyPragma = `key = "x'${Buffer.from(spec.testPepperBase64, 'base64').toString('hex')}'"`;

  const outDir = process.env.QT_FIXTURE_OUT_DIR;
  if (!outDir) throw new Error('QT_FIXTURE_OUT_DIR must name the directory for the four base .db files');
  mkdirSync(outDir, { recursive: true });

  // v4's migration logger writes to STDOUT; keep the NDJSON lines alone there.
  process.env.LOG_LEVEL = 'error';

  // 1. v4's own generateDDL for the table, from a scratch database.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-inform-ensure-ddl-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'ddl.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { ChatInformSchema } = await import('@/lib/schemas/chat-inform.types');
  await initializeDatabase();
  await ensureCollection('chat_informs', ChatInformSchema);
  const ddlRows = (await rawQuery(
    "SELECT sql FROM sqlite_master WHERE name = 'chat_informs'"
  )) as Array<{ sql: string }>;
  await closeDatabase();
  const currentDdl = ddlRows[0].sql;
  if (currentDdl.split(PERMANENT_LINE).length !== 2) {
    throw new Error(`expected exactly one permanent line in v4's DDL:\n${currentDdl}`);
  }
  const baselineDdl = currentDdl.replace(PERMANENT_LINE, '');

  const Database = createRequire(process.cwd() + '/')('better-sqlite3');
  const fresh = (path: string) => {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      if (existsSync(path + suffix)) rmSync(path + suffix);
    }
  };
  const seed = (db: { prepare: (s: string) => { run: (...a: unknown[]) => void } }) => {
    const insert = db.prepare(
      `INSERT INTO chat_informs (id, chatId, batchId, participantId, contentMarkdown,
         recordMessageId, createdAt, updatedAt, consumedAt, consumedByMessageId)
       VALUES (?, 'c1', 'b1', ?, 'A one-shot from before standing informs.', NULL,
         '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z', ?, ?)`
    );
    insert.run('i1', 'p1', null, null);
    insert.run('i2', 'p2', '2026-01-01T00:05:00.000Z', 'm1');
    insert.run('i3', 'p3', null, null);
  };
  const runOnCopy = async (base: string) => {
    const workDir = mkdtempSync(join(tmpdir(), 'qt-inform-ensure-'));
    const work = join(workDir, 'work.db');
    copyFileSync(base, work);
    const report = await runV4Migrations({
      dbPath: work,
      pepperBase64: spec.testPepperBase64,
      migrations: MIGRATIONS,
    });
    return { workDir, work, report };
  };

  for (const mode of ['a', 'b', 'c', 'd'] as Mode[]) {
    const out = join(outDir, `inform-ensure-${mode}.db`);
    fresh(out);
    // 2. The base-shape file for this mode.
    const base = new Database(out);
    base.pragma(keyPragma);
    base.pragma('journal_mode = TRUNCATE');
    if (mode === 'a' || mode === 'c') {
      base.exec(baselineDdl);
      seed(base);
    } else if (mode === 'b') {
      // No chat_informs: one unrelated table so the file is a real database.
      base.exec('CREATE TABLE "unrelated" ("id" TEXT PRIMARY KEY NOT NULL)');
    } else {
      base.exec(currentDdl);
      seed(base);
      // P4.157 R-D: the schema-order column is NULLABLE (generateDDL's
      // `"permanent" INTEGER DEFAULT 0`, no NOT NULL), so a NULL cell is a
      // legal shape here and the one the ensure's backfill question turns on.
      // The seed alone never writes one (the DEFAULT fills every insert).
      base.prepare("UPDATE chat_informs SET permanent = NULL WHERE id = 'i3'").run();
    }
    base.close();
    if (mode === 'c') {
      // (c) is (a)'s file after one v4 run — the migrated copy becomes the base.
      const first = await runOnCopy(out);
      if (!first.report[0]?.startsWith('+RAN')) {
        throw new Error(`(c) setup: v4's first run did not run: ${first.report}`);
      }
      fresh(out);
      copyFileSync(first.work, out);
      rmSync(first.workDir, { recursive: true, force: true });
    }

    // 3. v4's migration on a COPY.
    const { workDir, work, report } = await runOnCopy(out);
    const db = new Database(work, { readonly: true });
    db.pragma(keyPragma);
    const tableInfo = db.prepare('PRAGMA table_info("chat_informs")').all();
    const sqlRow = db.prepare("SELECT sql FROM sqlite_master WHERE name = 'chat_informs'").get() as
      | { sql: string }
      | undefined;
    const sql = sqlRow ? sqlRow.sql : null;
    const rows = sqlRow ? db.prepare('SELECT id, permanent FROM chat_informs ORDER BY id').all() : [];
    db.close();
    rmSync(workDir, { recursive: true, force: true });

    process.stdout.write(
      JSON.stringify({ case: 'chat-informs-permanent-ensure', mode, report, tableInfo, sql, rows }) +
        '\n'
    );
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-informs-permanent-ensure oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
