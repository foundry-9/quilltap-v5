/**
 * Tier-2 oracle case — the wardrobe wear ledger's boot ensure (P4.D255 Tier 2
 * item 19) against v4's REAL migration RUNNER over its two migrations,
 * `add-wardrobe-wear-stats-table-v1` and `seed-wardrobe-wear-stats-v1`.
 *
 * Why the runner, not `lib/v4-migrations.ts`: the R-B handshake IS the
 * runner's — it checks `migrations_state` BEFORE `shouldRun` and stamps each
 * migration that ran through `recordCompletedMigration`. `runV4Migrations`
 * calls `shouldRun()` / `run()` directly and writes no ledger row, so it could
 * see neither half. The case builds v4's real `MigrationRunner` and narrows its
 * (TypeScript-private, runtime-public) `migrations` list to the two scripts.
 *
 * Modes (each base file built here, at the pin, and written to
 * `QT_FIXTURE_OUT_DIR/wear-ensure-<mode>.db` for the Rust side):
 *   (a) NO table, `chats` with outfits, no ledger (the Friday shape) → both
 *       migrations RUN and are stamped;
 *   (b) the table present in the MIGRATION's shape (its three statements),
 *       unseeded, NO ledger row → the table migration's `shouldRun` is false
 *       (unrecorded) and the seed RUNS — only the seed row is stamped. (The
 *       survey's "generateDDL shape" mode is restated: that shape makes v4's
 *       seed FAIL on `ON CONFLICT`, and no v5 instance has it without the
 *       migration index re-dump's UNIQUE index — recorded.)
 *   (c) the table + BOTH ledger rows → the runner skips both on the ledger;
 *       nothing written;
 *   (d) the seed corpus in mode (a)'s shape: legacy whole-composite ids, one id
 *       in two slots, `'not json'`, `'[]'`, `null`, an `''` character key, an
 *       array-index character key, two chats sharing an item with EQUAL
 *       `updatedAt` and two with unequal (the rowid tie-break).
 * Comparands per mode: `PRAGMA table_info` + `sqlite_master.sql` for the table
 * and both hand indexes; every ledger-table row minus `id` / `createdAt` /
 * `updatedAt`, rowid order; the `migrations_state` rows minus `completedAt` /
 * `quilltapVersion`, by id. The runner's report is recorded (ran / skipped).
 *
 * Run from the pinned v4 worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   rm -rf /tmp/qt-wear-ensure
 *   QT_FIXTURE_OUT_DIR=/tmp/qt-wear-ensure \
 *     $N/npx tsx $V5W/harness/oracle/cases/wardrobe-wear-stats-ensure.ts \
 *     > /tmp/oracle-wear-ensure.ndjson
 */

import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

type Mode = 'a' | 'b' | 'c' | 'd';

/** v4's `ensureSQLiteMigrationsTable` text (the mode-(c) plant). */
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

const A = '11111111-1111-4111-8111-11111111111a';
const B = '11111111-1111-4111-8111-11111111111b';
const COAT = '22222222-2222-4222-8222-2222222222c0';
const SHIRT = '22222222-2222-4222-8222-2222222222c1';
const HAT = '22222222-2222-4222-8222-2222222222c2';
const SUIT = '22222222-2222-4222-8222-2222222222c3';

