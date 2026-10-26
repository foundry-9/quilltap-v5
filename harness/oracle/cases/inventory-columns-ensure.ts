/**
 * Tier-2 oracle case — the Inventory's two column boot ensures (P4.D324,
 * `db::inventory_columns_repair`) against v4's REAL migrations
 * `add-chat-equipped-inventory-v1` (`4fbdad186`) and
 * `add-character-inventory-flag-v1` (`e1872c94f`), through v4's REAL
 * `MigrationRunner` NARROWED to them (`runV4Migrations({ runner: true })` —
 * the ledger's silence is part of the comparand: v4's runner STAMPS each id
 * it runs, v5 never does — `UNSTAMPED_COLUMN_ENSURES`).
 *
 * Seven shapes, each base file built HERE at the pin from v4's own generateDDL
 * (the CREATEs `ensureCollection` makes for `chats` and `characters`, read
 * back from `sqlite_master`) and written to `QT_FIXTURE_OUT_DIR/inv-<mode>.db`,
 * each with two chats (one with an `equippedOutfit`, one without) and two
 * characters (`canCreateOutfits` 1 / NULL):
 *   C-A  the `"equippedInventory" TEXT,` line REMOVED → the chats migration
 *        RUNS and appends `TEXT DEFAULT NULL`;
 *   C-B  the pin's CREATE as is → `not needed`;
 *   C-C  C-A with the migration's ADD applied by hand → `not needed`;
 *   F-A / F-B / F-C  the same three for `"canManageInventory" INTEGER,` and
 *        the flag migration;
 *   D    BOTH lines removed → v4 runs BOTH in registration order (chats, then
 *        characters).
 * Each line is asserted present exactly once before its removal. Comparands:
 * `PRAGMA table_info` of both tables (order included), both `sqlite_master.sql`
 * texts, every row's raw new cell (NULL — no DEFAULT back-fill), and the
 * `migrations_state` ids after the run. v4's report is emitted per mode; the
 * Rust side asserts `+RAN` only on the A modes and D.
 *
 * Run from the pinned v4 worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   rm -rf /tmp/qt-inv-ensure
 *   QT_FIXTURE_OUT_DIR=/tmp/qt-inv-ensure \
 *     $N/npx tsx $V5W/harness/oracle/cases/inventory-columns-ensure.ts \
 *     > /tmp/oracle-inv-ensure.ndjson
 */

import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { INVENTORY_MIGRATIONS, runV4Migrations } from '../lib/v4-migrations';

const CHATS_LINE = `  "equippedInventory" TEXT,\n`;
const FLAG_LINE = `  "canManageInventory" INTEGER,\n`;
/** v4 `add-chat-equipped-inventory-v1.ts:41` through `addColumnIfMissing`. */
const CHATS_ADD = `ALTER TABLE "chats" ADD COLUMN "equippedInventory" TEXT DEFAULT NULL`;
/** v4 `add-character-inventory-flag-v1.ts:38` through `addColumnIfMissing`. */
const FLAG_ADD = `ALTER TABLE "characters" ADD COLUMN "canManageInventory" INTEGER DEFAULT NULL`;
const [CHATS_MIGRATION, FLAG_MIGRATION] = INVENTORY_MIGRATIONS;

