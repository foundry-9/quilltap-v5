/**
 * Tier-3 SEED fixture builder — regenerate-swipe (v4
 * `lib/services/chat-message/regenerate-swipe.service.ts`
 * `regenerateMessageAsSwipe`).
 *
 * Bakes the seed state both differential sides start from, across BOTH databases:
 *   - one connection profile + two participant characters (Bertie / Operator) via
 *     v4's REAL `repos.characters.create` (full vault provisioning in the
 *     mount-index DB — buildMessageContext's system prompt reads the vault);
 *   - two `chat_settings` rows: user1 `onSwipeRegenerate: DELETE_MEMORIES`, user2
 *     `KEEP_MEMORIES` (the memory-cascade gate is per-user);
 *   - four chats (`repos.chats.create` + `addMessages`, pinned ids/timestamps):
 *       A first_regen (an un-grouped ASSISTANT target) — since P4.D37 its history
 *         also carries two ad-hoc announcements ahead of the target: a PUBLIC one
 *         signed by an off-scene character (the attribution pass must tag it
 *         `[Operator] …` in context) and one WHISPERED to the user participant
 *         (which the every-role whisper filter must drop from Bertie's context —
 *         before v4 `a163862c` that filter tested TOOL messages only, so this row
 *         is the one that would have leaked),
 *       B existing_group (a target already grouped + one sibling swipe),
 *       C keep_memories (user2's chat),
 *       D not_assistant (a USER message — the error path);
 *   - four `memories` (sourceMessageId = the A/C targets) on a throwaway "ghost"
 *     characterId (NOT the responder Bertie — so buildContext's memory search stays
 *     empty and the completion prompt is memory-free/robust) + their vectors, so
 *     the DELETE cascade has something to remove and KEEP has something to preserve.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-regen-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-regen-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-regenerate-swipe-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CharacterSpec {
  id: string;
  name: string;
  controlledBy: string;
  talkativeness: number;
  connectionProfileId: string;
}
interface ParticipantSpec {
  id: string;
  character: string;
  controlledBy?: string;
  status?: string;
}
interface MessageSpec {
  id: string;
  role: string;
  content: string;
  participantId: string | null;
  createdAt: string;
  swipeGroupId?: string;
  swipeIndex?: number;
  /** P4.D37: an ad-hoc announcement's signed speaker (the attribution pass). */
  customAnnouncer?: { kind: string; characterId?: string; displayName?: string } | null;
  /** P4.D37: whisper targets — on ANY role since v4 `a163862c`. */
  targetParticipantIds?: string[] | null;
  systemKind?: string | null;
}
interface ChatSpec {
  id: string;
  userId: string;
  chatType: string;
  participants: ParticipantSpec[];
  messages: MessageSpec[];
}
interface MemorySpec {
  id: string;
  characterId: string;
  chatId: string | null;
  sourceMessageId: string | null;
  relatedMemoryIds: string[];
  vector: number[] | null;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  dimensions: number;
  connectionProfiles: Array<{ id: string; name: string; provider: string; modelName: string }>;
  characters: CharacterSpec[];
  chatSettings: Array<{ id: string; userId: string; onSwipeRegenerate: string }>;
  chats: ChatSpec[];
  seedMemories: MemorySpec[];
  seedMetaCharacterIds: string[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'regenerate-swipe-tier3.json'), 'utf8')) as Spec;

  const outMain = process.env.QT_FIXTURE_OUT;
  const outMount = process.env.QT_FIXTURE_MOUNT_OUT;
  if (!outMain || !outMount) {
    throw new Error('QT_FIXTURE_OUT and QT_FIXTURE_MOUNT_OUT must point at the fixture .db files');
  }
  for (const base of [outMain, outMount]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = base + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-regen-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  const repos = getRepositories();

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }

  for (const cp of spec.connectionProfiles) {
    await repos.connections.create(
      { userId: spec.chats[0].userId, name: cp.name, provider: cp.provider, modelName: cp.modelName } as never,
      { id: cp.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  for (const c of spec.characters) {
    await repos.characters.create(
      {
        name: c.name,
        userId: spec.chats[0].userId,
        controlledBy: c.controlledBy,
        talkativeness: c.talkativeness,
        defaultConnectionProfileId: c.connectionProfileId,
      } as never,
      { id: c.id }
    );
  }

  for (const cs of spec.chatSettings) {
    await repos.chatSettings.create(
      {
        userId: cs.userId,
        cheapLLMSettings: { strategy: 'PROVIDER_CHEAPEST', fallbackToLocal: false },
        contextCompressionSettings: { enabled: false },
        memoryCascadePreferences: { onSwipeRegenerate: cs.onSwipeRegenerate },
      } as never,
      { id: cs.id }
    );
  }

  for (const chat of spec.chats) {
    const participants = chat.participants.map((p) => ({
      id: p.id,
      type: 'CHARACTER',
      characterId: p.character,
      controlledBy: p.controlledBy ?? 'llm',
      status: p.status ?? 'active',
      createdAt: spec.seedTimestamp,
      updatedAt: spec.seedTimestamp,
    }));
    await repos.chats.create(
      { userId: chat.userId, title: `Fixture ${chat.id}`, participants, chatType: chat.chatType } as never,
      { id: chat.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
    await repos.chats.addMessages(
      chat.id,
      chat.messages.map((m) => ({
        id: m.id,
        type: 'message',
        role: m.role,
        content: m.content,
        participantId: m.participantId ?? undefined,
        createdAt: m.createdAt,
        attachments: [],
        ...(m.swipeGroupId !== undefined ? { swipeGroupId: m.swipeGroupId } : {}),
        ...(m.swipeIndex !== undefined ? { swipeIndex: m.swipeIndex } : {}),
        ...(m.customAnnouncer !== undefined ? { customAnnouncer: m.customAnnouncer } : {}),
        ...(m.targetParticipantIds !== undefined
          ? { targetParticipantIds: m.targetParticipantIds }
          : {}),
        ...(m.systemKind !== undefined ? { systemKind: m.systemKind } : {}),
      })) as never
    );
  }

  // Memories + their vectors (ghost character store).
  for (const characterId of spec.seedMetaCharacterIds) {
    await repos.vectorIndices.saveMeta(characterId, spec.dimensions);
  }
  for (const seed of spec.seedMemories) {
    await repos.memories.create(
      {
        characterId: seed.characterId,
        aboutCharacterId: null,
        chatId: seed.chatId,
        projectId: null,
        content: `content ${seed.id}`,
        summary: `summary ${seed.id}`,
        keywords: [],
        tags: [],
        importance: 0.5,
        embedding: null,
        source: 'AUTO',
        witnessedContext: null,
        sourceMessageId: seed.sourceMessageId,
        lastAccessedAt: null,
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: seed.relatedMemoryIds,
        reinforcedImportance: 0.5,
      } as never,
      { id: seed.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
    if (seed.vector) {
      await repos.vectorIndices.addEntry({
        id: seed.id,
        characterId: seed.characterId,
        embedding: new Float32Array(seed.vector),
      });
    }
  }

  // [40319484] `gcOrphanedFileRow` runs inside every content-addressed rewrite
  // and deletes from `doc_mount_blobs` unconditionally, so a mount index that
  // lacks the table now throws `no such table` on the SECOND write to any path.
  // v4's blobs repository creates it lazily from hand-written DDL
  // (`doc-mount-blobs.repository.ts:113`); copied verbatim so the fixture carries
  // exactly the table a real instance has (it is in `fresh_schema.json` too).
  midb.exec(`
    CREATE TABLE IF NOT EXISTS "doc_mount_blobs" (
      "id" TEXT PRIMARY KEY,
      "fileId" TEXT NOT NULL,
      "sha256" TEXT NOT NULL,
      "sizeBytes" INTEGER NOT NULL,
      "storedMimeType" TEXT NOT NULL,
      "data" BLOB NOT NULL,
      "createdAt" TEXT NOT NULL,
      "updatedAt" TEXT NOT NULL,
      FOREIGN KEY ("fileId") REFERENCES "doc_mount_files" ("id") ON DELETE CASCADE
    )
  `);
  midb.exec(
    'CREATE UNIQUE INDEX IF NOT EXISTS "idx_doc_mount_blobs_fileId" ON "doc_mount_blobs" ("fileId")'
  );
  // Pin the seed-minted vector timestamps to the sentinel.
  await rawQuery('UPDATE vector_indices SET createdAt = ?, updatedAt = ?', [
    spec.seedTimestamp,
    spec.seedTimestamp,
  ]);
  await rawQuery('UPDATE vector_entries SET createdAt = ?', [spec.seedTimestamp]);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built regenerate-swipe fixture: ${outMain} + ${outMount} ` +
      `(${spec.characters.length} chars, ${spec.chats.length} chats, ${spec.seedMemories.length} memories)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`regenerate-swipe fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
