/**
 * Shared Salon fixture builder (P4.6a unit 8) — the committed test-pepper
 * substrate for the whole Salon-server-surface differential set AND the sibling
 * lane's Playwright M4 e2e.
 *
 * Bakes, via v4's REAL repositories (all ids/timestamps pinned; the systemPrompt
 * id is path-derived-deterministic; conversation-chunk ids are minted but baked
 * into the committed file, so both differential sides read the identical rows):
 *   - one user (the FIXTURE_USER, rewritten to SINGLE_USER_ID at test setup),
 *   - a default chat_settings row (from the spec's cheap-LLM / danger knobs),
 *   - ONE OPENAI_COMPATIBLE connection profile (its `baseUrl` the e2e rewrites via
 *     `quilltap db --write` so the live send round-trips through a Node mock),
 *     an api key it references, and one image profile,
 *   - two legacy IMAGE `files` rows (an avatar + a story background),
 *   - one tag + one store-backed project,
 *   - four characters (Aria: avatar + a default systemPrompt; Bram; Cleo:
 *     user-controlled; Dax: off-scene, referenced only by a customAnnouncer),
 *   - a SOLO chat (Aria + Cleo, project + tag, reasoning + confirmation fields)
 *     and a GROUP chat (Aria + Bram + Cleo, a host whisper, a TOOL row, a swipe
 *     group, a customAnnouncer, a story background, rendered markdown + embedded
 *     conversation chunks),
 *   - two memories sourced from a group-chat assistant message (the delete cascade).
 *
 * Both the jest/tsx oracles and the Rust harness copy a FRESH copy of the SAME
 * committed fixture per case; read differentials diff with zero id remap (identical
 * baked ids), mutation differentials mint fresh copies per case.
 *
 * Regenerate (Node 24, from the v4 checkout) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   W=$V5/.claude/worktrees/salon-server-porting-67c54a
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SALON_MAIN=$W/crates/quilltap-web/tests/fixtures/salon-main.db \
 *   QT_FIXTURE_SALON_MOUNT=$W/crates/quilltap-web/tests/fixtures/salon-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-salon-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CharacterSpec {
  id: string;
  name: string;
  description?: string;
  aliases?: string[];
  controlledBy: string;
  talkativeness: number;
  connectionProfileId: string;
  defaultImageId?: string;
  systemPrompt?: { name: string; prompt: string; isDefault: boolean };
}
interface ParticipantSpec {
  id: string;
  character: string;
  controlledBy: string;
  connectionProfileId?: string;
  imageProfileId?: string;
  talkativeness?: number;
}
interface MessageSpec {
  id: string;
  role: string;
  content: string;
  participantId: string | null;
  provider?: string;
  modelName?: string;
  tokenCount?: number;
  promptTokens?: number;
  completionTokens?: number;
  reasoningContent?: string;
  reasoningSegments?: unknown[];
  systemSender?: string;
  systemKind?: string;
  hostEvent?: unknown;
  customAnnouncer?: unknown;
  swipeGroupId?: string;
  swipeIndex?: number;
  confirmed?: boolean;
  confirmationChecked?: boolean;
  confirmationNotes?: string;
}
interface ChunkSpec {
  interchangeIndex: number;
  content: string;
  embedded: boolean;
}
interface ChatSpec {
  id: string;
  title: string;
  chatType: string;
  projectId?: string;
  tags?: string[];
  storyBackgroundImageId?: string;
  renderedMarkdown?: string;
  chunks?: ChunkSpec[];
  participants: ParticipantSpec[];
  messages: MessageSpec[];
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  connectionProfiles: Array<Record<string, unknown>>;
  apiKeys: Array<Record<string, unknown>>;
  imageProfiles: Array<Record<string, unknown>>;
  files: Array<Record<string, unknown>>;
  tags: Array<{ id: string; name: string }>;
  projects: Array<{ id: string; name: string; color: string }>;
  characters: CharacterSpec[];
  chatSettings: {
    cheapLLMSettings: { strategy: string; fallbackToLocal: boolean };
    compressionEnabled: boolean;
    autoDetectRng: boolean;
    answerConfirmationEnabled: boolean;
    dangerMode: string;
  };
  chats: ChatSpec[];
  memories: Array<Record<string, unknown>>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'salon.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_SALON_MAIN;
  const mountOut = process.env.QT_FIXTURE_SALON_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_SALON_MAIN and QT_FIXTURE_SALON_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-salon-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, getCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  await ensureCollection('characters', CharacterSchema);

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }

  const repos = getRepositories();
  const TS = spec.seedTimestamp;

  // 1. The single user.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Default chat settings.
  await repos.chatSettings.create(
    {
      userId: spec.userId,
      cheapLLMSettings: spec.chatSettings.cheapLLMSettings,
      contextCompressionSettings: { enabled: spec.chatSettings.compressionEnabled },
      autoDetectRng: spec.chatSettings.autoDetectRng,
      answerConfirmationSettings: { enabled: spec.chatSettings.answerConfirmationEnabled },
      dangerousContentSettings: { mode: spec.chatSettings.dangerMode },
    } as never,
    { id: '5e000000-0000-4000-8000-000000000001', createdAt: TS, updatedAt: TS } as never,
  );

  // 3. Api keys (pinned ids via a direct collection insert), then connections.
  const { ApiKeySchema } = await import('@/lib/schemas/profile.types');
  await ensureCollection('api_keys', ApiKeySchema);
  const apiKeyCol = await getCollection('api_keys');
  for (const key of spec.apiKeys) {
    const row = ApiKeySchema.parse({ ...key, createdAt: TS, updatedAt: TS });
    await apiKeyCol.insertOne(row as never);
  }
  for (const profile of spec.connectionProfiles) {
    const { id, ...rest } = profile as Record<string, unknown>;
    await repos.connections.create({ userId: spec.userId, ...rest } as never, {
      id,
      createdAt: TS,
      updatedAt: TS,
    } as never);
  }

  // 4. Image profiles.
  for (const ip of spec.imageProfiles) {
    const { id, ...rest } = ip as Record<string, unknown>;
    await repos.imageProfiles.create({ userId: spec.userId, ...rest } as never, {
      id,
      createdAt: TS,
      updatedAt: TS,
    } as never);
  }

  // 5. Legacy image files (avatar + story background).
  for (const f of spec.files) {
    const { id, ...rest } = f as Record<string, unknown>;
    await repos.files.create({ userId: spec.userId, linkedTo: [], tags: [], ...rest } as never, {
      id,
      createdAt: TS,
      updatedAt: TS,
    } as never);
  }

  // 6. Tags.
  for (const t of spec.tags) {
    await repos.tags.create(
      { userId: spec.userId, name: t.name, nameLower: t.name.toLowerCase(), quickHide: false } as never,
      { id: t.id, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // 7. Projects (store-backed — provisions a store in the mount-index DB).
  for (const p of spec.projects) {
    await repos.projects.create({ userId: spec.userId, name: p.name, color: p.color } as never, {
      id: p.id,
      createdAt: TS,
      updatedAt: TS,
    } as never);
  }

  // 8. Characters (each mints a linked vault); Aria carries a default systemPrompt.
  for (const c of spec.characters) {
    await repos.characters.create(
      {
        name: c.name,
        userId: spec.userId,
        description: c.description ?? null,
        aliases: c.aliases ?? [],
        controlledBy: c.controlledBy,
        talkativeness: c.talkativeness,
        defaultConnectionProfileId: c.connectionProfileId,
        ...(c.defaultImageId !== undefined ? { defaultImageId: c.defaultImageId } : {}),
      } as never,
      { id: c.id, createdAt: TS, updatedAt: TS } as never,
    );
    if (c.systemPrompt) {
      await repos.characters.addSystemPrompt(c.id, {
        name: c.systemPrompt.name,
        prompt: c.systemPrompt.prompt,
        isDefault: c.systemPrompt.isDefault,
      } as never);
    }
  }

  // 9. Chats + messages + per-chat rendered markdown / chunks.
  for (const chat of spec.chats) {
    const participants = chat.participants.map((p) => ({
      id: p.id,
      type: 'CHARACTER',
      characterId: p.character,
      controlledBy: p.controlledBy,
      status: 'active',
      ...(p.connectionProfileId !== undefined ? { connectionProfileId: p.connectionProfileId } : {}),
      ...(p.imageProfileId !== undefined ? { imageProfileId: p.imageProfileId } : {}),
      ...(p.talkativeness !== undefined ? { talkativeness: p.talkativeness } : {}),
      createdAt: TS,
      updatedAt: TS,
    }));
    await repos.chats.create(
      {
        userId: spec.userId,
        title: chat.title,
        participants,
        chatType: chat.chatType,
        contextSummary: null,
        ...(chat.projectId !== undefined ? { projectId: chat.projectId } : {}),
        ...(chat.tags !== undefined ? { tags: chat.tags } : {}),
        ...(chat.storyBackgroundImageId !== undefined
          ? { storyBackgroundImageId: chat.storyBackgroundImageId }
          : {}),
      } as never,
      { id: chat.id, createdAt: TS, updatedAt: TS } as never,
    );
    if (chat.messages.length > 0) {
      await repos.chats.addMessages(
        chat.id,
        chat.messages.map((m) => ({
          id: m.id,
          type: 'message',
          role: m.role,
          content: m.content,
          participantId: m.participantId ?? undefined,
          createdAt: TS,
          attachments: [],
          ...(m.provider !== undefined ? { provider: m.provider } : {}),
          ...(m.modelName !== undefined ? { modelName: m.modelName } : {}),
          ...(m.tokenCount !== undefined ? { tokenCount: m.tokenCount } : {}),
          ...(m.promptTokens !== undefined ? { promptTokens: m.promptTokens } : {}),
          ...(m.completionTokens !== undefined ? { completionTokens: m.completionTokens } : {}),
          ...(m.reasoningContent !== undefined ? { reasoningContent: m.reasoningContent } : {}),
          ...(m.reasoningSegments !== undefined ? { reasoningSegments: m.reasoningSegments } : {}),
          ...(m.systemSender !== undefined ? { systemSender: m.systemSender } : {}),
          ...(m.systemKind !== undefined ? { systemKind: m.systemKind } : {}),
          ...(m.hostEvent !== undefined ? { hostEvent: m.hostEvent } : {}),
          ...(m.customAnnouncer !== undefined ? { customAnnouncer: m.customAnnouncer } : {}),
          ...(m.swipeGroupId !== undefined ? { swipeGroupId: m.swipeGroupId } : {}),
          ...(m.swipeIndex !== undefined ? { swipeIndex: m.swipeIndex } : {}),
          ...(m.confirmed !== undefined ? { confirmed: m.confirmed } : {}),
          ...(m.confirmationChecked !== undefined ? { confirmationChecked: m.confirmationChecked } : {}),
          ...(m.confirmationNotes !== undefined ? { confirmationNotes: m.confirmationNotes } : {}),
        })) as never,
      );
    }
    if (chat.renderedMarkdown !== undefined) {
      await repos.chats.update(chat.id, { renderedMarkdown: chat.renderedMarkdown } as never);
    }
    for (const chunk of chat.chunks ?? []) {
      await repos.conversationChunks.create({
        chatId: chat.id,
        interchangeIndex: chunk.interchangeIndex,
        content: chunk.content,
        messageIds: [],
        embedding: chunk.embedded ? [0.1, 0.2, 0.3, 0.4] : null,
      } as never);
    }
  }

  // 10. Memories (sourced from a group-chat assistant message).
  await ensureCollection('memories', (await import('@/lib/schemas/memory.types')).MemorySchema);
  for (const mem of spec.memories) {
    const { id, ...rest } = mem as Record<string, unknown>;
    await repos.memories.create({ userId: spec.userId, keywords: [], ...rest } as never, {
      id,
      createdAt: TS,
      updatedAt: TS,
    } as never);
  }

  // Force the empty tables the read/mutation paths touch into existence.
  await repos.backgroundJobs.findByUserId(spec.userId, 'PENDING');
  await repos.vectorIndices.findMetaByCharacterId(spec.characters[0].id);
  await repos.vectorIndices.findEntriesByCharacterId(spec.characters[0].id);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built salon fixture: main=${mainOut} mount=${mountOut} ` +
      `(${spec.characters.length} characters, ${spec.chats.length} chats)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`salon fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
