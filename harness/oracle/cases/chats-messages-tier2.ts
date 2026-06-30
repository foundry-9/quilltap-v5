/**
 * Tier-2 oracle case — the chats messages WRITE path (Phase-2, the chats repo —
 * sub-unit 4a: addMessage / addMessages).
 *
 * Runs the fixed op sequence (`chats-messages-tier2.json`) via v4's REAL
 * `ChatsRepository.addMessage` / `.addMessages` on a copy of the seed fixture,
 * then dumps BOTH the `chat_messages` rows (the write marshaling) and the `chats`
 * rows (the metadata side-effect: messageCount, lastMessageAt/updatedAt,
 * spokenThisCycleParticipantIds) canonically.
 *
 * NORMALIZATION SPEC (applied identically on the Rust side): the `chat_messages`
 * rows are fully pinned (ids + createdAt come from the input) — zero
 * normalization. On the `chats` rows, `lastMessageAt` and `updatedAt` are minted
 * `now` for the chats that received an actual message, so the Rust test collapses
 * each to `<ts>` — but ONLY when it differs from the seed sentinel (a chat that
 * got only a context-summary preserves its sentinel `updatedAt`, which must stay
 * pinned so a stray mint would be caught).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHATSMSG=/tmp/qt-chatsmsg-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-messages-tier2.ts > /tmp/oracle-chatsmsg.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind: 'addMessage' | 'addMessages';
  chatId: string;
  message?: Record<string, unknown>;
  messages?: Array<Record<string, unknown>>;
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
  const specPath = join(here, '..', 'fixtures', 'chats-messages-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_CHATSMSG;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_CHATSMSG must point at the seed fixture from build-chats-messages-fixture.ts');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chatsmsg-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'chatsmsg-work.db');
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
    if (op.kind === 'addMessage') {
      await repo.addMessage(op.chatId, op.message as never);
    } else {
      await repo.addMessages(op.chatId, op.messages as never);
    }
  }

  const messages = await dumpTable(rawQuery, 'chat_messages');
  const chats = await dumpTable(rawQuery, 'chats');

  await closeDatabase();

  process.stdout.write(JSON.stringify({ case: 'chats-messages-tier2', messages, chats }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats-messages-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
