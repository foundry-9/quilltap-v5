/**
 * @jest-environment node
 *
 * Tier-3 (mocked-completion + mocked-embedding) ORACLE for the fold
 * CATCH-UPS at the `83334b9a5` pin (work order P4.D296 Tier 1 #13–#14):
 * `findFoldCatchupCandidates` + `runFoldCatchup` (`lib/memory/extraction/
 * catchup.ts`), the data tick's `enqueueFoldCatchups` (`lib/background-jobs/
 * maintenance/fold-catchup.ts`, bug 232's in-flight guard) and
 * `handleMemoryExtraction`'s pass branch (`lib/background-jobs/handlers/
 * memory-extraction.ts` — the continuation, the legacy `foldOtherCatchup` /
 * `foldEpisodeCatchup` / `foldOtherIgnoreIdle` keys, a timeout failing the
 * job, a missing chat, and L-4's unknown `bogus` pass run TWICE, with v4's
 * own `findByChatId` / `findAllByChat` over the raw row it writes —
 * `UNKNOWN_PASS_CURSOR_ROW`). Rewrites the IMPORT-DEAD case of the same name
 * (its subject, `fold-other-pass.ts`, was deleted by `ac6348c5f`); the Rust
 * family keeps its name, `fold_other_pass_tier3_equivalence` (L-20).
 *
 * Seams pinned as the fold family's (`fold-episode-tier3.test.ts`): the
 * completion by rule, the exact call keys RECORDED per op; embeddings by a
 * deterministic per-text vector, RECORDED; the API key; the maintenance
 * watermark; v4's job host held off (memory note
 * `v4-oracle-job-host-and-log-capture`) and `ensureProcessorRunning` a no-op.
 * Every rule's usage is zero, so no cost event is booked here (the pipeline
 * family compares it).
 *
 * Run from the v4 checkout (or the lane's pin) under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-fold-catchup-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/fold-other-pass-tier3.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/fold-other-pass-tier3.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SPEC=fold-other-pass-tier3.json QT_FIXTURE_OUT=/tmp/qt-fold-catchup-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-fold-catchup-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-memory-processor-fixture.ts
 *   QT_FIXTURE_FOLD_CATCHUP_MAIN=/tmp/qt-fold-catchup-main.db QT_FIXTURE_FOLD_CATCHUP_MOUNT=/tmp/qt-fold-catchup-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-fold-catchup.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 --roots "$PWD" --roots "$TMPO/cases" -- "fold-other-pass-tier3\\.test\\.ts$"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

type Json = Record<string, any>; // eslint-disable-line @typescript-eslint/no-explicit-any

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

/** Deterministic unit vector for a text (FNV-1a over UTF-16 code units + xorshift) — the admission rig's. */
function hashVector(text: string, dims: number): number[] {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  const out: number[] = [];
  let x = h || 1;
  for (let i = 0; i < dims; i++) {
    x ^= x << 13; x >>>= 0;
    x ^= x >>> 17;
    x ^= x << 5; x >>>= 0;
    out.push((x % 2000) / 1000 - 1);
  }
  const norm = Math.sqrt(out.reduce((s, y) => s + y * y, 0)) || 1;
  return out.map((y) => y / norm);
}

