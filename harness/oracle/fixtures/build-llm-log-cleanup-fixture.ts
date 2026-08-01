/**
 * SEED fixture builder for the `LLM_LOG_CLEANUP` family (P4.24 — v4
 * `handleLLMLogCleanup`, `lib/background-jobs/handlers/llm-log-cleanup.ts`, and
 * `runScheduledCleanup`, `lib/background-jobs/scheduled-cleanup.ts`).
 *
 * TWO FILES, built through v4's REAL repos so the DDL and row shapes are
 * production-identical:
 *
 *   - `llm-log-cleanup-main.db`     — nine users, eight `chat_settings` rows
 *                                     (one per handler arm), and the eight
 *                                     pinned PENDING `LLM_LOG_CLEANUP` jobs the
 *                                     claim loop drains.
 *   - `llm-log-cleanup-llmlogs.db`  — the llm-logs PARTITION (v4 routes the
 *                                     LLMLogsRepository to its own database via
 *                                     `SQLITE_LLM_LOGS_PATH`), carrying log rows
 *                                     dated to straddle each arm's cutoff under
 *                                     BOTH timezone legs.
 *
 * The `nullBag` user's `llmLoggingSettings` cell is set to SQL NULL by raw SQL
 * *after* its create: that is not a shape any v4 write produces, but it is the
 * shape a row gets when the column predates the settings bag, and it is the only
 * way to reach v4's `LLMLoggingSettingsSchema.default({...})` — the path that
 * makes the handler's own `?? 30` unreachable. It is also the row the enqueuer
 * leg disagrees on, so the fixture must carry it.
 *
 * ⚠️ Both differential sides MUTATE these databases (cleanup deletes; the
 * enqueuer inserts), so both run against /tmp COPIES. This builder writes only
 * where its env vars point.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<the v5 worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_LLC_MAIN=/tmp/qt-llc-main.db QT_FIXTURE_LLC_LOGS=/tmp/qt-llc-llmlogs.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-llm-log-cleanup-fixture.ts
 *   cp /tmp/qt-llc-main.db    $V5/crates/quilltap-web/tests/fixtures/llm-log-cleanup-main.db
 *   cp /tmp/qt-llc-llmlogs.db $V5/crates/quilltap-web/tests/fixtures/llm-log-cleanup-llmlogs.db
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface UserSeed {
  key: string;
  id: string;
  username: string;
  settings: { enabled: boolean; verboseMode: boolean; retentionDays: number } | null;
  nullBag?: boolean;
}
interface JobSeed {
  id: string;
  name: string;
  user: string;
  payload: Record<string, unknown>;
  createdAt: string;
}
interface LogSeed {
  id: string;
  user: string;
  note: string;
  createdAt: string;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  nowIso: string;
  connectionProfileId: string;
  users: UserSeed[];
  jobs: JobSeed[];
  logs: LogSeed[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'llm-log-cleanup.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_LLC_MAIN;
  const logsOut = process.env.QT_FIXTURE_LLC_LOGS;
  if (!mainOut || !logsOut) {
    throw new Error('QT_FIXTURE_LLC_MAIN and QT_FIXTURE_LLC_LOGS must point at the .db files');
  }
  for (const out of [mainOut, logsOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-llc-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_LLM_LOGS_PATH = logsOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();
  const repos = getRepositories();

  // 1. The users.
  for (const u of spec.users) {
    await repos.users.create(
      { username: u.username, email: null, name: u.username } as never,
      { id: u.id, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // 2. One connection profile — nothing here reads it, but a settings row on a
  //    real instance never stands alone and the enqueuer's findAll is happier
  //    over production-shaped rows.
  await repos.connections.create(
    {
      userId: spec.users[0].id,
      name: 'Fixture Cheap',
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-cheap-model',
      baseUrl: 'http://127.0.0.1:1/v1',
      apiKeyId: null,
      isCheap: true,
    } as never,
    { id: spec.connectionProfileId, createdAt: TS, updatedAt: TS } as never,
  );

  // 3. The chat_settings rows — one per arm; `settings: null` means the user
  //    deliberately has NO row (step 2's bail, and step 4's short-circuit).
  for (const u of spec.users) {
    if (!u.settings) continue;
    await repos.chatSettings.create(
      { userId: u.id, llmLoggingSettings: u.settings } as never,
      { id: settingsIdFor(u.id), createdAt: TS, updatedAt: TS } as never,
    );
  }

  // 4. The one hand-made shape: NULL the bag so v4's Zod `.default({...})`
  //    supplies `{enabled: true, verboseMode: false, retentionDays: 30}`.
  for (const u of spec.users) {
    if (!u.nullBag) continue;
    await rawQuery('UPDATE "chat_settings" SET "llmLoggingSettings" = NULL WHERE "userId" = ?', [
      u.id,
    ]);
  }

  // 5. The jobs the claim loop drains.
  const userById = new Map(spec.users.map((u) => [u.key, u.id]));
  for (const j of spec.jobs) {
    const userId = userById.get(j.user);
    if (!userId) throw new Error(`job ${j.name} names unknown user key ${j.user}`);
    await repos.backgroundJobs.create(
      {
        userId,
        type: 'LLM_LOG_CLEANUP',
        status: 'PENDING',
        payload: j.payload,
        priority: 0,
        attempts: 0,
        maxAttempts: 3,
        lastError: null,
        scheduledAt: j.createdAt,
        startedAt: null,
        completedAt: null,
      } as never,
      { id: j.id, createdAt: j.createdAt, updatedAt: j.createdAt } as never,
    );
  }

  // 6. The log corpus, through the REAL LLMLogsRepository — whose overridden
  //    getCollection() lazily CREATE-TABLEs and writes into the llm-logs
  //    PARTITION, not the main DB.
  for (const log of spec.logs) {
    const userId = userById.get(log.user);
    if (!userId) throw new Error(`log ${log.id} names unknown user key ${log.user}`);
    await repos.llmLogs.create(
      {
        userId,
        type: 'CHAT_MESSAGE',
        messageId: null,
        chatId: null,
        characterId: null,
        autonomousRunId: null,
        provider: 'OPENAI_COMPATIBLE',
        modelName: 'mock-model',
        request: {
          messageCount: 1,
          messages: [
            {
              role: 'user',
              content: log.note,
              contentLength: log.note.length,
              hasAttachments: false,
            },
          ],
          temperature: 1,
          maxTokens: 1024,
          toolCount: 0,
        },
        response: {
          content: 'noted',
          contentLength: 5,
          error: null,
          finishReason: 'stop',
        },
      } as never,
      { id: log.id, createdAt: log.createdAt, updatedAt: log.createdAt } as never,
    );
  }

  await closeDatabase();
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(
    `llm-log-cleanup fixture wrote ${mainOut} (${spec.users.length} users, ` +
      `${spec.jobs.length} jobs) and ${logsOut} (${spec.logs.length} logs)\n`,
  );
}

/** A stable, pinned chat_settings id derived from the user's — no minting. */
function settingsIdFor(userId: string): string {
  return `c${userId.slice(1)}`;
}

main().catch((err) => {
  process.stderr.write(`${err instanceof Error ? err.stack : String(err)}\n`);
  process.exit(1);
});
