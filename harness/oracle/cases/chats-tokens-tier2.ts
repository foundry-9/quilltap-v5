/**
 * Tier-2 oracle case — the chats token-tracking ops (Phase-2, the chats repo —
 * sub-unit 6: incrementTokenAggregates / resetTokenAggregates).
 *
 * Runs the fixed op sequence (`chats-tokens-tier2.json`) via v4's REAL
 * `ChatsRepository` on a copy of the seed fixture, then dumps the `chats` rows
 * canonically.
 *
 * `incrementTokenAggregates` is the `$inc`-counters + always-minted `updatedAt`,
 * with the cost accumulator (`estimatedCostUSD = current + cost`) and optional
 * `priceSource` set only when cost>0 AND the chat exists. `resetTokenAggregates`
 * zeroes the counters + clears `estimatedCostUSD` to null, PRESERVING `updatedAt`
 * (v4's `_update` override).
 *
 * NORMALIZATION SPEC (applied identically to both dumps in the Rust test):
 *   - `updatedAt`: a value EQUAL to the seed sentinel stays pinned (diffed
 *     exactly — proves a reset did NOT mint it); any OTHER value (an increment's
 *     mint) → `<ts>`.
 *   - ids + `createdAt` pinned; `totalPromptTokens` / `totalCompletionTokens` /
 *     `estimatedCostUSD` / `priceSource` diffed EXACTLY (the increment math, the
 *     cost accumulation, the reset-to-null).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHTOK=/tmp/qt-chtok-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-tokens-tier2.ts > /tmp/oracle-chtok.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind: 'increment' | 'reset';
  chatId: string;
  promptTokens?: number;
  completionTokens?: number;
  estimatedCost?: number | null;
  priceSource?: string;
}
interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function dumpTable(rawQuery: (sql: string) => Promise<unknown>, table: string) {
  const columns = (
    (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
  return canonicalizeRows({ table, columns, rawRows, orderBy: 'id' });
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'chats-tokens-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_CHTOK;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_CHTOK must point at the seed fixture from build-chats-tokens-fixture.ts');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chtok-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'chtok-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');

  await initializeDatabase();
  const repo = new ChatsRepository();

  for (const op of spec.ops) {
    if (op.kind === 'increment') {
      await repo.incrementTokenAggregates(
        op.chatId,
        op.promptTokens as number,
        op.completionTokens as number,
        op.estimatedCost === undefined ? null : op.estimatedCost,
        op.priceSource,
      );
    } else {
      await repo.resetTokenAggregates(op.chatId);
    }
  }

  const chats = await dumpTable(rawQuery, 'chats');

  await closeDatabase();

  process.stdout.write(JSON.stringify({ case: 'chats-tokens-tier2', chats }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats-tokens-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
