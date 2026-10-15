/**
 * @jest-environment node
 *
 * Tier-3 (mocked-completion + mocked-embedding) ORACLE for the extraction
 * TURN grain — v4's ONE pipeline at the `83334b9a5` pin
 * (`lib/memory/extraction/{context,route,rate-limit,cursors,run,turn}.ts` +
 * the four passes; work order P4.D296 Tier 1 #5–#11). Rewrites the IMPORT-DEAD
 * case of the same name (its subject, `memory-processor.ts`, was deleted by
 * `ac6348c5f`); the Rust family keeps its name, `memory_processor_tier3_equivalence`
 * (the order's L-20).
 *
 * For every corpus op: plant (the chat's `turn` cursor through v4's REAL
 * `setCursor`, the extraction limits / mode rows, the chat settings' cheap-LLM
 * block, the rate ledger through admission's REAL `recordAdmissionInsert`),
 * then v4's REAL `buildExtractionContext(chatId, userId, { dryRun, backfill })`
 * and `runTurnExtraction(ctx, locator)` over the REAL fixture DBs. ONLY the
 * model / infra seams are stubbed:
 *
 *   - `createLLMProvider` — each call resolves to a corpus rule keyed on
 *     (pass, CONTEXT-footer label, a substring of the user message); the
 *     exact `provider|model|temperature|messages` key it answered is RECORDED
 *     (`kind: "canned"`) and the Rust side replays exactly those entries, so
 *     a prompt / selection / temperature divergence is a canned-miss. A rule
 *     whose label is the op's `failLabel` throws `Request timed out.` (both
 *     attempts spent → `timedOut`).
 *   - `generateEmbeddingForUser` — a deterministic vector per text (FNV-1a +
 *     xorshift, `embeddingDims` wide), RECORDED (`kind: "embedding"`) for the
 *     Rust replay.
 *   - `getApiKeyForCheapLLMSelection` → a constant; `maybeEnqueueMaintenance`
 *     → a no-op (the after-insert watermark call is the unifier's HANDOFF).
 *
 * v4's index cache is dropped before every index read (the admission rig's
 * LR-11 rule). Lines captured: every `[Memory]` / `[ExtractionCursors]` /
 * `[MemorySettings]` / `[MemoryCalendar]` / `[Canon]` line, per op.
 *
 * Run from the v4 checkout (or the lane's pin) under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-memory-processor-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/memory-processor-tier3.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/memory-processor-tier3.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-memory-processor-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-memory-processor-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-memory-processor-fixture.ts
 *   QT_FIXTURE_PROCESSOR_MAIN=/tmp/qt-memory-processor-main.db QT_FIXTURE_PROCESSOR_MOUNT=/tmp/qt-memory-processor-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-memory-processor.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 --roots "$PWD" --roots "$TMPO/cases" -- "memory-processor-tier3\\.test\\.ts$"
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

const CAPTURED_PREFIXES = ['[Memory]', '[ExtractionCursors]', '[MemorySettings]', '[MemoryCalendar]', '[Canon]'];

function ownContext(ctx: Record<string, unknown> | undefined): Record<string, unknown> {
  const { service: _s, environment: _e, ...rest } = ctx ?? {};
  return rest;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'memory-processor-tier3.json'), 'utf8')) as Json;
  const fixtureMain = process.env.QT_FIXTURE_PROCESSOR_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_PROCESSOR_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_PROCESSOR_MAIN / QT_FIXTURE_PROCESSOR_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-extraction-turn-oracle-'));
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
  let failLabel: string | null = null;

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
          const m = system.match(/\nCONTEXT\n(SUBJECT|OBSERVER): ([^\n]*)\n/);
          if (!m) throw new Error('canned completion: no CONTEXT footer label in system prompt');
          const pass = m[1] === 'SUBJECT' ? 'self' : 'other';
          const label = m[2];
          const rule = (spec.completionRules as Json[]).find(
            (r) => r.pass === pass && r.label === label && (r.contains === undefined || user.includes(r.contains)),
          );
          if (!rule) throw new Error(`no completion rule for pass=${pass} label=${label}`);
          const failing = pass === 'self' && failLabel === label;
          const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
          if (!cannedRecorded.has(key)) {
            cannedRecorded.set(key, {
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
  jest.doMock('@/lib/memory/maintenance/triggers', () => ({
    __esModule: true,
    maybeEnqueueMaintenance: async () => [],
  }));

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { buildExtractionContext, runTurnExtraction } = await import('@/lib/memory/extraction');
  const admit = await import('@/lib/memory/admission/admit');
  const { recordAdmissionInsert, resetRateLedgerForTests } = await import('@/lib/memory/admission/rate-ledger');
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
  const baseCheap = JSON.stringify(spec.chatSettings.cheapLLMSettings);
  const lines: string[] = [];

  for (const op of spec.ops as Json[]) {
    const plants = op.plants ?? {};
    const opts = op.opts ?? {};
    if ('cursor' in plants) {
      if (plants.cursor === null) {
        await rawQuery(`DELETE FROM "memory_extraction_cursors" WHERE "chatId" = ? AND "pass" = 'turn'`, [op.chatId]);
      } else {
        await repos.memoryExtractionCursors.setCursor(op.chatId, 'turn', plants.cursor);
      }
    }
    if (plants.limits) {
      await rawQuery(`INSERT INTO "instance_settings" ("key", "value") VALUES ('memoryExtractionLimits', ?)`, [JSON.stringify(plants.limits)]);
    }
    if (plants.mode) {
      await rawQuery(`INSERT INTO "instance_settings" ("key", "value") VALUES ('memoryExtractionMode', ?)`, [JSON.stringify(plants.mode)]);
    }
    if (plants.cheapLLMSettings) {
      await rawQuery(`UPDATE "chat_settings" SET "cheapLLMSettings" = ?`, [JSON.stringify(plants.cheapLLMSettings)]);
    }
    for (const [characterId, n] of Object.entries((plants.ledger ?? {}) as Record<string, number>)) {
      for (let i = 0; i < n; i++) recordAdmissionInsert(characterId, Date.now());
    }
    failLabel = plants.failLabel ?? null;
    if (plants.failContent) {
      // One candidate's admission throws (a planted trigger aborts its insert):
      // the per-candidate catch (v4 run.ts admitDraft) costs that candidate alone.
      await rawQuery(
        `CREATE TRIGGER "qt_fail_admission" BEFORE INSERT ON "memories" WHEN NEW."content" = '${plants.failContent}' BEGIN SELECT RAISE(ABORT, 'planted admission failure'); END`,
      );
    }

    sink = [];
    let result: Json;
    try {
      const ctx = await buildExtractionContext(op.chatId, userId, {
        ...(opts.dryRun ? { dryRun: true } : {}),
        ...(opts.backfill ? { backfill: true } : {}),
      });
      const r = await runTurnExtraction(ctx, op.locator);
      result = {
        pass: r.pass,
        calls: r.calls,
        timedOut: r.timedOut,
        failed: r.failed,
        admitted: r.admitted.map((a) => ({
          candidate: a.candidate,
          occurredAt: a.occurredAt,
          narrativeTime: a.narrativeTime,
          holderName: a.holderName,
          aboutName: a.aboutName,
          action: a.outcome.action.kind,
        })),
        createdIds: r.createdIds,
        reinforcedIds: r.reinforcedIds,
        usage: r.usage,
        debugLogs: r.debugLogs,
        rateDeferred: r.rateDeferred,
        sourceMessageId: r.sourceMessageId,
        hadContributions: r.hadContributions,
      };
    } catch (error) {
      result = { error: error instanceof Error ? error.message : String(error) };
    }
    const captured = sink;
    sink = null;

    failLabel = null;
    if (plants.failContent) await rawQuery(`DROP TRIGGER "qt_fail_admission"`);
    if (plants.limits) await rawQuery(`DELETE FROM "instance_settings" WHERE "key" = 'memoryExtractionLimits'`);
    if (plants.mode) await rawQuery(`DELETE FROM "instance_settings" WHERE "key" = 'memoryExtractionMode'`);
    if (plants.cheapLLMSettings) await rawQuery(`UPDATE "chat_settings" SET "cheapLLMSettings" = ?`, [baseCheap]);

    const cursors = (await rawQuery(`SELECT "id", "chatId", "pass", "messageId" FROM "memory_extraction_cursors" ORDER BY "id"`)) as Json[];
    lines.push(JSON.stringify({ kind: 'op', op: op.name, result, cursors, lines: captured }));
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
  process.stderr.write(`extraction-turn oracle wrote ${outPath}\n`);
}

test('extraction turn tier-3 oracle', async () => {
  await main();
});

const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
