/**
 * The committed chat-DELETE server-surface fixture builder (P4.80 deliverable) —
 * the test-pepper substrate for `chat_delete_equivalence` (tier 2).
 *
 * Baked entirely through v4's REAL repositories and store-write helpers (never
 * hand-rolled SQL — `project-store-fixture-needs-real-create`), with every id
 * and timestamp pinned so both differential sides read byte-identical rows.
 *
 * ## Why a NEW pair
 *
 * The survey (2026-09-07) found no committed pair that expresses the cascade:
 * `chat-scenario-*` and `chat-admin-*` carry no `conversation_annotations`, and
 * NOTHING committed carries a participant-vault conversation summary — the one
 * thing `delete_conversation_with_vault_sweep` exists to remove. The whole point
 * of this family is a table census across what v4 DOES delete and what it
 * deliberately LEAVES, so each of those rows has to exist.
 *
 * ## What is in it, and which arm each row exists for
 *
 *   1. ONE user + a UTC `chat_settings` row (the house rule for any fixture a
 *      clock-reading path touches).
 *   2. FOUR characters, each vault provisioned by v4's own `characters.create`:
 *        - ARIA — in CHAT_FULL **and** CHAT_SHARED. Her vault therefore holds
 *                 TWO summary files, which is how "deleting one chat leaves the
 *                 other chat's summary alone" becomes measurable.
 *        - BEA  — in CHAT_FULL only. Her vault holds ONE summary file, so the
 *                 sweep must empty her `Conversation Summaries/` folder.
 *        - MOTE — CHAT_BROKEN's lone participant. Her summary is written FIRST,
 *                 then her `characterDocumentMountPointId` is repointed at a
 *                 mount point with no row: the sweep can no longer find the
 *                 file, and the delete has to succeed anyway (v4's per-character
 *                 try/catch). The orphaned document rows survive on both sides.
 *        - CLIO — a USER-controlled seat on CHAT_IMP, the `stop-impersonate`
 *                 arm's participant.
 *   3. CHAT_FULL — the happy-delete target, carrying one row in every table that
 *      names a chat, so the census can tell what the cascade reached:
 *        deleted → `chats`, `chat_messages`, `conversation_annotations`,
 *                  the two vault summary files (`doc_mount_*` rows in ARIA's and
 *                  BEA's vaults)
 *        SURVIVES → `memories` (v4's separate `DELETE /api/v1/memories?chatId=`
 *                  is the re-extract flow), `conversation_chunks`,
 *                  `chat_documents`, `files` (a chat image, linked by
 *                  `linkedTo`), `background_jobs`, `llm_logs` (a whole other
 *                  partition), `characters.avatarOverrides` (a JSON array keyed
 *                  by chatId), and the Scriptorium render document in the
 *                  General store.
 *      (`folders` is seeded but is NOT chat-keyed — v4's chats have no
 *      `folderId`. It is censused as a flat control: a table that must not move.)
 *   4. CHAT_SHARED — ARIA only, with its own summary file and its own memory.
 *   5. CHAT_BROKEN — MOTE only, the unreadable-vault sweep arm.
 *   6. CHAT_STATE — a seeded `state` bag for `?action=reset-state`.
 *   7. CHAT_IMP — CLIO impersonated (`impersonatingParticipantIds` +
 *      `activeTypingParticipantId` seeded), plus a connection profile for the
 *      `newConnectionProfileId` hand-back.
 *
 * No id is minted, so there is no `.meta.json` sidecar: every id below is pinned
 * and shared verbatim with the oracle case and the Rust differential.
 *
 * Regenerate (Node 24, from the v4 checkout — the committed .db files land in
 * place):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TZ=UTC \
 *   QT_FIXTURE_CD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/chat-delete-main.db \
 *   QT_FIXTURE_CD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/chat-delete-mount.db \
 *   QT_FIXTURE_CD_LLMLOGS=$V5W/crates/quilltap-web/tests/fixtures/chat-delete-llmlogs.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-chat-delete-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  fullSummaryBody: string;
  sharedSummaryBody: string;
  brokenSummaryBody: string;
  renderBody: string;
}

// ── Pinned entity ids (shared verbatim with the oracle case + the Rust test) ──
const ARIA = 'a2000000-0000-4000-8000-000000000001';
const BEA = 'a2000000-0000-4000-8000-000000000002';
const MOTE = 'a2000000-0000-4000-8000-000000000003';
const CLIO = 'a2000000-0000-4000-8000-000000000004';

const GENERAL_MP = '84000000-0000-4000-8000-000000000003';
/** No `doc_mount_points` row — MOTE's vault pointer is repointed here. */
const DEAD_MP = '84000000-0000-4000-8000-0000000000de';

