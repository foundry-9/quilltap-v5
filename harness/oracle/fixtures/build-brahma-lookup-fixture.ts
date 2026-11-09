/**
 * Fixture builder for the P4.D374 Brahma lookup tools differential
 * (`brahma_lookup_tools_equivalence`).
 *
 * Bakes ONE state across main + mount-index + llm-logs through v4's REAL
 * repositories at the pin, from `brahma-lookup-tools.json` (SEEDED UUIDs — the
 * DB files are MINTED per run and never committed; both sides read COPIES of
 * the same files, so no id remap is needed):
 *   - seven characters via `repos.characters.create` (each mints its vault, so
 *     aliases and the physical description live where v4's hydration reads
 *     them); one is then archived through `repos.characters.update`;
 *   - five chats via `repos.chats.create` (salon / autonomous / help / a NULL
 *     `chatType` / salon), the NULL planted after create;
 *   - their messages via `repos.chats.addMessage`;
 *   - memories via `repos.memories.create` with Float32 vectors (memory note
 *     `memory-search-plants-need-vectors`), one cold + superseded;
 *   - the FTS5 message index through v4's own `ensureChatMessageFtsSchema` +
 *     `rebuildChatMessageFtsIndex` (the venue `lookup_messages` reads).
 *
 * Run (Node 24, from the pinned v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_BL_MAIN=/tmp/qt-bl-main.db QT_FIXTURE_BL_MOUNT=/tmp/qt-bl-mount.db \
 *   QT_FIXTURE_BL_LLM=/tmp/qt-bl-llm.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-brahma-lookup-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedCharacter {
  id: string;
  createdAt: string;
  archivedAt?: string;
  data: Record<string, unknown>;
}
interface SeedChat {
  id: string;
  title: string;
  chatType: string | null;
  createdAt: string;
  updatedAt: string;
  participants: Array<Record<string, unknown>>;
}
interface SeedMemory {
  id: string;
  characterId: string;
  aboutCharacterId: string | null;
  chatId: string | null;
  content: string;
  importance: number;
  reinforcedImportance: number | null;
  occurredAt?: string | null;
  narrativeTime?: string | null;
  tier?: string;
  supersededById?: string | null;
  vector: number[];
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  characters: SeedCharacter[];
  chats: SeedChat[];
  messages: Record<string, Array<Record<string, unknown>>>;
  memories: SeedMemory[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'brahma-lookup-tools.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_BL_MAIN;
  const mountOut = process.env.QT_FIXTURE_BL_MOUNT;
  const llmOut = process.env.QT_FIXTURE_BL_LLM;
  if (!mainOut || !mountOut || !llmOut) {
    throw new Error('QT_FIXTURE_BL_MAIN / _MOUNT / _LLM must point at the .db files to write');
  }
  for (const out of [mainOut, mountOut, llmOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-bl-fixture-build-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.SQLITE_LLM_LOGS_PATH = llmOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawLLMLogsDatabase } = await import('@/lib/database/backends/sqlite/llm-logs-client');
  const { CharacterSchema, ChatMetadataSchema, MemorySchema } = await import('@/lib/schemas/types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    ProjectDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');
  const { ensureChatMessageFtsSchema, rebuildChatMessageFtsIndex } = await import(
    '@/lib/database/backends/sqlite/chat-message-fts'
  );

  await initializeDatabase();
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('chats', ChatMetadataSchema);
  await ensureCollection('memories', MemorySchema);
  getRawLLMLogsDatabase();

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  for (const [name, schema] of [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
    ['project_doc_mount_links', ProjectDocMountLinkSchema],
  ] as Array<[string, unknown]>) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }

  // `chats.transcriptVersion` arrives by MIGRATION only (the
  // build-chat-message-update-fts-fixture precedent): without it every
  // message write logs "Failed to bump transcript version".
  const chatCols = ((await rawQuery('PRAGMA table_info("chats")')) as Array<{ name: string }>).map((c) => c.name);
  if (!chatCols.includes('transcriptVersion')) {
    await rawQuery(`ALTER TABLE "chats" ADD COLUMN "transcriptVersion" INTEGER DEFAULT 0`);
  }

  const repos = getRepositories();
  for (const c of spec.characters) {
    await repos.characters.create(c.data as never, { id: c.id, createdAt: c.createdAt, updatedAt: c.createdAt } as never);
    if (c.archivedAt) {
      await rawQuery('UPDATE "characters" SET "archivedAt" = ? WHERE "id" = ?', [c.archivedAt, c.id]);
    }
  }

  for (const chat of spec.chats) {
    await repos.chats.create(
      { userId: spec.userId, title: chat.title, chatType: chat.chatType ?? 'salon', participants: chat.participants } as never,
      { id: chat.id, createdAt: chat.createdAt, updatedAt: chat.createdAt } as never,
    );
    for (const m of spec.messages[chat.id] ?? []) {
      await repos.chats.addMessage(chat.id, m as never);
    }
    // Pin the stamps (and the NULL chatType) after the writes moved them.
    await rawQuery('UPDATE "chats" SET "updatedAt" = ?, "createdAt" = ? WHERE "id" = ?', [
      chat.updatedAt,
      chat.createdAt,
      chat.id,
    ]);
    if (chat.chatType === null) {
      await rawQuery('UPDATE "chats" SET "chatType" = NULL WHERE "id" = ?', [chat.id]);
    }
  }

  for (const m of spec.memories) {
    await repos.memories.create(
      {
        characterId: m.characterId,
        aboutCharacterId: m.aboutCharacterId,
        chatId: m.chatId,
        projectId: null,
        content: m.content,
        summary: m.content.slice(0, 40),
        keywords: [],
        tags: [],
        importance: m.importance,
        embedding: new Float32Array(m.vector),
        source: 'AUTO',
        witnessedContext: null,
        sourceMessageId: null,
        lastAccessedAt: null,
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: [],
        reinforcedImportance: m.reinforcedImportance ?? m.importance,
        occurredAt: m.occurredAt ?? null,
        narrativeTime: m.narrativeTime ?? null,
        kind: 'semantic',
        entities: [],
      } as never,
      { id: m.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp },
    );
    if (m.reinforcedImportance === null) {
      // The `filterImportance` `??` arm: a NULL cell v4's create refuses.
      await rawQuery('UPDATE "memories" SET "reinforcedImportance" = NULL WHERE "id" = ?', [m.id]);
    }
    if (m.tier) {
      await rawQuery('UPDATE "memories" SET "tier" = ?, "supersededById" = ? WHERE "id" = ?', [
        m.tier,
        m.supersededById ?? null,
        m.id,
      ]);
    }
  }

  // The FTS venue, through v4's own schema + rebuild.
  const raw = getRawDatabase() as never;
  if (!raw) throw new Error('main DB handle unavailable');
  ensureChatMessageFtsSchema(raw);
  rebuildChatMessageFtsIndex(raw);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(`built brahma-lookup fixtures: main=${mainOut} mount=${mountOut} llm=${llmOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`brahma-lookup fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
