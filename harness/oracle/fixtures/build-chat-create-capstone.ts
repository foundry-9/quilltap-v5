/**
 * Fixture builder for the chat-creation CAPSTONE tier-3 differential (P4.4 unit 2,
 * sub-unit 8) — proves `services::chat_create::handle_create` vs v4's REAL
 * `handleCreate` (app/api/v1/chats/route.ts POST).
 *
 * Bakes (via v4's REAL repositories, all ids/timestamps pinned):
 *   - one user (SINGLE_USER_ID),
 *   - a default chat_settings row,
 *   - three llm-controlled characters (Aria: firstMessage present, talkativeness 1;
 *     Cleo: firstMessage present, talkativeness 0; Bram: firstMessage EMPTY —
 *     the generated-greeting ladder), each with a real linked vault,
 *   - two connection profiles (one apiKeyId=null, one with an apiKeyId),
 *   - one api key.
 * Both v4's real `handleCreate` and the Rust port read a FRESH COPY of the SAME
 * baked fixture per case, so the created-chat ids/timestamps are minted afresh on
 * each side and remapped in the diff.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CC_MAIN=/tmp/qt-cc-main.db QT_FIXTURE_CC_MOUNT=/tmp/qt-cc-mount.db \
 *   QT_FIXTURE_CC_LLM=/tmp/qt-cc-llm.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-chat-create-capstone.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  connectionProfiles: Array<Record<string, unknown>>;
  apiKeys: Array<Record<string, unknown>>;
  characters: Record<string, Record<string, unknown>>;
}

const TS = '2026-02-01T00:00:00.000Z';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'chat-create-capstone.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_CC_MAIN;
  const mountOut = process.env.QT_FIXTURE_CC_MOUNT;
  const llmOut = process.env.QT_FIXTURE_CC_LLM;
  if (!mainOut || !mountOut || !llmOut) {
    throw new Error(
      'QT_FIXTURE_CC_MAIN, QT_FIXTURE_CC_MOUNT, and QT_FIXTURE_CC_LLM must all point at the .db files to write',
    );
  }
  for (const out of [mainOut, mountOut, llmOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cc-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.SQLITE_LLM_LOGS_PATH = llmOut;
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
  const { getRawLLMLogsDatabase, closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { LLMLogSchema } = await import('@/lib/schemas/llm-log.types');
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

  // Materialize the llm-logs table (the greeting/outfit logLLMCall partition).
  const lldb = getRawLLMLogsDatabase();
  if (!lldb) throw new Error('llm-logs DB handle unavailable');
  for (const sql of generateDDL('llm_logs', LLMLogSchema as never)) lldb.exec(sql);

  const repos = getRepositories();

  // 1. The single user.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Default chat settings (creates the table + a default row).
  await repos.chatSettings.updateForUser(spec.userId, {} as never);

  // 3. Api keys (pinned ids via a direct collection insert — createApiKey mints
  // its own id), then connection profiles.
  const { ApiKeySchema } = await import('@/lib/schemas/profile.types');
  await ensureCollection('api_keys', ApiKeySchema);
  const apiKeyCol = await getCollection('api_keys');
  for (const key of spec.apiKeys) {
    const row = ApiKeySchema.parse({ ...key, createdAt: TS, updatedAt: TS });
    await apiKeyCol.insertOne(row as never);
  }

  for (const profile of spec.connectionProfiles) {
    const { id, ...rest } = profile as Record<string, unknown>;
    await repos.connections.create(rest as never, {
      id,
      createdAt: TS,
      updatedAt: TS,
    } as never);
  }

  // 4. Characters (each mints a linked vault).
  for (const key of Object.keys(spec.characters)) {
    const char = spec.characters[key];
    await repos.characters.create(char as never, {
      id: char.id,
      createdAt: TS,
      updatedAt: TS,
    } as never);
  }

  // 5. Materialize the empty tables the create flow writes to (so the Rust port —
  // whose repos never CREATE tables — has them to write/dump). Use the exact
  // schemas v4's repos use, so the DDL matches the tier-2-verified surface.
  const { ChatMetadataBaseSchema } = await import('@/lib/schemas/chat.types');
  const { ChatMessageRowSchema } = await import(
    '@/lib/database/repositories/chats-messages.ops'
  );
  const { ProjectSchema, BackgroundJobSchema } = await import('@/lib/schemas/types');
  await ensureCollection('chats', ChatMetadataBaseSchema);
  await ensureCollection('chat_messages', ChatMessageRowSchema);
  await ensureCollection('projects', ProjectSchema);
  await ensureCollection('background_jobs', BackgroundJobSchema);

  closeMountIndexSQLiteClient();
  closeLLMLogsSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built chat-create-capstone fixtures: main=${mainOut} mount=${mountOut} llm=${llmOut}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-create-capstone fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
