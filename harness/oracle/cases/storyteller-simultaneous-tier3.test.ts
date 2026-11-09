/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for Storyteller "All at once" (P4.D368, item 6 —
 * `storyteller_simultaneous_tier3_equivalence`).
 *
 * Drives v4's REAL `handleStorytellerRun(repos, chatId, userId, { roundId,
 * participantIds, order: 'simultaneous', keepOrder: true })`
 * (`lib/services/chat-message/orchestrator.service.ts:330`) — the REAL
 * `planStorytellerRun` + `runStorytellerRound` + `processMessage` per seat —
 * over a COPY of the MINTED fixture trio built by
 * `build-storyteller-simultaneous-fixture.ts`, draining the SSE stream it
 * returns.
 *
 * Mocked (the model boundary and the job host only — every Salon subsystem runs
 * REAL, because the Rust side runs the REAL host `ChatSpine`):
 *   - `createLLMProvider` — `streamMessage` answers by SEAT (the responding
 *     character's name in the system prompt): `<Name> answers the storm.` in
 *     two content chunks + a done with usage, or, for the row's failing seat,
 *     a provider error after the first chunk. Prompt bytes are NOT the
 *     family's comparand (that is `orchestrator_tier3`'s job) — keying by seat
 *     keeps each seat's call a distinct entry without a byte-exact prompt
 *     match. `sendMessage` (any cheap-LLM call) answers `{}`.
 *   - embeddings: a fixed vector; the pricing cache: empty; the API-key
 *     lookups: a fixed key; the job processor: never started, the host
 *     `shuttingDown` (the round's housekeeping ENQUEUES; nothing may claim).
 *
 * Rows (each on its OWN copy of the fixture):
 *   (i)   `main` — Ada + Bram (Simple, profile A) all at once, then Cora's Act
 *         tail on profile B.
 *   (ii)  `stop` — the operator presses Stop from inside Ada's stream (v4's
 *         REAL `stopActiveTurns`): both concurrent turns finish, the tail does
 *         not run, `chainComplete {reason:"stopped", …, paused:true}`.
 *   (iii) `failure` — Bram's provider errors mid-stream (item 7).
 *
 * Emits one NDJSON row per case: `{ name, frames, rows, logs, threw }`.
 *
 * Run (Node 24, from the PINNED v4 worktree), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   mkdir -p /tmp/qt-sts3-jest && cp $V5W/harness/oracle/cases/storyteller-simultaneous-tier3.test.ts /tmp/qt-sts3-jest/
 *   TZ=UTC QT_FIXTURE_STS3_MAIN=/tmp/qt-sts3-main.db QT_FIXTURE_STS3_MOUNT=/tmp/qt-sts3-mount.db \
 *   QT_FIXTURE_STS3_LLMLOGS=/tmp/qt-sts3-llmlogs.db QT_ORACLE_OUT=/tmp/oracle-sts3.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots /tmp/qt-sts3-jest -- storyteller-simultaneous-tier3
 */

process.env.TZ = 'UTC';

import * as fs from 'fs';
import { join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

const TEST_PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
const USER = 'ffffffff-ffff-ffff-ffff-ffffffffffff';
const CHAT = 'c7000000-0000-4000-8000-000000000001';
const P_ADA = 'e7000000-0000-4000-8000-0000000000a1';
const P_BRAM = 'e7000000-0000-4000-8000-0000000000b2';
const P_CORA = 'e7000000-0000-4000-8000-0000000000c3';
const ROUNDS = ['57000000-0000-4000-8000-000000000001', '57000000-0000-4000-8000-000000000002', '57000000-0000-4000-8000-000000000003'];

type Case = { name: string; roundId: string; stopDuring?: string; failSeat?: string };
const CASES: Case[] = [
  { name: 'main', roundId: ROUNDS[0] },
  { name: 'stop', roundId: ROUNDS[1], stopDuring: 'Ada' },
  { name: 'failure', roundId: ROUNDS[2], failSeat: 'Bram' },
];
const SEATS = ['Ada', 'Bram', 'Cora'];

/** The pinned log lines (message → kept). */
const PINNED = [
  'Storyteller round running',
  'Storyteller round closed',
  '[TurnPreparation] Prospero context refresh due',
  'Streaming error',
];

async function runCase(c: Case): Promise<Record<string, unknown>> {
  const mainSrc = process.env.QT_FIXTURE_STS3_MAIN!;
  const mountSrc = process.env.QT_FIXTURE_STS3_MOUNT!;
  const llSrc = process.env.QT_FIXTURE_STS3_LLMLOGS!;
  const scratch = mkdtempSync(join(tmpdir(), 'qt-sts3-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'main.db');
  const workMount = join(scratch, 'mount.db');
  const workLl = join(scratch, 'llm-logs.db');
  copyFileSync(mainSrc, workMain);
  copyFileSync(mountSrc, workMount);
  copyFileSync(llSrc, workLl);

  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.SQLITE_LLM_LOGS_PATH = workLl;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'debug';

  (globalThis as Record<string, unknown>).__quilltapJobHost = {
    child: null, spawning: false, shuttingDown: true, childCrashed: true,
    restartTimestamps: [], signalHandlersInstalled: true, invalidationListeners: new Set(),
  };

  let stopFired = false;
  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/llm/pricing-fetcher', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/llm/pricing-fetcher'),
    getPricingCache: async () => ({ providers: {} }),
  }));
  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async () => ({
        supportsWebSearch: false,
        streamMessage: async function* streamMessage(params: { messages: Array<{ role: string; content: string }> }) {
          const system = params.messages.find((m) => m.role === 'system')?.content ?? '';
          const seat = SEATS.find((s) => system.includes(`You are ${s}`)) ?? SEATS.find((s) => system.includes(s)) ?? '?';
          yield { content: `${seat} answers ` };
          if (c.stopDuring === seat && !stopFired) {
            stopFired = true;
            const { stopActiveTurns } = await import('@/lib/chat/turn-control');
            void stopActiveTurns(CHAT);
          }
          if (c.failSeat === seat) throw new Error(`${seat}'s provider fell over`);
          yield { content: 'the storm.' };
          yield {
            done: true,
            usage: { promptTokens: 10, completionTokens: 4, totalTokens: 14 },
            cacheUsage: undefined,
            rawProviderUsage: undefined,
            attachmentResults: undefined,
            rawResponse: undefined,
          };
        },
        sendMessage: async () => ({ content: '{}', finishReason: 'stop', usage: undefined }),
      }),
    };
  });
  jest.doMock('@/lib/embedding/embedding-service', () => {
    const actual = jest.requireActual('@/lib/embedding/embedding-service');
    return {
      __esModule: true,
      ...actual,
      generateEmbeddingForUser: async () => ({
        embedding: new Float32Array([0.1, 0.2, 0.3]),
        model: 'canned',
        provider: 'canned',
        dimensions: 3,
      }),
    };
  });
  jest.doMock('@/lib/plugins/provider-validation', () => {
    const actual = jest.requireActual('@/lib/plugins/provider-validation');
    return { __esModule: true, ...actual, requiresApiKey: () => false };
  });
  jest.doMock('@/lib/services/api-key.service', () => {
    const actual = jest.requireActual('@/lib/services/api-key.service');
    return {
      __esModule: true,
      ...actual,
      getApiKeyForCheapLLMSelection: async () => 'test-key',
      getApiKeyForProfile: async () => 'test-key',
    };
  });
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
  });

  const { logger } = await import('@/lib/logger');
  const lines: unknown[] = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown> };
      if (!PINNED.includes(data.message)) return;
      const { service: _s, environment: _e, ...fields } = (data.context ?? {}) as Record<string, unknown>;
      lines.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
    },
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { handleStorytellerRun } = await import('@/lib/services/chat-message/orchestrator.service');
  await initializeDatabase();
  const repos = getRepositories();

  let threw: string | null = null;
  const frames: unknown[] = [];
  try {
    const stream = await handleStorytellerRun(repos as never, CHAT, USER, {
      roundId: c.roundId,
      participantIds: [P_ADA, P_BRAM, P_CORA],
      order: 'simultaneous',
      keepOrder: true,
    });
    const reader = stream.getReader();
    const decoder = new TextDecoder();
    let buf = '';
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      buf += decoder.decode(value, { stream: true });
    }
    for (const chunk of buf.split('\n\n')) {
      for (const line of chunk.split('\n')) {
        if (line.startsWith('data: ')) frames.push(JSON.parse(line.slice(6)));
      }
    }
  } catch (e) {
    threw = e instanceof Error ? e.message : String(e);
  }
  // Fire-and-forget tails (the housekeeping enqueues) settle before the dumps.
  await new Promise((r) => setTimeout(r, 300));
  await new Promise((r) => setImmediate(r));

  // The round's NEW rows (the five planted messages are skipped), content
  // through v4's REAL compressed-text codec. Insert order among concurrent
  // turns is scheduling: the Rust side sorts both sides the same way.
  const { blobToText } = await import('@/lib/database/text-compression');
  const messages = ((await rawQuery(
    `SELECT "role", "content", "participantId", "storytellerBid", "systemSender", "targetParticipantIds"
       FROM "chat_messages" WHERE "chatId" = ? AND "type" = 'message' ORDER BY "createdAt", "rowid"`,
    [CHAT],
  )) as Array<Record<string, unknown>>)
    .slice(5)
    .map((m) => ({ ...m, content: blobToText(m.content) }));
  const round = ((await rawQuery(
    `SELECT "status", "selection", "closedReason" FROM "storyteller_rounds" WHERE "id" = ?`,
    [c.roundId],
  )) as Array<Record<string, unknown>>)[0] ?? null;
  const { getRawLLMLogsDatabase } = await import('@/lib/database/backends/sqlite/llm-logs-client');
  const lldb = getRawLLMLogsDatabase();
  let llm: unknown = null;
  try {
    llm = lldb
      ? (lldb.prepare('SELECT "type", "storytellerRoundId" FROM "llm_logs" ORDER BY "type", "storytellerRoundId"').all() as unknown[])
      : null;
  } catch (e) {
    llm = { error: e instanceof Error ? e.message : String(e) };
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  rmSync(scratch, { recursive: true, force: true });
  return { name: c.name, threw, frames, rows: { messages, round, llm }, logs: lines };
}

test('storyteller simultaneous tier-3 oracle', async () => {
  const out = process.env.QT_ORACLE_OUT;
  if (!out) throw new Error('QT_ORACLE_OUT unset');
  for (const v of ['QT_FIXTURE_STS3_MAIN', 'QT_FIXTURE_STS3_MOUNT', 'QT_FIXTURE_STS3_LLMLOGS']) {
    if (!process.env[v]) throw new Error(`${v} unset`);
  }
  const rows: string[] = [];
  for (const c of CASES) rows.push(JSON.stringify(await runCase(c)));
  fs.writeFileSync(out, rows.join('\n') + '\n');
});
