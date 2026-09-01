/**
 * Read-differential fixture builder — the THREE databases (main + mount-index +
 * llm-logs) for the `self_inventory` tool handler (W4.1d).
 *
 * Seeds, through v4's REAL repos:
 *   - Four full-vault characters (`repos.characters.create`, pinned ids) — Aria
 *     (self, canBeCarina), Cass (user persona), Beck (present LLM peer), Dara (a
 *     group-only member, canBeCarina). Each mints a linked vault mount + scaffolds
 *     the preset folders/files.
 *   - A connection profile + a roleplay template (`repos.connections.create` /
 *     `repos.roleplayTemplates.create`).
 *   - A real project + a real group (`repos.projects.create` / `repos.groups.create`
 *     — each provisions an official store) and two group memberships (Aria, Dara)
 *     via the REAL `GroupCharacterMembersRepository`.
 *   - A chat (`repos.chats.create`) with three present participants
 *     (Aria=llm/self, Cass=user, Beck=llm), `allowCrossCharacterVaultReads=true`,
 *     `projectId`, `roleplayTemplateId`.
 *   - Aria's memories (`repos.memories.create`, pinned) at a spread of importances
 *     (three ≥ 0.7).
 *   - Two chat_documents for the chat (`repos.chatDocuments.create`).
 *   - An llm_logs row for the chat in the LLM-LOGS sibling DB (real
 *     `LLMLogsRepository`, with a usage object) so the lastTurn section resolves.
 *
 * Both the oracle and the Rust port then READ a COPY of the SAME baked fixtures, so
 * every cell is identical on both sides. The ONLY genuinely nondeterministic value
 * is the physicalDescription base's createdAt/updatedAt, minted at read time — but
 * self_inventory's prompt/character reads never surface a physicalDescription (the
 * seed characters carry none), and the sections it does surface are all pinned, so
 * the differential runs with NO normalization.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MAIN=/tmp/qt-selfinv-main.db \
 *   QT_FIXTURE_MOUNT=/tmp/qt-selfinv-mount.db \
 *   QT_FIXTURE_LLMLOGS=/tmp/qt-selfinv-llmlogs.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-self-inventory-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CharacterSpec {
  id: string;
  name: string;
  description: string | null;
  identity: string | null;
  personality?: string;
  canBeCarina: boolean;
  /** P4.D65: set only on the archived member of the cast (see `makeChar`). */
  archivedAt?: string;
}

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  aria: CharacterSpec;
  beck: CharacterSpec;
  cass: CharacterSpec;
  dara: CharacterSpec;
  edda: CharacterSpec;
  connectionProfile: { id: string; name: string; provider: string; modelName: string };
  roleplayTemplate: { id: string; name: string; systemPrompt: string };
  project: { id: string; name: string };
  group: { id: string; name: string };
  chat: { id: string; title: string; touchedUpdatedAt: string; participants: Array<Record<string, unknown>> };
  llmLog: {
    id: string;
    provider: string;
    modelName: string;
    promptTokens: number;
    completionTokens: number;
    totalTokens: number;
    createdAt: string;
  };
  memories: Array<{ id: string; importance: number }>;
  chatDocuments: Array<{
    id: string;
    filePath: string;
    scope: string;
    mountPoint: string | null;
    displayTitle: string | null;
  }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'self-inventory.json'), 'utf8')) as Spec;

  const outMain = process.env.QT_FIXTURE_MAIN;
  const outMount = process.env.QT_FIXTURE_MOUNT;
  const outLlm = process.env.QT_FIXTURE_LLMLOGS;
  if (!outMain || !outMount || !outLlm) {
    throw new Error('QT_FIXTURE_MAIN, QT_FIXTURE_MOUNT and QT_FIXTURE_LLMLOGS must be set');
  }
  for (const base of [outMain, outMount, outLlm]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = base + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-selfinv-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.SQLITE_LLM_LOGS_PATH = outLlm;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, ensureCollection, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { GroupCharacterMembersRepository } = await import(
    '@/lib/database/repositories/group-character-members.repository'
  );
  const { LLMLogsRepository } = await import(
    '@/lib/database/repositories/llm-logs.repository'
  );
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const { UserSchema, CharacterSchema } = await import('@/lib/schemas/types');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    ProjectDocMountLinkSchema,
    GroupDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();

  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);

  // Materialize the mount-index store tables (the ports never issue CREATE TABLE).
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
    ['project_doc_mount_links', ProjectDocMountLinkSchema],
    ['group_doc_mount_links', GroupDocMountLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
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

  const repos = getRepositories();

  // 1. User.
  await repos.users.create(
    { username: 'reader', name: 'Reader' } as never,
    { id: spec.userId, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp } as never
  );

  // 2. Characters (each provisions + scaffolds a vault). canBeCarina is a DB
  // (slim) column; description/identity/personality are vault-managed and hydrate
  // back on read.
  const makeChar = async (c: CharacterSpec): Promise<void> => {
    await repos.characters.create(
      {
        name: c.name,
        userId: spec.userId,
        description: c.description,
        identity: c.identity,
        personality: c.personality ?? null,
        controlledBy: c.name === 'Cass' ? 'user' : 'llm',
        canBeCarina: c.canBeCarina,
      } as never,
      { id: c.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
    // P4.D65 (the banked P4.D63 unit-4 arm): the tombstone goes on by raw
    // UPDATE, because `validateCharacterArchivePatch` sanctions exactly one
    // patch on an archived row and the repository would refuse this one. The
    // character keeps her scaffolded vault, exactly as a real archived one
    // does — `buildCarinaSection` must exclude her on the FLAG.
    if (c.archivedAt) {
      await rawQuery('UPDATE characters SET archivedAt = ? WHERE id = ?', [c.archivedAt, c.id]);
    }
  };
  await makeChar(spec.aria);
  await makeChar(spec.beck);
  await makeChar(spec.cass);
  await makeChar(spec.dara);
  await makeChar(spec.edda);

  // 3. Connection profile + roleplay template.
  await repos.connections.create(
    {
      userId: spec.userId,
      name: spec.connectionProfile.name,
      provider: spec.connectionProfile.provider,
      modelName: spec.connectionProfile.modelName,
    } as never,
    {
      id: spec.connectionProfile.id,
      createdAt: spec.seedTimestamp,
      updatedAt: spec.seedTimestamp,
    }
  );
  await repos.roleplayTemplates.create(
    { name: spec.roleplayTemplate.name, systemPrompt: spec.roleplayTemplate.systemPrompt } as never,
    { id: spec.roleplayTemplate.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
  );

  // 4. Project + group (each provisions an official store). P4.D103 (v4
  // `8f868109`): both carry `instructions`, so the `buildPromptSection` report
  // reconstructs the standing-instructions section a live turn would carry —
  // the project's body holds a `{{char}}` placeholder so the template
  // processing is observable in the output.
  await repos.projects.create(
    {
      userId: spec.userId,
      name: spec.project.name,
      instructions: spec.project.instructions,
    } as never,
    { id: spec.project.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp } as never
  );
  await repos.groups.create(
    { name: spec.group.name, instructions: spec.group.instructions } as never,
    { id: spec.group.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp } as never
  );

  // 5. Group memberships (Aria + Dara), pinned ids.
  const members = new GroupCharacterMembersRepository();
  await members.create(
    { groupId: spec.group.id, characterId: spec.aria.id },
    { id: 'a0a0a0a0-0000-4000-8000-000000000031', createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
  );
  await members.create(
    { groupId: spec.group.id, characterId: spec.dara.id },
    { id: 'a0a0a0a0-0000-4000-8000-000000000032', createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
  );

  // 6. Chat with three present participants.
  await repos.chats.create(
    {
      userId: spec.userId,
      title: spec.chat.title,
      participants: spec.chat.participants,
      projectId: spec.project.id,
      roleplayTemplateId: spec.roleplayTemplate.id,
      allowCrossCharacterVaultReads: true,
    } as never,
    { id: spec.chat.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
  );

  // 7. Aria's memories.
  for (const m of spec.memories) {
    await repos.memories.create(
      {
        characterId: spec.aria.id,
        aboutCharacterId: null,
        chatId: null,
        projectId: null,
        content: `memory ${m.id}`,
        summary: `summary ${m.id}`,
        keywords: [],
        tags: [],
        importance: m.importance,
        embedding: null,
        source: 'AUTO',
        witnessedContext: null,
        sourceMessageId: null,
        lastAccessedAt: null,
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: [],
        reinforcedImportance: m.importance,
      } as never,
      { id: m.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  // 8. chat_documents.
  for (const d of spec.chatDocuments) {
    await repos.chatDocuments.create(
      {
        chatId: spec.chat.id,
        filePath: d.filePath,
        scope: d.scope,
        mountPoint: d.mountPoint,
        displayTitle: d.displayTitle,
        isActive: true,
      } as never,
      { id: d.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  // 9. llm_logs row (sibling DB) for the lastTurn section.
  const llmLogs = new LLMLogsRepository();
  await llmLogs.create(
    {
      userId: spec.userId,
      type: 'CHAT_MESSAGE',
      messageId: null,
      chatId: spec.chat.id,
      characterId: spec.aria.id,
      autonomousRunId: null,
      provider: spec.llmLog.provider,
      modelName: spec.llmLog.modelName,
      request: { messageCount: 1, messages: [] },
      response: { content: '', contentLength: 0, finishReason: 'stop' },
      usage: {
        promptTokens: spec.llmLog.promptTokens,
        completionTokens: spec.llmLog.completionTokens,
        totalTokens: spec.llmLog.totalTokens,
      },
      cacheUsage: null,
      rawProviderUsage: null,
      requestHashes: null,
      durationMs: null,
    } as never,
    { id: spec.llmLog.id, createdAt: spec.llmLog.createdAt, updatedAt: spec.llmLog.createdAt }
  );

  // P4.D140 (v4 `735d9408c`): the chat has NO messages, so `lastMessageAt` is
  // NULL and `chatActivityAt` must fall back to `createdAt`. Push `updatedAt`
  // well past it so the chats section's `latestActivityAt` DISCRIMINATES: the
  // retired `lastMessageAt ?? updatedAt ?? createdAt` spelling would report this
  // value, the shipped one reports `createdAt`.
  await rawQuery('UPDATE chats SET updatedAt = ? WHERE id = ?', [
    spec.chat.touchedUpdatedAt,
    spec.chat.id,
  ]);

  closeMountIndexSQLiteClient();
  closeLLMLogsSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built self-inventory fixtures: main=${outMain} mount=${outMount} llmlogs=${outLlm}\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`self-inventory fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
