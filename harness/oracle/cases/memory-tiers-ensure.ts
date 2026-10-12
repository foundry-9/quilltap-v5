/**
 * Oracle case — v4's REAL `add-memory-tiers-v1` migration (P4.D265, v4
 * `d58548051`) against SIX starting shapes of `memories` / `chats`, so v5's
 * boot ensure (`db::memory_tiers_repair::ensure_memory_tiers`) is diffed against
 * what v4 itself does to an existing instance in every arm of its `shouldRun`
 * — plus the two HELPER-PARITY arms of C1 §13 (v4's
 * `fixtures/apply-memory-tiers-migration.ts` over an ENCRYPTED and a PLAIN
 * file vs v5's `test_support::ensure_memory_tiers_columns`).
 *
 * The baseline tables are derived from v4's OWN generateDDL at the pin (the
 * `chat-informs-permanent-ensure.ts` idiom): `ensureCollection` creates
 * `memories` / `chats` in a scratch database, the CREATEs are read back from
 * `sqlite_master`, and exactly the five moved lines are removed — the
 * `f5e953a3f` statements byte for byte (the D23 re-dump #7 moved those lines
 * and nothing else in either table).
 *
 *   (a) BASELINE — both tables pre-round, seeded rows → v4 RUNS: four columns
 *       APPENDED in the migration's text, the index, the chat column;
 *   (b) NO TABLES — an unrelated table only → `not needed`, nothing created;
 *   (c) ALREADY MIGRATED — (a)'s file after one v4 run, plus v4's ledger row →
 *       `not needed`, nothing moves;
 *   (d) GENERATEDDL-CURRENT, a NULL `tier` and a NULL `consolidatedFrom`
 *       planted, NO index → RUNS: the index + the heal (`rows` 2);
 *   (e) GENERATEDDL-CURRENT, NO index → RUNS: the index only;
 *   (f) memories migrated (+ index), `chats` pre-round → RUNS: the chat column.
 *   (parity-encrypted / parity-plain) — (a)'s shape, encrypted with the test
 *       pepper / unencrypted, through `applyMemoryTiersMigration` (the narrowed
 *       REAL `MigrationRunner`, ledger on) — and the encrypted copy must still
 *       open under the pepper afterwards (`opensAfter`).
 *
 * Each base file is written under `QT_FIXTURE_MEMORY_TIERS_ENSURE_DIR` as
 * `<mode>.db`; v4 migrates a COPY. The Rust side runs v5's ensure on its own
 * copy of the same file. Comparands (EFFECTS only — v4's migration lines are
 * NO-PORT, R-J): `PRAGMA table_info` of both tables (order included), the
 * `sqlite_master.sql` of both tables and the index, every memory's `(id, tier,
 * consolidatedFrom)` and every chat's `(id, watermark)`; v4's report /
 * `itemsAffected` is recorded against v5's outcome counts.
 *
 * Run from the v4 checkout (the round's pin) under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   rm -rf /tmp/qt-memory-tiers-ensure /tmp/oracle-memory-tiers-ensure.ndjson
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MEMORY_TIERS_ENSURE_DIR=/tmp/qt-memory-tiers-ensure \
 *     $N/npx tsx $V5W/harness/oracle/cases/memory-tiers-ensure.ts \
 *     > /tmp/oracle-memory-tiers-ensure.ndjson
 */