const CHAT_FULL = 'c2000000-0000-4000-8000-000000000001';
const CHAT_SHARED = 'c2000000-0000-4000-8000-000000000002';
const CHAT_BROKEN = 'c2000000-0000-4000-8000-000000000003';
const CHAT_STATE = 'c2000000-0000-4000-8000-000000000004';
const CHAT_IMP = 'c2000000-0000-4000-8000-000000000005';

const P_FULL_ARIA = 'e2000000-0000-4000-8000-000000000001';
const P_FULL_BEA = 'e2000000-0000-4000-8000-000000000002';
const P_SHARED_ARIA = 'e2000000-0000-4000-8000-000000000011';
const P_BROKEN_MOTE = 'e2000000-0000-4000-8000-000000000021';
const P_STATE_ARIA = 'e2000000-0000-4000-8000-000000000031';
const P_IMP_ARIA = 'e2000000-0000-4000-8000-000000000041';
const P_IMP_CLIO = 'e2000000-0000-4000-8000-000000000042';

const CHAT_SETTINGS = '92000000-0000-4000-8000-000000000001';
const CONN_PROFILE = '93000000-0000-4000-8000-000000000001';
const FOLDER = '94000000-0000-4000-8000-000000000001';

const M_FULL_1 = 'd2000000-0000-4000-8000-000000000001';
const M_FULL_2 = 'd2000000-0000-4000-8000-000000000002';
const M_SHARED_1 = 'd2000000-0000-4000-8000-000000000011';
const M_BROKEN_1 = 'd2000000-0000-4000-8000-000000000021';

