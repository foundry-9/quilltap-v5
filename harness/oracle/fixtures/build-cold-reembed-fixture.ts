/**
 * SEED fixture builder for the P4.d3 cold-chunk RE-EMBED differential (v4
 * `lib/scriptorium/cold-chunk-reembed.ts` `maybeEnqueueColdChunkReembed`).
 *
 * Seeds one MAIN db through v4's REAL repos: a default embedding profile, and
 * three chats exercising every arm of the cold/warm decision —
 *
 *   - `cold` chat: 4 conversation-chunks — two COLD-with-content (content, NULL
 *     embedding → each enqueues one EMBEDDING_GENERATE), one EMBEDDED (skipped),
 *     one COLD-with-empty-content (NULL embedding, blank content → the content
 *     guard skips it). total(4) > embedded(1) ⇒ cold ⇒ 2 enqueued.
 *   - `warm` chat: 2 chunks, both embedded ⇒ embedded ≥ total ⇒ 0.
 *   - `chunkless` chat: no chunks ⇒ total 0 ⇒ 0.
 *
 * The single default embedding profile makes the profile pick deterministic
 * (`findAll().find(isDefault)`).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-cold-reembed.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-cold-reembed-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTs: string;
  profileId: string;
  coldChatId: string;
  warmChatId: string;
  chunklessChatId: string;
  coldChunk1Id: string;
  coldChunk2Id: string;
  coldEmbeddedId: string;
  coldEmptyId: string;
  warmChunk1Id: string;
  warmChunk2Id: string;
  embedding: number[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'cold-reembed.json'), 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) throw new Error('QT_FIXTURE_OUT must point at the fixture .db to write');
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cold-reembed-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');
  const { ConversationChunksRepository } = await import(
    '@/lib/database/repositories/conversation-chunks.repository'
  );
  const { EmbeddingProfilesRepository } = await import(
    '@/lib/database/repositories/embedding-profiles.repository'
  );
  const { ConversationChunkSchema, EmbeddingProfileSchema } = await import('@/lib/schemas/types');

  await initializeDatabase();
  await ensureCollection('embedding_profiles', EmbeddingProfileSchema);
  await ensureCollection('conversation_chunks', ConversationChunkSchema);
  // Materialize the empty background_jobs table so the Rust port can INSERT the
  // enqueued EMBEDDING_GENERATE rows (v5 does not own DDL).
  await getRepositories().backgroundJobs.getStats();

  const chats = new ChatsRepository();
  const chunks = new ConversationChunksRepository();
  const profiles = new EmbeddingProfilesRepository();

  // The single default embedding profile.
  await profiles.create(
    {
      userId: spec.userId,
      name: 'Seed Default',
      provider: 'BUILTIN',
      apiKeyId: null,
      baseUrl: null,
      modelName: 'builtin-tfidf',
      dimensions: null,
      truncateToDimensions: null,
      normalizeL2: true,
      isDefault: true,
      tags: [],
    } as never,
    { id: spec.profileId, createdAt: spec.seedTs, updatedAt: spec.seedTs },
  );

  // Three chats (a minimal user-only participant — the re-embed reads chunks +
  // profiles, never the character vault).
  for (const chatId of [spec.coldChatId, spec.warmChatId, spec.chunklessChatId]) {
    await chats.create(
      { userId: spec.userId, title: `Reembed ${chatId}`, participants: [] } as never,
      { id: chatId, createdAt: spec.seedTs, updatedAt: spec.seedTs },
    );
  }

  const chunk = async (id: string, chatId: string, content: string, embedding: number[] | null) => {
    await chunks.create(
      {
        chatId,
        interchangeIndex: 0,
        content,
        participantNames: [],
        messageIds: [],
        embedding,
      } as never,
      { id, createdAt: spec.seedTs, updatedAt: spec.seedTs },
    );
  };

  // cold chat: 2 cold-with-content + 1 embedded + 1 cold-with-empty-content.
  await chunk(spec.coldChunk1Id, spec.coldChatId, 'cold chunk one', null);
  await chunk(spec.coldChunk2Id, spec.coldChatId, 'cold chunk two', null);
  await chunk(spec.coldEmbeddedId, spec.coldChatId, 'already embedded', spec.embedding);
  await chunk(spec.coldEmptyId, spec.coldChatId, '   ', null);
  // warm chat: both embedded.
  await chunk(spec.warmChunk1Id, spec.warmChatId, 'warm one', spec.embedding);
  await chunk(spec.warmChunk2Id, spec.warmChatId, 'warm two', spec.embedding);
  // chunkless chat: no chunks.

  await closeDatabase();
  process.stderr.write(`built cold-reembed fixture: ${out} (1 profile, 3 chats, 6 chunks)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`cold-reembed fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
