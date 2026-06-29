/**
 * Tier-2 oracle case — the `llm_logs` repo (the SECOND sibling-DB partition of
 * Phase 2; the WIDEST repo to date — 18 columns, FIVE nested JSON-object
 * columns).
 *
 * Proves what state v4 leaves the LLM-LOGS database in after a fixed
 * create + update + delete sequence, so the Rust port can be diffed against it.
 *
 * ── THE SIBLING-DB MACHINERY ─────────────────────────────────────────────────
 * The data lives in `quilltap-llm-logs.db`, NOT the main DB. So:
 *   - SQLITE_LLM_LOGS_PATH points at the working COPY of the fixture (seed + ops
 *     applied here); SQLITE_PATH + SQLITE_MOUNT_INDEX_PATH point at fresh
 *     throwaway DBs;
 *   - ops run through the REAL `LLMLogsRepository` (overridden getCollection
 *     routes to the llm-logs DB);
 *   - the raw read-back is done through the llm-logs handle directly
 *     (`getRawLLMLogsDatabase()`), NOT `rawQuery` — `rawQuery` targets the MAIN
 *     backend, so it would read the wrong (empty) database. CRUCIALLY this read
 *     happens BEFORE `closeDatabase()`, because the backend disconnect closes the
 *     llm-logs client (the handle would be gone afterward). The handle is a
 *     better-sqlite3 connection, so `pragma('table_info')` gives schema column
 *     order and `prepare('SELECT *').all()` gives the persisted rows.
 *
 * NORMALIZATION SPEC: none. Every id and timestamp is pinned on both sides, so
 * the dumps must match outright — no id remap, no timestamp placeholder. The
 * nested JSON columns are compared as their stored compact-JSON TEXT.
 *
 * Run from the v4 server checkout under Node 24, AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_LLM_LOGS=/tmp/qt-ll-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/llm-logs-tier2.ts \
 *     > /tmp/oracle-ll.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  copyFileSync,
  existsSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind: 'create' | 'update' | 'delete';
  id?: string;
  data?: Record<string, unknown>;
  options?: { id: string; createdAt: string; updatedAt: string };
}

interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'llm-logs-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_LLM_LOGS;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_LLM_LOGS must point at the seed fixture from build-llm-logs-fixture.ts'
    );
  }

  // Work on a fresh copy of the llm-logs fixture so the shared seed stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-ll-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'll-logs-work.db');
  copyFileSync(fixture, work);

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db'); // fresh throwaway main DB
  process.env.SQLITE_MOUNT_INDEX_PATH = join(scratch, 'data', 'mount-index.db'); // throwaway
  process.env.SQLITE_LLM_LOGS_PATH = work; // the working copy we mutate + read
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE; // journal_mode = TRUNCATE on all DBs
  process.env.LOG_LEVEL = 'error'; // keep the NDJSON pipe clean

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawLLMLogsDatabase } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const { LLMLogsRepository } = await import(
    '@/lib/database/repositories/llm-logs.repository'
  );

  await initializeDatabase();
  const repo = new LLMLogsRepository();

  for (const op of spec.ops) {
    if (op.kind === 'create') {
      await repo.create(op.data as never, op.options);
    } else if (op.kind === 'update') {
      await repo.update(op.id as string, op.data as never);
    } else {
      await repo.delete(op.id as string);
    }
  }

  // Read RAW on-disk state through the LLM-LOGS handle directly, BEFORE
  // closeDatabase() (the backend disconnect closes the llm-logs client, so the
  // handle would be gone afterward). table_info gives schema column order;
  // SELECT * gives the persisted rows.
  const lldb = getRawLLMLogsDatabase();
  if (!lldb) {
    throw new Error('llm-logs DB handle unavailable (degraded open?)');
  }
  const columns = (
    lldb.pragma('table_info(llm_logs)') as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = lldb
    .prepare('SELECT * FROM llm_logs')
    .all() as Array<Record<string, unknown>>;

  await closeDatabase();

  const dump = canonicalizeRows({
    table: 'llm_logs',
    columns,
    rawRows,
    orderBy: 'id',
  });

  process.stdout.write(JSON.stringify({ case: 'llm-logs-tier2', ...dump }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`llm-logs-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
