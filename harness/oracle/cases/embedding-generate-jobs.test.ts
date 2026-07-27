/**
 * @jest-environment node
 *
 * Tier-3 (recorded-embedding) ORACLE for the EMBEDDING_GENERATE job handler
 * (P4.6BL — v4 `handleEmbeddingGenerate`,
 * lib/background-jobs/handlers/embedding-generate.ts).
 *
 * Drives v4's REAL handler through v4's REAL queue claim/mark machinery over
 * the committed corpus (harness/oracle/fixtures/embedding-generate-jobs.json)
 * against the REAL two-database fixture, mocking ONLY the embedding seam the
 * Rust port injects:
 *
 *   - `generateEmbeddingForUser` — RECORD-and-replay: a text listed in the
 *     corpus `failingTexts` THROWS its pinned message on every call (recorded
 *     as `kind:"cannedEmbeddingFailure"`); any other text embeds to the fixed
 *     unit vector e0 (dim 8) and is recorded (`kind:"cannedEmbedding"`). The
 *     Rust side registers exactly the recorded entries, so a divergent
 *     embedding input surfaces as a canned-miss. `EMBEDDING_MAX_CHARS` is the
 *     REAL constant (spread from the actual module) — the oversize guard under
 *     test uses v4's own cap.
 *
 * Everything else — the handler's four entity branches, `skipIfOversize`,
 *`isPermanentEmbeddingError`, the repos (memories / conversationChunks /
 * helpDocs / docMountChunks / embeddingStatus / vectorIndices), and the queue's
 * REAL `claimNextJob` / `markCompleted` / `markFailed` (incl. the
 * attempts-vs-maxAttempts DEAD transition and the retry backoff) — is v4's
 * REAL code against the REAL fixture DBs.
 *
 * The drive is a claim loop, identical on the Rust side: claim the next due
 * job; run the handler; markCompleted on return / markFailed(error.message) on
 * throw. When the queue has no due job, any FAILED retry-eligible job's
 * `scheduledAt` is REWOUND to the epoch (raw SQL, both sides) so the backoff's
 * wall-clock wait never enters the differential; when no such job remains the
 * loop ends. Each processed job is recorded as a `kind:"processed"` row
 * (jobId, outcome, error) — the Rust side asserts the same sequence
 * element-for-element, which pins the claim order too.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<the v5 worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-eg-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-eg-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-embedding-generate-fixture.ts
 *   mkdir -p /tmp/qt-eg-oracle/cases /tmp/qt-eg-oracle/fixtures
 *   cp $V5/harness/oracle/cases/embedding-generate-jobs.test.ts /tmp/qt-eg-oracle/cases/
 *   cp $V5/harness/oracle/fixtures/embedding-generate-jobs.json /tmp/qt-eg-oracle/fixtures/
 *   TZ=UTC QT_FIXTURE_EG_MAIN=/tmp/qt-eg-main.db QT_FIXTURE_EG_MOUNT=/tmp/qt-eg-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-embedding-generate.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 --roots "$PWD" --roots "/tmp/qt-eg-oracle/cases" -- embedding-generate-jobs
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}
function canonicalizeRows(opts: {
  table: string;
  columns: string[];
  rawRows: Array<Record<string, unknown>>;
  orderBy?: string;
}): { table: string; columns: string[]; rows: Array<Record<string, unknown>> } {
  const { table, columns, rawRows, orderBy = 'id' } = opts;
  const rows = rawRows
    .map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
      return out;
    })
    .sort((a, b) => {
      const av = String(a[orderBy] ?? '');
      const bv = String(b[orderBy] ?? '');
      return av < bv ? -1 : av > bv ? 1 : 0;
    });
  return { table, columns, rows };
}

interface RouteCase {
  name: string;
  kind: 'generate' | 'rebuild';
  body: Record<string, unknown>;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  profileId: string;
  embedVector: number[];
  failingTexts: Array<{ text: string; message: string }>;
  routeCases: RouteCase[];
  jobs: Array<{ id: string; name: string }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'embedding-generate-jobs.json'), 'utf8')
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_EG_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_EG_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_EG_MAIN / QT_FIXTURE_EG_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-eg-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'eg-main.db');
  const workMount = join(scratch, 'eg-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const embeddingRecorded = new Set<string>();
  const failureRecorded = new Map<string, string>();

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
        const failing = spec.failingTexts.find((f) => f.text === text);
        if (failing) {
          failureRecorded.set(text, failing.message);
          throw new Error(failing.message);
        }
        embeddingRecorded.add(text);
        return {
          embedding: new Float32Array(spec.embedVector),
          model: 'canned',
          dimensions: spec.embedVector.length,
          provider: 'canned',
        };
      },
    };
  });

  // The route phase (the tier-2 arms) runs through v4's real
  // `createAuthenticatedHandler` — mock the session + startup gate exactly as
  // the memories-routes family does.
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: spec.userId } }),
  }));
  jest.doMock('@/lib/startup/startup-state', () => {
    const actual = jest.requireActual('@/lib/startup/startup-state');
    return {
      __esModule: true,
      ...actual,
      startupState: {
        ...actual.startupState,
        isReady: () => true,
        waitForReady: async () => true,
        isPepperResolved: () => true,
        getPepperState: () => 'resolved',
        getPhase: () => 'ready',
        isLockedMode: () => false,
      },
    };
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { handleEmbeddingGenerate } = await import(
    '@/lib/background-jobs/handlers/embedding-generate'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();
  const repos = getRepositories();

  const lines: string[] = [];

  // The claim loop (see the header). Bounded far above any legitimate run so a
  // port bug cannot spin it forever.
  const REWIND_SQL =
    "UPDATE \"background_jobs\" SET \"scheduledAt\" = '1999-01-01T00:00:00.000Z' " +
    "WHERE \"status\" = 'FAILED' AND \"attempts\" < \"maxAttempts\"";
  let guard = 0;
  for (;;) {
    if (++guard > 200) throw new Error('claim loop failed to converge');
    const job = await repos.backgroundJobs.claimNextJob();
    if (!job) {
      const mainDb = getRawDatabase();
      if (!mainDb) throw new Error('main DB handle unavailable');
      const rewound = mainDb.prepare(REWIND_SQL).run().changes;
      if (rewound === 0) break;
      continue;
    }
    let error: string | null = null;
    try {
      await handleEmbeddingGenerate(job);
      await repos.backgroundJobs.markCompleted(job.id);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      await repos.backgroundJobs.markFailed(job.id, error);
    }
    lines.push(
      JSON.stringify({
        kind: 'processed',
        jobId: job.id,
        outcome: error === null ? 'completed' : 'failed',
        error,
      })
    );
  }

  // ── The tier-2 route phase: drive v4's REAL route arms (POST/PUT
  // `?action=embeddings`) over the post-claim-loop state, in corpus order.
  // The Rust side dispatches the same requests through its ported arms and
  // compares status + body; the table dumps below capture BOTH phases.
  const routes = await import('@/app/api/v1/memories/route');
  for (const rc of spec.routeCases) {
    const url = 'http://localhost:3000/api/v1/memories?action=embeddings';
    const req = {
      method: rc.kind === 'generate' ? 'POST' : 'PUT',
      url,
      nextUrl: new URL(url),
      headers: new Headers({ 'Content-Type': 'application/json' }),
      json: jest.fn().mockResolvedValue(rc.body),
    };
    const res =
      rc.kind === 'generate'
        ? await routes.POST(req as never)
        : await routes.PUT(req as never);
    const body = await (res as Response).json();
    lines.push(
      JSON.stringify({ kind: 'route', name: rc.name, status: (res as Response).status, body })
    );
  }

  for (const text of embeddingRecorded.values()) {
    lines.push(JSON.stringify({ kind: 'cannedEmbedding', text, vector: spec.embedVector }));
  }
  for (const [text, message] of failureRecorded.entries()) {
    lines.push(JSON.stringify({ kind: 'cannedEmbeddingFailure', text, message }));
  }

  const dumpTable = async (table: string, orderBy: string) => {
    const columns = (
      (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
    ).map((c) => c.name);
    const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };
  const dumpMountTable = (table: string, orderBy: string) => {
    const midb = getRawMountIndexDatabase();
    if (!midb) throw new Error('mount-index DB handle unavailable');
    const columns = (
      midb.prepare(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>
    ).map((c) => c.name);
    const rawRows = midb.prepare(`SELECT * FROM ${table}`).all() as Array<
      Record<string, unknown>
    >;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };

  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('background_jobs', 'id')) }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('embedding_status', 'id')) }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('memories', 'id')) }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('vector_entries', 'id')) }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('vector_indices', 'id')) }));
  lines.push(
    JSON.stringify({ kind: 'table', ...(await dumpTable('conversation_chunks', 'id')) })
  );
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('help_docs', 'id')) }));
  lines.push(JSON.stringify({ kind: 'table', ...dumpMountTable('doc_mount_chunks', 'id') }));

  closeMountIndexSQLiteClient();
  await closeDatabase();

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`embedding-generate oracle wrote ${outPath}\n`);
}

test('embedding-generate jobs tier-3 oracle', async () => {
  await main();
});
