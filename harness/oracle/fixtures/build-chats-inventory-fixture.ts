/**
 * Tier-2 SEED fixture builder (P4.D324, the Inventory keystone — v4
 * `4fbdad186`): the chats carried-Inventory ops (`getEquippedInventory` /
 * `setEquippedInventory` / `findChatIdsCarryingInventoryId` /
 * `removeEquippedInventoryId`), the `build-chats-outfits-fixture.ts` twin — a
 * main-only file.
 *
 * Creates the spec chats (ids + timestamps pinned) through v4's REAL
 * `ChatsRepository.create` on a fresh main DB (`generateDDL` at the pin — the
 * `equippedInventory` column present), then plants each chat's raw
 * `equippedInventory` cell (well-formed maps, `junk`, `[]`, a non-array entry,
 * NULL, a substring trap) with a raw UPDATE, so both sides start from the same
 * bytes. The pre-column plant (R-K) is the CASE's (`dropColumn` op), on the
 * working copy, so no second file is committed.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-chinv-fixture.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-chats-inventory-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  characters: Record<string, string>;
  chats: Array<{ id: string; title: string; cell: string | null }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'chats-inventory-tier2.json'), 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) throw new Error('QT_FIXTURE_OUT must point at the seed fixture .db to write');
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    if (existsSync(out + suffix)) rmSync(out + suffix);
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-chinv-fixture-build-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');
  await initializeDatabase();

  const repo = new ChatsRepository();
  const ts = spec.seedTimestamp;
  const seat = (characterId: string, n: number) => ({
    id: `fa00000${n}-0000-4000-8000-000000000001`,
    type: 'CHARACTER',
    characterId,
    createdAt: ts,
    updatedAt: ts,
  });
  for (const c of spec.chats) {
    await repo.create(
      {
        userId: '11111111-1111-4111-8111-111111111111',
        title: c.title,
        participants: [seat(spec.characters.A, 1), seat(spec.characters.B, 2)],
      } as never,
      { id: c.id, createdAt: ts, updatedAt: ts },
    );
    await rawQuery('UPDATE "chats" SET "equippedInventory" = ? WHERE "id" = ?', [c.cell, c.id]);
  }
  const check = (await rawQuery('SELECT "id", "equippedInventory" FROM "chats" ORDER BY "id"')) as Array<{
    id: string;
    equippedInventory: string | null;
  }>;
  for (const c of spec.chats) {
    const row = check.find((r) => r.id === c.id);
    if (!row || row.equippedInventory !== c.cell) throw new Error(`plant failed for ${c.id}`);
  }

  await closeDatabase();
  process.stderr.write(`built chats inventory seed fixture: ${out} (${spec.chats.length} chats)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats inventory fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
