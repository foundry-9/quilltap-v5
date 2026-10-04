/**
 * Oracle case — v4's REAL `add-chat-informs-permanent-v1` migration (P4.D249,
 * v4 `52d6e7ecd`) run against a BASELINE-shape `chat_informs`, so v5's boot
 * ensure (`db::chat_informs_permanent_repair`) can be diffed against what v4
 * itself does to an existing instance.
 *
 * One pin, the TARGET. The baseline shape is derived from v4's OWN
 * generateDDL rather than transcribed: the case lets v4's real
 * `ensureCollection('chat_informs', ChatInformSchema)` create the table in a
 * scratch database, reads the CREATE back from `sqlite_master`, and removes
 * exactly the `"permanent"` line — which is the pre-`52d6e7ecd` statement byte
 * for byte (the D23 re-dump #4 moved that one line and nothing else). It
 * builds the baseline file at `QT_FIXTURE_OUT` (encrypted with the
 * chat-informs spec's test pepper, three seed rows), then runs v4's migration
 * on a COPY. The Rust side runs v5's ensure on its own copy of the same file.
 *
 * Output (one NDJSON line): the migration report, `PRAGMA table_info`, the
 * table's `sqlite_master.sql`, and every row's `(id, permanent)`.
 *
 * Run from the v4 checkout (TARGET-pinned) under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-inform-ensure-base.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chat-informs-permanent-ensure.ts \
 *     > /tmp/oracle-inform-ensure.ndjson
 */

import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { runV4Migrations } from '../lib/v4-migrations';

const PERMANENT_LINE = '  "permanent" INTEGER DEFAULT 0,\n';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'chat-informs-tier2.json'), 'utf8')
  ) as { testPepperBase64: string };
  const keyPragma = `key = "x'${Buffer.from(spec.testPepperBase64, 'base64').toString('hex')}'"`;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) throw new Error('QT_FIXTURE_OUT must name the baseline .db to write');
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    if (existsSync(out + suffix)) rmSync(out + suffix);
  }

  // v4's migration logger writes to STDOUT; keep the NDJSON line alone there.
  process.env.LOG_LEVEL = 'error';

  // 1. v4's own generateDDL for the table, from a scratch database.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-inform-ensure-ddl-'));
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
  const current = ddlRows[0].sql;
  if (current.split(PERMANENT_LINE).length !== 2) {
    throw new Error(`expected exactly one permanent line in v4's DDL:\n${current}`);
  }
  const baselineDdl = current.replace(PERMANENT_LINE, '');

  // 2. The baseline-shape file, three rows (one consumed).
  const Database = createRequire(process.cwd() + '/')('better-sqlite3');
  const base = new Database(out);
  base.pragma(keyPragma);
  base.pragma('journal_mode = TRUNCATE');
  base.exec(baselineDdl);
  const insert = base.prepare(
    `INSERT INTO chat_informs (id, chatId, batchId, participantId, contentMarkdown,
       recordMessageId, createdAt, updatedAt, consumedAt, consumedByMessageId)
     VALUES (?, 'c1', 'b1', ?, 'A one-shot from before standing informs.', NULL,
       '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z', ?, ?)`
  );
  insert.run('i1', 'p1', null, null);
  insert.run('i2', 'p2', '2026-01-01T00:05:00.000Z', 'm1');
  insert.run('i3', 'p3', null, null);
  base.close();

  // 3. v4's migration on a COPY.
  const work = join(mkdtempSync(join(tmpdir(), 'qt-inform-ensure-')), 'work.db');
  copyFileSync(out, work);
  const report = await runV4Migrations({
    dbPath: work,
    pepperBase64: spec.testPepperBase64,
    migrations: [
      {
        file: 'migrations/scripts/add-chat-informs-permanent.ts',
        exportName: 'addChatInformsPermanentMigration',
      },
    ],
  });

  const db = new Database(work, { readonly: true });
  db.pragma(keyPragma);
  const tableInfo = db.prepare('PRAGMA table_info("chat_informs")').all();
  const sql = (
    db.prepare("SELECT sql FROM sqlite_master WHERE name = 'chat_informs'").get() as {
      sql: string;
    }
  ).sql;
  const rows = db.prepare('SELECT id, permanent FROM chat_informs ORDER BY id').all();
  db.close();

  process.stdout.write(
    JSON.stringify({ case: 'chat-informs-permanent-ensure', report, tableInfo, sql, rows }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-informs-permanent-ensure oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
