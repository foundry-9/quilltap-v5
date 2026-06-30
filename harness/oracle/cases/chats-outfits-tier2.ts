/**
 * Tier-2 oracle case — the chats equipped-outfit ops (Phase-2, the chats repo —
 * sub-unit 6: getEquippedOutfit / getEquippedOutfitForCharacter / setEquippedOutfit
 * / removeEquippedItemFromAllChats).
 *
 * Runs the fixed op sequence (`chats-outfits-tier2.json`) via v4's REAL
 * `ChatsRepository` on a copy of the seed fixture, then dumps the `chats` rows
 * canonically. The write ops (setEquippedOutfit / removeEquippedItemFromAllChats)
 * are read-modify-writes of the `equippedOutfit` JSON column; the CHAT's own
 * `updatedAt` is never bumped (v4 `_update` preserves it). All four methods are on
 * the public repository surface.
 *
 * After the ops, the case emits a `reads` array — the result of each read query
 * in `spec.reads` (getEquippedOutfit / getEquippedOutfitForCharacter against the
 * post-op state) — so the Rust side can assert the read marshaling too.
 *
 * NORMALIZATION: NONE. These ops mint no ids/timestamps and `update` preserves the
 * chat `updatedAt`, so the `chats` dump is diffed EXACTLY. The equippedOutfit JSON
 * column's key order is reproduced byte-for-byte (closed-schema slots as a typed
 * struct; outer characterId keys constrained to sorted order — see
 * `chats_outfits.rs`).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHOUTFIT=/tmp/qt-choutfit-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-outfits-tier2.ts > /tmp/oracle-choutfit.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind: 'setEquippedOutfit' | 'removeEquippedItemFromAllChats';
  chatId?: string;
  characterId?: string;
  slots?: Record<string, unknown>;
  itemId?: string;
}
interface Read {
  kind: 'getEquippedOutfit' | 'getEquippedOutfitForCharacter';
  chatId: string;
  characterId?: string;
}
interface Spec {
  testPepperBase64: string;
  ops: Op[];
  reads?: Read[];
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
  const specPath = join(here, '..', 'fixtures', 'chats-outfits-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_CHOUTFIT;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_CHOUTFIT must point at the seed fixture from build-chats-outfits-fixture.ts');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-choutfit-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'choutfit-work.db');
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
    if (op.kind === 'setEquippedOutfit') {
      await repo.setEquippedOutfit(op.chatId as string, op.characterId as string, op.slots as never);
    } else {
      await repo.removeEquippedItemFromAllChats(op.itemId as string);
    }
  }

  // Read queries against the post-op state (banks the two read methods).
  const reads: unknown[] = [];
  for (const r of spec.reads ?? []) {
    if (r.kind === 'getEquippedOutfit') {
      reads.push(await repo.getEquippedOutfit(r.chatId));
    } else {
      reads.push(await repo.getEquippedOutfitForCharacter(r.chatId, r.characterId as string));
    }
  }

  const chats = await dumpTable(rawQuery, 'chats');

  await closeDatabase();

  process.stdout.write(JSON.stringify({ case: 'chats-outfits-tier2', chats, reads }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats-outfits-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
