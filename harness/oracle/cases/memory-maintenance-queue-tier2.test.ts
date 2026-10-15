/**
 * @jest-environment node
 *
 * P4.D297 tier-2 ORACLE — the maintenance queue, the watermark trigger, the
 * daily scheduler, the job handler and the dispatcher's two commit hooks,
 * through v4's REAL code at the `83334b9a5` pin:
 * `enqueueMemoryMaintenance` / `…Many` / `…ForUser`
 * (`lib/background-jobs/queue-service.ts`), `maybeEnqueueMaintenance`
 * (`lib/memory/maintenance/triggers.ts`), `runScheduledMemoryMaintenance` /
 * `runStartupTick` (`scheduler.ts`), `handleMemoryMaintenance`
 * (`handlers/memory-maintenance.ts`), the ledger (`ledger.ts` — seeded from
 * the planted jobs, `getMaintenanceLedgerStatus`), and the dispatcher's
 * success arm: `recordMaintenanceOutcome(result)` then
 * `mirrorCommonplaceAfterCommit(type, payload, writes)`.
 *
 * The steps (`memory-maintenance-queue-steps.json`) run IN ORDER on ONE copy
 * of the maintenance fixture pair (`build-maintenance-fixture.ts`). Clocks:
 * the planted jobs are stamped `base − hoursAgo` (base = `Date.now()` at plant
 * time); the watermark and the startup tick read `base + offsetMs` (v4's own
 * `nowMs` seam); the handler's run reads the spec's clock through ONE wrapper
 * over the real `runMaintenance` (an omitted `ctx.now` filled from `spec.now`
 * — the fixture's memory dates are relative to it; the calendar time bomb).
 *
 * The handler runs as v4's child would (`QUILLTAP_JOB_CHILD=1` around the
 * call): the run reports `deferredToCommit`, announces nothing, and its fold
 * goes over host-RPC — `callHost('foldNearDuplicates', …)` routed to the
 * parent's real `applyNearDuplicateFolds`, as the host's RPC server does. The
 * handler's `setJobResult` is captured (outside a job scope v4 drops it) and
 * handed to the two hooks as the dispatcher does after the batch commits; a
 * handler batch's `writes` are empty (the oracle has no buffered batch, and
 * no LLM-free phase creates a digest — v5's stashed touched subjects are
 * empty too). The `hook` steps drive `mirrorCommonplaceAfterCommit` directly
 * with one CONSOLIDATED create per touched bucket.
 *
 * Output rows: `{kind:'step', name, value, lines}` (`lines` = every
 * `memory:maintenance*`, `jobs:memory-maintenance`,
 * `memory:commonplace-mirror` and `[Maintenance]` line, plus the façade's
 * `[MemorySettings] Resolved memory settings` — P4.D288's, asserted by TAG
 * only); `{kind:'embedding', text, vector}`; `{kind:'jobs', rows}` — every
 * `background_jobs` row with `scheduledAt − createdAt` as `scheduledDelayMs`
 * and the stamps dropped; `{kind:'table', rows}` — `memories`;
 * `{kind:'documents', rows}` — the `Commonplace/` documents.
 *
 * Run (Node 24, from the v4 checkout or the round's pin — a /tmp mirror):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=${M:-/tmp/qt-maintenance-queue-oracle}
 *   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=${QT_MAIN:-/tmp/qt-maintenance-main.db} \
 *   QT_FIXTURE_MOUNT_OUT=${QT_MOUNT:-/tmp/qt-maintenance-mount.db} \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-maintenance-fixture.ts
 *   rm -f ${QT_OUT:-/tmp/qt-maintenance-queue.ndjson}
 *   QT_FIXTURE_MAINTENANCE_MAIN=${QT_MAIN:-/tmp/qt-maintenance-main.db} \
 *   QT_FIXTURE_MAINTENANCE_MOUNT=${QT_MOUNT:-/tmp/qt-maintenance-mount.db} \
 *   QT_ORACLE_OUT=${QT_OUT:-/tmp/qt-maintenance-queue.ndjson} \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$M/cases" -- memory-maintenance-queue-tier2
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

/** The tier-2 case's canned vector (copied: importing a case file would register its test). */
function cannedVector(text: string): number[] {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  let a = h;
  const next = () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296 - 0.5;
  };
  const raw = Array.from({ length: 8 }, next);
  const n = Math.sqrt(raw.reduce((s, v) => s + v * v, 0));
  return raw.map((v) => Math.fround(v / n));
}

