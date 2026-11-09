/**
 * The storyteller-simultaneous fixture builder (P4.D368, item 6) — the
 * test-pepper substrate for `storyteller_simultaneous_tier3_equivalence`
 * (tier 3). MINTED per run (never committed), built entirely through v4's REAL
 * code at the pin, every id and timestamp pinned:
 *
 *   1. ONE user — v5's single-user id, so both sides resolve the same owner.
 *   2. An API key + TWO connection profiles (A and B), OPENAI_COMPATIBLE.
 *   3. Three LLM characters: Ada and Bram on profile A, Cora on profile B.
 *   4. A store-backed project WITH a description (Prospero's cadence context
 *      has something to post) — v4's REAL `repos.projects.create`.
 *   5. A storyteller chat in that project with the three LLM seats (the
 *      operator holds no seat — v4's participant schema has no user type), put in storyteller mode
 *      through v4's REAL `setTurnMode`, with FIVE planted `message` events
 *      (the operator's post last), so every turn of the round lands ON the
 *      reinject cadence (`messageCount % 5 === 0`, the default interval).
 *   6. Three OPEN rounds, one per oracle row (the main row, the stop row, the
 *      failure row), each with three answered cards — Ada `simple`, Bram
 *      `simple`, Cora `act` — through v4's REAL
 *      `StorytellerRoundsRepository.create`. Each row runs on its own COPY of
 *      the fixture, so the rounds never see one another's turns.
 *
 * Every participant and message id is a seeded UUID (non-UUID ids are dropped
 * by v5's raw reads — memory note `memory-raw-reads-drop-non-uuid-fixtures`).
 *
 * Node 24, from the PINNED v4 worktree:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server     # (the pinned worktree)
 *   TZ=UTC \
 *   QT_FIXTURE_STS3_MAIN=/tmp/qt-sts3-main.db \
 *   QT_FIXTURE_STS3_MOUNT=/tmp/qt-sts3-mount.db \
 *   QT_FIXTURE_STS3_LLMLOGS=/tmp/qt-sts3-llmlogs.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-storyteller-simultaneous-fixture.ts
 */

