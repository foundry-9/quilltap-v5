/**
 * @jest-environment node
 *
 * Tier-2 ORACLE for the extraction ENQUEUE surface at the `83334b9a5` pin (work
 * order P4.D296 Tier 1 #14–#15): v4's `enqueueMemoryExtraction` (the strict
 * five-term dedupe — chatId, `turnOpenerMessageId` by `===`, the anchor `?? null`,
 * the pass via `memoryExtractionPassOf`, the cursor `?? null` — L-6's absent key,
 * `skipDedupCheck`), `enqueueMemoryExtractionBatch` (`backfill: true`, the
 * conditional `extractAfterMessageId`, the priority option) and
 * `handleQueueMemories` (`app/api/v1/chats/[id]/actions/memories.ts`: the seat /
 * default-cheap / user-defined profile arms, the greeting turn, the autonomous
 * bounds, both empty-chat refusals). No model call; v4's job host is held off
 * and `ensureProcessorRunning` is a no-op (memory note
 * `v4-oracle-job-host-and-log-capture`).
 *
 * Per op: whether the returned job id was already pending (the dedupe) and the
 * payload BYTES of the row it names; the batch's payload bytes in order; the
 * handler's status and body; the whole `background_jobs` table; the captured
 * `[Chats v1]` / `[MemoryExtraction]` / `Memory extraction batch enqueued` lines.
 *
 * Run from the v4 checkout (or the lane's pin) under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-extraction-enqueue-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/extraction-enqueue-tier2.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/extraction-enqueue-tier2.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SPEC=extraction-enqueue-tier2.json QT_FIXTURE_OUT=/tmp/qt-extraction-enqueue-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-extraction-enqueue-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-memory-processor-fixture.ts
 *   QT_FIXTURE_ENQUEUE_MAIN=/tmp/qt-extraction-enqueue-main.db QT_FIXTURE_ENQUEUE_MOUNT=/tmp/qt-extraction-enqueue-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-extraction-enqueue.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 --roots "$PWD" --roots "$TMPO/cases" -- "extraction-enqueue-tier2\\.test\\.ts$"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

type Json = Record<string, any>; // eslint-disable-line @typescript-eslint/no-explicit-any

const CAPTURED_PREFIXES = ['[Chats v1]', '[MemoryExtraction]', 'Memory extraction batch enqueued'];

function ownContext(ctx: Record<string, unknown> | undefined): Record<string, unknown> {
  const { service: _s, environment: _e, ...rest } = ctx ?? {};
  return rest;
}

async function main(): Promise<void> {
  // Hold v4's job host off BEFORE any import (memory note
  // `v4-oracle-job-host-and-log-capture` §1).
  (globalThis as unknown as Record<string, unknown>).__quilltapJobHost = {
    child: null,
    spawning: false,
    shuttingDown: true,
    childCrashed: false,
    restartTimestamps: [],
    signalHandlersInstalled: false,
    invalidationListeners: new Set(),
  };
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'extraction-enqueue-tier2.json'), 'utf8')) as Json;
  const fixtureMain = process.env.QT_FIXTURE_ENQUEUE_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_ENQUEUE_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_ENQUEUE_MAIN / _MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-extraction-enqueue-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'main.db');
  const workMount = join(scratch, 'mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  // A scratch llm-logs DB for the un-mocked `logLLMCall` (never the live one).
  process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'llm-logs.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  delete process.env.QUILLTAP_JOB_CHILD;
  delete process.env.QUILLTAP_TIMEZONE;
  process.env.LOG_LEVEL = 'error';

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(process.cwd(), 'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers');
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { enqueueMemoryExtraction, enqueueMemoryExtractionBatch } = await import('@/lib/background-jobs/queue-service');
  const { handleQueueMemories } = await import('@/app/api/v1/chats/[id]/actions/memories');
  const { Logger } = await import('@/lib/logger');

  let sink: Json[] | null = null;
  for (const level of ['error', 'warn', 'info', 'debug'] as const) {
    const proto = Logger.prototype as unknown as Record<string, (...a: unknown[]) => void>;
    const original = proto[level];
    proto[level] = function (this: { context?: Record<string, unknown> }, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
      const merged = { ...ownContext(this.context), ...(context ?? {}) };
      if (sink && typeof message === 'string' && CAPTURED_PREFIXES.some((p) => message.startsWith(p))) {
        sink.push({ level, message, context: JSON.parse(JSON.stringify(merged)) });
      }
      return original.call(this, message, context, ...rest);
    };
  }

  await initializeDatabase();
  const repos = getRepositories();
  const userId = spec.userId as string;
  const lines: string[] = [];

  const jobsDump = async (): Promise<Json[]> =>
    (await rawQuery(
      `SELECT "type", "payload", "status", "priority", "maxAttempts", "attempts" FROM "background_jobs" ORDER BY "payload", "status"`,
    )) as Json[];
  const payloadOf = async (id: string): Promise<string | null> =>
    (((await rawQuery(`SELECT "payload" FROM "background_jobs" WHERE "id" = ?`, [id])) as Json[])[0]?.payload as string) ?? null;
  const baseCheap = JSON.stringify(spec.chatSettings.cheapLLMSettings);
  const profileId = spec.connectionProfiles[0].id as string;

  for (const op of spec.ops as Json[]) {
    const plants = op.plants ?? {};
    if (plants.cheapLLMSettings) {
      await rawQuery(`UPDATE "chat_settings" SET "cheapLLMSettings" = ?`, [JSON.stringify(plants.cheapLLMSettings)]);
    }
    const before = new Set(((await rawQuery(`SELECT "id" FROM "background_jobs"`)) as Json[]).map((r) => r.id as string));

    sink = [];
    let result: Json;
    try {
      if (op.kind === 'enqueue') {
        const id = await enqueueMemoryExtraction(userId, op.payload, op.options);
        if (op.rewritePayload) {
          await rawQuery(`UPDATE "background_jobs" SET "payload" = ? WHERE "id" = ?`, [op.rewritePayload, id]);
        }
        if (op.stripKey) {
          await rawQuery(`UPDATE "background_jobs" SET "payload" = json_remove("payload", '$.' || ?) WHERE "id" = ?`, [op.stripKey, id]);
        }
        result = { reused: before.has(id), payload: await payloadOf(id) };
      } else if (op.kind === 'batch') {
        const ids = await enqueueMemoryExtractionBatch(userId, op.chatId, profileId, op.entries, { priority: op.priority });
        result = { payloads: await Promise.all(ids.map((id) => payloadOf(id))) };
      } else {
        const chat = await repos.chats.findById(op.chatId);
        if (!chat) throw new Error(`queue-memories op ${op.name}: chat not found`);
        const res = await handleQueueMemories({} as never, op.chatId, chat as never, { user: { id: userId }, repos } as never);
        result = { status: (res as Response).status, body: await (res as Response).json() };
      }
    } catch (error) {
      result = { error: error instanceof Error ? error.message : String(error) };
    }
    const captured = sink;
    sink = null;
    if (plants.cheapLLMSettings) await rawQuery(`UPDATE "chat_settings" SET "cheapLLMSettings" = ?`, [baseCheap]);
    lines.push(JSON.stringify({ kind: 'op', op: op.name, result, jobs: await jobsDump(), lines: captured }));
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`extraction-enqueue oracle wrote ${outPath}\n`);
}

test('extraction enqueue tier-2 oracle', async () => {
  await main();
});

const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
