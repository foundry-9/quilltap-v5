/**
 * P4.6ar fixture builder (lane A) — the committed test-pepper substrate shared by
 * the LLM-Inspector + default-aesthetics differentials:
 *
 *   - `llm_logs_routes_equivalence`        (units 1–2: the llm-logs read surface)
 *   - `image_aesthetics_routes_equivalence` (unit 3: system/image-aesthetics)
 *
 * Everything is baked via v4's REAL repos so the DDL + row shapes are
 * production-identical.
 *
 * ── FOUR FILES, ONE FAMILY ───────────────────────────────────────────────────
 *   - `inspector-main.db`         — users / chats+messages / characters, and the
 *                                   `instance_settings.generalMountPointId`
 *                                   pointer at the Quilltap General store.
 *   - `inspector-mount.db`        — the mount-index sibling: the character's vault
 *                                   (minted by the REAL `repos.characters.create`
 *                                   — the P4.6ab lesson) plus the Quilltap General
 *                                   store carrying `lantern-aesthetics.md` and
 *                                   deliberately NO `aurora-aesthetics.md` (so the
 *                                   present-file and absent-file GET arms are both
 *                                   covered with no mutation ordering).
 *   - `inspector-llm.db`          — the llm-logs PARTITION (v4 routes this repo to
 *                                   its own database), carrying the log corpus.
 *   - `inspector-nostore-main.db` — a byte-copy of the main DB with the
 *                                   `generalMountPointId` row DELETED. That
 *                                   pointer is a singleton in a key/value table,
 *                                   so the "Quilltap General not provisioned"
 *                                   arms (GET → `{content:''}`, PUT → serverError)
 *                                   cannot coexist with the provisioned arms in
 *                                   one main file. A second main file is the
 *                                   honest way to stage both without either side
 *                                   mutating its copy mid-case.
 *
 * ── THE LOG CORPUS ───────────────────────────────────────────────────────────
 * All twelve types the SPA's `LLMInspectorEntry` badge table enumerates, across
 * message-linked / chat-linked / character-linked / standalone rows, with and
 * without `usage` / `cacheUsage` / `durationMs`, one error-response row, and one
 * row owned by a SECOND user (the item routes have NO ownership check — the
 * cross-user read is a real, faithful arm).
 *
 * Every log row gets a DISTINCT `createdAt`: v4's translated sort is
 * `ORDER BY "createdAt" DESC` with no secondary key, so distinct values are what
 * make the row order deterministic and byte-identical on both sides.
 *
 * `rawProviderUsage` is left NULL on every row on purpose — it is the one
 * open-JSON column, and `serde_json::Value` sorts object keys where v4's
 * `JSON.stringify` preserves insertion order (the standing seam recorded in
 * `db/llm_logs.rs`). Nothing in this surface needs it populated.
 *
 * Regenerate (Node 24, from the v4 checkout) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_INSP_MAIN=$W/crates/quilltap-web/tests/fixtures/inspector-main.db \
 *   QT_FIXTURE_INSP_MOUNT=$W/crates/quilltap-web/tests/fixtures/inspector-mount.db \
 *   QT_FIXTURE_INSP_LLM=$W/crates/quilltap-web/tests/fixtures/inspector-llm.db \
 *   QT_FIXTURE_INSP_NOSTORE=$W/crates/quilltap-web/tests/fixtures/inspector-nostore-main.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-inspector-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  copyFileSync,
  rmSync,
  existsSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';

interface MessageSeed {
  id: string;
  role: string;
  content: string;
  createdAt: string;
}
interface LogSeed {
  id: string;
  note?: string;
  user: 'primary' | 'other';
  type: string;
  messageId: string | null;
  chatId: string | null;
  characterId: string | null;
  provider: string;
  modelName: string;
  request: Record<string, unknown>;
  response: Record<string, unknown>;
  usage?: Record<string, unknown>;
  cacheUsage?: Record<string, unknown>;
  durationMs?: number;
  createdAt: string;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  otherUserId: string;
  connectionProfileId: string;
  characterId: string;
  chatId: string;
  generalMountPointId: string;
  messages: MessageSeed[];
  lanternAesthetic: string;
  logs: LogSeed[];
}

function cipherDriverPath(): string {
  return join(process.cwd(), 'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers');
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'inspector-web.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_INSP_MAIN;
  const mountOut = process.env.QT_FIXTURE_INSP_MOUNT;
  const llmOut = process.env.QT_FIXTURE_INSP_LLM;
  const nostoreOut = process.env.QT_FIXTURE_INSP_NOSTORE;
  if (!mainOut || !mountOut || !llmOut || !nostoreOut) {
    throw new Error(
      'QT_FIXTURE_INSP_{MAIN,MOUNT,LLM,NOSTORE} must point at the .db files to write',
    );
  }
  for (const out of [mainOut, mountOut, llmOut, nostoreOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-insp-fixture-'));
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
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema, ChatMetadataSchema } = await import('@/lib/schemas/types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
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

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('chats', ChatMetadataSchema);

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
  // doc_mount_blobs has hand-written DDL (BLOB column) — trigger CREATE via a read.
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');

  // 1. The two users. The second exists only to own one log row (the item routes
  //    have no ownership check — that arm must be a real cross-user read).
  for (const [id, username] of [
    [spec.userId, 'friday'],
    [spec.otherUserId, 'stranger'],
  ] as const) {
    await repos.users.create(
      { username, email: null, name: username } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // 2. The connection profile the character names.
  await repos.connections.create(
    {
      userId: spec.userId,
      name: 'Fixture Cheap',
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-cheap-model',
      baseUrl: 'http://127.0.0.1:1/v1',
      apiKeyId: null,
      isCheap: true,
    } as never,
    { id: spec.connectionProfileId, createdAt: TS, updatedAt: TS } as never,
  );

  // 3. The character (REAL create → mints its vault into the mount-index sibling).
  await repos.characters.create(
    {
      name: 'Lorian',
      userId: spec.userId,
      title: null,
      description: 'Lorian, of the fixture.',
      personality: null,
      aliases: [],
      controlledBy: 'llm',
      talkativeness: 0.5,
      defaultConnectionProfileId: spec.connectionProfileId,
      isFavorite: false,
      npc: false,
      tags: [],
    } as never,
    { id: spec.characterId, createdAt: TS, updatedAt: TS } as never,
  );

  // 4. The chat + its messages (the `includeMessages` union reads these ids).
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'A Lesson in Lift',
      chatType: 'salon',
      contextSummary: null,
      participants: [],
      tags: [],
    } as never,
    { id: spec.chatId, createdAt: TS, updatedAt: TS } as never,
  );
  for (const m of spec.messages) {
    await repos.chats.addMessage(spec.chatId, {
      type: 'message',
      id: m.id,
      role: m.role,
      content: m.content,
      attachments: [],
      createdAt: m.createdAt,
    } as never);
  }

  // 5. The Quilltap General store + its pointer, then the ONE seeded aesthetic
  //    file (lantern present, aurora deliberately absent).
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
    { id: spec.generalMountPointId, createdAt: TS, updatedAt: TS } as never,
  );
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    spec.generalMountPointId,
  ]);
  {
    const content = spec.lanternAesthetic;
    await repos.docMountFileLinks.linkDocumentContent({
      mountPointId: spec.generalMountPointId,
      relativePath: 'lantern-aesthetics.md',
      fileName: 'lantern-aesthetics.md',
      folderId: null,
      fileType: 'markdown',
      content,
      contentSha256: createHash('sha256').update(content, 'utf-8').digest('hex'),
      plainTextLength: content.length,
      fileSizeBytes: Buffer.byteLength(content, 'utf-8'),
    } as never);
  }

  // 6. The log corpus, through the REAL LLMLogsRepository — whose overridden
  //    getCollection() lazily CREATE-TABLEs and writes into the llm-logs
  //    PARTITION (SQLITE_LLM_LOGS_PATH), not the main DB.
  const userIdOf = (u: LogSeed['user']) => (u === 'other' ? spec.otherUserId : spec.userId);
  for (const log of spec.logs) {
    await repos.llmLogs.create(
      {
        userId: userIdOf(log.user),
        type: log.type,
        messageId: log.messageId,
        chatId: log.chatId,
        characterId: log.characterId,
        autonomousRunId: null,
        provider: log.provider,
        modelName: log.modelName,
        request: log.request,
        response: log.response,
        ...(log.usage !== undefined ? { usage: log.usage } : {}),
        ...(log.cacheUsage !== undefined ? { cacheUsage: log.cacheUsage } : {}),
        ...(log.durationMs !== undefined ? { durationMs: log.durationMs } : {}),
      } as never,
      { id: log.id, createdAt: log.createdAt, updatedAt: log.createdAt } as never,
    );
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  // 7. The unprovisioned-store sibling: a byte-copy of the finished main DB with
  //    the singleton pointer removed. Opened directly with the cipher driver
  //    (ChaCha20/sqleet; raw-hex key — the KDF is skipped, see CLAUDE.md).
  copyFileSync(mainOut, nostoreOut);
  {
    const Database = createRequire(import.meta.url)(cipherDriverPath());
    const hex = Buffer.from(spec.testPepperBase64, 'base64').toString('hex');
    const db = new Database(nostoreOut);
    db.pragma(`key = "x'${hex}'"`);
    db.prepare('DELETE FROM "instance_settings" WHERE "key" = ?').run('generalMountPointId');
    db.close();
  }

  process.stderr.write(
    `built inspector fixture: ${mainOut} (+${mountOut}, ${llmOut}, ${nostoreOut}) — ` +
      `2 users, 1 character, 1 chat, ${spec.messages.length} messages, ` +
      `${spec.logs.length} llm-logs rows\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`inspector fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
