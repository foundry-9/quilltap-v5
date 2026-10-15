/**
 * @jest-environment node
 *
 * P4.D297 tier-3 (mocked cheap-LLM + mocked-embedding) ORACLE — the
 * maintenance run's `consolidate` and `mirror` phases through v4's REAL
 * `runMaintenance` (`lib/memory/maintenance/run.ts` + `consolidate.ts` +
 * `lib/memory/consolidation.ts` at the `83334b9a5` pin): the reshaped
 * `consolidation_service_tier3` (P4.D271's runs, re-aimed at the one job).
 *
 * Over a COPY of the consolidation fixture pair (`build-consolidation-fixture.ts`,
 * shared with P4.D288's Commonplace families) the runs
 * (`memory-maintenance-consolidate-runs.json`) run IN ORDER, each at its own
 * `ctx.now`. Only the two model calls are pinned: `createLLMProvider` answers
 * a canned JSON chosen by the run's answer set and the cluster's topic (the
 * first `[<topic><n>]` marker in the NOTES) and RECORDS the exact
 * `provider|model|temperature|messages` key — a topic with no answer is
 * recorded as a failure — so the Rust side replays it (a prompt-byte or
 * routing divergence is a canned miss); `generateEmbeddingForUser` answers
 * `cannedEmbeddingFor(text)` (`consolidation-corpus.ts`) and RECORDS each
 * text. `logLLMCall` is a no-op: the oracle writes ZERO `llm_logs` rows.
 *
 * Covered: a dry run; a scheduled run lifted by `settings.consolidation.enabled`
 * (the request override over `consolidation-disabled`); the re-run ten days
 * later; a scheduled run without the override (`consolidation-disabled`,
 * nothing read); invalid / thrown / timed-out / unparseable answers; an
 * embedding failure forcing the re-plan; no model; the configured profile (+ a
 * threshold override); bug 246 — the routed profile's `maxTokens` 12000, then
 * 2000 (the 4000 floor), on the `[Consolidation] Sending cluster to model`
 * line; a missing configured profile (+ `maxClustersPerRun` 3); the empty /
 * whitespace answer verdict; a JOB-mode run (`QUILLTAP_JOB_CHILD` set around
 * the run only: the mirror is `deferredToCommit`, then the dispatcher's
 * `mirrorCommonplaceAfterCommit` runs over the batch's digest writes);
 * `maxClustersPerRun` 0; bug 211's demoted candidates; the 2,001-row bucket
 * cap (dry, then real); the archived / unknown skip arms; an empty holder; a
 * BUILT-IN default embedding profile (`tfidfClusterThreshold`).
 *
 * Output rows: `{kind:'run', name, report, lines}` per run;
 * `{kind:'canned', …}` per recorded completion key; `{kind:'embedding', text,
 * vector|null}`; `{kind:'table', rows}` (`memories`); `{kind:'projection',
 * rows}` (the `Commonplace/` documents).
 *
 * Run (Node 24, from the v4 checkout or the round's pin — a /tmp mirror):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=${M:-/tmp/qt-maintenance-tier3-oracle}
 *   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=${QT_MAIN:-/tmp/qt-maintenance-cons-main.db} \
 *   QT_FIXTURE_MOUNT_OUT=${QT_MOUNT:-/tmp/qt-maintenance-cons-mount.db} \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-consolidation-fixture.ts
 *   rm -f ${QT_OUT:-/tmp/qt-maintenance-tier3.ndjson}
 *   QT_FIXTURE_CONSOLIDATION_MAIN=${QT_MAIN:-/tmp/qt-maintenance-cons-main.db} \
 *   QT_FIXTURE_CONSOLIDATION_MOUNT=${QT_MOUNT:-/tmp/qt-maintenance-cons-mount.db} \
 *   QT_ORACLE_OUT=${QT_OUT:-/tmp/qt-maintenance-tier3.ndjson} \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$M/cases" -- memory-maintenance-consolidate-tier3
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
  'Committed maintenance batch touched no mirrored bucket',
]);

interface Run {
  name: string;
  characterId: string;
  request: Record<string, unknown>;
  now: string;
  answers: string;
  userId?: string;
  mode?: 'job';
  plantSql?: string[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'consolidation-tier3.json'), 'utf8'));
  const { runs } = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'memory-maintenance-consolidate-runs.json'), 'utf8'),
  ) as { runs: Run[] };
  const fixtureMain = process.env.QT_FIXTURE_CONSOLIDATION_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_CONSOLIDATION_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_CONSOLIDATION_MAIN / _MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-maintenance-tier3-oracle-'));
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
      '@/lib/file-storage/character-vault-bridge',
      '@/lib/file-storage/manager',
    ]) {
      jest.doMock(real, () => jest.requireActual(real));
    }
    jest.doMock('@/lib/services/llm-logging.service', () => {
      const actual = jest.requireActual('@/lib/services/llm-logging.service');
      return { __esModule: true, ...actual, logLLMCall: async () => null };
    });
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
            const answer: Answer = currentAnswers[topic] ?? { fail: `no canned answer for topic ${topic}` };
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
    (globalThis as Record<string, unknown>).__quilltapJobHost = { shuttingDown: true };

    const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
    const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    const { runMaintenance } = await import('@/lib/memory/maintenance/run');
    const { mirrorCommonplaceAfterCommit } = await import('@/lib/memory/commonplace-mirror');
    const { Logger } = await import('@/lib/logger');

    let sink: Array<{ level: string; message: string; context: unknown }> | null = null;
    for (const level of ['error', 'warn', 'info', 'debug'] as const) {
      const proto = Logger.prototype as unknown as Record<string, (...a: unknown[]) => void>;
      const original = proto[level];
      proto[level] = function (this: { context?: Record<string, unknown> }, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
        const { service: _s, environment: _e, ...own } = this.context ?? {};
        const merged = { ...own, ...(context ?? {}) };
        const mod = typeof merged.module === 'string' ? merged.module : '';
        if (
          sink &&
          typeof message === 'string' &&
          (mod.startsWith('memory:maintenance') || mod === 'memory:consolidation' || CAPTURED_MESSAGES.has(message))
        ) {
          sink.push({ level, message, context: JSON.parse(JSON.stringify(merged)) });
        }
        return original.call(this, message, context, ...rest);
      };
    }

    await initializeDatabase();
    const maindb = (await import('@/lib/database/backends/sqlite/client')).getRawDatabase?.();
    if (!maindb) throw new Error('main DB handle unavailable');
    const lines: string[] = [];
    for (const run of runs) {
      currentAnswers = ANSWER_SETS[run.answers];
      for (const sql of run.plantSql ?? []) maindb.exec(sql);
      sink = [];
      if (run.mode === 'job') process.env.QUILLTAP_JOB_CHILD = '1';
      let report: Record<string, unknown>;
      try {
        report = (await runMaintenance(
          { characterId: run.characterId, ...run.request } as never,
          { now: new Date(run.now), ...(run.userId ? { userId: run.userId } : {}) },
        )) as unknown as Record<string, unknown>;
      } finally {
        delete process.env.QUILLTAP_JOB_CHILD;
      }
      if (run.mode === 'job') {
        // The dispatcher's commit hook over the batch's digest writes: a
        // created digest names its holder and subject on its data, a revised
        // one (`updateForCharacter` with `consolidatedFrom`) is read back.
        const consolidation = report.consolidation as { clusters: Array<Record<string, unknown>> } | undefined;
        const writes: Array<{ method: string; args: unknown[] }> = [];
        for (const cluster of consolidation?.clusters ?? []) {
          if (cluster.status !== 'written') continue;
          const bucket = cluster.bucket as { kind: string; subjectCharacterId: string | null };
          const about = bucket.kind === 'none' ? null : bucket.kind === 'self' ? run.characterId : bucket.subjectCharacterId;
          for (const digest of cluster.digests as Array<{ id: string; action: string }>) {
            if (digest.action === 'create') {
              writes.push({ method: 'memories.create', args: [{ source: 'CONSOLIDATED', characterId: run.characterId, aboutCharacterId: about }] });
            } else {
              writes.push({ method: 'memories.updateForCharacter', args: [run.characterId, digest.id, { consolidatedFrom: [] }] });
            }
          }
        }
        await mirrorCommonplaceAfterCommit('MEMORY_MAINTENANCE', { characterId: run.characterId, ...run.request }, writes as never);
      }
      for (let i = 0; i < 10; i++) await new Promise((r) => setImmediate(r));
      lines.push(JSON.stringify({ kind: 'run', name: run.name, report, lines: sink }));
      sink = null;
    }

    for (const entry of canned.values()) lines.push(JSON.stringify({ kind: 'canned', ...entry }));
    for (const [text, vector] of embeddings) lines.push(JSON.stringify({ kind: 'embedding', text, vector }));
    const hex = (v: unknown) => (v instanceof Uint8Array ? Buffer.from(v).toString('hex') : v);
    const rows = ((await rawQuery('SELECT * FROM memories ORDER BY characterId, content, id')) as Array<Record<string, unknown>>).map((r) => {
      const out: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(r)) out[k] = hex(v);
      return out;
    });
    lines.push(JSON.stringify({ kind: 'table', table: 'memories', rows }));
    const midb = getRawMountIndexDatabase();
    if (!midb) throw new Error('mount-index DB handle unavailable');
    const docs = midb
      .prepare(
        `SELECT l.mountPointId, l.relativePath, d.content FROM doc_mount_file_links l
           JOIN doc_mount_documents d ON d.fileId = l.fileId
          WHERE l.relativePath LIKE 'Commonplace/%' ORDER BY l.relativePath, d.content`,
      )
      .raw(true)
      .all();
    lines.push(JSON.stringify({ kind: 'projection', name: 'commonplace', rows: docs }));
    closeMountIndexSQLiteClient();
    await closeDatabase();
    fs.writeFileSync(outPath, lines.join('\n') + '\n');
    process.stderr.write(`memory maintenance consolidate tier-3 oracle wrote ${outPath}\n`);
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

test('memory maintenance consolidate tier-3 oracle', async () => {
  await main();
}, 180000);
