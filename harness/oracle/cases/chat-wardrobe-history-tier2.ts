/**
 * Tier-2 oracle case — the chat wardrobe history's DATA layer (P4.D315 Tier 1
 * item 9; v4 `lib/database/repositories/chat-wardrobe-history.repository.ts`
 * at the `e8e17b2d9` pin).
 *
 * 1. Builds the base DB PER RUN at the pin: an empty encrypted main partition
 *    (v4's `initializeDatabase`), the spec's chats through v4's REAL
 *    `repos.chats.create` (ids and timestamps pinned), and the table through
 *    the round's shared plant `plantChatWardrobeHistoryTable()`
 *    (`harness/oracle/lib/v4-migrations.ts` — v4's REAL DDL through
 *    `rawQuery`); copies it to `QT_FIXTURE_OUT_DIR/base.db` (the Rust side's
 *    starting point; not committed).
 * 2. Runs the committed spec's ops (`fixtures/chat-wardrobe-history-tier2.json`)
 *    through v4's REAL container: `getRepositories().wardrobeHistory` (v4's
 *    `new ChatWardrobeHistoryRepository(chatsRepo)` over the REAL
 *    `ChatsRepository`) for `settle` / `upsertRows` / `deleteByCharacter` /
 *    the reads, and `getRepositories().chats.setEquippedOutfit` for the live
 *    slots between settles. Raw rows (`upsertRawRows`) go to the SAME
 *    `upsertRows` — v4 validates neither.
 * 3. After every op: the op's result (a `SettleOutcome`, a written count, a
 *    thrown message), the four reads (`findEntries` / `findCurrent` for the
 *    op's chat + character when it names them, `findByChatId` for its chat,
 *    `findAll`), the realtime publishes it queued (`Realtime publish queued`
 *    lines; the bus's pending map is cleared after each op so every op
 *    queues afresh), and every log line it emitted (a `Logger.prototype` spy —
 *    level, message, the CALL's context in v4's key order with `undefined`
 *    values dropped, as v4's own output drops them).
 *
 * Writes ZERO `llm_logs` rows. Run from the pinned v4 worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   rm -rf /tmp/qt-cwh-t2
 *   QT_FIXTURE_OUT_DIR=/tmp/qt-cwh-t2 \
 *     $N/npx tsx $V5W/harness/oracle/cases/chat-wardrobe-history-tier2.ts \
 *     > /tmp/oracle-cwh-t2.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { plantChatWardrobeHistoryTable } from '../lib/v4-migrations.js';

interface Op {
  kind: string;
  label: string;
  chatId?: string;
  characterId?: string;
  slots?: Record<string, string[]>;
  intent?: unknown;
  sql?: string;
  rows?: Array<Record<string, unknown>>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'chat-wardrobe-history-tier2.json'), 'utf8'),
  ) as {
    testPepperBase64: string;
    chats: Array<Record<string, unknown> & { id: string; createdAt: string; updatedAt: string }>;
    ops: Op[];
  };
  const outDir = process.env.QT_FIXTURE_OUT_DIR;
  if (!outDir) throw new Error('QT_FIXTURE_OUT_DIR must name the dir for base.db');
  mkdirSync(outDir, { recursive: true });

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cwh-t2-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const base = join(scratch, 'base.db');

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = base;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery, ensureCollection } = await import(
    '@/lib/database/manager'
  );
  const { ChatMetadataSchema } = await import('@/lib/schemas/types');
  const { getRepositories } = await import('@/lib/repositories/factory');
  await initializeDatabase();
  for (const c of spec.chats) {
    const { id, createdAt, updatedAt, ...data } = c;
    await getRepositories().chats.create(data as never, { id, createdAt, updatedAt } as never);
  }
  await plantChatWardrobeHistoryTable();
  await closeDatabase();
  copyFileSync(base, join(outDir, 'base.db'));

  await initializeDatabase();
  // A re-initialised manager has forgotten the collection's column types —
  // without this every chat read fails Zod (memory note
  // `v4-oracle-reinit-and-throwing-plants`).
  await ensureCollection('chats', ChatMetadataSchema);
  const repos = getRepositories();
  const history = repos.wardrobeHistory;

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
      opLogs.push({
        level,
        message,
        fields: Object.entries(context ?? {}).filter(([, v]) => v !== undefined),
      });
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }
  const clearBus = () => {
    const bus = (globalThis as { __quilltapRealtimeBus?: { pending: Map<string, { timer: unknown }> } })
      .__quilltapRealtimeBus;
    if (!bus) return;
    for (const entry of bus.pending.values()) clearTimeout(entry.timer as never);
    bus.pending.clear();
  };

  const results: unknown[] = [];
  for (const op of spec.ops) {
    opLogs = [];
    clearBus();
    let result: unknown;
    try {
      switch (op.kind) {
        case 'setOutfit':
          await repos.chats.setEquippedOutfit(op.chatId!, op.characterId!, op.slots as never);
          result = null;
          break;
        case 'settle':
          result = await history.settle(op.chatId!, op.characterId!, op.intent as never);
          break;
        case 'execSql':
          await rawQuery(op.sql!);
          result = null;
          break;
        case 'dropTable':
          await rawQuery('DROP TABLE "chat_wardrobe_history"');
          result = null;
          break;
        case 'exportDeleteUpsert': {
          const exported = await history.findByChatId(op.chatId!);
          await rawQuery('DELETE FROM "chat_wardrobe_history" WHERE "chatId" = ?', [op.chatId!]);
          result = { exported: exported.length, written: await history.upsertRows(exported) };
          break;
        }
        case 'upsertRows':
        case 'upsertRawRows':
          result = await history.upsertRows(op.rows as never);
          break;
        case 'deleteByCharacter':
          await history.deleteByCharacter(op.characterId!);
          result = null;
          break;
        case 'dump':
          result = await rawQuery(
            'SELECT * FROM "chat_wardrobe_history" ORDER BY "chatId", "characterId", "entryIndex", rowid',
          );
          break;
        default:
          throw new Error(`unknown op kind ${op.kind}`);
      }
    } catch (err) {
      result = { threw: err instanceof Error ? err.message : String(err) };
    }
    const logs = opLogs;
    opLogs = [];
    const read = async (f: () => Promise<unknown>) => {
      try {
        return await f();
      } catch (err) {
        return { threw: err instanceof Error ? err.message : String(err) };
      }
    };
    const reads = {
      entries:
        op.chatId && op.characterId
          ? await read(() => history.findEntries(op.chatId!, op.characterId!))
          : null,
      current:
        op.chatId && op.characterId
          ? await read(() => history.findCurrent(op.chatId!, op.characterId!))
          : null,
      byChat: op.chatId ? await read(() => history.findByChatId(op.chatId!)) : null,
      all: await read(() => history.findAll()),
    };
    opLogs = [];
    const publishes = logs
      .filter((l) => l.message === 'Realtime publish queued')
      .map((l) => Object.fromEntries(l.fields));
    results.push({ kind: op.kind, label: op.label, result, reads, publishes, logs });
  }

  clearBus();
  await closeDatabase();
  process.stdout.write(JSON.stringify({ case: 'chat-wardrobe-history-tier2', results }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-wardrobe-history-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
