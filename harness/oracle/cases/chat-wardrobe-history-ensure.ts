/**
 * Tier-2 oracle case — the chat wardrobe history's three boot ensures (P4.D315
 * Tier 1 item 6) against v4's REAL migration RUNNER narrowed to
 * `CHAT_WARDROBE_HISTORY_MIGRATIONS` (`add-chat-wardrobe-history-table-v1`,
 * `add-chat-wardrobe-history-tool-name-v1`, `seed-chat-wardrobe-history-v1` —
 * v4's registration order at the `e8e17b2d9` pin).
 *
 * Why the runner, not `runV4Migrations`' direct arm: the ledger gate (checked
 * BEFORE `shouldRun`) and `recordCompletedMigration`'s stamps ARE the
 * behaviour v5's ensures port (R-C); a direct `shouldRun()` / `run()` call
 * writes no ledger row. The runner's (TypeScript-private, runtime-public)
 * `migrations` list is narrowed to the three scripts.
 *
 * Modes (each base file built here, at the pin, and written to
 * `QT_FIXTURE_OUT_DIR/cwh-ensure-<mode>.db` for the Rust side; `chats` is
 * v4's own generateDDL table):
 *   A  no table, dressed chats, no ledger → table RAN, column not needed
 *      (unrecorded), seed RAN;
 *   B  the `83387d0e5` PRE-COLUMN table (10 columns) + both indexes + rows (a
 *      `pascal` row among them) + the TABLE's ledger row → column RAN, seed RAN;
 *   B' B + the SEED's ledger row → column only;
 *   C  A after v4's runner (all its rows and stamps) → nothing;
 *   D  the pin's final table, unseeded, NO ledger, one pair already holding a
 *      row (the per-pair skip) → table / column not needed, seed RAN;
 *   E  A's shape over the seed's edge cells (`{"c": null}`, `{"c": 5}`,
 *      `{"c": {"top": "x"}}`, an `''` key, `'[]'`, `'nope'`, `'null'`, `'{}'`,
 *      an INTEGER cell, index-keyed characters stored last) → as A;
 *   E' a hand-built `chats` WITHOUT `NOT NULL` on `updatedAt` and one dressed
 *      chat whose `updatedAt` is NULL (R-3) → the runner FAILS the seed
 *      (`result.success` false — RECORDED here, never thrown).
 * ("A pair with rows already" sits in D, not E: a base with history rows has
 * the table, so the table migration cannot run there.)
 *
 * Comparands per mode: `sqlite_master` (type, name, sql) for the table and
 * both indexes; `PRAGMA table_info`; every history row minus `id`, ordered
 * `chatId, characterId, entryIndex`; `migrations_state` minus `completedAt` /
 * `quilltapVersion`, by id. The runner's report (run / skipped / ran / success)
 * is recorded so no mode can go vacuous.
 *
 * Run from the pinned v4 worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   rm -rf /tmp/qt-cwh-ensure
 *   QT_FIXTURE_OUT_DIR=/tmp/qt-cwh-ensure \
 *     $N/npx tsx $V5W/harness/oracle/cases/chat-wardrobe-history-ensure.ts \
 *     > /tmp/oracle-cwh-ensure.ndjson
 */

import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { CHAT_WARDROBE_HISTORY_MIGRATIONS } from '../lib/v4-migrations.js';

type Mode = 'A' | 'B' | "B'" | 'C' | 'D' | 'E' | "E'";
const MODES: Mode[] = ['A', 'B', "B'", 'C', 'D', 'E', "E'"];

/** v4's `ensureSQLiteMigrationsTable` text (the ledger plants). */
const LEDGER_DDL = `CREATE TABLE IF NOT EXISTS "migrations_state" (
        "id" TEXT PRIMARY KEY,
        "completedAt" TEXT NOT NULL,
        "quilltapVersion" TEXT NOT NULL,
        "itemsAffected" INTEGER NOT NULL DEFAULT 0,
        "message" TEXT
      );
      CREATE TABLE IF NOT EXISTS "migrations_metadata" (
        "key" TEXT PRIMARY KEY,
        "value" TEXT NOT NULL
      );`;

