/**
 * The committed Brahma Console server-surface fixture builder (P4.9I1A
 * deliverable) — the test-pepper substrate for BOTH the tier-2 CRUD/route
 * differential (`brahma_console_routes_equivalence`) and the tier-3 orchestrator
 * differential (`brahma_orchestrator_tier3_equivalence`).
 *
 * Bakes, via v4's REAL repositories (all ids/timestamps pinned so both
 * differential sides read identical rows — reads diff with no remap; only the
 * create/rename/set-model mutations and the sent messages mint fresh
 * ids/stamps, blanked by the harness):
 *   - TWO users: USER_A (the fixture owner) + USER_B (the ownership/404 arm),
 *   - ONE api key + TWO connection profiles for USER_A — P1 (isDefault, carries
 *     the key) and P2 (non-default, the set-model target). Both ANTHROPIC with a
 *     fictional model NOT in FALLBACK_PRICING, so v4's `checkModelSupportsTools`
 *     returns `true` deterministically (no pricing fetch) — matching the Rust
 *     injected `model_supports_native_tools = true`,
 *   - THREE brahma chats owned by USER_A: CHAT_A (pinned P1, WITH a 4-message
 *     transcript incl. an empty tool-turn assistant + a TOOL row — the
 *     get-messages / list-count / `[Tool Result]` history-replay arms), CHAT_B
 *     (pinned P1, empty — the delete + tier-3 send target), CHAT_C (unpinned
 *     `consoleConnectionProfileId: null` — the default-fallback resolution arm),
 *   - ONE non-brahma salon chat (pins the list filter + the `verify_brahma_chat`
 *     404 arm),
 *   - CHAT_G (pinned P1, empty — P4.79's `stream_error_mid_turn` tier-3 arm: a
 *     mid-stream provider throw, v4's `for await` propagating it to
 *     `handleBrahmaConsoleMessage`'s outer catch with nothing persisted).
 *
 * Regenerate (Node 24, from the v4 checkout) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_BRAHMA_MAIN=$W/crates/quilltap-web/tests/fixtures/brahma-main.db \
 *   QT_FIXTURE_BRAHMA_MOUNT=$W/crates/quilltap-web/tests/fixtures/brahma-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-brahma-console-web-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  userIdB: string;
  seedTimestamp: string;
}

// ── Pinned entity ids (shared verbatim with the Rust differentials) ─────────
const APIKEY = 'a0000001-0000-4000-8000-000000000001';
const P1 = 'c0000001-0000-4000-8000-000000000001'; // default, carries the key
const P2 = 'c0000001-0000-4000-8000-000000000002'; // non-default, set-model target
const P3 = 'c0000001-0000-4000-8000-000000000003'; // text-block pseudoToolMode

const CHAT_A = 'c1000000-0000-4000-8000-00000000000a'; // brahma, pinned P1, has history
const CHAT_B = 'c1000000-0000-4000-8000-00000000000b'; // brahma, pinned P1, empty
const CHAT_C = 'c1000000-0000-4000-8000-00000000000c'; // brahma, unpinned (null profile)
const CHAT_SALON = 'c1000000-0000-4000-8000-00000000000d'; // non-brahma (list filter + 404)
const CHAT_D = 'c1000000-0000-4000-8000-00000000000e'; // brahma, pinned P3 (text-block)
const CHAT_E = 'c1000000-0000-4000-8000-00000000000f'; // brahma, pinned P1 (tier-3 submit_final)
const CHAT_F = 'c1000000-0000-4000-8000-000000000010'; // brahma, pinned P1 (tier-3 dup_stuck)
const CHAT_G = 'c1000000-0000-4000-8000-000000000011'; // brahma, pinned P1 (tier-3 stream_error_mid_turn, P4.79)

// Pinned message ids for CHAT_A's transcript.
const MSG_A1 = 'd1000000-0000-4000-8000-000000000001'; // USER
const MSG_A2 = 'd1000000-0000-4000-8000-000000000002'; // ASSISTANT (empty tool-turn)
const MSG_A3 = 'd1000000-0000-4000-8000-000000000003'; // TOOL (run_sql)
const MSG_A4 = 'd1000000-0000-4000-8000-000000000004'; // ASSISTANT (prior answer)

const PROVIDER = 'ANTHROPIC';
const MODEL = 'brahma-fixture-model'; // absent from FALLBACK_PRICING → supportsTools true

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'brahma-console-web.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_BRAHMA_MAIN;
  const mountOut = process.env.QT_FIXTURE_BRAHMA_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_BRAHMA_MAIN and QT_FIXTURE_BRAHMA_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-brahma-web-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase, getCollection } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { ApiKeySchema } = await import('@/lib/schemas/profile.types');
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
  const { initializePlugins } = await import('@/lib/startup/plugin-initialization');
  await initializePlugins();
  await ensureCollection('api_keys', ApiKeySchema);

  // Materialize the standard 3-partition mount-index shape (the console's
  // `document_editing_enabled` doc tools reach it; the Db opens with the full
  // shape even though the corpus's `run_sql` reads MAIN).
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

  // 1. Users.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.users.create(
    { username: 'saturday', email: null, name: 'Saturday' } as never,
    { id: spec.userIdB, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Api key + two connection profiles (USER_A). P1 default carries the key.
  const apiKeyCol = await getCollection('api_keys');
  await apiKeyCol.insertOne(
    ApiKeySchema.parse({
      id: APIKEY,
      userId: spec.userId,
      label: 'Brahma test key',
      provider: PROVIDER,
      key_value: 'sk-synthetic-brahma-key',
      isActive: true,
      lastUsed: null,
      createdAt: TS,
      updatedAt: TS,
    }) as never,
  );
  await repos.connections.create(
    {
      userId: spec.userId,
      name: 'Primary Console',
      provider: PROVIDER,
      modelName: MODEL,
      apiKeyId: APIKEY,
      isDefault: true,
    } as never,
    { id: P1, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.connections.create(
    {
      userId: spec.userId,
      name: 'Secondary Console',
      provider: PROVIDER,
      modelName: MODEL,
      apiKeyId: APIKEY,
      isDefault: false,
    } as never,
    { id: P2, createdAt: TS, updatedAt: TS } as never,
  );
  // P3 forces text-block tool mode (the tier-3 text-block arm).
  await repos.connections.create(
    {
      userId: spec.userId,
      name: 'Text-Block Console',
      provider: PROVIDER,
      modelName: MODEL,
      apiKeyId: APIKEY,
      isDefault: false,
      pseudoToolMode: 'text-block',
    } as never,
    { id: P3, createdAt: TS, updatedAt: TS } as never,
  );

  // 3. The brahma chats (+ salon). A brahma chat mirrors v4's `handleCreate`.
  const mkBrahma = async (
    id: string,
    title: string,
    consoleConnectionProfileId: string | null,
  ): Promise<void> => {
    await repos.chats.create(
      {
        userId: spec.userId,
        participants: [],
        title,
        contextSummary: null,
        tags: [],
        chatType: 'brahma',
        consoleConnectionProfileId,
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  };
  await mkBrahma(CHAT_A, 'A Prior Audience at the Console', P1);
  await mkBrahma(CHAT_B, 'A Fresh Audience at the Console', P1);
  await mkBrahma(CHAT_C, 'An Unpinned Audience at the Console', null);
  await mkBrahma(CHAT_D, 'A Text-Block Audience at the Console', P3);
  await mkBrahma(CHAT_E, 'A Second Fresh Audience', P1);
  await mkBrahma(CHAT_F, 'A Third Fresh Audience', P1);
  await mkBrahma(CHAT_G, 'A Curtailed Audience at the Console', P1);

  // 4. CHAT_A's transcript (via addMessage, so lastMessageAt/messageCount bake).
  //    USER → empty tool-turn ASSISTANT → TOOL row → substantive ASSISTANT.
  const addMsg = async (id: string, role: string, content: string, createdAt: string) => {
    await repos.chats.addMessage(CHAT_A, {
      type: 'message',
      id,
      role,
      content,
      attachments: [],
      createdAt,
    } as never);
  };
  await addMsg(MSG_A1, 'USER', 'How many connection profiles do I have?', '2026-05-01T00:00:01.000Z');
  await addMsg(MSG_A2, 'ASSISTANT', '', '2026-05-01T00:00:02.000Z');
  await addMsg(
    MSG_A3,
    'TOOL',
    JSON.stringify({
      toolName: 'run_sql',
      success: true,
      result: '{"rows":[{"n":2}]}',
      arguments: { database: 'main', sql: 'SELECT COUNT(*) AS n FROM connection_profiles' },
      callId: 'call_prior',
    }),
    '2026-05-01T00:00:03.000Z',
  );
  await addMsg(MSG_A4, 'ASSISTANT', 'You have two connection profiles.', '2026-05-01T00:00:04.000Z');

  // 5. A non-brahma salon chat (list filter + the verify_brahma_chat 404 arm).
  await repos.chats.create(
    {
      userId: spec.userId,
      participants: [],
      title: 'An Ordinary Salon',
      contextSummary: null,
      tags: [],
      chatType: 'salon',
    } as never,
    { id: CHAT_SALON, createdAt: TS, updatedAt: TS } as never,
  );

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built brahma-console-web fixture: main=${mainOut} mount=${mountOut} ` +
      `(2 users, 3 profiles, 7 brahma chats [A has 4 msgs], 1 salon)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`brahma-console-web fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