const ANN_1 = '95000000-0000-4000-8000-000000000001';
const ANN_2 = '95000000-0000-4000-8000-000000000002';
const ANN_SHARED = '95000000-0000-4000-8000-000000000011';
const CHUNK_1 = '96000000-0000-4000-8000-000000000001';
const DOC_1 = '97000000-0000-4000-8000-000000000001';
const MEM_FULL = '98000000-0000-4000-8000-000000000001';
const MEM_SHARED = '98000000-0000-4000-8000-000000000002';
const FILE_1 = '99000000-0000-4000-8000-000000000001';
const JOB_1 = '9a000000-0000-4000-8000-000000000001';
const LOG_1 = '9b000000-0000-4000-8000-000000000001';

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(`chat-delete fixture must be built under TZ=UTC (getTimezoneOffset=${offset})`);
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'chat-delete-web.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_CD_MAIN;
  const mountOut = process.env.QT_FIXTURE_CD_MOUNT;
  const llmLogsOut = process.env.QT_FIXTURE_CD_LLMLOGS;
  if (!mainOut || !mountOut || !llmLogsOut) {
    throw new Error(
      'QT_FIXTURE_CD_MAIN, QT_FIXTURE_CD_MOUNT and QT_FIXTURE_CD_LLMLOGS must point at the .db files',
    );
  }
  for (const out of [mainOut, mountOut, llmLogsOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cd-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.SQLITE_LLM_LOGS_PATH = llmLogsOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const {
    CharacterSchema,
    ChatMetadataSchema,
    BackgroundJobSchema,
    ConnectionProfileSchema,
  } = await import('@/lib/schemas/types');
  const { ChatSettingsSchema } = await import('@/lib/schemas/settings.types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { FolderSchema } = await import('@/lib/schemas/folder.types');
  const { FileEntrySchema } = await import('@/lib/schemas/file.types');
  const { MemorySchema } = await import('@/lib/schemas/memory.types');
  const { ChatDocumentSchema } = await import('@/lib/schemas/chat-document.types');
  const { ConversationAnnotationSchema, ConversationChunkSchema } = await import(
    '@/lib/schemas/scriptorium.types'
  );
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    GroupCharacterMemberSchema,
    GroupDocMountLinkSchema,
    ProjectDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  for (const [name, schema] of [
    ['users', UserSchema],
    ['characters', CharacterSchema],
    ['chats', ChatMetadataSchema],
    ['background_jobs', BackgroundJobSchema],
    ['chat_settings', ChatSettingsSchema],
    ['connection_profiles', ConnectionProfileSchema],
    ['folders', FolderSchema],
    ['files', FileEntrySchema],
    ['memories', MemorySchema],
    ['chat_documents', ChatDocumentSchema],
    ['conversation_annotations', ConversationAnnotationSchema],
    ['conversation_chunks', ConversationChunkSchema],
  ] as Array<[string, unknown]>) {
    await ensureCollection(name, schema as never);
  }

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  for (const [name, schema] of [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
    ['group_character_members', GroupCharacterMemberSchema],
    ['group_doc_mount_links', GroupDocMountLinkSchema],
    ['project_doc_mount_links', ProjectDocMountLinkSchema],
  ] as Array<[string, unknown]>) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  const repos = getRepositories();
  // doc_mount_blobs has hand-written DDL (BLOB column) — trigger CREATE via a read.
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');

  // 1. The user + UTC chat settings.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.chatSettings.create(
    { userId: spec.userId, timezone: 'UTC' } as never,
    { id: CHAT_SETTINGS, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Characters — each `create` provisions a real vault store.
  const mkChar = async (id: string, name: string, controlledBy: string, extra: object = {}) => {
    await repos.characters.create(
      { name, userId: spec.userId, controlledBy, ...extra } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  };
  await mkChar(ARIA, 'Aria', 'llm', {
    description: 'The Salon’s resident correspondent.',
    // A chat-keyed JSON array v4 never sweeps — the census's proof that a
    // delete does not go looking through character rows.
    avatarOverrides: [{ chatId: CHAT_FULL, imageId: FILE_1 }],
  });
  await mkChar(BEA, 'Bea', 'llm', { description: 'A signal-keeper from the lower deck.' });
  await mkChar(MOTE, 'Mote', 'llm', { description: 'Whose vault will be broken below.' });
  await mkChar(CLIO, 'Clio', 'user', { description: 'The operator’s voice at the table.' });

  // 3. A connection profile (the stop-impersonate hand-back) and a folder (a
  //    flat control table).
  await repos.connections.create(
    {
      userId: spec.userId,
      name: 'The Understudy',
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-model',
      isActive: true,
    } as never,
    { id: CONN_PROFILE, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.folders.create(
    { userId: spec.userId, path: '/post/', name: 'post', parentFolderId: null } as never,
    { id: FOLDER, createdAt: TS, updatedAt: TS } as never,
  );

  // 4. The General store — where the Scriptorium render lands (a mount-index
  //    document the delete must NOT touch).
  await repos.docMountPoints.create(
    {
      name: 'Quilltap General',
      basePath: '',
      mountType: 'database',
      storeType: 'documents',
      includePatterns: [],
      excludePatterns: [],
      enabled: true,
      lastScannedAt: null,
      scanStatus: 'idle',
      lastScanError: null,
      conversionStatus: 'idle',
      conversionError: null,
      fileCount: 0,
      chunkCount: 0,
      totalSizeBytes: 0,
    } as never,
    { id: GENERAL_MP, createdAt: TS, updatedAt: TS },
  );
  const maindb = getRawDatabase();
  if (!maindb) throw new Error('main DB handle unavailable');
  maindb.exec(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  maindb
    .prepare('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)')
    .run('generalMountPointId', GENERAL_MP);
  const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
  await writeDatabaseDocument(GENERAL_MP, 'Conversations/The Evening Post.md', spec.renderBody);

  // 5. The chats.
  const mkParticipant = (
    id: string,
    characterId: string,
    controlledBy: string,
    displayOrder: number,
    extra: object = {},
  ) => ({
    id,
    type: 'CHARACTER',
    characterId,
    controlledBy,
    status: 'active',
    isActive: true,
    displayOrder,
    connectionProfileId: null,
    hasHistoryAccess: false,
    createdAt: TS,
    updatedAt: TS,
    ...extra,
  });

  const mkChat = async (id: string, title: string, participants: object[], extra: object = {}) => {
    await repos.chats.create(
      {
        userId: spec.userId,
        title,
        chatType: 'salon',
        projectId: null,
        participants,
        tags: [],
        ...extra,
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  };

  await mkChat(CHAT_FULL, 'The Evening Post', [
    mkParticipant(P_FULL_ARIA, ARIA, 'llm', 0),
    mkParticipant(P_FULL_BEA, BEA, 'llm', 1),
  ]);
  await mkChat(CHAT_SHARED, 'A Second Sitting', [mkParticipant(P_SHARED_ARIA, ARIA, 'llm', 0)]);
  await mkChat(CHAT_BROKEN, 'The Unreadable Guest', [
    mkParticipant(P_BROKEN_MOTE, MOTE, 'llm', 0),
  ]);
  await mkChat(CHAT_STATE, 'The Room With A State', [mkParticipant(P_STATE_ARIA, ARIA, 'llm', 0)], {
    state: { lamps: 'lit', porter: 'absent' },
  });
  await mkChat(
    CHAT_IMP,
    'The Borrowed Voice',
    [mkParticipant(P_IMP_ARIA, ARIA, 'llm', 0), mkParticipant(P_IMP_CLIO, CLIO, 'user', 1)],
    { impersonatingParticipantIds: [P_IMP_CLIO], activeTypingParticipantId: P_IMP_CLIO },
  );

  // 6. Messages.
  const add = async (chatId: string, event: object) =>
    repos.chats.addMessage(chatId, event as never);
  await add(CHAT_FULL, {
    type: 'message',
    id: M_FULL_1,
    role: 'USER',
    content: 'The evening post is late again.',
    createdAt: '2026-05-01T10:00:00.000Z',
    attachments: [],
  });
  await add(CHAT_FULL, {
    type: 'message',
    id: M_FULL_2,
    role: 'ASSISTANT',
    content: 'Aria sets down her pen. "Late, and the lamps unlit besides."',
    participantId: P_FULL_ARIA,
    createdAt: '2026-05-01T10:01:00.000Z',
    attachments: [],
  });
  await add(CHAT_SHARED, {
    type: 'message',
    id: M_SHARED_1,
    role: 'USER',
    content: 'A second sitting, then.',
    createdAt: '2026-05-01T11:00:00.000Z',
    attachments: [],
  });
  await add(CHAT_BROKEN, {
    type: 'message',
    id: M_BROKEN_1,
    role: 'USER',
    content: 'Mote says nothing at all.',
    createdAt: '2026-05-01T12:00:00.000Z',
    attachments: [],
  });

  // 7. The rows the cascade DELETES besides the chat: annotations on two chats,
  //    so a swept chat proves the sweep is chat-scoped rather than table-wide.
  const ann = async (id: string, chatId: string, index: number, name: string, content: string) =>
    repos.conversationAnnotations.create(
      { chatId, messageIndex: index, characterName: name, content } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  await ann(ANN_1, CHAT_FULL, 0, 'Aria', 'She reads the docket twice.');
  await ann(ANN_2, CHAT_FULL, 1, 'Bea', 'Bea has heard this complaint before.');
  await ann(ANN_SHARED, CHAT_SHARED, 0, 'Aria', 'A second sitting is not a second chance.');

  // 8. The rows that SURVIVE.
  await repos.conversationChunks.create(
    {
      chatId: CHAT_FULL,
      interchangeIndex: 0,
      content: 'USER: The evening post is late again.',
      participantNames: ['Aria'],
      messageIds: [M_FULL_1, M_FULL_2],
      embedding: null,
    } as never,
    { id: CHUNK_1, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.chatDocuments.create(
    { chatId: CHAT_FULL, filePath: 'Notes/docket.md', scope: 'general', isActive: true } as never,
    { id: DOC_1, createdAt: TS, updatedAt: TS } as never,
  );
  const mem = async (id: string, chatId: string, content: string) =>
    repos.memories.create(
      {
        characterId: ARIA,
        chatId,
        content,
        summary: content,
        keywords: [],
        tags: [],
        importance: 0.5,
        embedding: null,
        source: 'AUTO',
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  await mem(MEM_FULL, CHAT_FULL, 'The post is chronically late.');
  await mem(MEM_SHARED, CHAT_SHARED, 'A second sitting was arranged.');
  await repos.files.create(
    {
      userId: spec.userId,
      sha256: 'a'.repeat(64),
      originalFilename: 'the-docket.png',
      mimeType: 'image/png',
      size: 128,
      linkedTo: [CHAT_FULL],
      source: 'GENERATED',
      category: 'IMAGE',
      tags: [],
    } as never,
    { id: FILE_1, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.backgroundJobs.create(
    {
      userId: spec.userId,
      type: 'MEMORY_EXTRACTION',
      status: 'PENDING',
      payload: { chatId: CHAT_FULL },
      priority: 5,
      attempts: 0,
      maxAttempts: 3,
      lastError: null,
      scheduledAt: TS,
      startedAt: null,
      completedAt: null,
    } as never,
    { id: JOB_1, createdAt: TS, updatedAt: TS } as never,
  );

  // 9. The participant-vault summary files — the ONE thing the sweep exists to
  //    remove. Written through v4's REAL bridge so both sides read v4's bytes.
  const { writeConversationSummaryToVaults } = await import(
    '@/lib/file-storage/conversation-summary-vault-bridge'
  );
  await writeConversationSummaryToVaults({
    chatId: CHAT_FULL,
    chatTitle: 'The Evening Post',
    summary: spec.fullSummaryBody,
    summaryGeneration: 1,
    participantCharacterIds: [ARIA, BEA],
    messageCount: 2,
    firstMessageAt: '2026-05-01T10:00:00.000Z',
    lastMessageAt: '2026-05-01T10:01:00.000Z',
    updatedAt: TS,
  } as never);
  await writeConversationSummaryToVaults({
    chatId: CHAT_SHARED,
    chatTitle: 'A Second Sitting',
    summary: spec.sharedSummaryBody,
    summaryGeneration: 1,
    participantCharacterIds: [ARIA],
    messageCount: 1,
    firstMessageAt: '2026-05-01T11:00:00.000Z',
    lastMessageAt: '2026-05-01T11:00:00.000Z',
    updatedAt: TS,
  } as never);
  await writeConversationSummaryToVaults({
    chatId: CHAT_BROKEN,
    chatTitle: 'The Unreadable Guest',
    summary: spec.brokenSummaryBody,
    summaryGeneration: 1,
    participantCharacterIds: [MOTE],
    messageCount: 1,
    firstMessageAt: '2026-05-01T12:00:00.000Z',
    lastMessageAt: '2026-05-01T12:00:00.000Z',
    updatedAt: TS,
  } as never);

  // 10. An llm_logs row for CHAT_FULL — a whole other partition the delete has
  //     no reach into. (Its DDL runs lazily on the first write.)
  const created = await repos.llmLogs.create(
    {
      userId: spec.userId,
      chatId: CHAT_FULL,
      type: 'CHAT_MESSAGE',
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-model',
      request: { messages: [], messageCount: 0, temperature: null, maxTokens: null },
      response: { content: '', contentLength: 0, error: null, finishReason: 'stop' },
    } as never,
    { id: LOG_1, createdAt: TS } as never,
  );
  if (!created) throw new Error('llm_logs seed row failed');
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );

  // 11. Break MOTE's vault LAST, so her summary is already on disk: repoint the
  //     keystone at a mount point that has no `doc_mount_points` row.
  maindb
    .prepare('UPDATE "characters" SET "characterDocumentMountPointId" = ? WHERE "id" = ?')
    .run(DEAD_MP, MOTE);

  // 12. Pin the live-clock stamps the store writes left behind, so a rebuild is
  //     byte-reproducible (the `build-attach-file-fixture` precedent).
  for (const t of ['doc_mount_file_links', 'doc_mount_files', 'doc_mount_documents']) {
    midb.prepare(`UPDATE "${t}" SET "createdAt" = ?, "updatedAt" = ?`).run(TS, TS);
  }
  midb.prepare('UPDATE "doc_mount_folders" SET "createdAt" = ?, "updatedAt" = ?').run(TS, TS);

  closeLLMLogsSQLiteClient();
  closeMountIndexSQLiteClient();
  await closeDatabase();

  for (const out of [mainOut, mountOut, llmLogsOut]) {
    for (const suffix of ['-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
  }
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(
    `built chat-delete fixtures: main=${mainOut} mount=${mountOut} llmlogs=${llmLogsOut}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-delete fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