/** The `83387d0e5` pre-column table (its DDL module `:18-29`), byte-exact. */
const PRE_COLUMN_TABLE = `CREATE TABLE IF NOT EXISTS "chat_wardrobe_history" (
    "id" TEXT PRIMARY KEY,
    "chatId" TEXT NOT NULL REFERENCES "chats"("id") ON DELETE CASCADE,
    "characterId" TEXT NOT NULL,
    "entryIndex" INTEGER NOT NULL,
    "slots" TEXT NOT NULL,
    "slotsHash" TEXT NOT NULL,
    "source" TEXT NOT NULL,
    "actorCharacterId" TEXT,
    "isCurrent" INTEGER NOT NULL DEFAULT 0,
    "createdAt" TEXT NOT NULL
  )`;

const A = '11111111-1111-4111-8111-11111111111a';
const B = '11111111-1111-4111-8111-11111111111b';
const COAT = '22222222-2222-4222-8222-2222222222c0';
const HAT = '22222222-2222-4222-8222-2222222222c2';
const CH = (n: number) => `c0000000-0000-4000-8000-0000000000${String(n).padStart(2, '0')}`;

type ChatRow = [string, string | null, string | number | null];

function corpus(mode: Mode): ChatRow[] {
  if (mode === 'E') {
    return [
      [CH(31), '2026-01-01T00:00:00.000Z', '{"c": null}'],
      [CH(32), '2026-01-02T00:00:00.000Z', '{"c": 5}'],
      [CH(33), '2026-01-03T00:00:00.000Z', '{"c": {"top": "x"}}'],
      [CH(34), '2026-01-04T00:00:00.000Z', JSON.stringify({ '': { top: [HAT] }, [B]: { top: [HAT] } })],
      [CH(35), '2026-01-05T00:00:00.000Z', '[]'],
      [CH(36), '2026-01-06T00:00:00.000Z', 'nope'],
      [CH(37), '2026-01-07T00:00:00.000Z', 'null'],
      [CH(38), '2026-01-08T00:00:00.000Z', '{}'],
      [CH(39), '2026-01-09T00:00:00.000Z', 5],
      // Hand-written: the array-index keys LAST, so only `Object.entries`'
      // index-first order puts them first.
      [CH(40), '2026-01-10T00:00:00.000Z', `{"${B}":{"top":["${COAT}"]},"7":{"top":["${HAT}"]},"0":{"hair":["${COAT}"]}}`],
      [CH(41), '2026-01-11T00:00:00.000Z', null],
    ];
  }
  if (mode === "E'") {
    return [
      [CH(51), '2026-01-01T00:00:00.000Z', JSON.stringify({ [A]: { top: [COAT] } })],
      [CH(52), null, JSON.stringify({ [A]: { top: [HAT] } })],
    ];
  }
  return [
    [CH(1), '2026-01-01T00:00:00.000Z', JSON.stringify({ [A]: { top: [COAT], hair: [HAT] }, [B]: { bottom: [COAT] } })],
    [CH(2), '2026-03-01T00:00:00.000Z', JSON.stringify({ [A]: { top: [COAT], accessories: [COAT] } })],
    [CH(3), '2026-02-01T00:00:00.000Z', null],
  ];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'chat-settings-tier2.json'), 'utf8'),
  ) as { testPepperBase64: string };
  const pepper = spec.testPepperBase64;
  const keyPragma = `key = "x'${Buffer.from(pepper, 'base64').toString('hex')}'"`;
  const outDir = process.env.QT_FIXTURE_OUT_DIR;
  if (!outDir) throw new Error('QT_FIXTURE_OUT_DIR must name the dir for the base files');
  mkdirSync(outDir, { recursive: true });
  process.env.LOG_LEVEL = 'error';

  // v4's own generateDDL for `chats`, from a scratch database.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-cwh-ensure-ddl-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = pepper;
  process.env.SQLITE_PATH = join(scratch, 'ddl.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { ChatMetadataSchema } = await import('@/lib/schemas/types');
  await initializeDatabase();
  await ensureCollection('chats', ChatMetadataSchema);
  const chatsDdl = (
    (await rawQuery("SELECT sql FROM sqlite_master WHERE name = 'chats'")) as Array<{ sql: string }>
  )[0].sql;
  await closeDatabase();
  if (!chatsDdl.includes('"updatedAt" TEXT NOT NULL')) {
    throw new Error(`generateDDL's chats no longer spells "updatedAt" TEXT NOT NULL: ${chatsDdl}`);
  }
  const { CHAT_WARDROBE_HISTORY_DDL } = await import(
    '@/lib/database/backends/sqlite/chat-wardrobe-history-ddl'
  );
  const { hashEquippedSlots } = await import('@/lib/wardrobe/outfit-hash');

  const v4Root = process.cwd();
  const Database = createRequire(v4Root + '/')('better-sqlite3');
  const utils = await import(pathToFileURL(join(v4Root, 'migrations/lib/database-utils.ts')).href);
  const { MigrationRunner } = await import(pathToFileURL(join(v4Root, 'migrations/index.ts')).href);
  const migrations: unknown[] = [];
  for (const ref of CHAT_WARDROBE_HISTORY_MIGRATIONS) {
    migrations.push((await import(pathToFileURL(join(v4Root, ref.file)).href))[ref.exportName]);
  }

  const runRunner = async (work: string, workDir: string) => {
    process.env.SQLITE_PATH = work;
    process.env.QUILLTAP_DATA_DIR = workDir;
    utils.closeSQLite();
    const runner = new MigrationRunner();
    (runner as unknown as { migrations: unknown[] }).migrations = migrations;
    const result = await runner.runMigrations();
    utils.closeSQLite();
    return result;
  };

  const ts = '2026-01-01T00:00:00.000Z';
  const historyRow = (
    id: string,
    chatId: string,
    characterId: string,
    entryIndex: number,
    slots: Record<string, string[]>,
    source: string,
    actor: string | null,
    isCurrent: number,
  ) => {
    const full = { top: [], bottom: [], footwear: [], accessories: [], hair: [], ...slots };
    return [id, chatId, characterId, entryIndex, JSON.stringify(full), hashEquippedSlots(full as never), source, actor, isCurrent, ts];
  };

  for (const mode of MODES) {
    const file = `cwh-ensure-${mode.replace("'", 'p')}.db`;
    const out = join(outDir, file);
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
    const base = new Database(out);
    base.pragma(keyPragma);
    base.pragma('journal_mode = TRUNCATE');
    base.exec(mode === "E'" ? chatsDdl.replace('"updatedAt" TEXT NOT NULL', '"updatedAt" TEXT') : chatsDdl);
    for (const [id, updatedAt, outfit] of corpus(mode)) {
      base
        .prepare(
          'INSERT INTO chats (id, userId, title, participants, createdAt, updatedAt, equippedOutfit) VALUES (?, ?, ?, ?, ?, ?, ?)',
        )
        .run(id, 'u1', id, '[]', ts, updatedAt, outfit);
    }
    if (mode === 'B' || mode === "B'") {
      base.exec(PRE_COLUMN_TABLE);
      base.exec(CHAT_WARDROBE_HISTORY_DDL[1]);
      base.exec(CHAT_WARDROBE_HISTORY_DDL[2]);
      const insert = base.prepare(
        'INSERT INTO chat_wardrobe_history (id, chatId, characterId, entryIndex, slots, slotsHash, source, actorCharacterId, isCurrent, createdAt) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)',
      );
      insert.run(...historyRow('99999999-9999-4999-8999-000000000001', CH(1), A, 0, {}, 'chat-start', null, 0));
      insert.run(...historyRow('99999999-9999-4999-8999-000000000002', CH(1), A, 1, { top: [COAT] }, 'pascal', A, 1));
      base.exec(LEDGER_DDL);
      const stamp = base.prepare(
        'INSERT INTO migrations_state (id, completedAt, quilltapVersion, itemsAffected, message) VALUES (?, ?, ?, ?, ?)',
      );
      stamp.run('add-chat-wardrobe-history-table-v1', ts, '4.11.0', 1, 'Created chat_wardrobe_history table');
      if (mode === "B'") {
        stamp.run('seed-chat-wardrobe-history-v1', ts, '4.11.0', 3, 'Seeded 3 outfit record(s) across 2 chat(s)');
      }
    }
    if (mode === 'D') {
      for (const sql of CHAT_WARDROBE_HISTORY_DDL) base.exec(sql);
      base
        .prepare(
          'INSERT INTO chat_wardrobe_history (id, chatId, characterId, entryIndex, slots, slotsHash, source, actorCharacterId, actorToolName, isCurrent, createdAt) VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?)',
        )
        .run(...historyRow('99999999-9999-4999-8999-000000000003', CH(2), A, 0, { top: [HAT] }, 'operator', null, 1));
    }
    base.close();

    // v4's REAL runner, narrowed to the three migrations, on a COPY (mode C:
    // the runner twice — the first pass IS the plant).
    const workDir = mkdtempSync(join(tmpdir(), `qt-cwh-ensure-${mode.replace("'", 'p')}-`));
    mkdirSync(join(workDir, 'data'), { recursive: true });
    const work = join(workDir, 'work.db');
    if (mode === 'C') {
      const plantDir = mkdtempSync(join(tmpdir(), 'qt-cwh-ensure-Cplant-'));
      mkdirSync(join(plantDir, 'data'), { recursive: true });
      const plant = join(plantDir, 'plant.db');
      copyFileSync(out, plant);
      const first = await runRunner(plant, plantDir);
      if (!first.success) throw new Error(`mode C plant: the runner failed: ${JSON.stringify(first)}`);
      copyFileSync(plant, out);
      rmSync(plantDir, { recursive: true, force: true });
    }
    copyFileSync(out, work);
    const result = await runRunner(work, workDir);
    if (!result.success && mode !== "E'") {
      throw new Error(`mode ${mode}: the runner failed: ${JSON.stringify(result)}`);
    }

    const db = new Database(work, { readonly: true });
    db.pragma(keyPragma);
    const master = db
      .prepare(
        "SELECT type, name, sql FROM sqlite_master WHERE tbl_name = 'chat_wardrobe_history' AND sql IS NOT NULL ORDER BY name",
      )
      .all();
    const tableInfo = db.prepare('PRAGMA table_info("chat_wardrobe_history")').all();
    const hasTable = master.some((m: { type: string }) => m.type === 'table');
    const rows = hasTable
      ? db
          .prepare(
            'SELECT * FROM "chat_wardrobe_history" ORDER BY "chatId", "characterId", "entryIndex"',
          )
          .all()
          .map((r: Record<string, unknown>) => {
            const { id: _id, ...rest } = r;
            return rest;
          })
      : [];
    const ledger = db
      .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'migrations_state'")
      .get()
      ? db
          .prepare('SELECT "id", "itemsAffected", "message" FROM "migrations_state" ORDER BY "id"')
          .all()
      : [];
    db.close();
    rmSync(workDir, { recursive: true, force: true });

    process.stdout.write(
      JSON.stringify({
        case: 'chat-wardrobe-history-ensure',
        mode,
        file,
        report: {
          success: result.success,
          run: result.migrationsRun,
          skipped: result.migrationsSkipped,
          ran: (result.results ?? []).map(
            (r: { id: string; success: boolean; message: string }) =>
              `${r.id}: ${r.success ? 'ok' : 'FAILED'}: ${r.message}`,
          ),
        },
        master,
        tableInfo,
        rows,
        ledger,
      }) + '\n',
    );
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-wardrobe-history-ensure oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
