/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the memory-REGENERATE job handlers at the `83334b9a5` pin
 * (work order P4.D296 R-V): v4's `handleMemoryRegenerateChat`
 * (`lib/background-jobs/handlers/memory-regenerate-chat.ts`) and
 * `handleMemoryRegenerateAll` (`memory-regenerate-all.ts`) over the shared
 * fixture builder. Mirrors v4's own `memory-regenerate-chat-fold-catchup.test.ts`
 * on real repositories: the wipe (`retireChatMemories` — MANUAL rows spared) and
 * the cursors cleared; one turn job per turn without the dedupe scan; an
 * autonomous room's bounded turns; the catch-ups after every turn job (only
 * `episode-fold` in turn mode, the `regenerate:<jobId>` anchor); the orphan arm;
 * the profile throw BEFORE any wipe; and the fan-out (the in-flight snapshot,
 * the Unmoderated chat's dangerous profile, the orphan chat ids). No model call;
 * v4's job host is held off and `ensureProcessorRunning` is a no-op.
 *
 * Per op: the handler's thrown message (or null), then the `background_jobs`,
 * `memory_extraction_cursors` and `memories` tables, and the captured
 * `[MemoryRegenerateChat]` / `[MemoryRegenerateAll]` lines.
 *
 * Run from the v4 checkout (or the lane's pin) under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-memory-regenerate-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/memory-regenerate-jobs-tier3.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/memory-regenerate-jobs-tier3.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SPEC=memory-regenerate-jobs-tier3.json QT_FIXTURE_OUT=/tmp/qt-memory-regenerate-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-memory-regenerate-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-memory-processor-fixture.ts
 *   QT_FIXTURE_REGENERATE_MAIN=/tmp/qt-memory-regenerate-main.db QT_FIXTURE_REGENERATE_MOUNT=/tmp/qt-memory-regenerate-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-memory-regenerate.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 --roots "$PWD" --roots "$TMPO/cases" -- "memory-regenerate-jobs-tier3\\.test\\.ts$"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

type Json = Record<string, any>; // eslint-disable-line @typescript-eslint/no-explicit-any

const CAPTURED_PREFIXES = [
  '[MemoryRegenerateChat]',
  '[MemoryRegenerateAll]',
  // P4.168 (survey B5): the repository fallback lines the fan-out's three
  // reads log.
  'Error finding entities by filter',
  'Error finding background jobs by user ID',
  'Error listing distinct memory chatIds',
];

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
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'memory-regenerate-jobs-tier3.json'), 'utf8')) as Json;
  const fixtureMain = process.env.QT_FIXTURE_REGENERATE_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_REGENERATE_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_REGENERATE_MAIN / _MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-memory-regenerate-oracle-'));
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
  const { enqueueMemoryRegenerateChat } = await import('@/lib/background-jobs/queue-service');
  const { handleMemoryRegenerateChat } = await import('@/lib/background-jobs/handlers/memory-regenerate-chat');
  const { handleMemoryRegenerateAll } = await import('@/lib/background-jobs/handlers/memory-regenerate-all');
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
  const cursorsDump = async (): Promise<Json[]> =>
    (await rawQuery(`SELECT "id", "chatId", "pass", "messageId" FROM "memory_extraction_cursors" ORDER BY "id"`)) as Json[];
  const memoriesDump = async (): Promise<Json[]> =>
    (await rawQuery(`SELECT "id", "characterId", "chatId", "source", "content" FROM "memories" ORDER BY "id"`)) as Json[];

  for (const op of spec.ops as Json[]) {
    const plants = op.plants ?? {};
    if (plants.cursor) {
      const { chatId, ...byPass } = plants.cursor as Record<string, string>;
      for (const [pass, messageId] of Object.entries(byPass)) {
        await repos.memoryExtractionCursors.setCursor(chatId, pass as never, messageId);
      }
    }
    if (plants.mode) {
      await rawQuery(`INSERT INTO "instance_settings" ("key", "value") VALUES ('memoryExtractionMode', ?)`, [JSON.stringify(plants.mode)]);
    }
    if (plants.inFlightWipe) {
      await enqueueMemoryRegenerateChat(userId, { chatId: plants.inFlightWipe, connectionProfileId: spec.connectionProfiles[0].id });
    }

    // P4.168: a failing-read plant (a held table), undone right after the op.
    for (const sql of (plants.sqlBefore ?? []) as string[]) await rawQuery(sql);

    sink = [];
    let error: string | null = null;
    const job = { id: `job-${op.name}`, userId, type: op.kind === 'chat' ? 'MEMORY_REGENERATE_CHAT' : 'MEMORY_REGENERATE_ALL', payload: op.payload };
    try {
      if (op.kind === 'chat') await handleMemoryRegenerateChat(job as never);
      else await handleMemoryRegenerateAll(job as never);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
    const captured = sink;
    sink = null;
    for (const sql of (plants.sqlAfter ?? []) as string[]) await rawQuery(sql);
    if (plants.mode) await rawQuery(`DELETE FROM "instance_settings" WHERE "key" = 'memoryExtractionMode'`);
    lines.push(
      JSON.stringify({
        kind: 'op',
        op: op.name,
        error,
        jobs: await jobsDump(),
        cursors: await cursorsDump(),
        memories: await memoriesDump(),
        lines: captured,
      }),
    );
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`memory-regenerate oracle wrote ${outPath}\n`);
}

test('memory regenerate tier-3 oracle', async () => {
  await main();
});

const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
