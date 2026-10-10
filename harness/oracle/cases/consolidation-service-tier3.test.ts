/**
 * @jest-environment node
 *
 * Tier-3 (mocked cheap-LLM + mocked-embedding) ORACLE for the consolidation
 * service (P4.D271, v4 `runConsolidation`, `lib/memory/consolidation.ts`,
 * byte-identical at the `01a83539d` pin), on the `fold-episode-tier3` shape.
 *
 * Drives v4's REAL `runConsolidation` over a COPY of the consolidation fixture
 * pair (`build-consolidation-fixture.ts`) with only the two model calls pinned:
 * `createLLMProvider` answers a canned JSON chosen by the run's answer set and
 * the cluster's topic (the first `[<topic><n>]` marker in the NOTES) and
 * RECORDS the exact `provider|model|temperature|messages` key it answered — the
 * Rust side replays those keys, so a prompt-byte divergence surfaces as a
 * canned miss; `generateEmbeddingForUser` answers `cannedEmbeddingFor(text)`
 * (`consolidation-corpus.ts`) and RECORDS each text. Everything else is v4's
 * real code: the repositories, the vector store, the Commonplace bridge, the
 * cheap-LLM executor and its `llm_logs` writer (the copy's own llm-logs DB).
 *
 * The runs (`consolidation-service-runs.json`) run IN ORDER on ONE copy, each
 * on its own holder (the re-run reuses `real_all`'s): a dry run; a real run;
 * the re-run ten days later (new rows attracted to the run's digests, one
 * revised, one rewritten identically — the mirror's `unchanged`); invalid /
 * thrown / timed-out / unparseable answers; an embedding failure forcing the
 * re-plan; no model; the configured profile (+ a threshold override); a
 * missing configured profile (+ `maxClustersPerRun` 3); a spent budget;
 * `maxClustersPerRun` 0; the 2,001-row bucket cap (dry, then real); an
 * archived holder; an unknown one; an empty holder.
 *
 * Output rows: `{kind:'run', name, report, lines}` per run;
 * `{kind:'canned', …}` per recorded completion key; `{kind:'embedding', text,
 * vector|null}` per embedding text; `{kind:'table', …}` for `memories`,
 * `vector_entries`, `vector_indices`; `{kind:'projection', …}` for the
 * `Commonplace/` documents; `{kind:'llmLogs', rows}` (type → count).
 *
 * Run (Node 24, from the v4 checkout or the round's pin — a /tmp mirror, jest
 * ignores .claude/ paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=/tmp/qt-consolidation-oracle
 *   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-consolidation-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-consolidation-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-consolidation-fixture.ts
 *   rm -f /tmp/qt-consolidation-service.ndjson
 *   QT_FIXTURE_CONSOLIDATION_MAIN=/tmp/qt-consolidation-main.db \
 *   QT_FIXTURE_CONSOLIDATION_MOUNT=/tmp/qt-consolidation-mount.db \
 *   QT_ORACLE_OUT=/tmp/qt-consolidation-service.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$M/cases" -- consolidation-service-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { cannedEmbeddingFor } from '../fixtures/consolidation-corpus';
import { ANSWER_SETS, VALID, type Answer } from '../fixtures/consolidation-answers';

const CAPTURED_MESSAGES = new Set([
  '[Consolidation] Sending cluster to model',
  'Moved memories between tiers',
  'Skipping Commonplace mirror: holder not found',
  'Skipping Commonplace mirror for archived character',
  'Skipping Commonplace mirror: holder has no character vault',
  'Wrote Commonplace digest mirror',
  'Failed to write a Commonplace digest mirror',
  'Commonplace mirror failed',
  'Commonplace mirror complete',
  'Document absent from database-backed store',
]);

function ownContext(ctx: Record<string, unknown> | undefined): Record<string, unknown> {
  const { service: _s, environment: _e, ...rest } = ctx ?? {};
  return rest;
}

const CANON_TABLES: Array<[string, string]> = [
  ['memories', 'SELECT * FROM memories ORDER BY characterId, content, id'],
  ['vector_entries', 'SELECT * FROM vector_entries ORDER BY characterId, embedding'],
  ['vector_indices', 'SELECT * FROM vector_indices ORDER BY id'],
];

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'consolidation-tier3.json'), 'utf8'));
  const { runs } = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'consolidation-service-runs.json'), 'utf8'),
  ) as { runs: Array<{ name: string; characterId: string; opts: Record<string, unknown>; answers: string }> };

  const fixtureMain = process.env.QT_FIXTURE_CONSOLIDATION_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_CONSOLIDATION_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_CONSOLIDATION_MAIN / _MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-consolidation-service-oracle-'));
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
      '@/lib/services/llm-logging.service',
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
            const answer = currentAnswers[topic];
            if (answer === undefined) throw new Error(`no canned answer for topic ${topic}`);
            const fail = typeof answer === 'object' && 'fail' in answer ? (answer.fail as string) : undefined;
            const response = fail ? '' : typeof answer === 'string' ? answer : JSON.stringify(answer);
            const usage = { promptTokens: 100, completionTokens: 40, totalTokens: 140 };
            const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
            if (!canned.has(key)) {
              canned.set(key, {
                provider,
                model: params.model,
                temperature: params.temperature ?? null,
                messages,
                response,
                usage,
                ...(fail ? { fail } : {}),
              });
            }
            if (fail) throw new Error(fail);
            return { content: response, finishReason: 'stop', usage };
          },
        }),
      };
    });
    jest.doMock('@/lib/services/api-key.service', () => {
      const actual = jest.requireActual('@/lib/services/api-key.service');
      return { __esModule: true, ...actual, getApiKeyForCheapLLMSelection: async () => 'test-key' };
    });

    const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
    const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    const { runConsolidation } = await import('@/lib/memory/consolidation');
    const { Logger } = await import('@/lib/logger');

    let sink: Array<{ level: string; message: string; context: unknown }> | null = null;
    for (const level of ['error', 'warn', 'info', 'debug'] as const) {
      const proto = Logger.prototype as unknown as Record<string, (...a: unknown[]) => void>;
      const original = proto[level];
      proto[level] = function (this: { context?: Record<string, unknown> }, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
        const merged = { ...ownContext(this.context), ...(context ?? {}) };
        if (sink && typeof message === 'string' && (merged.module === 'memory:consolidation' || CAPTURED_MESSAGES.has(message))) {
          sink.push({ level, message, context: JSON.parse(JSON.stringify(merged)) });
        }
        return original.call(this, message, context, ...rest);
      };
    }

    await initializeDatabase();
    const lines: string[] = [];
    for (const run of runs) {
      currentAnswers = ANSWER_SETS[run.answers];
      const o = run.opts;
      sink = [];
      const report = await runConsolidation(run.characterId, {
        dryRun: o.dryRun as boolean,
        trigger: o.trigger as never,
        now: new Date(o.now as string),
        ...(o.userId !== undefined ? { userId: o.userId as string } : {}),
        ...(o.settings !== undefined ? { settings: o.settings as never } : {}),
        ...(o.maxClustersPerRun !== undefined ? { maxClustersPerRun: o.maxClustersPerRun as number } : {}),
        ...(o.timeBudgetMs !== undefined ? { timeBudgetMs: o.timeBudgetMs as number } : {}),
      });
      lines.push(JSON.stringify({ kind: 'run', name: run.name, report, lines: sink }));
      sink = null;
      for (let i = 0; i < 10; i++) await new Promise((r) => setImmediate(r));
    }

    for (const entry of canned.values()) lines.push(JSON.stringify({ kind: 'canned', ...entry }));
    for (const [text, vector] of embeddings) lines.push(JSON.stringify({ kind: 'embedding', text, vector }));

    const hex = (v: unknown) => (v instanceof Uint8Array ? Buffer.from(v).toString('hex') : v);
    for (const [table, sql] of CANON_TABLES) {
      const rows = ((await rawQuery(sql)) as Array<Record<string, unknown>>).map((r) => {
        const out: Record<string, unknown> = {};
        for (const [k, v] of Object.entries(r)) out[k] = hex(v);
        return out;
      });
      lines.push(JSON.stringify({ kind: 'table', table, rows }));
    }
    const midb = getRawMountIndexDatabase();
    if (!midb) throw new Error('mount-index DB handle unavailable');
    const docs = midb
      .prepare(
        `SELECT l.mountPointId, l.relativePath, d.content FROM doc_mount_file_links l
           JOIN doc_mount_documents d ON d.fileId = l.fileId
          WHERE l.relativePath LIKE 'Commonplace/%' ORDER BY l.mountPointId, l.relativePath`,
      )
      .raw(true)
      .all();
    lines.push(JSON.stringify({ kind: 'projection', name: 'commonplace', rows: docs }));

    const llmDb = new (jest.requireActual(cipherDriverPath))(join(scratch, 'llm-logs.db'));
    const keyHex = Buffer.from(spec.testPepperBase64, 'base64').toString('hex');
    llmDb.pragma(`key = "x'${keyHex}'"`);
    const logRows = llmDb.prepare('SELECT type, COUNT(*) AS n FROM llm_logs GROUP BY type ORDER BY type').all();
    llmDb.close();
    lines.push(JSON.stringify({ kind: 'llmLogs', rows: logRows }));

    closeMountIndexSQLiteClient();
    await closeDatabase();
    fs.writeFileSync(outPath, lines.join('\n') + '\n');
    process.stderr.write(`consolidation service oracle wrote ${outPath}\n`);
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

test('consolidation service tier-3 oracle', async () => {
  await main();
});