const CAPTURED_PREFIXES = [
  '[Memory]',
  '[ExtractionCursors]',
  '[MemorySettings]',
  '[MemoryCalendar]',
  '[Canon]',
  '[FoldCatchup]',
  '[MemoryExtraction]',
  // `maintenance.fold-catchup`'s child-logger lines carry no prefix.
  'Enqueued fold catch-up',
  'Catch-up already queued',
  'Fold catch-up capped',
  'Fold catch-up sweep done',
  'Could not read in-flight jobs',
  'Failed to enqueue fold catch-up',
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
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'fold-other-pass-tier3.json'), 'utf8')) as Json;
  const fixtureMain = process.env.QT_FIXTURE_FOLD_CATCHUP_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_FOLD_CATCHUP_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_FOLD_CATCHUP_MAIN / _MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-extraction-catchup-oracle-'));
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

  const cannedRecorded = new Map<string, Json>();
  const embeddings = new Map<string, number[]>();
  let failContains: string | null = null;
  let currentOp: string | null = null;

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(process.cwd(), 'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers');
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () => jest.requireActual('@/lib/embedding/vector-store'));
  jest.doMock('@/lib/embedding/embedding-service', () => {
    const actual = jest.requireActual('@/lib/embedding/embedding-service');
    return {
      __esModule: true,
      ...actual,
      generateEmbeddingForUser: async (text: string) => {
        const vec = hashVector(text, spec.embeddingDims);
        const f32 = new Float32Array(vec);
        embeddings.set(text, Array.from(f32));
        return { embedding: f32, model: 'canned', dimensions: f32.length, provider: 'canned' };
      },
    };
  });
  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async (provider: string) => ({
        sendMessage: async (params: { messages: Array<{ role: string; content: string }>; model: string; temperature?: number }) => {
          const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
          const system = messages.find((m) => m.role === 'system')?.content ?? '';
          const user = messages.find((m) => m.role === 'user')?.content ?? '';
          let pass: string;
          let label: string | null = null;
          if (system.startsWith('You are consolidating a batch')) {
            pass = 'episode';
          } else {
            const m = system.match(/\nCONTEXT\n(SUBJECT|OBSERVER): ([^\n]*)\n/);
            if (!m) throw new Error('canned completion: no CONTEXT footer label in system prompt');
            pass = m[1] === 'SUBJECT' ? 'self' : 'other';
            label = m[2];
          }
          const rule = (spec.completionRules as Json[]).find(
            (r) => r.pass === pass && (r.label === undefined || r.label === label) && (r.contains === undefined || user.includes(r.contains)),
          );
          if (!rule) throw new Error(`no completion rule for pass=${pass} label=${label}`);
          const failing = failContains !== null && user.includes(failContains);
          // Op-scoped: a slice that timed out in one op is the SAME call key
          // when a later op re-reads it and succeeds.
          const key = `${currentOp}|${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
          if (!cannedRecorded.has(key)) {
            cannedRecorded.set(key, {
              op: currentOp,
              provider,
              model: params.model,
              temperature: params.temperature ?? null,
              messages,
              response: rule.response,
              usage: rule.usage,
              failure: failing ? 'Request timed out.' : null,
            });
          }
          if (failing) throw new Error('Request timed out.');
          return { content: rule.response, finishReason: 'stop', usage: rule.usage };
        },
      }),
    };
  });
  jest.doMock('@/lib/services/api-key.service', () => {
    const actual = jest.requireActual('@/lib/services/api-key.service');
    return { __esModule: true, ...actual, getApiKeyForCheapLLMSelection: async () => 'test-key' };
  });
  jest.doMock('@/lib/services/llm-logging.service', () => jest.requireActual('@/lib/services/llm-logging.service'));
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
  });
  jest.doMock('@/lib/memory/maintenance/triggers', () => ({
    __esModule: true,
    maybeEnqueueMaintenance: async () => [],
  }));

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { buildExtractionContext, findFoldCatchupCandidates, runFoldCatchup } = await import('@/lib/memory/extraction');
  const { enqueueFoldCatchups } = await import('@/lib/background-jobs/maintenance/fold-catchup');
  const { handleMemoryExtraction } = await import('@/lib/background-jobs/handlers/memory-extraction');
  const admit = await import('@/lib/memory/admission/admit');
  const { resetRateLedgerForTests } = await import('@/lib/memory/admission/rate-ledger');
  const { memoryIndex } = await import('@/lib/memory/index-store');
  const { resetMemoryTimeZoneCacheForTests } = await import('@/lib/memory/time/calendar');
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
  const idx = memoryIndex as unknown as Record<string, (...a: unknown[]) => unknown>;
  for (const method of ['search', 'shape', 'entries']) {
    const original = idx[method].bind(memoryIndex);
    idx[method] = (...a: unknown[]) => {
      memoryIndex.reloadAll();
      return original(...a);
    };
  }

  await initializeDatabase();
  admit.resetAdmissionStateForTests();
  resetRateLedgerForTests();
  resetMemoryTimeZoneCacheForTests();
  const repos = getRepositories();
  const userId = spec.userId as string;
  const lines: string[] = [];

  const foldResultOf = (r: Json): Json => ({
    pass: r.pass,
    calls: r.calls,
    timedOut: r.timedOut,
    failed: r.failed,
    admitted: r.admitted.length,
    createdIds: r.createdIds,
    reinforcedIds: r.reinforcedIds,
    debugLogs: r.debugLogs,
    rateDeferred: r.rateDeferred,
    skippedReason: r.skippedReason ?? null,
    slices: r.slices,
    slicesDeferred: r.slicesDeferred,
    cursorAdvancedTo: r.cursorAdvancedTo,
  });
  const jobsDump = async (): Promise<Json[]> =>
    (await rawQuery(
      `SELECT "type", "payload", "status", "priority", "maxAttempts", "attempts" FROM "background_jobs" ORDER BY "payload", "status"`,
    )) as Json[];

  for (const op of spec.ops as Json[]) {
    const plants = op.plants ?? {};
    for (const [chatId, byPass] of Object.entries((plants.cursors ?? {}) as Record<string, Record<string, string | null>>)) {
      for (const [pass, messageId] of Object.entries(byPass)) {
        if (messageId === null) {
          await rawQuery(`DELETE FROM "memory_extraction_cursors" WHERE "chatId" = ? AND "pass" = ?`, [chatId, pass]);
        } else {
          await repos.memoryExtractionCursors.setCursor(chatId, pass as never, messageId);
        }
      }
    }
    if (plants.mode) {
      await rawQuery(`INSERT INTO "instance_settings" ("key", "value") VALUES ('memoryExtractionMode', ?)`, [JSON.stringify(plants.mode)]);
    }
    if (plants.lastMessageAtNow) {
      await rawQuery(`UPDATE "chats" SET "lastMessageAt" = ? WHERE "id" = ?`, [new Date().toISOString(), op.chatId]);
    }
    failContains = plants.failContains ?? null;
    currentOp = op.name;
    const now = op.now ? Date.parse(op.now) : undefined;

    sink = [];
    let result: Json;
    try {
      if (op.kind === 'select') {
        const sel = await findFoldCatchupCandidates(op.pass, { now, ...(op.limit !== undefined ? { limit: op.limit } : {}) });
        result = { candidates: sel.candidates, deferred: sel.deferred };
      } else if (op.kind === 'sweep') {
        const summary = await enqueueFoldCatchups(op.pass, { now, ...(op.limit !== undefined ? { limit: op.limit } : {}) });
        result = { summary, jobs: await jobsDump() };
      } else if (op.kind === 'catchup') {
        const ctx = await buildExtractionContext(op.chatId, userId, {});
        const r = await runFoldCatchup(ctx, op.pass, { now, ...(op.ignoreIdle ? { ignoreIdle: true } : {}) });
        result = foldResultOf(r);
      } else {
        const job = { id: `job-${op.name}`, userId, type: 'MEMORY_EXTRACTION', payload: op.payload };
        let error: string | null = null;
        try {
          await handleMemoryExtraction(job as never);
        } catch (e) {
          error = e instanceof Error ? e.message : String(e);
        }
        result = { error, jobs: await jobsDump() };
      }
    } catch (error) {
      result = { error: error instanceof Error ? error.message : String(error) };
    }
    const captured = sink;
    sink = null;

    failContains = null;
    if (plants.mode) await rawQuery(`DELETE FROM "instance_settings" WHERE "key" = 'memoryExtractionMode'`);

    const cursors = (await rawQuery(`SELECT "id", "chatId", "pass", "messageId" FROM "memory_extraction_cursors" ORDER BY "id"`)) as Json[];
    lines.push(JSON.stringify({ kind: 'op', op: op.name, result, cursors, lines: captured }));
  }

  // UNKNOWN_PASS_CURSOR_ROW (L-4): what v4's own reads do with the raw
  // `<chatId>:bogus` row the unknown pass wrote.
  {
    const chatId = spec.unknownPassChatId as string;
    const byChat = await repos.memoryExtractionCursors.findByChatId(chatId);
    const all = await repos.memoryExtractionCursors.findAllByChat();
    lines.push(
      JSON.stringify({
        kind: 'unknown-pass',
        findByChatId: byChat,
        findAllByChat: Object.fromEntries([...all.entries()].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))),
      }),
    );
  }

  for (const entry of cannedRecorded.values()) lines.push(JSON.stringify({ kind: 'canned', ...entry }));
  for (const [text, embedding] of embeddings) lines.push(JSON.stringify({ kind: 'embedding', text, embedding }));

  const columns = ((await rawQuery(`PRAGMA table_info(memories)`)) as Array<{ name: string }>).map((c) => c.name);
  const rawRows = (await rawQuery(`SELECT * FROM memories ORDER BY "content"`)) as Json[];
  const rows = rawRows.map((r) => {
    const out: Json = {};
    for (const col of columns) out[col] = canonValue(r[col]);
    return out;
  });
  lines.push(JSON.stringify({ kind: 'table', table: 'memories', columns, rows }));

  closeMountIndexSQLiteClient();
  await closeDatabase();
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`extraction-catchup oracle wrote ${outPath}\n`);
}

test('extraction catch-up tier-3 oracle', async () => {
  await main();
});

const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