import { join } from 'node:path';
import { mkdtempSync, mkdirSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

// ── Pinned values (the oracle case and the Rust side repeat them) ──
const TEST_PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
const USER = 'ffffffff-ffff-ffff-ffff-ffffffffffff';
const TS = '2026-05-01T00:00:00.000Z';
const APIKEY = '71000000-0000-4000-8000-0000000000a1';
const PROF_A = '70000000-0000-4000-8000-0000000000a1';
const PROF_B = '70000000-0000-4000-8000-0000000000b1';
const CH_ADA = 'a7000000-0000-4000-8000-000000000001';
const CH_BRAM = 'a7000000-0000-4000-8000-000000000002';
const CH_CORA = 'a7000000-0000-4000-8000-000000000003';
const PROJECT = '79000000-0000-4000-8000-000000000001';
const CHAT = 'c7000000-0000-4000-8000-000000000001';
const P_ADA = 'e7000000-0000-4000-8000-0000000000a1';
const P_BRAM = 'e7000000-0000-4000-8000-0000000000b2';
const P_CORA = 'e7000000-0000-4000-8000-0000000000c3';
const M = (n: number) => `d7000000-0000-4000-8000-0000000000${String(n).padStart(2, '0')}`;
const ROUNDS = ['57000000-0000-4000-8000-000000000001', '57000000-0000-4000-8000-000000000002', '57000000-0000-4000-8000-000000000003'];
const ASKED = '2026-05-02T10:00:01.000Z';
const ANSWERED = '2026-05-02T10:00:09.000Z';

function card(pid: string, characterId: string, characterName: string, tier: string) {
  return {
    participantId: pid,
    characterId,
    characterName,
    status: 'bid',
    statedTier: tier,
    tier,
    tierRaised: false,
    tools: [],
    plan: `${characterName} means to ${tier}.`,
    reason: null,
    talkativeness: 0.5,
    silent: false,
    askedAt: ASKED,
    answeredAt: ANSWERED,
  };
}

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) throw new Error(`storyteller-simultaneous fixture must be built under TZ=UTC (offset=${offset})`);
  const mainOut = process.env.QT_FIXTURE_STS3_MAIN;
  const mountOut = process.env.QT_FIXTURE_STS3_MOUNT;
  const llOut = process.env.QT_FIXTURE_STS3_LLMLOGS;
  if (!mainOut || !mountOut || !llOut) {
    throw new Error('QT_FIXTURE_STS3_MAIN / QT_FIXTURE_STS3_MOUNT / QT_FIXTURE_STS3_LLMLOGS must point at the .db files');
  }
  for (const out of [mainOut, mountOut, llOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-sts3-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.SQLITE_LLM_LOGS_PATH = llOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase, getCollection } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema, ChatMetadataSchema } = await import('@/lib/schemas/types');
  const { ApiKeySchema } = await import('@/lib/schemas/profile.types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { StorytellerRoundSchema } = await import('@/lib/schemas/storyteller.types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const mi = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  await ensureCollection('users', UserSchema as never);
  await ensureCollection('chats', ChatMetadataSchema as never);
  await ensureCollection('characters', CharacterSchema as never);
  await ensureCollection('api_keys', ApiKeySchema as never);
  await ensureCollection('storyteller_rounds', StorytellerRoundSchema as never);
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  for (const [name, schema] of [
    ['doc_mount_points', mi.DocMountPointSchema],
    ['doc_mount_files', mi.DocMountFileSchema],
    ['doc_mount_documents', mi.DocMountDocumentSchema],
    ['doc_mount_folders', mi.DocMountFolderSchema],
    ['doc_mount_file_links', mi.DocMountFileLinkSchema],
    ['doc_mount_chunks', mi.DocMountChunkSchema],
  ] as Array<[string, unknown]>) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  // v4's blobs repository creates it lazily from hand-written DDL
  // (`doc-mount-blobs.repository.ts`); copied so a store write never throws.
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
  midb.exec('CREATE UNIQUE INDEX IF NOT EXISTS "idx_doc_mount_blobs_fileId" ON "doc_mount_blobs" ("fileId")');

  const repos = getRepositories();
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: USER, createdAt: TS, updatedAt: TS } as never,
  );
  const apiKeyCol = await getCollection('api_keys');
  await apiKeyCol.insertOne(
    ApiKeySchema.parse({
      id: APIKEY,
      userId: USER,
      label: 'mock-key',
      provider: 'OPENAI_COMPATIBLE',
      key_value: 'sk-synthetic-mock-key',
      isActive: true,
      lastUsed: null,
      createdAt: TS,
      updatedAt: TS,
    }) as never,
  );
  for (const [id, name, model] of [
    [PROF_A, 'Profile A', 'mock-model-a'],
    [PROF_B, 'Profile B', 'mock-model-b'],
  ]) {
    await repos.connections.create(
      {
        userId: USER,
        name,
        provider: 'OPENAI_COMPATIBLE',
        modelName: model,
        baseUrl: 'http://127.0.0.1:1/v1',
        apiKeyId: APIKEY,
        isCheap: false,
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  }
  for (const [id, name, prof] of [
    [CH_ADA, 'Ada', PROF_A],
    [CH_BRAM, 'Bram', PROF_A],
    [CH_CORA, 'Cora', PROF_B],
  ]) {
    await repos.characters.create(
      {
        name,
        userId: USER,
        title: null,
        description: `${name} keeps the lighthouse.`,
        personality: null,
        aliases: [],
        controlledBy: 'llm',
        talkativeness: 0.5,
        defaultConnectionProfileId: prof,
        isFavorite: false,
        npc: false,
        tags: [],
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  }
  const project = await repos.projects.create(
    { name: 'The Lighthouse', description: 'A storm is coming to the point.', state: {} } as never,
    { id: PROJECT, createdAt: TS, updatedAt: TS } as never,
  );
  if (project.id !== PROJECT) throw new Error(`project id drift: ${project.id}`);

  const seat = (id: string, characterId: string | null, displayOrder: number, controlledBy: string, prof: string | null) => ({
    id,
    type: 'CHARACTER',
    characterId,
    controlledBy,
    status: 'active',
    isActive: true,
    displayOrder,
    connectionProfileId: prof,
    hasHistoryAccess: false,
    createdAt: TS,
    updatedAt: TS,
  });
  await repos.chats.create(
    {
      userId: USER,
      title: 'The Lighthouse Watch',
      chatType: 'salon',
      projectId: PROJECT,
      participants: [
        seat(P_ADA, CH_ADA, 1, 'llm', PROF_A),
        seat(P_BRAM, CH_BRAM, 2, 'llm', PROF_A),
        seat(P_CORA, CH_CORA, 3, 'llm', PROF_B),
      ],
      tags: [],
    } as never,
    { id: CHAT, createdAt: TS, updatedAt: TS } as never,
  );
  // Five `message` events, the operator's post last: every turn of the round
  // lands on the default cadence (5).
  const lines: Array<[string, string, string | null]> = [
    ['USER', 'The wind rises over the point.', null],
    ['ASSISTANT', 'Ada trims the lamp.', P_ADA],
    ['ASSISTANT', 'Bram bars the door.', P_BRAM],
    ['ASSISTANT', 'Cora reads the glass.', P_CORA],
    ['USER', 'The storm breaks — what now?', null],
  ];
  for (const [i, [role, content, pid]] of lines.entries()) {
    await repos.chats.addMessage(CHAT, {
      type: 'message',
      id: M(i + 1),
      role,
      content,
      ...(pid ? { participantId: pid } : {}),
      createdAt: `2026-05-02T09:0${i}:00.000Z`,
      attachments: [],
    } as never);
  }
  if (!(await repos.chats.setTurnMode(CHAT, 'storyteller' as never))) throw new Error('setTurnMode wrote nothing');

  for (const [i, id] of ROUNDS.entries()) {
    await repos.storytellerRounds.create(
      {
        chatId: CHAT,
        triggerMessageId: M(5),
        status: 'open',
        bids: [card(P_ADA, CH_ADA, 'Ada', 'simple'), card(P_BRAM, CH_BRAM, 'Bram', 'simple'), card(P_CORA, CH_CORA, 'Cora', 'act')],
        selection: null,
        closedReason: null,
        closedAt: null,
      } as never,
      { id, createdAt: `2026-05-02T10:0${i}:30.000Z`, updatedAt: `2026-05-02T10:0${i}:30.000Z` },
    );
  }

  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const maindb = getRawDatabase();
  if (!maindb) throw new Error('main DB handle unavailable');
  maindb.prepare('UPDATE "chats" SET "updatedAt" = ?').run(TS);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  for (const out of [mainOut, mountOut, llOut]) {
    for (const suffix of ['-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
  }
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(`built storyteller-simultaneous fixtures: main=${mainOut} mount=${mountOut} llmlogs=${llOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`storyteller-simultaneous fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
