/**
 * Tier-2 oracle case — the chats messages mutation path (Phase-2, the chats repo —
 * sub-unit 4b: updateMessage / deleteMessagesByIds / clearMessages).
 *
 * Runs the fixed op sequence (`chats-messages-ops-tier2.json`) via v4's REAL
 * `ChatsRepository` on a copy of the seed fixture, then dumps BOTH the
 * `chat_messages` rows (the row mutations) and the `chats` rows (the metadata
 * side-effects: deleteMessagesByIds recounts messageCount; clearMessages resets
 * messageCount→0 + lastMessageAt→null; updateMessage touches no metadata) canonically.
 *
 * NORMALIZATION SPEC: NONE. The seed's minted `lastMessageAt`/`updatedAt` are baked
 * once and read identically by both sides, and no 4b op mints a new chat timestamp
 * (updateMessage touches no metadata; delete/clear preserve `updatedAt`), so every
 * cell is deterministic.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHATSMSGOPS=/tmp/qt-chatsmsgops-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-messages-ops-tier2.ts > /tmp/oracle-chatsmsgops.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind: 'updateMessage' | 'deleteMessagesByIds' | 'clearMessages';
  chatId: string;
  messageId?: string;
  updates?: Record<string, unknown>;
  messageIds?: string[];
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
  const specPath = join(here, '..', 'fixtures', 'chats-messages-ops-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_CHATSMSGOPS;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_CHATSMSGOPS must point at the seed fixture from build-chats-messages-ops-fixture.ts');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chatsmsgops-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'chatsmsgops-work.db');
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
    if (op.kind === 'updateMessage') {
      await repo.updateMessage(op.chatId, op.messageId as string, op.updates as never);
    } else if (op.kind === 'deleteMessagesByIds') {
      await repo.deleteMessagesByIds(op.chatId, op.messageIds as string[]);
    } else {
      await repo.clearMessages(op.chatId);
    }
  }

  const messages = await dumpTable(rawQuery, 'chat_messages');
  const chats = await dumpTable(rawQuery, 'chats');

  await closeDatabase();

  process.stdout.write(JSON.stringify({ case: 'chats-messages-ops-tier2', messages, chats }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats-messages-ops-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
