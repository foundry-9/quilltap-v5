/**
 * @jest-environment node
 *
 * Tier-2 ORACLE for the consolidation triggers, the enqueue, the scheduled
 * housekeeping pass and the job handler (P4.D271, v4 `d58548051`,
 * byte-identical at the `01a83539d` pin), over a COPY of the consolidation
 * fixture pair: v4's REAL `maybeEnqueueConsolidationForCharacters`,
 * `enqueueMemoryConsolidation`, `runScheduledHousekeeping` (→
 * `runScheduledConsolidation`), `handleMemoryConsolidation` (→ the real
 * `runConsolidation`, the model canned as the service oracle cans it), and
 * `characterIdsFromMemoryWrites` for the `WATERMARK_ROWID_WINDOW` table.
 *
 * The steps (`consolidation-triggers-steps.json`) run IN ORDER on ONE copy
 * after the corpus's plants (memory rows for a missing / archived / below-
 * watermark character, the throttle's four exclusion jobs + one counted, the
 * dedupe split's dry-run job, `autoHousekeepingSettings` on): disabled; an
 * empty list; the watermark reached / throttled / excluded; missing /
 * archived / below; the debounce and its expiry; the dedupe split by
 * `dryRun`; a `maxClustersPerRun` payload; the scheduled pass deferring
 * housekeeping, then disabled; the handler's gates, a dry run (one cluster's
 * call fails) and a zero-budget real run.
 *
 * The handler steps consolidate "as of" `spec.now` — the instant the fixture's
 * memory dates are relative to — through ONE wrapper over the real
 * `runConsolidation` (see the 2026-10-13 time-bomb comment at the mock).
 *
 * Output: `{kind:'step', name, value, lines}` per step; `{kind:'canned', …}`;
 * `{kind:'embedding', …}`; `{kind:'window', name, ids}`; `{kind:'jobs',
 * rows}` (every `background_jobs` row, with `scheduledAt − createdAt` as
 * `scheduledDelayMs`).
 *
 * Run (Node 24, from the v4 checkout or the round's pin — a /tmp mirror):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=/tmp/qt-consolidation-oracle
 *   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-consolidation-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-consolidation-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-consolidation-fixture.ts
 *   rm -f /tmp/qt-consolidation-triggers.ndjson
 *   QT_FIXTURE_CONSOLIDATION_MAIN=/tmp/qt-consolidation-main.db \
 *   QT_FIXTURE_CONSOLIDATION_MOUNT=/tmp/qt-consolidation-mount.db \
 *   QT_ORACLE_OUT=/tmp/qt-consolidation-triggers.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$M/cases" -- consolidation-triggers-tier2
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { cannedEmbeddingFor } from '../fixtures/consolidation-corpus';
import { ANSWER_SETS, VALID, answerFor, type Answer } from '../fixtures/consolidation-answers';

const CAPTURED_MODULES = new Set(['memory:consolidation-triggers', 'jobs:memory-consolidation', 'scheduled-housekeeping']);
const CAPTURED_MESSAGES = new Set([
  '[Consolidation] Job already in flight for character; not enqueuing another',
  '[Consolidation] Failed to check for existing jobs during enqueue',
]);

function ownContext(ctx: Record<string, unknown> | undefined): Record<string, unknown> {
  const { service: _s, environment: _e, ...rest } = ctx ?? {};
  return rest;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'consolidation-tier3.json'), 'utf8'));
  const corpus = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'consolidation-triggers-steps.json'), 'utf8'));

  const fixtureMain = process.env.QT_FIXTURE_CONSOLIDATION_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_CONSOLIDATION_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_CONSOLIDATION_MAIN / _MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-consolidation-triggers-oracle-'));
  try {
    mkdirSync(join(scratch, 'data'), { recursive: true });
    copyFileSync(fixtureMain, join(scratch, 'main.db'));
    copyFileSync(fixtureMount, join(scratch, 'mount.db'));
    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.SQLITE_PATH = join(scratch, 'main.db');
    process.env.SQLITE_MOUNT_INDEX_PATH = join(scratch, 'mount.db');
    process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'llm-logs.db');
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.QUILLTAP_JOB_CHILD;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    let currentAnswers: Record<string, Answer> = VALID;
    const canned = new Map<string, Record<string, unknown>>();
    const embeddings = new Map<string, number[] | null>();

    jest.resetModules();
    const cipherDriverPath = require('node:path').join(
      process.cwd(),
      'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
    );
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    for (const real of [
      '@/lib/database/manager',
      '@/lib/database/repositories',
      '@/lib/repositories/factory',
      '@/lib/embedding/vector-store',
      '@/lib/file-storage/character-vault-bridge',
      '@/lib/file-storage/manager',
    ]) {
      jest.doMock(real, () => jest.requireActual(real));
    }
    jest.doMock('@/lib/embedding/embedding-service', () => {
      const actual = jest.requireActual('@/lib/embedding/embedding-service');
      return {
        __esModule: true,
        ...actual,
        generateEmbeddingForUser: async (text: string) => {
          const vec = cannedEmbeddingFor(text);
          embeddings.set(text, vec);
          if (!vec) throw new actual.EmbeddingError('canned embedding failure');
          return { embedding: new Float32Array(vec), model: 'canned', dimensions: vec.length, provider: 'canned' };
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
            const user = messages.find((m) => m.role === 'user')?.content ?? '';
            const notes = user.slice(user.indexOf('NOTES (oldest first)'));
            const topic = /\[([A-J])\d\]/.exec(notes)?.[1] ?? '?';
            const { response, fail } = answerFor(currentAnswers, topic);
            const usage = { promptTokens: 100, completionTokens: 40, totalTokens: 140 };
            const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
            if (!canned.has(key)) {
              canned.set(key, { provider, model: params.model, temperature: params.temperature ?? null, messages, response, usage, ...(fail ? { fail } : {}) });
            }
            if (fail) throw new Error(fail);
            return { content: response, finishReason: 'stop', usage };
          },
        }),
      };
    });
    // THE TIME BOMB (red 2026-10-13): the fixture writes every memory's dates
    // relative to the FIXED `spec.now` (2026-10-08), but the handler calls
    // `runConsolidation` with no `now`, so the run clustered on the WALL CLOCK —
    // the corpus's "fresh pair below the floor" (D1–D2, 9 days old) aged past
    // the maturity floor days later and formed a topic-D cluster no answer set
    // covers (and the immature / 2×-floor / already-considered rows cross their
    // thresholds on later dates). The ONE seam: v4's REAL `runConsolidation`,
    // an omitted `now` filled from `spec.now` (an explicit one passes through).
    // v5's `CannedRunner` fills `now_ms` the same way, so both sides run "as
    // of" the fixture's instant. Everything else stays wall-clock on both sides
    // and is self-consistent (the planted jobs, the debounce offsets and the
    // housekeeping window all read the same `Date.now()`).
    const consolidationNow = new Date(spec.now);
    jest.doMock('@/lib/memory/consolidation', () => {
      const actual = jest.requireActual('@/lib/memory/consolidation');
      return {
        __esModule: true,
        ...actual,
        runConsolidation: (characterId: string, opts: { now?: Date } = {}) =>
          actual.runConsolidation(characterId, { ...opts, now: opts.now ?? consolidationNow }),
      };
    });
    jest.doMock('@/lib/services/api-key.service', () => {
      const actual = jest.requireActual('@/lib/services/api-key.service');
      return { __esModule: true, ...actual, getApiKeyForCheapLLMSelection: async () => 'test-key' };
    });

    // Hold v4's job host off BEFORE any import: an enqueue spawns a runner that
    // would claim the PENDING jobs mid-case (memory note
    // `v4-oracle-job-host-and-log-capture`).
    (globalThis as Record<string, unknown>).__quilltapJobHost = {
      child: null,
      spawning: false,
      shuttingDown: true,
      childCrashed: false,
      restartTimestamps: [],
      signalHandlersInstalled: false,
      invalidationListeners: new Set(),
    };
    const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
    const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
    const triggers = await import('@/lib/memory/consolidation-triggers');
    const { enqueueMemoryConsolidation } = await import('@/lib/background-jobs/queue-service');
    const { runScheduledHousekeeping } = await import('@/lib/background-jobs/scheduled-housekeeping');
    const { handleMemoryConsolidation } = await import('@/lib/background-jobs/handlers/memory-consolidation');
    const { setMemoryConsolidationSettings } = await import('@/lib/instance-settings');
    const { Logger } = await import('@/lib/logger');

    let sink: Array<{ level: string; message: string; context: unknown }> | null = null;
    for (const level of ['error', 'warn', 'info', 'debug'] as const) {
      const proto = Logger.prototype as unknown as Record<string, (...a: unknown[]) => void>;
      const original = proto[level];
      proto[level] = function (this: { context?: Record<string, unknown> }, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
        const merged = { ...ownContext(this.context), ...(context ?? {}) };
        if (sink && typeof message === 'string' && (CAPTURED_MODULES.has(merged.module as string) || CAPTURED_MESSAGES.has(message))) {
          sink.push({ level, message, context: JSON.parse(JSON.stringify(merged)) });
        }
        return original.call(this, message, context, ...rest);
      };
    }


    await initializeDatabase();
    triggers.resetConsolidationTriggerStateForTests();
    const base = Date.now();

    for (const sql of corpus.plants) await rawQuery(sql);
    for (const j of corpus.jobs) {
      const at = new Date(base - j.hoursAgo * 3_600_000).toISOString();
      await rawQuery(
        `INSERT INTO background_jobs (id, userId, type, status, payload, priority, attempts, maxAttempts, lastError, scheduledAt, startedAt, completedAt, createdAt, updatedAt)
         VALUES (?, ?, 'MEMORY_CONSOLIDATION', ?, ?, 0, 0, 1, NULL, ?, NULL, NULL, ?, ?)`,
        [j.id, corpus.userId, j.status, JSON.stringify(j.payload), at, at, at],
      );
    }
    await rawQuery(corpus.chatSettingsSql);

    const lines: string[] = [];
    for (const step of corpus.steps) {
      if (step.op === 'setSettings') {
        await setMemoryConsolidationSettings(step.patch);
        continue;
      }
      if (step.op === 'sql') {
        await rawQuery(step.sql);
        continue;
      }
      sink = [];
      let value: unknown = null;
      if (step.op === 'maybeEnqueue') {
        value = await triggers.maybeEnqueueConsolidationForCharacters(step.ids, base + step.offsetMs);
      } else if (step.op === 'enqueue') {
        value = await enqueueMemoryConsolidation(step.userId, step.payload);
      } else if (step.op === 'housekeeping') {
        value = await runScheduledHousekeeping();
      } else if (step.op === 'handler') {
        if (step.pre) await setMemoryConsolidationSettings(step.pre);
        currentAnswers = step.answers ? ANSWER_SETS[step.answers] : VALID;
        await handleMemoryConsolidation({
          id: step.job.id,
          userId: corpus.userId,
          type: 'MEMORY_CONSOLIDATION',
          payload: step.job.payload,
        } as never);
        value = 'completed';
      }
      lines.push(JSON.stringify({ kind: 'step', name: step.name, value, lines: sink }));
      sink = null;
      for (let i = 0; i < 10; i++) await new Promise((r) => setImmediate(r));
    }

    for (const entry of canned.values()) lines.push(JSON.stringify({ kind: 'canned', ...entry }));
    for (const [text, vector] of embeddings) lines.push(JSON.stringify({ kind: 'embedding', text, vector }));

    // WATERMARK_ROWID_WINDOW: v4's write-list reading of each scenario.
    for (const w of corpus.window) {
      const writes = [
        ...w.job.map((k: string) => ({ method: 'memories.create', args: [{ characterId: corpus.windowCharacters[k] }] })),
        ...(w.updatesOnly ? [{ method: 'memories.updateForCharacter', args: [corpus.windowCharacters.A, 'x', {}] }] : []),
      ];
      lines.push(JSON.stringify({ kind: 'window', name: w.name, ids: triggers.characterIdsFromMemoryWrites(writes) }));
    }

    const jobs = (await rawQuery(
      `SELECT id, userId, type, status, payload, priority, attempts, maxAttempts, lastError, scheduledAt, createdAt
         FROM background_jobs ORDER BY type, payload, status`,
    )) as Array<Record<string, unknown>>;
    lines.push(
      JSON.stringify({
        kind: 'jobs',
        rows: jobs.map((r) => ({
          id: r.id,
          userId: r.userId,
          type: r.type,
          status: r.status,
          payload: r.payload,
          priority: r.priority,
          attempts: r.attempts,
          maxAttempts: r.maxAttempts,
          lastError: r.lastError,
          scheduledDelayMs: Date.parse(r.scheduledAt as string) - Date.parse(r.createdAt as string),
        })),
      }),
    );

    closeMountIndexSQLiteClient();
    await closeDatabase();
    fs.writeFileSync(outPath, lines.join('\n') + '\n');
    process.stderr.write(`consolidation triggers oracle wrote ${outPath}\n`);
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

test('consolidation triggers tier-2 oracle', async () => {
  await main();
});
