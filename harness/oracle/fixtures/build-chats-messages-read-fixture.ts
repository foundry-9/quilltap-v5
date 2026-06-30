/**
 * Read-differential fixture builder — the shared starting state for v4's message
 * read surface (`ChatMessagesOps`: getMessages / getMessageCount /
 * findChatIdForMessage). Phase-2, the chats repo — sub-unit 3 (chat_messages read).
 *
 * Runs v4's REAL `repos.chats.create(chat, { id, createdAt, updatedAt })` for the
 * one spec chat, then `repos.chats.addMessages(chatId, messages)` for the spec
 * message list. `addMessages` runs `ChatEventSchema.parse` on each message before
 * the insert, so every Zod `.default(...)` (attachments → [], DangerFlag booleans
 * → false, …) is baked into the stored bytes — exactly what the read path marshals
 * back. Ids + createdAt are pinned on both the chat and every message, so the
 * fixture is fully deterministic; both the oracle and the Rust port
 * (db::chats_messages_read) READ a COPY of this SAME baked fixture and the
 * hydrated `getMessages` results compare exactly, with NO normalization.
 *
 * (`addMessages` mints `now` into the *chat* row's metadata — messageCount,
 * lastMessageAt, updatedAt, spokenThisCycle — but the read-differential reads only
 * the messages, and the fixture is baked once and copied to both sides, so that
 * minted metadata is identical on both sides regardless.)
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHATSMSGREAD=/tmp/qt-chatsmsgread.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chats-messages-read-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  chat: Record<string, unknown> & { id: string; createdAt: string; updatedAt: string };
  messages: Array<Record<string, unknown>>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'chats-messages-read-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_CHATSMSGREAD;
  if (!out) {
    throw new Error('QT_FIXTURE_CHATSMSGREAD must point at the fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chatsmsgread-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');

  await initializeDatabase();

  const repo = new ChatsRepository();
  const { id, createdAt, updatedAt, ...chatData } = spec.chat;
  await repo.create(chatData as never, { id, createdAt, updatedAt });
  await repo.addMessages(id, spec.messages as never);

  await closeDatabase();
  process.stderr.write(`built chats messages read fixture: ${out} (${spec.messages.length} messages)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats messages read fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
