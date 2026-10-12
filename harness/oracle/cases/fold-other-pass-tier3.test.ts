/**
 * @jest-environment node
 *
 * Tier-3 (mocked cheap-LLM + record-and-replay embedding) ORACLE for the
 * fold-grain OTHER pass (P4.D270 — v4 `d58548051`, pin `01a83539d`):
 * `runFoldOtherPass`, `findFoldOtherCatchupCandidates` (lib/memory/
 * fold-other-pass.ts), `enqueueFoldOtherCatchups` (lib/background-jobs/
 * maintenance/fold-other-catchup.ts) and `handleMemoryExtraction`'s
 * `foldOtherCatchup` branch (→ `runFoldOtherCatchup`), all v4's REAL code over
 * the REAL fixture pair `build-fold-other-pass-fixture.ts` bakes.
 *
 * Mocked (the seams the Rust port injects) — nothing else:
 *   - `createLLMProvider` — every call must carry the GRAIN block (anything
 *     else is an unrecognized prompt and throws); the rule is the one whose
 *     `op` matches the `[op:<tag>]` the orienting chat summary carries and
 *     whose `observer` prefixes the `OBSERVER:` label. The exact
 *     `provider|model|temperature|messages` key is RECORDED (`kind:"canned"`)
 *     for the Rust side's `CannedCompletionProvider`; a rule with `error`
 *     records then THROWS (a `Request timed out.` error is a timeout).
 *   - `generateEmbeddingForUser` — record-and-replay: the n-th DISTINCT text
 *     embeds to the one-hot e_n (dim 256), recorded as `kind:"cannedEmbedding"`.
 *   - `getApiKeyForCheapLLMSelection` → a constant; `ensureProcessorRunning` →
 *     no-op, and v4's job host is held off before any import (memory note
 *     `v4-oracle-job-host-and-log-capture` §1), so the PENDING rows the
 *     enqueue ops write stay PENDING and the dedupe reads them.
 *   - `logLLMCall` stays jest.setup's global mock: this family writes ZERO
 *     `llm_logs` rows (`memory_pipeline_jobs_tier3` compares the fold OTHER
 *     call's rows end to end).
 * `@/lib/instance-settings` is NOT mocked: the mode, the limits and the
 * fold cap are read by v4's REAL getters; the per-op plants go through v4's
 * REAL setters and are deleted after the op.
 *
 * Log lines: `Logger.prototype.{error,warn,info,debug}` is wrapped (the
 * fold-episode family's idiom — above `log()`'s level filter, and a child
 * logger's bound `module` never reaches the wrapped call's context): every
 * `[FoldOtherPass]` / `[FoldOtherCatchup]` / `[MemoryExtraction] Fold-grain…` /
 * `[Canon]` line and the sweep's four lines land per op as `kind:"logs"`.
 *
 * Run from the v4 server checkout under Node 24 (jest ignores .claude/ paths —
 * mirror the cases to /tmp):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-fop-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/fold-other-pass-tier3.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/fold-other-pass-tier3.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-fop-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-fop-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-fold-other-pass-fixture.ts
 *   TZ=UTC QT_FIXTURE_FOP_MAIN=/tmp/qt-fop-main.db QT_FIXTURE_FOP_MOUNT=/tmp/qt-fop-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-fold-other-pass.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 --roots "$PWD" --roots "$TMPO/cases" -- "fold-other-pass-tier3\\.test\\.ts$"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

interface Rule {
  op: string;
  observer: string;
  response: string;
  error?: string;
  usage: { promptTokens: number; completionTokens: number; totalTokens: number };
}
interface OpSpec {
  op: 'pass' | 'find' | 'enqueue' | 'handler';
  name: string;
  chatId?: string;
  tag?: string;
  mode?: Record<string, unknown>;
  limits?: Record<string, unknown>;
  inAutonomousRoom?: boolean;
  timelineMode?: 'realtime' | 'narrative';
  limit?: number | null;
  priorTurnJob?: { chatId: string; lastMessageId: string; connectionProfileId: string };
  foldOtherCatchup?: unknown;
  foldOtherIgnoreIdle?: boolean;
  plantLastMessageAtNow?: boolean;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  fixedNow: string;
  connectionProfileId: string;
  chatSettings: { cheapLLMSettings: { strategy: string; fallbackToLocal: boolean } };
  completionRules: Rule[];
  ops: OpSpec[];
}

const EMBED_DIM = 256;
const CAPTURED_PREFIXES = ['[FoldOtherPass]', '[FoldOtherCatchup]', '[MemoryExtraction] Fold-grain', '[Canon]'];
const CAPTURED_SWEEP = new Set([
  'Enqueued fold-grain OTHER catch-up',
  'Failed to enqueue fold-grain OTHER catch-up',
  'Fold-grain OTHER catch-up capped; remaining chats wait for the next sweep',
  'Fold-grain OTHER catch-up sweep done',
]);

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
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'fold-other-pass-tier3.json'), 'utf8')
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_FOP_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_FOP_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_FOP_MAIN / QT_FIXTURE_FOP_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-fop-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'fop-main.db');
  const workMount = join(scratch, 'fop-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'fop-llm-logs.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cannedRecorded = new Map<string, Record<string, unknown>>();
  const embeddingOrder: string[] = [];
  const unrecognized: string[] = [];

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories')
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store')
  );
  jest.doMock('@/lib/embedding/embedding-service', () => {
    const actual = jest.requireActual('@/lib/embedding/embedding-service');
    return {
      __esModule: true,
      ...actual,
      generateEmbeddingForUser: async (text: string) => {
        let idx = embeddingOrder.indexOf(text);
        if (idx === -1) {
          embeddingOrder.push(text);
          idx = embeddingOrder.length - 1;
        }
        if (idx >= EMBED_DIM) throw new Error('fold-other-pass oracle: raise EMBED_DIM');
        const vec = new Array(EMBED_DIM).fill(0);
        vec[idx] = 1;
        return { embedding: new Float32Array(vec), model: 'canned', dimensions: EMBED_DIM, provider: 'canned' };
      },
    };
  });
  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async (provider: string, _baseUrl?: string) => ({
        sendMessage: async (
          params: { messages: Array<{ role: string; content: string }>; model: string; temperature?: number },
          _apiKey: string
        ) => {
          const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
          const system = messages.find((m) => m.role === 'system')?.content ?? '';
          if (!system.includes('GRAIN — READ THIS FIRST')) {
            unrecognized.push(system.slice(0, 120));
            throw new Error('canned completion: unrecognized system prompt');
          }
          const tag = system.match(/\[op:([a-z0-9_]+)\]/)?.[1];
          const observer = system.match(/\nOBSERVER: ([^\n]*)\n/)?.[1] ?? '';
          const rule = spec.completionRules.find((r) => r.op === tag && observer.startsWith(r.observer));
          if (!rule) {
            unrecognized.push(`no rule for op=${tag} observer=${observer}`);
            throw new Error(`no completion rule for op=${tag} observer=${observer}`);
          }
          const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
          if (!cannedRecorded.has(key)) {
            cannedRecorded.set(key, {
              provider,
              model: params.model,
              temperature: params.temperature ?? null,
              messages,
              response: rule.response,
              usage: rule.usage,
              ...(rule.error ? { fail: rule.error } : {}),
            });
          }
          if (rule.error) throw new Error(rule.error);
          return { content: rule.response, finishReason: 'stop', usage: rule.usage };
        },
      }),
    };
  });
  jest.doMock('@/lib/services/api-key.service', () => {
    const actual = jest.requireActual('@/lib/services/api-key.service');
    return { __esModule: true, ...actual, getApiKeyForCheapLLMSelection: async () => 'test-key' };
  });
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { runFoldOtherPass, findFoldOtherCatchupCandidates } = await import(
    '@/lib/memory/fold-other-pass'
  );
  const { enqueueFoldOtherCatchups } = await import(
    '@/lib/background-jobs/maintenance/fold-other-catchup'
  );
  const { enqueueMemoryExtraction } = await import('@/lib/background-jobs/queue-service');
  const { handleMemoryExtraction } = await import('@/lib/background-jobs/handlers/memory-extraction');
  const { setMemoryExtractionModeSettings, setMemoryExtractionLimits } = await import(
    '@/lib/instance-settings'
  );
  const { getCheapLLMProvider } = await import('@/lib/llm/cheap-llm');
  const { resolveMaxTokens } = await import('@/lib/llm/model-context-data');
  const { Logger } = await import('@/lib/logger');

  let sink: Array<{ level: string; message: string; context: unknown }> | null = null;
  for (const level of ['error', 'warn', 'info', 'debug'] as const) {
    const original = (Logger.prototype as unknown as Record<string, (...a: unknown[]) => void>)[level];
    (Logger.prototype as unknown as Record<string, unknown>)[level] = function (
      this: unknown,
      message: string,
      context?: Record<string, unknown>,
      ...rest: unknown[]
    ) {
      if (
        sink &&
        typeof message === 'string' &&
        (CAPTURED_PREFIXES.some((p) => message.startsWith(p)) || CAPTURED_SWEEP.has(message))
      ) {
        sink.push({ level, message, context: JSON.parse(JSON.stringify(context ?? {})) });
      }
      return original.call(this, message, context, ...rest);
    };
  }

  await initializeDatabase();
  const repos = getRepositories();

  const availableProfiles = await repos.connections.findByUserId(spec.userId);
  const currentProfile = availableProfiles.find((p) => p.id === spec.connectionProfileId)!;
  const selection = getCheapLLMProvider(
    currentProfile,
    {
      strategy: spec.chatSettings.cheapLLMSettings.strategy as never,
      fallbackToLocal: spec.chatSettings.cheapLLMSettings.fallbackToLocal,
    },
    availableProfiles,
    false
  );
  const now = Date.parse(spec.fixedNow);

  const lines: string[] = [];
  for (const op of spec.ops) {
    if (op.mode) await setMemoryExtractionModeSettings(op.mode as never);
    if (op.limits) await setMemoryExtractionLimits(op.limits as never);
    sink = [];
    let result: unknown = null;
    let error: string | null = null;
    try {
      if (op.op === 'pass') {
        const window = (await repos.chats.getMessages(op.chatId!)).filter((m) => m.type === 'message');
        result = await runFoldOtherPass({
          chatId: op.chatId!,
          userId: spec.userId,
          windowMessages: window as never,
          cheapLLM: selection,
          cheapMaxTokens: resolveMaxTokens(currentProfile),
          timelineMode: op.timelineMode ?? 'realtime',
          projectId: null,
          inAutonomousRoom: op.inAutonomousRoom ?? false,
          projectDescription: null,
          chatContextSummary: `[op:${op.tag}] The household is planning the harbor festival.`,
        });
      } else if (op.op === 'find') {
        result = await findFoldOtherCatchupCandidates({ now, limit: op.limit ?? undefined });
      } else if (op.op === 'enqueue') {
        if (op.priorTurnJob) {
          await enqueueMemoryExtraction(spec.userId, {
            chatId: op.priorTurnJob.chatId,
            turnOpenerMessageId: null,
            extractionAnchorMessageId: op.priorTurnJob.lastMessageId,
            connectionProfileId: op.priorTurnJob.connectionProfileId,
          });
        }
        result = await enqueueFoldOtherCatchups({ now, limit: op.limit ?? undefined });
      } else {
        if (op.plantLastMessageAtNow) {
          await rawQuery(`UPDATE "chats" SET "lastMessageAt" = ? WHERE "id" = ?`, [
            new Date().toISOString(),
            op.chatId!,
          ]);
        }
        const job = {
          id: `job-${op.name}`,
          userId: spec.userId,
          type: 'MEMORY_EXTRACTION',
          payload: {
            chatId: op.chatId,
            turnOpenerMessageId: null,
            extractionAnchorMessageId: null,
            connectionProfileId: spec.connectionProfileId,
            foldOtherCatchup: op.foldOtherCatchup,
            ...(op.foldOtherIgnoreIdle !== undefined ? { foldOtherIgnoreIdle: op.foldOtherIgnoreIdle } : {}),
          },
        };
        await handleMemoryExtraction(job as never);
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
    // Let the gate's fire-and-forget housekeeping probes settle.
    await new Promise((resolve) => setTimeout(resolve, 50));
    if (op.mode) await rawQuery(`DELETE FROM "instance_settings" WHERE "key" = 'memoryExtractionMode'`);
    if (op.limits) await rawQuery(`DELETE FROM "instance_settings" WHERE "key" = 'memoryExtractionLimits'`);
    lines.push(JSON.stringify({ kind: 'result', op: op.name, result, error }));
    lines.push(JSON.stringify({ kind: 'logs', op: op.name, lines: sink }));
    sink = null;
  }

  if (unrecognized.length > 0) {
    // Loud: an unanswered call is a corpus gap, not v4 behaviour under test.
    throw new Error(`unanswered completion call(s): ${JSON.stringify(unrecognized)}`);
  }

  for (const entry of cannedRecorded.values()) {
    lines.push(JSON.stringify({ kind: 'canned', ...entry }));
  }
  embeddingOrder.forEach((text, idx) => {
    lines.push(JSON.stringify({ kind: 'cannedEmbedding', text, index: idx, dim: EMBED_DIM }));
  });

  const dump = async (sql: string) =>
    ((await rawQuery(sql)) as Array<Record<string, unknown>>).map((r) => {
      const out: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(r)) out[k] = canonValue(v);
      return out;
    });
  lines.push(JSON.stringify({ kind: 'table', table: 'memories', rows: await dump('SELECT * FROM memories ORDER BY content, characterId') }));
  lines.push(
    JSON.stringify({
      kind: 'table',
      table: 'chats',
      rows: await dump(
        'SELECT id, "otherExtractionWatermarkMessageId", "updatedAt", "lastMessageAt", "messageCount" FROM chats ORDER BY id'
      ),
    })
  );
  lines.push(
    JSON.stringify({
      kind: 'table',
      table: 'background_jobs',
      rows: await dump('SELECT type, payload, status, "maxAttempts", "attempts" FROM background_jobs ORDER BY payload, status'),
    })
  );

  closeMountIndexSQLiteClient();
  await closeDatabase();

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`fold-other-pass oracle wrote ${outPath}\n`);
}

test('fold-other-pass tier-3 oracle', async () => {
  await main();
});

const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
