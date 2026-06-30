/**
 * Read-differential oracle — v4's message read surface (`ChatMessagesOps`:
 * getMessages / getMessageCount / findChatIdForMessage). Phase-2, the chats repo —
 * sub-unit 3 (chat_messages read).
 *
 * Opens a COPY of the pre-baked fixture and drives v4's REAL repository read
 * methods for each spec query, emitting each result verbatim. The Rust port
 * (db::chats_messages_read) reads the same fixture and must produce the same
 * results — exactly (no normalization; nothing is mutated).
 *
 * Result shapes: a ChatEvent[] (getMessages), a number (getMessageCount), a string
 * or null (findChatIdForMessage). Each hydrated ChatEvent is the per-member
 * Zod-parsed object (defaults materialized at write time; nullable-optionals
 * dropped; JSON columns parsed; numbers rendered the JS way).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHATSMSGREAD=/tmp/qt-chatsmsgread.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-messages-read.ts > /tmp/oracle-chatsmsgread.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Query {
  kind: string;
  chatId?: string;
  messageId?: string;
}
interface Spec {
  testPepperBase64: string;
  queries: Query[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'chats-messages-read-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_CHATSMSGREAD;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_CHATSMSGREAD must point at the fixture from build-chats-messages-read-fixture.ts');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chatsmsgread-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'chatsmsgread-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');

  await initializeDatabase();
  const repo = new ChatsRepository();

  const results: Array<{ kind: string; result: unknown }> = [];
  for (const q of spec.queries) {
    let result: unknown;
    switch (q.kind) {
      case 'getMessages':
        result = await repo.getMessages(q.chatId as string);
        break;
      case 'getMessageCount':
        result = await repo.getMessageCount(q.chatId as string);
        break;
      case 'findChatIdForMessage':
        result = await repo.findChatIdForMessage(q.messageId as string);
        break;
      default:
        throw new Error(`unknown query kind: ${q.kind}`);
    }
    results.push({ kind: q.kind, result });
  }

  await closeDatabase();

  process.stdout.write(JSON.stringify({ case: 'chats-messages-read', queries: results }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats-messages-read oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
