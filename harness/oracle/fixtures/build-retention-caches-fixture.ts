/**
 * SEED fixture builder for the P4.d3 stale-chat CACHE-collapse differential (v4
 * `lib/background-jobs/maintenance/collapse-stale-chat-caches.ts`
 * `collapseStaleChatCaches`).
 *
 * Seeds one MAIN db (conversation_chunks lives on the main partition) through
 * v4's REAL repos + raw SQL, deterministic on FIXED timestamps (the differential
 * injects the same `nowIso` on both sides — no wall-clock skew):
 *
 *   - `chat-stale` (last played message 40 d before `nowIso` → stale under the
 *     default 30-day window): compressionCache + renderedMarkdown set; a played
 *     message (`type:'message'`, `systemSender:null`) carrying every discardable
 *     column (rawResponse / reasoningContent / reasoningSegments / renderedHtml /
 *     debugMemoryLogs); a SECOND played message with NONE of them set (the
 *     `IS NOT NULL` guard must skip it → messageRowsCleared stays 1); an EMBEDDED
 *     conversation-chunk and an already-COLD one (embedding NULL — the guard must
 *     skip it → chunkEmbeddingsCleared stays 1). `chats.updatedAt` is pinned to
 *     `seedTs` so the "collapse never bumps updatedAt" invariant is checkable.
 *   - `chat-active` (last played message 1 d before `nowIso` → NOT stale):
 *     identical shape (caches + a discardable-laden message + an embedded chunk),
 *     and every one of them must survive UNTOUCHED.
 *
 * No `dataRetention` instance setting is written, so `resolveStaleChatDays`
 * falls back to the documented 30-day default on both sides.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-retention-caches.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-retention-caches-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  nowIso: string;
  seedTs: string;
  staleMsgAt: string;
  activeMsgAt: string;
  staleChatId: string;
  activeChatId: string;
  participantId: string;
  characterId: string;
  staleMsgId: string;
  staleMsg2Id: string;
  activeMsgId: string;
  staleChunkEmbeddedId: string;
  staleChunkColdId: string;
  activeChunkId: string;
  embedding: number[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'retention-caches.json'), 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) throw new Error('QT_FIXTURE_OUT must point at the fixture .db to write');
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-retention-caches-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');
  const { ConversationChunksRepository } = await import(
    '@/lib/database/repositories/conversation-chunks.repository'
  );
  const { ConversationChunkSchema } = await import('@/lib/schemas/types');

  await initializeDatabase();
  await ensureCollection('conversation_chunks', ConversationChunkSchema);

  const chats = new ChatsRepository();
  const chunks = new ConversationChunksRepository();

  const participant = {
    id: spec.participantId,
    type: 'CHARACTER',
    characterId: spec.characterId,
    createdAt: spec.seedTs,
    updatedAt: spec.seedTs,
  };

  // --- create both chats (pinned ids + seed timestamps) ---------------------
  for (const chatId of [spec.staleChatId, spec.activeChatId]) {
    await chats.create(
      { userId: spec.userId, title: `Retention ${chatId}`, participants: [participant] } as never,
      { id: chatId, createdAt: spec.seedTs, updatedAt: spec.seedTs },
    );
  }

  // --- played messages (type:'message', systemSender:null) ------------------
  const playedMessage = async (chatId: string, id: string, createdAt: string) => {
    await chats.addMessage(chatId, {
      type: 'message',
      id,
      role: 'ASSISTANT',
      content: `content of ${id}`,
      opaqueContent: `content of ${id}`,
      attachments: [],
      createdAt,
      participantId: spec.participantId,
      systemSender: null,
    } as never);
  };
  await playedMessage(spec.staleChatId, spec.staleMsgId, spec.staleMsgAt);
  await playedMessage(spec.staleChatId, spec.staleMsg2Id, spec.staleMsgAt);
  await playedMessage(spec.activeChatId, spec.activeMsgId, spec.activeMsgAt);

  // Load the discardable columns onto the two "laden" messages (stale + active).
  const loadDiscardable = async (id: string) => {
    await rawQuery(
      `UPDATE chat_messages
          SET rawResponse = ?, reasoningContent = ?, reasoningSegments = ?,
              renderedHtml = ?, debugMemoryLogs = ?
        WHERE id = ?`,
      ['RAW-PAYLOAD', 'THINKING', '[{"t":"x"}]', '<p>rendered</p>', 'MEMLOG', id],
    );
  };
  await loadDiscardable(spec.staleMsgId);
  await loadDiscardable(spec.activeMsgId);

  // --- conversation chunks --------------------------------------------------
  const chunk = async (id: string, chatId: string, embedding: number[] | null) => {
    await chunks.create(
      {
        chatId,
        interchangeIndex: 0,
        content: `chunk ${id}`,
        participantNames: [],
        messageIds: [],
        embedding,
      } as never,
      { id, createdAt: spec.seedTs, updatedAt: spec.seedTs },
    );
  };
  await chunk(spec.staleChunkEmbeddedId, spec.staleChatId, spec.embedding);
  await chunk(spec.staleChunkColdId, spec.staleChatId, null);
  await chunk(spec.activeChunkId, spec.activeChatId, spec.embedding);

  // --- set caches + pin chats.updatedAt/lastMessageAt (addMessage bumped them) --
  const dressChat = async (chatId: string, lastMessageAt: string) => {
    await rawQuery(
      `UPDATE chats
          SET compressionCache = ?, renderedMarkdown = ?, updatedAt = ?, lastMessageAt = ?
        WHERE id = ?`,
      ['COMPRESSION-CACHE-JSON', '# rendered markdown', spec.seedTs, lastMessageAt, chatId],
    );
  };
  await dressChat(spec.staleChatId, spec.staleMsgAt);
  await dressChat(spec.activeChatId, spec.activeMsgAt);

  await closeDatabase();
  process.stderr.write(`built retention-caches fixture: ${out} (2 chats, 3 messages, 3 chunks)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`retention-caches fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