const SETTINGS_TAG = '[MemorySettings] Resolved memory settings';

function captured(mod: string, message: string): boolean {
  return (
    mod.startsWith('memory:maintenance') ||
    mod === 'jobs:memory-maintenance' ||
    mod === 'memory:commonplace-mirror' ||
    message.startsWith('[Maintenance]') ||
    message === SETTINGS_TAG
  );
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'memory-maintenance-tier2.json'), 'utf8'));
  const corpus = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'memory-maintenance-queue-steps.json'), 'utf8'));
  const fixtureMain = process.env.QT_FIXTURE_MAINTENANCE_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_MAINTENANCE_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_MAINTENANCE_MAIN / _MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  // '@holder' → the holder's id, in values AND object keys.
  const holderId = (key: string): string =>
    key === 'unknown' ? spec.unknownId : key === 'subject' ? spec.subject.id : spec.holders[key].id;
  const resolve = (v: unknown): unknown => {
    if (typeof v === 'string') return v.startsWith('@') ? holderId(v.slice(1)) : v;
    if (Array.isArray(v)) return v.map(resolve);
    if (v && typeof v === 'object') {
      const out: Record<string, unknown> = {};
      for (const [k, x] of Object.entries(v)) out[k.startsWith('@') ? holderId(k.slice(1)) : k] = resolve(x);
      return out;
    }
    return v;
  };

  const scratch = mkdtempSync(join(tmpdir(), 'qt-maintenance-queue-oracle-'));
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

    const embeddings = new Map<string, number[]>();
    let jobResult: unknown = undefined;
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
    jest.doMock('@/lib/embedding/embedding-service', () => {
      const actual = jest.requireActual('@/lib/embedding/embedding-service');
      return {
        __esModule: true,
        ...actual,
        generateEmbeddingForUser: async (text: string) => {
          const vec = cannedVector(text);
          embeddings.set(text, vec);
          return { embedding: new Float32Array(vec), model: 'canned', dimensions: vec.length, provider: 'canned' };
        },
      };
    });
    // The handler's run "as of" the fixture's instant (the ONE clock seam).
    const runNow = new Date(spec.now);
    jest.doMock('@/lib/memory/maintenance/run', () => {
      const actual = jest.requireActual('@/lib/memory/maintenance/run');
      return {
        __esModule: true,
        ...actual,
        runMaintenance: (request: unknown, ctx: { now?: Date } = {}) =>
          actual.runMaintenance(request, { ...ctx, now: ctx.now ?? runNow }),
      };
    });
    // The child's host-RPC: the parent's RPC server answers `foldNearDuplicates`
    // with the real `applyNearDuplicateFolds`.
    jest.doMock('@/lib/background-jobs/child/host-rpc-client', () => ({
      __esModule: true,
      callHost: async (method: string, request: unknown) => {
        if (method !== 'foldNearDuplicates') throw new Error(`unexpected host RPC ${method}`);
        const fold = jest.requireActual('@/lib/memory/maintenance/fold-near-duplicates');
        // The RPC server runs in the PARENT (one process here: step out of the child for the call).
        delete process.env.QUILLTAP_JOB_CHILD;
        try {
          return await fold.applyNearDuplicateFolds(request);
        } finally {
          process.env.QUILLTAP_JOB_CHILD = '1';
        }
      },
    }));
    jest.doMock('@/lib/background-jobs/child/child-repositories-proxy', () => {
      const actual = jest.requireActual('@/lib/background-jobs/child/child-repositories-proxy');
      return { __esModule: true, ...actual, setJobResult: (v: unknown) => { jobResult = v; } };
    });
    // Hold v4's job host off BEFORE any import (memory note
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
    const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import('@/lib/database/backends/sqlite/mount-index-client');
    const queue = await import('@/lib/background-jobs/queue-service');
    const triggers = await import('@/lib/memory/maintenance/triggers');
    const scheduler = await import('@/lib/memory/maintenance/scheduler');
    const ledger = await import('@/lib/memory/maintenance/ledger');
    const { handleMemoryMaintenance } = await import('@/lib/background-jobs/handlers/memory-maintenance');
    const { mirrorCommonplaceAfterCommit } = await import('@/lib/memory/commonplace-mirror');
    const settings = await import('@/lib/instance-settings');
    const { Logger } = await import('@/lib/logger');

    let sink: Array<{ level: string; message: string; context: unknown }> | null = null;
    for (const level of ['error', 'warn', 'info', 'debug'] as const) {
      const proto = Logger.prototype as unknown as Record<string, (...a: unknown[]) => void>;
      const original = proto[level];
      proto[level] = function (this: { context?: Record<string, unknown> }, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
        const { service: _s, environment: _e, ...own } = this.context ?? {};
        const merged = { ...own, ...(context ?? {}) };
        const mod = typeof merged.module === 'string' ? merged.module : '';
        if (sink && typeof message === 'string' && captured(mod, message)) {
          sink.push({ level, message, context: JSON.parse(JSON.stringify(merged)) });
        }
        return original.call(this, message, context, ...rest);
      };
    }

    await initializeDatabase();
    ledger.resetMaintenanceLedgerForTests();
    const plantJob = async (j: { id: string; type: string; status: string; payload: unknown; hoursAgo: number }) => {
      const at = new Date(Date.now() - j.hoursAgo * 3_600_000).toISOString();
      await rawQuery(
        `INSERT INTO background_jobs (id, userId, type, status, payload, priority, attempts, maxAttempts, lastError, scheduledAt, startedAt, completedAt, createdAt, updatedAt)
         VALUES (?, ?, ?, ?, ?, 0, 0, 1, NULL, ?, NULL, NULL, ?, ?)`,
        [j.id, spec.userId, j.type, j.status, JSON.stringify(resolve(j.payload)), at, at, at],
      );
    };
    for (const sql of corpus.plants) await rawQuery(sql);
    for (const j of corpus.jobs) await plantJob(j);
    const base = Date.now();

    const dumpJobs = async () =>
      ((await rawQuery(
        `SELECT id, userId, type, status, payload, priority, attempts, maxAttempts, lastError, scheduledAt, createdAt
           FROM background_jobs ORDER BY type, payload, status`,
      )) as Array<Record<string, unknown>>).map((r) => ({
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
      }));

    const lines: string[] = [];
    for (const raw of corpus.steps) {
      const step = resolve(raw) as Record<string, any>;
      if (step.op === 'sql') {
        await rawQuery(step.sql);
        continue;
      }
      if (step.op === 'job') {
        await plantJob(step.job);
        continue;
      }
      if (step.op === 'maintenanceSettings') {
        await settings.setMemoryMaintenanceSettings(step.value);
        continue;
      }
      if (step.op === 'consolidationSettings') {
        await settings.setMemoryConsolidationSettings(step.value);
        continue;
      }
      sink = [];
      let value: unknown = null;
      if (step.op === 'jobs') {
        value = await dumpJobs();
      } else if (step.op === 'status') {
        value = await ledger.getMaintenanceLedgerStatus(step.characterId);
      } else if (step.op === 'enqueue') {
        value = await queue.enqueueMemoryMaintenance(
          spec.userId,
          step.payload,
          step.scheduledOffsetMs !== undefined ? { scheduledAt: new Date(Date.now() + step.scheduledOffsetMs) } : undefined,
        );
      } else if (step.op === 'enqueueMany') {
        value = await queue.enqueueMemoryMaintenanceMany(spec.userId, step.characterIds, step.payload);
      } else if (step.op === 'enqueueForUser') {
        value = await queue.enqueueMemoryMaintenanceForUser(spec.userId, step.payload);
      } else if (step.op === 'maybeEnqueue') {
        value = await triggers.maybeEnqueueMaintenance(step.ids, base + step.offsetMs);
      } else if (step.op === 'scheduled') {
        value = await scheduler.runScheduledMemoryMaintenance();
      } else if (step.op === 'startupTick') {
        await scheduler.runStartupTick(base + step.offsetMs);
        value = 'done';
      } else if (step.op === 'handler') {
        const at = new Date(Date.now() - step.job.hoursAgo * 3_600_000).toISOString();
        const job = {
          id: step.job.id,
          userId: spec.userId,
          type: 'MEMORY_MAINTENANCE',
          status: 'PROCESSING',
          payload: step.job.payload,
          createdAt: at,
          updatedAt: at,
        };
        jobResult = undefined;
        process.env.QUILLTAP_JOB_CHILD = '1';
        try {
          await handleMemoryMaintenance(job as never);
        } finally {
          delete process.env.QUILLTAP_JOB_CHILD;
        }
        // The dispatcher's success arm, after the batch commits.
        if (jobResult && typeof jobResult === 'object') ledger.recordMaintenanceOutcome(jobResult as never);
        await mirrorCommonplaceAfterCommit(job.type, job.payload as never, []);
        value = 'completed';
      } else if (step.op === 'hook') {
        const writes = (step.touched as Array<[string, string[]]>).flatMap(([holder, subjects]) =>
          subjects.map((s) => ({ method: 'memories.create', args: [{ source: 'CONSOLIDATED', characterId: holder, aboutCharacterId: s }] })),
        );
        await mirrorCommonplaceAfterCommit(step.jobType, step.payload, writes);
        value = 'done';
      }
      for (let i = 0; i < 10; i++) await new Promise((r) => setImmediate(r));
      lines.push(JSON.stringify({ kind: 'step', name: step.name, value, lines: sink }));
      sink = null;
    }

    for (const [text, vector] of embeddings) lines.push(JSON.stringify({ kind: 'embedding', text, vector }));
    lines.push(JSON.stringify({ kind: 'jobs', rows: await dumpJobs() }));
    const hex = (v: unknown) => (v instanceof Uint8Array ? Buffer.from(v).toString('hex') : v);
    const rows = ((await rawQuery('SELECT * FROM memories ORDER BY characterId, id')) as Array<Record<string, unknown>>).map((r) => {
      const out: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(r)) out[k] = hex(v);
      return out;
    });
    lines.push(JSON.stringify({ kind: 'table', table: 'memories', rows }));
    const midb = getRawMountIndexDatabase();
    const docs = midb
      ? (midb
          .prepare(
            `SELECT l.mountPointId, l.relativePath, d.content FROM doc_mount_file_links l
             JOIN doc_mount_documents d ON d.fileId = l.fileId
             WHERE l.relativePath LIKE 'Commonplace/%' ORDER BY l.relativePath, d.content`,
          )
          .all() as unknown[])
      : [];
    lines.push(JSON.stringify({ kind: 'documents', rows: docs }));

    closeMountIndexSQLiteClient();
    await closeDatabase();
    fs.writeFileSync(outPath, lines.join('\n') + '\n');
    process.stderr.write(`memory maintenance queue tier-2 oracle wrote ${outPath}\n`);
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

test('memory maintenance queue tier-2 oracle', async () => {
  await main();
}, 180000);