import { copyFileSync, existsSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { runV4Migrations } from '../lib/v4-migrations';
import { applyMemoryTiersMigration } from '../fixtures/apply-memory-tiers-migration';

const PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
const MIGRATION = {
  file: 'migrations/scripts/add-memory-tiers-v1.ts',
  exportName: 'addMemoryTiersMigration',
};
const MEMORY_LINES = [
  `  "tier" TEXT DEFAULT 'hot',\n`,
  `  "supersededById" TEXT,\n`,
  `  "consolidatedFrom" TEXT DEFAULT '[]',\n`,
  `  "consolidatedAt" TEXT,\n`,
];
const CHAT_LINE = `  "otherExtractionWatermarkMessageId" TEXT,\n`;
const INDEX_SQL = `CREATE INDEX "idx_memories_character_tier" ON "memories" ("characterId", "tier")`;
const LEDGER_DDL = `CREATE TABLE IF NOT EXISTS "migrations_state" (
        "id" TEXT PRIMARY KEY,
        "completedAt" TEXT NOT NULL,
        "quilltapVersion" TEXT NOT NULL,
        "itemsAffected" INTEGER NOT NULL DEFAULT 0,
        "message" TEXT
      );`;

type Mode = 'a' | 'b' | 'c' | 'd' | 'e' | 'f' | 'parity-encrypted' | 'parity-plain';

function strip(ddl: string, lines: string[]): string {
  let out = ddl;
  for (const line of lines) {
    if (out.split(line).length !== 2) throw new Error(`expected exactly one ${JSON.stringify(line)} in:\n${ddl}`);
    out = out.replace(line, '');
  }
  return out;
}

async function main(): Promise<void> {
  const outDir = process.env.QT_FIXTURE_MEMORY_TIERS_ENSURE_DIR;
  if (!outDir) throw new Error('QT_FIXTURE_MEMORY_TIERS_ENSURE_DIR must name the dir for the base files');
  mkdirSync(outDir, { recursive: true });
  process.env.LOG_LEVEL = 'error';
  const keyPragma = `key = "x'${Buffer.from(PEPPER, 'base64').toString('hex')}'"`;

  // 1. v4's own generateDDL for both tables, from a scratch database.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-memory-tiers-ensure-ddl-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.SQLITE_PATH = join(scratch, 'ddl.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { MemorySchema, ChatMetadataSchema } = await import('@/lib/schemas/types');
  await initializeDatabase();
  await ensureCollection('memories', MemorySchema);
  await ensureCollection('chats', ChatMetadataSchema);
  const ddlOf = async (name: string) =>
    ((await rawQuery(`SELECT sql FROM sqlite_master WHERE name = '${name}'`)) as Array<{ sql: string }>)[0].sql;
  const memoriesNow = await ddlOf('memories');
  const chatsNow = await ddlOf('chats');
  await closeDatabase();
  const memoriesBase = strip(memoriesNow, MEMORY_LINES);
  const chatsBase = strip(chatsNow, [CHAT_LINE]);

  const Database = createRequire(process.cwd() + '/')('better-sqlite3');
  const fresh = (path: string) => {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      if (existsSync(path + suffix)) rmSync(path + suffix);
    }
  };
  const seedMemories = (db: any, nullPlants: boolean) => {
    const insert = db.prepare(
      `INSERT INTO memories (id, characterId, content, summary, createdAt, updatedAt)
       VALUES (?, 'c1', 'x', 'y', '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')`,
    );
    insert.run('m1');
    insert.run('m2');
    insert.run('m3');
    if (nullPlants) {
      db.prepare(`UPDATE memories SET tier = NULL WHERE id = 'm1'`).run();
      db.prepare(`UPDATE memories SET consolidatedFrom = NULL WHERE id = 'm2'`).run();
    }
  };
  const seedChats = (db: any) => {
    db.prepare(
      `INSERT INTO chats (id, userId, title, createdAt, updatedAt)
       VALUES ('chat1', 'u1', 't', '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')`,
    ).run();
  };
  const copy = (base: string) => {
    const dir = mkdtempSync(join(tmpdir(), 'qt-memory-tiers-ensure-'));
    const work = join(dir, 'work.db');
    copyFileSync(base, work);
    return { dir, work };
  };
  const inspect = (path: string, encrypted: boolean) => {
    const db = new Database(path, { readonly: true });
    if (encrypted) db.pragma(keyPragma);
    const has = (n: string) =>
      !!db.prepare(`SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?`).get(n);
    const sqlOf = (n: string) =>
      (db.prepare(`SELECT sql FROM sqlite_master WHERE name = ?`).get(n) as { sql: string } | undefined)?.sql ??
      null;
    const out = {
      memoriesInfo: has('memories') ? db.prepare('PRAGMA table_info("memories")').all() : [],
      chatsInfo: has('chats') ? db.prepare('PRAGMA table_info("chats")').all() : [],
      memoriesSql: sqlOf('memories'),
      chatsSql: sqlOf('chats'),
      indexSql: sqlOf('idx_memories_character_tier'),
      memoryRows: has('memories')
        ? db
            .prepare(
              `SELECT id, ${
                (db.prepare('PRAGMA table_info("memories")').all() as Array<{ name: string }>).some(
                  (c) => c.name === 'tier',
                )
                  ? 'tier, consolidatedFrom'
                  : "NULL AS tier, NULL AS consolidatedFrom"
              } FROM memories ORDER BY id`,
            )
            .all()
        : [],
      chatRows: has('chats')
        ? db
            .prepare(
              `SELECT id, ${
                (db.prepare('PRAGMA table_info("chats")').all() as Array<{ name: string }>).some(
                  (c) => c.name === 'otherExtractionWatermarkMessageId',
                )
                  ? 'otherExtractionWatermarkMessageId AS watermark'
                  : 'NULL AS watermark'
              } FROM chats ORDER BY id`,
            )
            .all()
        : [],
    };
    db.close();
    return out;
  };

  const modes: Mode[] = ['a', 'b', 'c', 'd', 'e', 'f', 'parity-encrypted', 'parity-plain'];
  for (const mode of modes) {
    const out = join(outDir, `${mode}.db`);
    fresh(out);
    const encrypted = mode !== 'parity-plain';
    const base = new Database(out);
    if (encrypted) base.pragma(keyPragma);
    base.pragma('journal_mode = TRUNCATE');
    if (mode === 'a' || mode === 'c' || mode.startsWith('parity')) {
      base.exec(memoriesBase);
      base.exec(chatsBase);
      seedMemories(base, false);
      seedChats(base);
    } else if (mode === 'b') {
      base.exec('CREATE TABLE "unrelated" ("id" TEXT PRIMARY KEY NOT NULL)');
    } else if (mode === 'd' || mode === 'e') {
      base.exec(memoriesNow);
      base.exec(chatsNow);
      seedMemories(base, mode === 'd');
      seedChats(base);
    } else {
      // (f) memories current + its index; chats pre-round.
      base.exec(memoriesNow);
      base.exec(INDEX_SQL);
      base.exec(chatsBase);
      seedMemories(base, false);
      seedChats(base);
    }
    base.close();
    if (mode === 'c') {
      const first = copy(out);
      const report = await runV4Migrations({ dbPath: first.work, pepperBase64: PEPPER, migrations: [MIGRATION] });
      if (!report[0]?.startsWith('+RAN')) throw new Error(`(c) setup: v4's first run did not run: ${report}`);
      fresh(out);
      copyFileSync(first.work, out);
      rmSync(first.dir, { recursive: true, force: true });
      const db = new Database(out);
      db.pragma(keyPragma);
      db.exec(LEDGER_DDL);
      db.prepare(
        `INSERT INTO migrations_state (id, completedAt, quilltapVersion, itemsAffected, message)
         VALUES ('add-memory-tiers-v1', '2026-10-08T22:46:00.000Z', '4.10.0', 6, 'Memory tiers in place (6 changes)')`,
      ).run();
      db.close();
    }

    // 2. v4 on a COPY.
    const { dir, work } = copy(out);
    let report: string[] | string;
    let opensAfter: boolean | null = null;
    if (mode.startsWith('parity')) {
      report = await applyMemoryTiersMigration(work, encrypted ? { pepperBase64: PEPPER } : {});
      if (encrypted) {
        const db = new Database(work, { readonly: true });
        db.pragma(keyPragma);
        opensAfter = !!db.prepare(`SELECT 1 AS ok FROM memories LIMIT 1`).get();
        db.close();
      }
    } else {
      report = await runV4Migrations({ dbPath: work, pepperBase64: PEPPER, migrations: [MIGRATION] });
    }
    const state = inspect(work, encrypted);
    rmSync(dir, { recursive: true, force: true });
    process.stdout.write(
      JSON.stringify({ case: 'memory-tiers-ensure', mode, encrypted, report, opensAfter, ...state }) + '\n',
    );
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-tiers-ensure oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
