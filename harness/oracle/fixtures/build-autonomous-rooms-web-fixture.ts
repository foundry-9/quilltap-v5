/**
 * The committed autonomous-rooms server-surface fixture builder (P4.6ad
 * deliverable #3) — the test-pepper substrate for
 * `autonomous_rooms_routes_equivalence`.
 *
 * Bakes, via v4's REAL repositories (all ids/timestamps pinned so both
 * differential sides read identical rows — reads diff with no remap; only the
 * start/resume/pause/stop/update-settings mutations mint fresh ids/jobs/stamps,
 * blanked by the harness):
 *   - TWO users: USER_A (the fixture owner) + USER_B (the ownership arm), each
 *     with a chat_settings row carrying autonomousRoomSettings — USER_A's policy
 *     honors the per-room destructive flag (`opt_in_per_room`), USER_B's clamps
 *     it (`always_refuse`),
 *   - one connection profile + api key,
 *   - THREE LLM characters for USER_A (CHAR_1/CHAR_2 + a spare), TWO for USER_B,
 *   - one store-backed project (the projectName join),
 *   - EIGHT autonomous rooms owned by USER_A covering every runState — idle ×3
 *     (one project-linked; two share a band for the updatedAt tiebreak), running,
 *     paused (a live run for resume-continues), stopped, budgetExhausted, error —
 *     each with ≥2 LLM participants and no user participant,
 *   - one NON-autonomous salon chat (the 400 guard),
 *   - one autonomous room owned by USER_B (the ownership/listing-exclusion arm).
 *
 * Regenerate (Node 24, from the v4 checkout) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_AUTO_MAIN=$W/crates/quilltap-web/tests/fixtures/autonomous-main.db \
 *   QT_FIXTURE_AUTO_MOUNT=$W/crates/quilltap-web/tests/fixtures/autonomous-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-autonomous-rooms-web-fixture.ts
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

// ── Pinned entity ids (shared verbatim with the Rust differential) ──────────
const CONN = 'c0000001-0000-4000-8000-000000000001';
const APIKEY = 'a0000001-0000-4000-8000-000000000001';

const CHAR_1 = 'a1000000-0000-4000-8000-000000000001'; // Lorian (llm)
const CHAR_2 = 'a1000000-0000-4000-8000-000000000002'; // Riya (llm)
const CHAR_3 = 'a1000000-0000-4000-8000-000000000003'; // Sable (llm, spare)
const CHAR_B1 = 'a1000000-0000-4000-8000-0000000000b1'; // user B's Vesper (llm)
const CHAR_B2 = 'a1000000-0000-4000-8000-0000000000b2'; // user B's Corin (llm)

const PROJECT_1 = '90000000-0000-4000-8000-000000000001'; // "The Cartographers"

const ROOM_IDLE = 'c1000000-0000-4000-8000-000000000001';
const ROOM_IDLE2 = 'c1000000-0000-4000-8000-000000000002';
const ROOM_PROJECT = 'c1000000-0000-4000-8000-000000000003';
const ROOM_RUNNING = 'c1000000-0000-4000-8000-000000000004';
const ROOM_PAUSED = 'c1000000-0000-4000-8000-000000000005';
const ROOM_STOPPED = 'c1000000-0000-4000-8000-000000000006';
const ROOM_BUDGET = 'c1000000-0000-4000-8000-000000000007';
const ROOM_ERROR = 'c1000000-0000-4000-8000-000000000008';
const CHAT_SALON = 'c1000000-0000-4000-8000-000000000009';
const ROOM_USER_B = 'c1000000-0000-4000-8000-00000000000b';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'autonomous-rooms-web.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_AUTO_MAIN;
  const mountOut = process.env.QT_FIXTURE_AUTO_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_AUTO_MAIN and QT_FIXTURE_AUTO_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-auto-web-fixture-'));
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
  const { CharacterSchema } = await import('@/lib/schemas/types');
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
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('api_keys', ApiKeySchema);

  // Materialize the standard 3-partition mount-index shape (the projects store
  // rides it; the autonomous surface itself never touches most tables, but the
  // Db opens with the full shape).
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
  const TS = spec.seedTimestamp;

  // 1. Users.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.users.create(
    { username: 'saturday', email: null, name: 'Saturday' } as never,
    { id: spec.userIdB, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Api key + connection profile (USER_A owns; participants reference CONN).
  const apiKeyCol = await getCollection('api_keys');
  await apiKeyCol.insertOne(
    ApiKeySchema.parse({
      id: APIKEY,
      userId: spec.userId,
      label: 'mock-key',
      provider: 'OPENAI_COMPATIBLE',
      key_value: 'sk-synthetic-mock-key',
      isActive: true,
      lastUsed: null,
      createdAt: TS,
      updatedAt: TS,
    }) as never,
  );
  await repos.connections.create(
    {
      userId: spec.userId,
      name: 'Local Mock',
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-model',
      baseUrl: 'http://127.0.0.1:1/v1',
      apiKeyId: APIKEY,
      isCheap: false,
    } as never,
    { id: CONN, createdAt: TS, updatedAt: TS } as never,
  );

  // 3. chat_settings — autonomousRoomSettings per user. USER_A honors the
  // per-room destructive flag; USER_B clamps it (always_refuse).
  await repos.chatSettings.create(
    {
      userId: spec.userId,
      autonomousRoomSettings: {
        dailyTokenBudget: null,
        defaultFreshnessWindowMs: 12 * 60 * 60 * 1000,
        visibilityDefault: 'owner_only',
        destructiveToolPolicy: 'opt_in_per_room',
      },
    } as never,
    { id: 'c8000000-0000-4000-8000-000000000001', createdAt: TS, updatedAt: TS } as never,
  );
  await repos.chatSettings.create(
    {
      userId: spec.userIdB,
      autonomousRoomSettings: {
        dailyTokenBudget: null,
        defaultFreshnessWindowMs: 6 * 60 * 60 * 1000,
        visibilityDefault: 'owner_only',
        destructiveToolPolicy: 'always_refuse',
      },
    } as never,
    { id: 'c8000000-0000-4000-8000-000000000002', createdAt: TS, updatedAt: TS } as never,
  );

  // 4. Characters.
  const mkChar = async (id: string, name: string, userId: string) =>
    repos.characters.create(
      {
        name,
        userId,
        title: null,
        description: `${name} the test character.`,
        personality: null,
        aliases: [],
        controlledBy: 'llm',
        talkativeness: 0.5,
        defaultConnectionProfileId: CONN,
        isFavorite: false,
        npc: false,
        tags: [],
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  await mkChar(CHAR_1, 'Lorian', spec.userId);
  await mkChar(CHAR_2, 'Riya', spec.userId);
  await mkChar(CHAR_3, 'Sable', spec.userId);
  await mkChar(CHAR_B1, 'Vesper', spec.userIdB);
  await mkChar(CHAR_B2, 'Corin', spec.userIdB);

  // 5. A store-backed project (the projectName join).
  const project = await repos.projects.create(
    { name: 'The Cartographers', description: 'Mapping the archipelago.', state: {} } as never,
    { id: PROJECT_1, createdAt: TS, updatedAt: TS } as never,
  );
  if (project.id !== PROJECT_1) throw new Error(`project id drift: ${project.id}`);

  // 6. The rooms. A participant helper (all LLM — autonomous precondition:
  // no user participant, ≥2 LLM characters).
  let partSeq = 0;
  const mkParticipant = (characterId: string) => ({
    id: `e1000000-0000-4000-8000-${(++partSeq).toString().padStart(12, '0')}`,
    type: 'CHARACTER',
    characterId,
    controlledBy: 'llm',
    status: 'active',
    connectionProfileId: CONN,
    createdAt: TS,
    updatedAt: TS,
  });

  // A room create with the full run-state column set threaded through the schema
  // (v4 `_create` persists every ChatMetadata column present on `data`).
  type RoomSeed = {
    id: string;
    userId?: string;
    title: string;
    updatedAt: string;
    participants: string[];
    projectId?: string | null;
    runState: string;
    runStateMessage?: string | null;
    currentRunId?: string | null;
    runStartedAt?: string | null;
    runEndedAt?: string | null;
    runPausedAt?: string | null;
    runPausedAccumMs?: number;
    runTurnsConsumed?: number;
    runTokensConsumed?: number;
    scheduleCron?: string | null;
    scheduleNextRunAt?: string | null;
    scheduleLastRunAt?: string | null;
    scheduleFreshnessWindowMs?: number | null;
    budgetMaxTurns?: number | null;
    budgetMaxTokens?: number | null;
    budgetMaxWallClockMs?: number | null;
    budgetEstimatedSpendCapUSD?: number | null;
    budgetExcludeCacheHits?: number;
    runDestructiveToolsAllowed?: number;
    runVisibility?: string | null;
  };
  const mkRoom = async (r: RoomSeed) => {
    await repos.chats.create(
      {
        userId: r.userId ?? spec.userId,
        title: r.title,
        participants: r.participants.map(mkParticipant),
        chatType: 'autonomous',
        contextSummary: null,
        tags: [],
        projectId: r.projectId ?? null,
        runState: r.runState,
        runStateMessage: r.runStateMessage ?? null,
        currentRunId: r.currentRunId ?? null,
        runStartedAt: r.runStartedAt ?? null,
        runEndedAt: r.runEndedAt ?? null,
        runPausedAt: r.runPausedAt ?? null,
        runPausedAccumMs: r.runPausedAccumMs ?? 0,
        runTurnsConsumed: r.runTurnsConsumed ?? 0,
        runTokensConsumed: r.runTokensConsumed ?? 0,
        scheduleCron: r.scheduleCron ?? null,
        scheduleNextRunAt: r.scheduleNextRunAt ?? null,
        scheduleLastRunAt: r.scheduleLastRunAt ?? null,
        scheduleFreshnessWindowMs: r.scheduleFreshnessWindowMs ?? null,
        budgetMaxTurns: r.budgetMaxTurns ?? null,
        budgetMaxTokens: r.budgetMaxTokens ?? null,
        budgetMaxWallClockMs: r.budgetMaxWallClockMs ?? null,
        budgetEstimatedSpendCapUSD: r.budgetEstimatedSpendCapUSD ?? null,
        budgetExcludeCacheHits: r.budgetExcludeCacheHits ?? 1,
        runDestructiveToolsAllowed: r.runDestructiveToolsAllowed ?? 0,
        runVisibility: r.runVisibility ?? null,
      } as never,
      { id: r.id, createdAt: TS, updatedAt: r.updatedAt } as never,
    );
  };

  // idle ×3 (two share the band for the updatedAt-desc tiebreak; one linked to
  // the project for the projectName join).
  await mkRoom({
    id: ROOM_IDLE,
    title: 'The Lantern Vigil',
    updatedAt: '2026-03-05T00:00:00.000Z',
    participants: [CHAR_1, CHAR_2],
    runState: 'idle',
  });
  await mkRoom({
    id: ROOM_IDLE2,
    title: 'The Quiet Study',
    updatedAt: '2026-03-04T00:00:00.000Z',
    participants: [CHAR_1, CHAR_3],
    runState: 'idle',
    runDestructiveToolsAllowed: 1,
    budgetExcludeCacheHits: 0,
    runVisibility: 'open',
  });
  await mkRoom({
    id: ROOM_PROJECT,
    title: 'The Cartographers Room',
    updatedAt: '2026-03-03T00:00:00.000Z',
    participants: [CHAR_2, CHAR_3],
    projectId: PROJECT_1,
    runState: 'idle',
    scheduleFreshnessWindowMs: 3 * 60 * 60 * 1000,
  });
  // running — cron + a live run + consumed budget + caps.
  await mkRoom({
    id: ROOM_RUNNING,
    title: 'The Midnight Debate',
    updatedAt: '2026-03-06T00:00:00.000Z',
    participants: [CHAR_1, CHAR_2],
    runState: 'running',
    currentRunId: 'aa000000-0000-4000-8000-000000000001',
    runStartedAt: '2026-03-06T00:00:00.000Z',
    runTurnsConsumed: 3,
    runTokensConsumed: 1500,
    scheduleCron: '0 9 * * *',
    scheduleNextRunAt: '2026-03-07T09:00:00.000Z',
    scheduleLastRunAt: '2026-03-06T00:00:00.000Z',
    budgetMaxTurns: 20,
    budgetMaxTokens: 50000,
    budgetMaxWallClockMs: 3600000,
    budgetEstimatedSpendCapUSD: 1.5,
    runVisibility: 'owner_only',
    runDestructiveToolsAllowed: 1,
  });
  // paused — a genuine live paused run (resume-continues folds runPausedAccumMs).
  await mkRoom({
    id: ROOM_PAUSED,
    title: 'The Interrupted Council',
    updatedAt: '2026-03-05T12:00:00.000Z',
    participants: [CHAR_1, CHAR_2],
    runState: 'paused',
    runStateMessage: 'manual:paused',
    currentRunId: 'aa000000-0000-4000-8000-000000000002',
    runStartedAt: '2026-03-05T10:00:00.000Z',
    runPausedAt: '2026-03-05T11:00:00.000Z',
    runPausedAccumMs: 1000,
    runTurnsConsumed: 2,
    runTokensConsumed: 800,
  });
  // stopped.
  await mkRoom({
    id: ROOM_STOPPED,
    title: 'The Closed Salon',
    updatedAt: '2026-03-02T00:00:00.000Z',
    participants: [CHAR_2, CHAR_3],
    runState: 'stopped',
    runStateMessage: 'manual:stopped',
    currentRunId: 'aa000000-0000-4000-8000-000000000003',
    runEndedAt: '2026-03-02T00:00:00.000Z',
    runTurnsConsumed: 5,
    runTokensConsumed: 2200,
  });
  // budgetExhausted.
  await mkRoom({
    id: ROOM_BUDGET,
    title: 'The Spent Ledger',
    updatedAt: '2026-03-01T12:00:00.000Z',
    participants: [CHAR_1, CHAR_3],
    runState: 'budgetExhausted',
    runStateMessage: 'budget:turns_exhausted',
    currentRunId: 'aa000000-0000-4000-8000-000000000004',
    runStartedAt: '2026-03-01T00:00:00.000Z',
    runEndedAt: '2026-03-01T12:00:00.000Z',
    runTurnsConsumed: 10,
    runTokensConsumed: 40000,
    budgetMaxTurns: 10,
    budgetMaxTokens: 40000,
    runVisibility: 'household',
  });
  // error.
  await mkRoom({
    id: ROOM_ERROR,
    title: 'The Broken Quill',
    updatedAt: '2026-03-01T06:00:00.000Z',
    participants: [CHAR_2, CHAR_3],
    runState: 'error',
    runStateMessage: 'turn:failed',
    currentRunId: 'aa000000-0000-4000-8000-000000000005',
    runEndedAt: '2026-03-01T06:00:00.000Z',
    runTurnsConsumed: 1,
    runTokensConsumed: 300,
  });

  // 7. A non-autonomous salon chat (the 400 guard).
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'An Ordinary Salon',
      participants: [mkParticipant(CHAR_1), mkParticipant(CHAR_2)],
      chatType: 'salon',
      contextSummary: null,
      tags: [],
    } as never,
    { id: CHAT_SALON, createdAt: TS, updatedAt: TS } as never,
  );

  // 8. USER_B's autonomous room (ownership/listing-exclusion + the clamp arm).
  await mkRoom({
    id: ROOM_USER_B,
    userId: spec.userIdB,
    title: 'Saturday’s Room',
    updatedAt: '2026-03-05T00:00:00.000Z',
    participants: [CHAR_B1, CHAR_B2],
    runState: 'idle',
  });

  // Materialize the empty background_jobs table (the start/resume enqueue writes
  // it; v4 auto-creates it at runtime, but v5's schema is frozen, so it must
  // exist in the committed fixture).
  await repos.backgroundJobs.findByUserId(spec.userId, 'PENDING');

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built autonomous-rooms-web fixture: main=${mainOut} mount=${mountOut} ` +
      `(2 users, 5 chars, 1 project, 8 USER_A rooms + salon + USER_B room)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`autonomous-rooms-web fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
