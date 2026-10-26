/**
 * Tier-2 oracle case (P4.D324, the Inventory keystone — v4 `4fbdad186`): the
 * chats carried-Inventory ops.
 *
 * Runs `chats-inventory-tier2.json`'s op sequence through v4's REAL
 * `ChatsRepository` (`getEquippedInventory`, `setEquippedInventory`,
 * `findChatIdsCarryingInventoryId`, `removeEquippedInventoryId`) on ONE copy of
 * the seed fixture: duplicates and out-of-order ids, `null`, a re-set keeping
 * its position, a new key appending, a delete-then-re-add APPENDING, a missing
 * row, the planted junk / `[]` / non-array / NULL cells, the substring trap
 * (an id inside another character's list, and inside a longer id), a remove
 * that does NOT re-sort; then `dropColumn` drops `chats.equippedInventory` on
 * the working copy (R-K — the pre-column table a v5 reaches without the
 * host's ensures) and the ops run again. Per op: the result, the
 * `equippedInventory` BYTES of every chat after it, and every line v4 logged
 * (recorded off the `Logger` prototype, before the level check).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHINV=/tmp/qt-chinv-fixture.db \
 *     $N/node --import tsx $V5W/harness/oracle/cases/chats-inventory-tier2.ts \
 *     > /tmp/oracle-chinv.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Op {
  kind: 'get' | 'set' | 'find' | 'remove' | 'dropColumn';
  chatId?: string;
  character?: string;
  ids?: string[] | null;
  itemId?: string;
}
interface Spec {
  testPepperBase64: string;
  characters: Record<string, string>;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'chats-inventory-tier2.json'), 'utf8'),
  ) as Spec;
  const fixture = process.env.QT_FIXTURE_CHINV;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_CHINV must point at the seed fixture from build-chats-inventory-fixture.ts');
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-chinv-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'chinv-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');
  const { Logger } = await import('@/lib/logger');

  let opLogs: Array<{ level: string; message: string; fields: Array<[string, unknown]> }> = [];
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

  await initializeDatabase();
  const repo = new ChatsRepository();
  const cells = async () =>
    (await rawQuery('SELECT "id", "equippedInventory" FROM "chats" ORDER BY "id"').catch(async () =>
      rawQuery('SELECT "id", NULL AS "equippedInventory" FROM "chats" ORDER BY "id"'),
    )) as Array<{ id: string; equippedInventory: string | null }>;

  for (const [i, op] of spec.ops.entries()) {
    const character = op.character ? spec.characters[op.character] : undefined;
    opLogs = [];
    let result: unknown = null;
    switch (op.kind) {
      case 'get':
        result = await repo.getEquippedInventory(op.chatId!);
        break;
      case 'set':
        result = await repo.setEquippedInventory(op.chatId!, character!, op.ids ?? null);
        break;
      case 'find':
        result = await repo.findChatIdsCarryingInventoryId(character!, op.itemId!);
        break;
      case 'remove':
        result = await repo.removeEquippedInventoryId(op.chatId!, character!, op.itemId!);
        break;
      case 'dropColumn':
        await rawQuery('ALTER TABLE "chats" DROP COLUMN "equippedInventory"');
        break;
    }
    // Snapshot BEFORE the dump (whose own read may log, post-`dropColumn`).
    const logs = [...opLogs];
    process.stdout.write(
      JSON.stringify({ index: i, op, result: JSON.stringify(result), cells: await cells(), logs }) + '\n',
    );
  }

  await closeDatabase();
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats-inventory-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