function corpus(mode: Mode): Array<[string, string | null, string | null]> {
  if (mode === 'd') {
    return [
      ['d1', '2026-01-01T00:00:00.000Z', JSON.stringify({ [A]: { top: [SUIT] } })],
      ['d2', '2026-01-02T00:00:00.000Z', JSON.stringify({ [A]: { top: [COAT], accessories: [COAT, HAT] } })],
      ['d3', '2026-01-03T00:00:00.000Z', 'not json'],
      ['d4', '2026-01-04T00:00:00.000Z', '[]'],
      ['d5', '2026-01-05T00:00:00.000Z', null],
      ['d6', '2026-01-06T00:00:00.000Z', JSON.stringify({ '': { top: [HAT] }, [B]: { top: [HAT] } })],
      // Hand-written text: the array-index key LAST, so only `Object.entries`'
      // index-first order (not the stored order) puts it first.
      ['d7', '2026-01-07T00:00:00.000Z', `{"${B}":{"top":["${SHIRT}"]},"7":{"top":["${HAT}"]}}`],
      ['zz-first', '2026-02-01T00:00:00.000Z', JSON.stringify({ [B]: { bottom: [COAT] } })],
      ['aa-second', '2026-02-01T00:00:00.000Z', JSON.stringify({ [B]: { bottom: [COAT] } })],
      ['late', '2026-03-01T00:00:00.000Z', JSON.stringify({ [A]: { hair: [SHIRT] } })],
      ['early', '2025-12-01T00:00:00.000Z', JSON.stringify({ [A]: { hair: [SHIRT] } })],
      ['legacy', '2026-01-09T00:00:00.000Z', JSON.stringify({ [A]: { top: ['not-a-uuid'] } })],
    ];
  }
  return [
    ['chat-1', '2026-01-01T00:00:00.000Z', JSON.stringify({ [A]: { top: [COAT, SHIRT] }, [B]: { top: [COAT] } })],
    ['chat-2', '2026-03-01T00:00:00.000Z', JSON.stringify({ [A]: { top: [COAT], accessories: [COAT] } })],
    ['chat-3', '2026-02-01T00:00:00.000Z', null],
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

  // 1. v4's own generateDDL for `chats`, from a scratch database.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-wear-ensure-ddl-'));
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
  const { WARDROBE_WEAR_STATS_DDL } = await import(
    '@/lib/database/backends/sqlite/wardrobe-wear-stats-ddl'
  );

  const v4Root = process.cwd();
  const Database = createRequire(v4Root + '/')('better-sqlite3');
  const utils = await import(pathToFileURL(join(v4Root, 'migrations/lib/database-utils.ts')).href);
  const { MigrationRunner } = await import(pathToFileURL(join(v4Root, 'migrations/index.ts')).href);
  const { addWardrobeWearStatsTableMigration } = await import(
    pathToFileURL(join(v4Root, 'migrations/scripts/add-wardrobe-wear-stats-table-v1.ts')).href
  );
  const { seedWardrobeWearStatsMigration } = await import(
    pathToFileURL(join(v4Root, 'migrations/scripts/seed-wardrobe-wear-stats-v1.ts')).href
  );

  for (const mode of ['a', 'b', 'c', 'd'] as Mode[]) {
    const out = join(outDir, `wear-ensure-${mode}.db`);
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
    const base = new Database(out);
    base.pragma(keyPragma);
    base.pragma('journal_mode = TRUNCATE');
    base.exec(chatsDdl);
    const ts = '2026-01-01T00:00:00.000Z';
    for (const [id, updatedAt, outfit] of corpus(mode)) {
      base
        .prepare(
          'INSERT INTO chats (id, userId, title, participants, createdAt, updatedAt, equippedOutfit) VALUES (?, ?, ?, ?, ?, ?, ?)',
        )
        .run(id, 'u1', id, '[]', ts, updatedAt, outfit);
    }
    if (mode === 'b' || mode === 'c') {
      for (const sql of WARDROBE_WEAR_STATS_DDL) base.exec(sql);
    }
    if (mode === 'c') {
      base.exec(LEDGER_DDL);
      const stamp = base.prepare(
        'INSERT INTO migrations_state (id, completedAt, quilltapVersion, itemsAffected, message) VALUES (?, ?, ?, ?, ?)',
      );
      stamp.run('add-wardrobe-wear-stats-table-v1', ts, '4.10.0', 1, 'Created wardrobe_wear_stats table');
      stamp.run('seed-wardrobe-wear-stats-v1', ts, '4.10.0', 0, 'Credited 0 wear(s) across 0 chat(s)');
    }
    base.close();

    // 2. v4's REAL runner, narrowed to the two migrations, on a COPY.
    const workDir = mkdtempSync(join(tmpdir(), `qt-wear-ensure-${mode}-`));
    mkdirSync(join(workDir, 'data'), { recursive: true });
    const work = join(workDir, 'work.db');
    copyFileSync(out, work);
    process.env.SQLITE_PATH = work;
    process.env.QUILLTAP_DATA_DIR = workDir;
    utils.closeSQLite();
    const runner = new MigrationRunner();
    (runner as unknown as { migrations: unknown[] }).migrations = [
      addWardrobeWearStatsTableMigration,
      seedWardrobeWearStatsMigration,
    ];
    const result = await runner.runMigrations();
    utils.closeSQLite();
    if (!result.success) throw new Error(`mode ${mode}: the runner failed: ${JSON.stringify(result)}`);

    const db = new Database(work, { readonly: true });
    db.pragma(keyPragma);
    const master = db
      .prepare(
        "SELECT type, name, sql FROM sqlite_master WHERE tbl_name = 'wardrobe_wear_stats' AND sql IS NOT NULL ORDER BY name",
      )
      .all();
    const tableInfo = db.prepare('PRAGMA table_info("wardrobe_wear_stats")').all();
    const hasTable = master.some((m: { type: string }) => m.type === 'table');
    const rows = hasTable
      ? db
          .prepare(
            'SELECT "itemId", "wearerCharacterId", "wearCount", "firstWornAt", "lastWornAt", "lastWornChatId" FROM "wardrobe_wear_stats" ORDER BY rowid',
          )
          .all()
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
        case: 'wardrobe-wear-stats-ensure',
        mode,
        report: {
          run: result.migrationsRun,
          skipped: result.migrationsSkipped,
          ran: (result.results ?? []).map((r: { id: string; message: string }) => `${r.id}: ${r.message}`),
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
  process.stderr.write(`wardrobe-wear-stats-ensure oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
