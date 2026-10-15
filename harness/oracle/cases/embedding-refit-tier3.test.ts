/**
 * @jest-environment node
 *
 * Tier-3 (DB-state) ORACLE for v4's EMBEDDING_REFIT handler (W4.7e2 — the BUILTIN
 * TF-IDF embedder refit). The refit path touches no LLM (fitCorpus is pure), but
 * v4's `enqueueEmbeddingReindexAll` calls `ensureProcessorRunning()`, which would
 * auto-start the in-process job runner and claim the freshly-enqueued reindex row
 * (PENDING → PROCESSING). So this runs the jest-real-DB way and pins exactly that
 * one extra seam: `ensureProcessorRunning` → no-op (the Rust port defers the runner
 * the same way — the enqueued PENDING row is the state under test).
 *
 * Beyond the processor mock, the standard jest-real-DB mocks wire v4's REAL DB
 * stack back in past jest.setup's globals (better-sqlite3 → the cipher driver,
 * manager / repositories / factory / character-vault-bridge → requireActual) so the
 * character overlay reads the real mount-index vaults.
 *
 * `Date` is frozen to `spec.frozenDate` so the minted fittedAt / createdAt /
 * updatedAt / scheduledAt are the pinned value; the Rust harness placeholders
 * id/createdAt/updatedAt/scheduledAt, keeps fittedAt exact, and compares the `idf`
 * column numerically (the Math.log libm seam).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_REFIT_MAIN=/tmp/qt-refit-main.db \
 *   QT_FIXTURE_REFIT_MOUNT=/tmp/qt-refit-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-embedding-refit-fixture.ts
 *   QT_FIXTURE_REFIT_MAIN=/tmp/qt-refit-main.db \
 *   QT_FIXTURE_REFIT_MOUNT=/tmp/qt-refit-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-refit.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- embedding-refit-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

// Inlined canonicalizer (jest can't resolve the `.js` ESM specifier of
// `../lib/tier2.js`, and this file also runs from a non-`.claude` copy). No BLOB
// columns in these two tables, but hex-encode defensively for parity with tier2.js.
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

interface Spec {
  testPepperBase64: string;
  frozenDate: string;
  userId: string;
  profileId: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'embedding-refit-tier3.json'), 'utf8')
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_REFIT_MAIN;
  const mountFixture = process.env.QT_FIXTURE_REFIT_MOUNT;
  if (!mainFixture || !mountFixture || !existsSync(mainFixture) || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_REFIT_MAIN / QT_FIXTURE_REFIT_MOUNT must point at the built fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );

  const scratch = mkdtempSync(join(tmpdir(), 'qt-refit-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'refit-main.db');
  const mountWork = join(scratch, 'refit-mount.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'debug';

  jest.resetModules();
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories')
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge')
  );
  // Keep the in-process job runner OFF so the enqueued reindex row stays PENDING.
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
  });

  // P4.D292: the handler's nine lines (v5 emitted none before the lane).
  const CAPTURED = new Set([
    '[EmbeddingRefit] Starting vocabulary refit',
    '[EmbeddingRefit] Profile is not BUILTIN, skipping refit',
    '[EmbeddingRefit] Corpus gathered',
    '[EmbeddingRefit] No memories found, skipping refit',
    '[EmbeddingRefit] Failed to load help docs for corpus, continuing without them',
    '[EmbeddingRefit] Vocabulary fitted and saved',
    '[EmbeddingRefit] Enqueued reindex job',
  ]);
  const captured: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  const { logger } = await import('@/lib/logger');
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown>; error?: { message?: string } };
      if (!CAPTURED.has(data.message)) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      if (data.error?.message !== undefined) fields.error = data.error.message;
      captured.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
    },
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { handleEmbeddingRefit } = await import(
    '@/lib/background-jobs/handlers/embedding-refit'
  );

  // Freeze Date so every minted timestamp equals frozenDate.
  const FROZEN = new Date(spec.frozenDate).getTime();
  const RealDate = Date;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  global.Date = class extends RealDate {
    constructor(...a: unknown[]) {
      if (a.length === 0) super(FROZEN);
      // @ts-expect-error forward variadic args
      else super(...a);
    }
    static now(): number {
      return FROZEN;
    }
  } as unknown as DateConstructor;

  const lines: string[] = [];
  try {
    await initializeDatabase();

    const job = {
      id: 'job-refit-0001',
      userId: spec.userId,
      type: 'EMBEDDING_REFIT',
      status: 'PROCESSING',
      payload: { profileId: spec.profileId, triggerReindex: true },
    };
    await handleEmbeddingRefit(job as never);

    const dumpMain = async (table: string, orderBy: string) => {
      const columns = (
        (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
      ).map((c) => c.name);
      const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
      return canonicalizeRows({ table, columns, rawRows, orderBy });
    };

    lines.push(
      JSON.stringify({
        case: 'embedding-refit-tier3',
        ...(await dumpMain('tfidf_vocabularies', 'profileId')),
      })
    );
    lines.push(
      JSON.stringify({
        case: 'embedding-refit-tier3',
        ...(await dumpMain('background_jobs', 'type')),
      })
    );
    lines.push(JSON.stringify({ case: 'embedding-refit-tier3', kind: 'lines', lines: captured.splice(0) }));

    // P4.D292: the handler's other reachable arms, AFTER the dumps (they write
    // nothing the dumps compare): a non-BUILTIN profile (WARN, no fit), a
    // missing profile (the throw), an empty corpus (INFO, no fit).
    const arms: Array<Record<string, unknown>> = [];
    const arm = async (name: string, profileId: string) => {
      let error: string | null = null;
      try {
        await handleEmbeddingRefit({ ...job, id: `job-refit-${name}`, payload: { profileId, triggerReindex: true } } as never);
      } catch (e) {
        error = e instanceof Error ? e.message : String(e);
      }
      arms.push({ name, profileId, error, lines: captured.splice(0) });
    };
    await rawQuery(
      `INSERT INTO "embedding_profiles" ("id","userId","name","provider","modelName","isDefault","createdAt","updatedAt") VALUES ('e2920000-0000-4000-8000-0000000000e2','${spec.userId}','OpenAI','OPENAI','text-embedding-3-small',0,'2020-01-01T00:00:00.000Z','2020-01-01T00:00:00.000Z')`,
    );
    await arm('not-builtin', 'e2920000-0000-4000-8000-0000000000e2');
    await arm('missing', 'e2920000-0000-4000-8000-0000000000ee');
    await rawQuery('DELETE FROM "memories"');
    await arm('no-memories', spec.profileId);
    lines.push(JSON.stringify({ case: 'embedding-refit-tier3', kind: 'arms', arms }));
  } finally {
    global.Date = RealDate;
    await new Promise((resolve) => setTimeout(resolve, 50));
    closeMountIndexSQLiteClient();
    await closeDatabase();
    rmSync(scratch, { recursive: true, force: true });
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`embedding-refit oracle wrote ${outPath}\n`);
}

test('embedding-refit tier-3 oracle', async () => {
  await main();
});
