/**
 * Tier-2 oracle case (P4.D324, the Inventory keystone — v4 `4fbdad186`):
 * Pascal's plain Inventory line, `postPascalInventoryChange`
 * (`lib/services/pascal/writer.ts`).
 *
 * Builds ONE seed main DB (a chat through v4's REAL `ChatsRepository.create`,
 * pinned ids) at `QT_FIXTURE_PIC`, then runs each op on its own fresh copy
 * through v4's REAL writer: a line posted (the row's key set — NO
 * `pascalMeta`), a missing chat (the `dropped` DEBUG), an empty body (what
 * `addMessage` makes of it), a markdown / unicode body. Per op: the answer's
 * presence, the `chat_messages` rows after it (minted id / `createdAt`
 * normalised by the Rust side), the chat row's message counters, and every
 * line logged (recorded off the `Logger` prototype).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_PIC=/tmp/qt-pic-fixture.db \
 *     $N/node --import tsx $V5W/harness/oracle/cases/pascal-inventory-change-tier2.ts \
 *     > /tmp/oracle-pic.ndjson
 */

import { fileURLToPath } from 'node:url';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';

const PEPPER = 'ZpjI5jcj5CYsyBA6zPH90G4frQEbv2WsAhERvEKrjJk=';
const TS = '2026-02-01T00:00:00.000Z';
const CHAT = 'b5000000-0000-4000-8000-000000000001';
const OPS: Array<{ name: string; chatId: string; content: string }> = [
  { name: 'posted', chatId: CHAT, content: 'Vivienne takes up Excalibur and a signet ring.' },
  { name: 'missing-chat', chatId: 'b9000000-0000-4000-8000-000000000009', content: 'Nobody sets down the lantern.' },
  { name: 'empty-content', chatId: CHAT, content: '' },
  { name: 'markdown-unicode', chatId: CHAT, content: '*Wren* sets down the café lantern — and picks up some lockpicks.' },
];

function env(path: string, scratch: string): void {
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.SQLITE_PATH = path;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';
}

async function build(out: string): Promise<void> {
  for (const s of ['', '-journal', '-wal', '-shm']) if (existsSync(out + s)) rmSync(out + s);
  const scratch = mkdtempSync(join(tmpdir(), 'qt-pic-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  env(out, scratch);
  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  await initializeDatabase();
  // A migrated instance's `chats` (v4 `add-transcript-version-column-v1`'s
  // shape): every message write bumps it.
  await getRepositories().chats.create(
    { userId: '11111111-1111-4111-8111-111111111111', title: 'Pascal’s Table', participants: [] } as never,
    { id: CHAT, createdAt: TS, updatedAt: TS },
  );
  await rawQuery('ALTER TABLE "chats" ADD COLUMN "transcriptVersion" INTEGER DEFAULT 0');
  // The messages table a provisioned instance has from its first boot (v4
  // creates it lazily, through the repository's own `ensureCollection`).
  const { ensureCollection } = await import('@/lib/database/manager');
  const { ChatMessageRowSchema } = await import('@/lib/database/repositories/chats-messages.ops');
  await ensureCollection('chat_messages', ChatMessageRowSchema);
  await closeDatabase();
  rmSync(scratch, { recursive: true, force: true });
}

async function runOp(index: number, fixture: string): Promise<void> {
  const op = OPS[index];
  const scratch = mkdtempSync(join(tmpdir(), 'qt-pic-op-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'main.db');
  copyFileSync(fixture, work);
  env(work, scratch);
  const { Logger } = await import('@/lib/logger');
  const logs: Array<{ level: string; message: string; fields: Array<[string, unknown]> }> = [];
  for (const level of ['error', 'warn', 'info', 'debug'] as const) {
    const original = Logger.prototype[level];
    Logger.prototype[level] = function (this: unknown, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
      logs.push({ level, message, fields: Object.entries(context ?? {}).filter(([, v]) => v !== undefined) });
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }
  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  await initializeDatabase();
  const { postPascalInventoryChange } = await import('@/lib/services/pascal/writer');
  logs.length = 0;
  const posted = await postPascalInventoryChange({ chatId: op.chatId, content: op.content });
  const lines = [...logs];
  // `chat_messages` exists only once a message was ever written.
  const rows = ((await rawQuery('SELECT * FROM "chat_messages" ORDER BY rowid').catch(() => [])) ?? []) as Array<
    Record<string, unknown>
  >;
  const chat = (await rawQuery('SELECT "messageCount", "lastMessageAt", "transcriptVersion" FROM "chats" WHERE "id" = ?', [
    CHAT,
  ])) as Array<Record<string, unknown>>;
  await closeDatabase();
  process.stdout.write(
    JSON.stringify({ index, name: op.name, op, posted: posted !== null, postedKeys: posted ? Object.keys(posted) : null, rows, chat, logs: lines }) +
      '\n',
  );
  process.exit(0);
}

async function main(): Promise<void> {
  const fixture = process.env.QT_FIXTURE_PIC;
  if (!fixture) throw new Error('QT_FIXTURE_PIC must name the seed fixture to write');
  const only = process.env.QT_PIC_OP_INDEX;
  if (only !== undefined) return runOp(Number(only), fixture);
  await build(fixture);
  for (let i = 0; i < OPS.length; i++) {
    const child = spawnSync(process.execPath, [...process.execArgv, fileURLToPath(import.meta.url)], {
      env: { ...process.env, QT_PIC_OP_INDEX: String(i) },
      encoding: 'utf8',
    });
    if (child.status !== 0 || !child.stdout.trim()) {
      process.stderr.write(`op #${i} failed:\n${child.stderr}\n`);
      process.exit(1);
    }
    process.stdout.write(child.stdout.trim().split('\n').pop() + '\n');
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`pascal-inventory-change oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