type Mode = 'C-A' | 'C-B' | 'C-C' | 'F-A' | 'F-B' | 'F-C' | 'D';

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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-inv-ensure-ddl-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'ddl.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { ChatMetadataBaseSchema } = await import('@/lib/schemas/chat.types');
  const { CharacterSchema } = await import('@/lib/schemas/character.types');
  await initializeDatabase();
  await ensureCollection('chats', ChatMetadataBaseSchema);
  await ensureCollection('characters', CharacterSchema);
  const ddlOf = async (name: string) =>
    ((await rawQuery(`SELECT sql FROM sqlite_master WHERE type = 'table' AND name = '${name}'`)) as Array<{ sql: string }>)[0].sql;
  const chats = await ddlOf('chats');
  const characters = await ddlOf('characters');
  await closeDatabase();
  for (const [ddl, line] of [[chats, CHATS_LINE], [characters, FLAG_LINE]] as const) {
    if (ddl.split(line).length !== 2) {
      throw new Error(`expected exactly one ${JSON.stringify(line)} in v4's DDL:\n${ddl}`);
    }
  }
  const chatsWithout = chats.replace(CHATS_LINE, '');
  const charactersWithout = characters.replace(FLAG_LINE, '');

  const plan: Record<Mode, { chats: string; characters: string; add?: string; migrations: typeof INVENTORY_MIGRATIONS }> = {
    'C-A': { chats: chatsWithout, characters, migrations: [CHATS_MIGRATION] },
    'C-B': { chats, characters, migrations: [CHATS_MIGRATION] },
    'C-C': { chats: chatsWithout, characters, add: CHATS_ADD, migrations: [CHATS_MIGRATION] },
    'F-A': { chats, characters: charactersWithout, migrations: [FLAG_MIGRATION] },
    'F-B': { chats, characters, migrations: [FLAG_MIGRATION] },
    'F-C': { chats, characters: charactersWithout, add: FLAG_ADD, migrations: [FLAG_MIGRATION] },
    D: { chats: chatsWithout, characters: charactersWithout, migrations: [CHATS_MIGRATION, FLAG_MIGRATION] },
  };

  const Database = createRequire(process.cwd() + '/')('better-sqlite3');
  const ts = '2026-01-01T00:00:00.000Z';
  // Every NOT NULL, default-less, non-PK column gets a filler so the rows
  // insert whatever the pin's schemas require.
  const insert = (db: any, table: string, values: Record<string, unknown>) => {
    const info = db.prepare(`PRAGMA table_info("${table}")`).all() as Array<{
      name: string; notnull: number; dflt_value: unknown; pk: number;
    }>;
    const row: Record<string, unknown> = {};
    for (const c of info) if (c.notnull && c.dflt_value === null && !c.pk) row[c.name] = 'x';
    Object.assign(row, { createdAt: ts, updatedAt: ts }, values);
    const cols = Object.keys(row);
    db.prepare(`INSERT INTO "${table}" (${cols.map((c) => `"${c}"`).join(', ')}) VALUES (${cols.map(() => '?').join(', ')})`)
      .run(...Object.values(row));
  };
  for (const mode of Object.keys(plan) as Mode[]) {
    const { chats: chatsDdl, characters: charactersDdl, add, migrations } = plan[mode];
    const out = join(outDir, `inv-${mode}.db`);
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
    const base = new Database(out);
    base.pragma(keyPragma);
    base.pragma('journal_mode = TRUNCATE');
    base.exec(chatsDdl);
    base.exec(charactersDdl);
    if (add) base.exec(add);
    insert(base, 'chats', { id: 'c1', userId: 'u1', title: 'dressed', equippedOutfit: '{"k1":{"top":["i1"]}}' });
    insert(base, 'chats', { id: 'c2', userId: 'u1', title: 'bare' });
    insert(base, 'characters', { id: 'k1', userId: 'u1', name: 'A', canCreateOutfits: 1 });
    insert(base, 'characters', { id: 'k2', userId: 'u1', name: 'B' });
    base.close();

    const workDir = mkdtempSync(join(tmpdir(), `qt-inv-ensure-${mode}-`));
    const work = join(workDir, 'work.db');
    copyFileSync(out, work);
    const report = await runV4Migrations({
      dbPath: work,
      pepperBase64: spec.testPepperBase64,
      migrations,
      runner: true,
    });
    const db = new Database(work, { readonly: true });
    db.pragma(keyPragma);
    const snap = (table: string, column: string) => ({
      tableInfo: db.prepare(`PRAGMA table_info("${table}")`).all(),
      sql: (db.prepare(`SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?`).get(table) as { sql: string }).sql,
      cells: db
        .prepare(`SELECT id, ${db.prepare(`PRAGMA table_info("${table}")`).all().some((c: { name: string }) => c.name === column) ? `quote("${column}")` : `'absent'`} AS cell FROM "${table}" ORDER BY id`)
        .all(),
    });
    const ledger = db.prepare(`SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'migrations_state'`).get()
      ? (db.prepare('SELECT id FROM migrations_state ORDER BY id').all() as Array<{ id: string }>).map((r) => r.id)
      : [];
    const row = {
      case: 'inventory-columns-ensure',
      mode,
      report,
      chats: snap('chats', 'equippedInventory'),
      characters: snap('characters', 'canManageInventory'),
      ledger,
    };
    db.close();
    rmSync(workDir, { recursive: true, force: true });
    process.stdout.write(JSON.stringify(row) + '\n');
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`inventory-columns-ensure oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
