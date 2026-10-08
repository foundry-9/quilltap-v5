/**
 * Tier-2 oracle case — the wardrobe wear ledger's DATA layer (P4.D255; v4
 * `lib/database/repositories/wardrobe-wear.repository.ts`, `3ee3b1342`, minus
 * the chokepoint `commitEquippedOutfit`, which is P4.D262's).
 *
 * 1. Builds the base DB PER RUN at the pin: an empty encrypted main partition
 *    (v4's `initializeDatabase`), then `wardrobe_wear_stats` through v4's REAL
 *    `addWardrobeWearStatsTableMigration.run()` (`harness/oracle/lib/
 *    v4-migrations.ts` — NEVER the repository, whose generateDDL shape has no
 *    `ON CONFLICT` target, §R.4(a)); copies it to `QT_FIXTURE_OUT_DIR/base.db`
 *    (the Rust side's starting point; not committed).
 * 2. Runs the committed spec's ops (`fixtures/wardrobe-wear-stats-tier2.json`)
 *    through v4's REAL `WardrobeWearRepository`, recording each op's result
 *    (rows with the minted `id` / `createdAt` / `updatedAt` stripped; a Map as
 *    its ordered entries), its thrown message, and every log line it emitted
 *    (a `Logger.prototype` spy — level, message, the CALL's context in v4's key
 *    order; the child logger's `module` is not part of the call context).
 * 3. A `dump` op records the table (rowid order, minus the three minted /
 *    clocked columns).
 *
 * RULED DIVERGENCE `FIND_ALL_DROPS_NULLABLE` (the human, 2026-10-08 — FIX v5,
 * file v4): v4's inherited `findAll()` validates each row through
 * `WardrobeWearStatsRowSchema`, and a SQL NULL hydrates to `undefined`, which
 * `z.string().nullable()` refuses — so every unattributed row (and any NULL
 * `lastWornChatId`) is DROPPED after `Data validation failed` + `Safe
 * validation failed`, and v4's full backup loses those tallies. v5's
 * `find_all` returns every row; the Rust test pins both sides.
 *
 * Recorded divergence (not driven): v4's inherited `findAll()` on an ABSENT
 * table runs `ensureCollection` and CREATES the generateDDL table before
 * answering `[]`; v5's `find_all` answers `[]` without creating anything (the
 * boot ensure is the table's only creator). Unreachable on a booted instance.
 *
 * Writes ZERO `llm_logs` rows. Run from the pinned v4 worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   QT_FIXTURE_OUT_DIR=/tmp/qt-wear-stats-tier2 \
 *     $N/npx tsx $V5W/harness/oracle/cases/wardrobe-wear-stats-tier2.ts \
 *     > /tmp/oracle-wear-stats-tier2.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { runV4Migrations } from '../lib/v4-migrations.js';

interface Op {
  kind: string;
  label: string;
  entries?: Array<{ itemId: string; wearerCharacterId: string | null; chatId: string | null; at: string }>;
  itemIds?: string[];
  itemId?: string;
  characterId?: string;
  rows?: Array<Record<string, unknown>>;
  sql?: string;
}

const MINTED = new Set(['id', 'createdAt', 'updatedAt']);
const strip = (row: Record<string, unknown>) =>
  Object.fromEntries(Object.entries(row).filter(([k]) => !MINTED.has(k)));

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'wardrobe-wear-stats-tier2.json'), 'utf8'),
  ) as { testPepperBase64: string; ops: Op[] };
  const outDir = process.env.QT_FIXTURE_OUT_DIR;
  if (!outDir) throw new Error('QT_FIXTURE_OUT_DIR must name the dir for base.db');
  mkdirSync(outDir, { recursive: true });

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wear-stats-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const base = join(scratch, 'base.db');

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = base;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  await initializeDatabase();
  await closeDatabase();

  const report = await runV4Migrations({
    dbPath: base,
    pepperBase64: spec.testPepperBase64,
    migrations: [
      {
        file: 'migrations/scripts/add-wardrobe-wear-stats-table-v1.ts',
        exportName: 'addWardrobeWearStatsTableMigration',
      },
    ],
  });
  if (!report.some((r) => r.startsWith('+RAN add-wardrobe-wear-stats-table-v1'))) {
    throw new Error(`the table migration did not run: ${report.join('; ')}`);
  }
  copyFileSync(base, join(outDir, 'base.db'));

  await initializeDatabase();
  const { WardrobeWearRepository } = await import(
    '@/lib/database/repositories/wardrobe-wear.repository'
  );
  const repo = new WardrobeWearRepository();

  let opLogs: Array<{ level: string; message: string; fields: Array<[string, unknown]> }> = [];
  const { Logger } = await import('@/lib/logger');
  for (const level of ['error', 'warn', 'info', 'debug'] as const) {
    const original = Logger.prototype[level];
    Logger.prototype[level] = function (
      this: unknown,
      message: string,
      context?: Record<string, unknown>,
      ...rest: unknown[]
    ) {
      opLogs.push({ level, message, fields: Object.entries(context ?? {}) });
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }

  const results: Array<{ kind: string; label: string; result: unknown; logs: unknown[] }> = [];
  for (const op of spec.ops) {
    opLogs = [];
    let result: unknown;
    try {
      switch (op.kind) {
        case 'increment':
          await repo.incrementWears(op.entries as never);
          result = null;
          break;
        case 'execSql':
          await rawQuery(op.sql!);
          result = null;
          break;
        case 'dropTable':
          await rawQuery('DROP TABLE "wardrobe_wear_stats"');
          result = null;
          break;
        case 'findSummaries':
          result = Array.from((await repo.findSummaries(op.itemIds!)).entries());
          break;
        case 'findHistory':
          result = await repo.findHistory(op.itemId!);
          break;
        case 'findRowsForItems':
          result = (await repo.findRowsForItems(op.itemIds!)).map(strip);
          break;
        case 'findRowsForWearer':
          result = (await repo.findRowsForWearer(op.characterId!)).map(strip);
          break;
        case 'fold':
          result = await repo.foldWearerIntoUnattributed(op.characterId!);
          result = result === undefined ? null : result;
          break;
        case 'upsertRows':
          await repo.upsertRows(op.rows as never);
          result = null;
          break;
        case 'deleteByItemIds':
          await repo.deleteByItemIds(op.itemIds!);
          result = null;
          break;
        case 'findAll':
          result = (await repo.findAll()).map((r) => strip(r as never));
          break;
        case 'dump':
          result = (
            (await rawQuery('SELECT * FROM "wardrobe_wear_stats" ORDER BY rowid')) as Array<
              Record<string, unknown>
            >
          ).map(strip);
          break;
        default:
          throw new Error(`unknown op kind ${op.kind}`);
      }
    } catch (err) {
      result = { threw: err instanceof Error ? err.message : String(err) };
    }
    results.push({ kind: op.kind, label: op.label, result, logs: opLogs });
  }

  opLogs = []; // the shutdown lines belong to no op
  await closeDatabase();
  process.stdout.write(
    JSON.stringify({ case: 'wardrobe-wear-stats-tier2', migrations: report, results }) + '\n',
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`wardrobe-wear-stats-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
