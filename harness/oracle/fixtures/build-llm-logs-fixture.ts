/**
 * Tier-2 fixture builder — materializes the shared starting **llm-logs** sibling
 * DB for the `llm_logs` repo from the committed plaintext spec
 * (`llm-logs-tier2.json`).
 *
 * ── THE SIBLING-DB MACHINERY ─────────────────────────────────────────────────
 * Like the mount-index repos, this repo's data lives in v4's dedicated llm-logs
 * database (`quilltap-llm-logs.db`), NOT the main DB. v4 resolves that file from
 * `SQLITE_LLM_LOGS_PATH` (or `<dataDir>/quilltap-llm-logs.db`), keyed with the
 * SAME `ENCRYPTION_MASTER_PEPPER`. So the recipe is:
 *   - point SQLITE_LLM_LOGS_PATH at the fixture we want to KEEP (`QT_FIXTURE_OUT`);
 *   - point SQLITE_PATH at a THROWAWAY main DB AND SQLITE_MOUNT_INDEX_PATH at a
 *     throwaway mount-index DB, both in scratch (initializeDatabase stands all
 *     three backends up; we only read the llm-logs one);
 *   - seed through the REAL `LLMLogsRepository`, whose overridden getCollection()
 *     lazily CREATE-TABLE-IF-NOT-EXISTS-es and writes to the llm-logs DB on first
 *     access — no explicit ensureCollection.
 * Unlike the mount-index client, the backend disconnect DOES close the llm-logs
 * client (backend.ts calls closeLLMLogsSQLiteClient()), so `closeDatabase()`
 * alone flushes the llm-logs file.
 *
 * journal_mode is TRUNCATE on all DBs (SQLITE_WAL_MODE unset → walMode=false →
 * journalMode default 'truncate'), so each committed transaction is
 * self-contained in the `.db` file — the Rust `Writer::open_writable` (also
 * TRUNCATE) then opens the fixture copy directly.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-ll-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-llm-logs-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seed: Array<Record<string, unknown>>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'llm-logs-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the llm-logs fixture .db to write');
  }

  // Fresh output: drop any prior fixture so we never seed on top of stale state.
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  // Throwaway data dir absorbs v4's operational scaffolding (instance lock,
  // startup physical backups, the THROWAWAY main + mount-index DBs). A unique dir
  // per run avoids stale-lock collisions. The LLM-LOGS db lands at QT_FIXTURE_OUT.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-ll-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db'); // throwaway main DB
  process.env.SQLITE_MOUNT_INDEX_PATH = join(scratch, 'data', 'mount-index.db'); // throwaway
  process.env.SQLITE_LLM_LOGS_PATH = out; // the fixture we KEEP
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE; // all DBs use journal_mode = TRUNCATE
  process.env.LOG_LEVEL = 'error'; // keep stdout/stderr quiet for clean runs

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { LLMLogsRepository } = await import(
    '@/lib/database/repositories/llm-logs.repository'
  );

  await initializeDatabase();

  // The repo's overridden getCollection() routes to the llm-logs DB and creates
  // the table on first access — no explicit ensureCollection needed.
  const repo = new LLMLogsRepository();
  for (const row of spec.seed) {
    const { id, createdAt, updatedAt, ...data } = row as Record<string, unknown>;
    await repo.create(data as never, {
      id: id as string,
      createdAt: createdAt as string,
      updatedAt: updatedAt as string,
    });
  }

  // The backend disconnect closes the llm-logs client, so closeDatabase() flushes
  // the llm-logs file for us.
  await closeDatabase();

  process.stderr.write(
    `built llm_logs llm-logs-DB fixture: ${out} (${spec.seed.length} seed rows)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
