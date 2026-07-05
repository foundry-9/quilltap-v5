/**
 * Fixture builder for the W4.1d5 `state` + `run_sql` tool differential
 * (state_sql_tools_equivalence).
 *
 * Bakes a shared state across the THREE databases (main + mount-index + llm-logs)
 * so BOTH v4 and the Rust port read it identically; each case runs on its own
 * fresh copy (state WRITES). Seeds, via the REAL repos:
 *   - MAIN: a project P (store-backed, initial `state.json` set through the REAL
 *     projects.update overlay), a chat scoped to project P (userId=U, initial
 *     `state`), a solo chat (no project, initial `state`), and one memory with a
 *     Float32 `embedding` BLOB (for run_sql's blob-sanitize case on main).
 *   - MOUNT-INDEX: P's "Project Files: <name>" store (minted by projects.create).
 *   - LLM-LOGS: one llm_logs row (for run_sql's llm-logs target).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TMP_MAIN=/tmp/qt-state-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-state-mount.db \
 *   QT_FIXTURE_TMP_LLM=/tmp/qt-state-llm.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-state-sql-tools-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  chatProjectId: string;
  chatSoloId: string;
  projectId: string;
  projectName: string;
  charAId: string;
  seedTimestamp: string;
  chatState: Record<string, unknown>;
  chatSoloState: Record<string, unknown>;
  projectState: Record<string, unknown>;
  memoryId: string;
  memoryEmbedding: number[];
  llmLogId: string;
}

const PINNED_TS = '2026-02-01T00:00:00.000Z';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'state-sql-tools.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_TMP_MAIN;
  const mountOut = process.env.QT_FIXTURE_TMP_MOUNT;
  const llmOut = process.env.QT_FIXTURE_TMP_LLM;
  if (!mainOut || !mountOut || !llmOut) {
    throw new Error('QT_FIXTURE_TMP_MAIN, QT_FIXTURE_TMP_MOUNT, QT_FIXTURE_TMP_LLM must all point at .db files to write');
  }
  for (const out of [mainOut, mountOut, llmOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-state-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.SQLITE_LLM_LOGS_PATH = llmOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema, ProjectSchema, ChatMetadataSchema, MemorySchema } = await import(
    '@/lib/schemas/types'
  );
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
  const { LLMLogsRepository } = await import(
    '@/lib/database/repositories/llm-logs.repository'
  );

  await initializeDatabase();
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('projects', ProjectSchema);
  await ensureCollection('chats', ChatMetadataSchema);
  await ensureCollection('memories', MemorySchema);

  // Materialize the mount-index DDL up front (projects.create scaffolds the store
  // via linkDocumentContent, which needs these tables).
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
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }

  const repos = getRepositories();

  // 1. Project P (mints its store) + set the initial state.json via the overlay.
  await repos.projects.create(
    {
      name: spec.projectName,
      description: null,
      instructions: null,
      characterRoster: [],
      allowAnyCharacter: false,
    } as never,
    { id: spec.projectId, createdAt: PINNED_TS, updatedAt: PINNED_TS } as never,
  );
  await repos.projects.update(spec.projectId, { state: spec.projectState } as never);

  // 2. Chats (real create) with initial state. Chat scoped to project P; solo chat.
  const participant = (order: number) => ({
    id: `fa000000-0000-4000-8000-00000000000${order}`,
    type: 'CHARACTER',
    characterId: spec.charAId,
    controlledBy: 'llm',
    displayOrder: 0,
    isActive: true,
    status: 'active',
    hasHistoryAccess: false,
    createdAt: spec.seedTimestamp,
    updatedAt: spec.seedTimestamp,
  });
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Project Chat',
      participants: [participant(1)],
      projectId: spec.projectId,
      state: spec.chatState,
    } as never,
    { id: spec.chatProjectId, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp } as never,
  );
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Solo Chat',
      participants: [participant(2)],
      state: spec.chatSoloState,
    } as never,
    { id: spec.chatSoloId, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp } as never,
  );

  // 3. One memory with an embedding BLOB (run_sql blob-sanitize on main).
  await repos.memories.create(
    {
      characterId: spec.charAId,
      aboutCharacterId: null,
      chatId: null,
      projectId: null,
      content: 'A star chart of the northern sky.',
      summary: 'Star chart.',
      keywords: [],
      tags: [],
      importance: 1,
      embedding: new Float32Array(spec.memoryEmbedding),
      source: 'MANUAL',
      witnessedContext: null,
      sourceMessageId: null,
      lastAccessedAt: null,
      reinforcementCount: 1,
      lastReinforcedAt: null,
      relatedMemoryIds: [],
      reinforcedImportance: 1,
    } as never,
    { id: spec.memoryId, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp },
  );

  // 4. One llm_logs row (run_sql llm-logs target). Overridden getCollection() on
  // the repo routes to the llm-logs DB and creates the table on first access.
  const llmRepo = new LLMLogsRepository();
  await llmRepo.create(
    {
      userId: spec.userId,
      type: 'CHAT_MESSAGE',
      messageId: null,
      chatId: null,
      characterId: null,
      autonomousRunId: null,
      provider: 'anthropic',
      modelName: 'claude-opus-4-8',
      request: { messageCount: 1, messages: [], temperature: 0.7, maxTokens: 1024, toolCount: 0 },
      response: { content: 'ok', contentLength: 2, finishReason: 'end_turn' },
      usage: { promptTokens: 10, completionTokens: 5, totalTokens: 15 },
      cacheUsage: null,
      requestHashes: null,
      rawProviderUsage: null,
      durationMs: 120,
      estimatedCost: null,
      status: 'success',
      errorMessage: null,
    } as never,
    { id: spec.llmLogId, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp },
  );

  await closeDatabase();

  process.stderr.write(
    `built state-sql fixtures: main=${mainOut} mount=${mountOut} llm=${llmOut}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`state-sql fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
