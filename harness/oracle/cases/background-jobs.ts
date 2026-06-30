/**
 * Tier-2 oracle case — the `background_jobs` repo (Phase-2, main DB).
 *
 * Proves what state v4 leaves the database in after a fixed sequence of CRUD +
 * queue ops (create / claimNextJob / markFailed×2 / pause / resume /
 * resetAllProcessingJobs / update), so the Rust port can be diffed against it
 * (structural DB diff).
 *
 * Flow (mirrors text-replacement-rules-tier2.ts):
 *   1. copy the SEED-ONLY fixture to a fresh working copy;
 *   2. open the copy through v4's real `initializeDatabase()` and run the op
 *      sequence from `background-jobs-tier2.json` via the real
 *      `BackgroundJobsRepository`. Ops with assertable outcomes
 *      (`expectClaimId` / `expectCount`) are checked against v4 (the oracle of
 *      truth) — a mismatch aborts;
 *   3. close, then dump the table canonically (RAW on-disk cells via `rawQuery`:
 *      the REAL number columns come back as JS numbers, the JSON payload as its
 *      text) and emit it as one NDJSON row.
 *
 * NORMALIZATION SPEC: minted-timestamp placeholder. ids + createdAt are pinned;
 * status / attempts / lastError / payload / priority / maxAttempts are pinned or
 * deterministic; only the four mintable timestamp columns (scheduledAt,
 * startedAt, completedAt, updatedAt) are nondeterministic and get collapsed to
 * `<ts>` by the Rust harness on BOTH dumps. This case emits the raw (un-collapsed)
 * dump; the harness does the symmetric placeholdering.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`), AFTER
 * building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_BJ=/tmp/qt-bj-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/background-jobs.ts \
 *     > /tmp/oracle-bj.ndjson
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
  kind:
    | 'create'
    | 'update'
    | 'delete'
    | 'claimNextJob'
    | 'markFailed'
    | 'markCompleted'
    | 'pause'
    | 'resume'
    | 'resetAllProcessingJobs'
    | 'resetStuckJobs'
    | 'cancel'
    | 'cancelByType'
    | 'deleteByTypesAndStatuses';
  id?: string;
  error?: string;
  expectClaimId?: string;
  expectCount?: number;
  expectModified?: boolean;
  result?: Record<string, unknown>;
  timeoutMinutes?: number;
  type?: string;
  types?: string[];
  statuses?: string[];
  data?: Record<string, unknown>;
  options?: { id: string; createdAt: string; updatedAt: string };
}

interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'background-jobs-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_BJ;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_BJ must point at the seed fixture from build-background-jobs-fixture.ts'
    );
  }

  // Work on a fresh copy so the shared seed fixture stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-bj-oracle-'));
  // v4 nests working files under `<dataDir>/data/` (instance lock, sibling DBs).
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'bj-work.db');
  copyFileSync(fixture, work);

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE; // writable path uses journal_mode = TRUNCATE
  // Keep stdout clean for the NDJSON: v4's console logger sends INFO to stdout.
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { BackgroundJobsRepository } = await import(
    '@/lib/database/repositories/background-jobs.repository'
  );

  await initializeDatabase();
  const repo = new BackgroundJobsRepository();

  for (const op of spec.ops) {
    switch (op.kind) {
      case 'create':
        await repo.create(op.data as never, op.options);
        break;
      case 'update':
        await repo.update(op.id as string, op.data as never);
        break;
      case 'delete':
        await repo.delete(op.id as string);
        break;
      case 'claimNextJob': {
        const claimed = await repo.claimNextJob();
        if (op.expectClaimId !== undefined) {
          const got = claimed?.id;
          if (got !== op.expectClaimId) {
            throw new Error(
              `claimNextJob claimed ${got ?? 'null'}, expected ${op.expectClaimId} — fixture mis-designed`
            );
          }
        }
        break;
      }
      case 'markFailed':
        await repo.markFailed(op.id as string, op.error as string);
        break;
      case 'markCompleted':
        await repo.markCompleted(op.id as string, op.result);
        break;
      case 'pause':
        await repo.pause(op.id as string);
        break;
      case 'resume':
        await repo.resume(op.id as string);
        break;
      case 'resetAllProcessingJobs': {
        const count = await repo.resetAllProcessingJobs();
        if (op.expectCount !== undefined && count !== op.expectCount) {
          throw new Error(
            `resetAllProcessingJobs modified ${count}, expected ${op.expectCount} — fixture mis-designed`
          );
        }
        break;
      }
      case 'resetStuckJobs': {
        const count = await repo.resetStuckJobs(op.timeoutMinutes as number);
        if (op.expectCount !== undefined && count !== op.expectCount) {
          throw new Error(
            `resetStuckJobs modified ${count}, expected ${op.expectCount} — fixture mis-designed`
          );
        }
        break;
      }
      case 'cancel': {
        const modified = await repo.cancel(op.id as string);
        if (op.expectModified !== undefined && modified !== op.expectModified) {
          throw new Error(
            `cancel returned ${modified}, expected ${op.expectModified} — fixture mis-designed`
          );
        }
        break;
      }
      case 'cancelByType': {
        const count = await repo.cancelByType(op.type as never);
        if (op.expectCount !== undefined && count !== op.expectCount) {
          throw new Error(
            `cancelByType modified ${count}, expected ${op.expectCount} — fixture mis-designed`
          );
        }
        break;
      }
      case 'deleteByTypesAndStatuses': {
        const count = await repo.deleteByTypesAndStatuses(
          op.types as never,
          op.statuses as never
        );
        if (op.expectCount !== undefined && count !== op.expectCount) {
          throw new Error(
            `deleteByTypesAndStatuses removed ${count}, expected ${op.expectCount} — fixture mis-designed`
          );
        }
        break;
      }
      default:
        throw new Error(`unknown op kind: ${(op as { kind: string }).kind}`);
    }
  }

  // Read RAW on-disk state through v4's own connected backend. table_info gives
  // schema column order; SELECT * gives the persisted rows (REAL numbers as JS
  // numbers, the payload as its JSON text).
  const columns = (
    (await rawQuery('PRAGMA table_info(background_jobs)')) as Array<{
      name: string;
    }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM background_jobs')) as Array<
    Record<string, unknown>
  >;

  await closeDatabase();

  const dump = canonicalizeRows({
    table: 'background_jobs',
    columns,
    rawRows,
    orderBy: 'id',
  });

  process.stdout.write(JSON.stringify({ case: 'background-jobs-tier2', ...dump }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`background-jobs-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
