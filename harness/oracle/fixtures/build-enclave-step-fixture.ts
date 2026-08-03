/**
 * Tier-3 SEED fixture builder — the enclave `step()` + schedule-tick capstone
 * (U4.4; v4 `handleAutonomousRoomTurn` / `handleAutonomousRoomScheduleTick`).
 *
 * Bakes the seed state both differential sides start from, across THREE
 * databases (main + mount-index + llm-logs):
 *   - one connection profile + the two participant characters via v4's REAL
 *     `repos.characters.create` (pinned ids, full vault provisioning — the
 *     speaker selection's talkativeness + the run-start banner's names read
 *     through the vault overlay);
 *   - one `chat_settings` row for user 1 (cheapLLM present, compression off,
 *     danger DETECT_ONLY, `autonomousRoomSettings.dailyTokenBudget = 500` — the
 *     BROKEN-BUT-EXACT daily gate's bait); user 2 (the tick user) deliberately
 *     has NO row (the 12 h default freshness fallback);
 *   - the corpus chats via `repos.chats.create` (pinned ids/timestamps,
 *     CHARACTER participants only) with the autonomous run/budget/schedule
 *     columns from the spec, plus their seed opener messages;
 *   - the seeded `llm_logs` rows (the token-accounting substrate: tagged
 *     per-run spend + the untagged over-budget daily spend the broken
 *     `getTotalTokenUsageSince` sums to zero);
 *   - the pinned PROCESSING sibling `background_jobs` row (the concurrency
 *     tie-break case).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-enclave-step-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-enclave-step-mount.db \
 *   QT_FIXTURE_LLMLOGS_OUT=/tmp/qt-enclave-step-llmlogs.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-enclave-step-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  user1: string;
  user2: string;
  connectionProfile: { id: string; name: string; provider: string; modelName: string };
  chatSettings: { id: string; dailyTokenBudget: number };
  characters: Array<{
    id: string; name: string; description: string; talkativeness: number;
    connectionProfileId: string;
  }>;
  chats: Array<{
    id: string; userId: string; title: string;
    participants: Array<{ id: string; character: string; status: string }>;
    messages: Array<{ id: string; role: string; content: string; participantId: string }>;
    columns: Record<string, unknown>;
  }>;
  siblingJob: {
    id: string; userId: string; type: string; status: string;
    payload: Record<string, unknown>; createdAt: string;
  };
  llmLogRows: Array<{
    id: string; userId: string; chatId?: string; autonomousRunId?: string;
    usage: { promptTokens: number; completionTokens: number; totalTokens: number };
    cacheUsage?: { cacheReadInputTokens: number };
    createdAt: string;
  }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'enclave-step-tier3.json'), 'utf8')) as Spec;

  const outMain = process.env.QT_FIXTURE_OUT;
  const outMount = process.env.QT_FIXTURE_MOUNT_OUT;
  const outLl = process.env.QT_FIXTURE_LLMLOGS_OUT;
  if (!outMain || !outMount || !outLl) {
    throw new Error(
      'QT_FIXTURE_OUT / QT_FIXTURE_MOUNT_OUT / QT_FIXTURE_LLMLOGS_OUT must point at the fixture .db files',
    );
  }
  for (const base of [outMain, outMount, outLl]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = base + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-enclave-step-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.SQLITE_LLM_LOGS_PATH = outLl;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // Neutralize the job-processor host (an enqueue during seeding must never
  // fork a child that claims rows).
  (globalThis as Record<string, unknown>).__quilltapJobHost = {
    child: null, spawning: false, shuttingDown: false, childCrashed: true,
    restartTimestamps: [], signalHandlersInstalled: true, invalidationListeners: new Set(),
  };

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  const repos = getRepositories();

  // Materialize the mount-index store tables the vault provisioning writes.
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) {
      midb.exec(sql);
    }
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

  // Connection profile (both characters default to it).
  await repos.connections.create(
    {
      userId: spec.user1,
      name: spec.connectionProfile.name,
      provider: spec.connectionProfile.provider,
      modelName: spec.connectionProfile.modelName,
    } as never,
    { id: spec.connectionProfile.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp },
  );

  // Characters (full vault provisioning; the banner + talkativeness read them).
  for (const c of spec.characters) {
    await repos.characters.create(
      {
        name: c.name,
        userId: spec.user1,
        description: c.description,
        aliases: [],
        controlledBy: 'llm',
        talkativeness: c.talkativeness,
        defaultConnectionProfileId: c.connectionProfileId,
      } as never,
      { id: c.id },
    );
  }

  // Chat settings for user 1 only (user 2 banks the no-row defaults).
  await repos.chatSettings.create(
    {
      userId: spec.user1,
      cheapLLMSettings: { strategy: 'PROVIDER_CHEAPEST', fallbackToLocal: false },
      contextCompressionSettings: { enabled: false },
      autoDetectRng: false,
      answerConfirmationSettings: { enabled: false },
      dangerousContentSettings: { mode: 'DETECT_ONLY' },
      autonomousRoomSettings: {
        dailyTokenBudget: spec.chatSettings.dailyTokenBudget,
        defaultFreshnessWindowMs: 12 * 60 * 60 * 1000,
        visibilityDefault: 'owner_only',
        destructiveToolPolicy: 'opt_in_per_room',
      },
    } as never,
    { id: spec.chatSettings.id },
  );

  // Chats + their seed messages, with the autonomous columns from the spec.
  for (const chat of spec.chats) {
    const participants = chat.participants.map((p) => ({
      id: p.id,
      type: 'CHARACTER',
      characterId: p.character,
      controlledBy: 'llm',
      status: p.status,
      createdAt: spec.seedTimestamp,
      updatedAt: spec.seedTimestamp,
    }));
    await repos.chats.create(
      {
        userId: chat.userId,
        title: chat.title,
        participants,
        ...chat.columns,
      } as never,
      { id: chat.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp },
    );
    if (chat.messages.length > 0) {
      await repos.chats.addMessages(
        chat.id,
        chat.messages.map((m, i) => ({
          id: m.id,
          type: 'message',
          role: m.role,
          content: m.content,
          participantId: m.participantId,
          // Distinct, ordered createdAt so getMessages ordering is unambiguous.
          createdAt: `2026-01-01T00:0${Math.floor(i / 10)}:${String((i % 10) * 6).padStart(2, '0')}.000Z`,
          attachments: [],
        })) as never,
      );
      // addMessages bumps lastMessageAt/updatedAt/messageCount with a live
      // clock; re-pin the volatile chat columns to the seed sentinel so both
      // sides start from identical bytes.
      await rawQuery(
        'UPDATE chats SET updatedAt = ?, lastMessageAt = ? WHERE id = ?',
        [spec.seedTimestamp, spec.seedTimestamp, chat.id],
      );
    } else {
      await repos.chats.getMessageCount(chat.id);
    }
  }

  // The pinned PROCESSING sibling turn job (the concurrency tie-break case).
  await repos.backgroundJobs.findByUserId(spec.user1, 'PENDING'); // materialize table
  await rawQuery(
    'INSERT INTO background_jobs (id, userId, type, status, payload, priority, attempts, ' +
      'maxAttempts, lastError, scheduledAt, startedAt, completedAt, createdAt, updatedAt) ' +
      'VALUES (?, ?, ?, ?, ?, 0, 1, 1, NULL, ?, ?, NULL, ?, ?)',
    [
      spec.siblingJob.id,
      spec.siblingJob.userId,
      spec.siblingJob.type,
      spec.siblingJob.status,
      JSON.stringify(spec.siblingJob.payload),
      spec.siblingJob.createdAt,
      spec.siblingJob.createdAt,
      spec.siblingJob.createdAt,
      spec.siblingJob.createdAt,
    ],
  );

  // Seeded llm_logs rows (pinned ids/timestamps through v4's REAL repo create).
  for (const row of spec.llmLogRows) {
    const created = await repos.llmLogs.create(
      {
        userId: row.userId,
        type: 'CHAT_MESSAGE',
        ...(row.chatId !== undefined ? { chatId: row.chatId } : {}),
        ...(row.autonomousRunId !== undefined ? { autonomousRunId: row.autonomousRunId } : {}),
        provider: 'ANTHROPIC',
        modelName: 'claude-sonnet',
        request: {
          messages: [
            { role: 'user', content: 'seed', contentLength: 4, hasAttachments: false },
          ],
          messageCount: 1,
          temperature: null,
          maxTokens: null,
        },
        response: { content: 'seed reply', contentLength: 10, error: null, finishReason: 'stop' },
        usage: row.usage,
        ...(row.cacheUsage !== undefined ? { cacheUsage: row.cacheUsage } : {}),
      } as never,
      { id: row.id, createdAt: row.createdAt } as never,
    );
    if (!created) throw new Error(`llm_logs seed row failed: ${row.id}`);
  }

  // Materialize the tables the run touches but nothing seeds.
  await repos.users.findById(spec.user1); // users (the identity resolver's read)
  await repos.memories.findAll();
  await repos.vectorIndices.findMetaByCharacterId(spec.characters[0].id);
  await repos.vectorIndices.findEntriesByCharacterId(spec.characters[0].id);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built enclave-step fixture: ${outMain} + ${outMount} + ${outLl} ` +
      `(${spec.characters.length} characters, ${spec.chats.length} chats)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`enclave-step fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
