/**
 * @jest-environment node
 *
 * P4.d6 oracle case — `ensureHelpDocsSynced` (v4 `6c59b1ca` bug 1 +
 * `551f090b`'s divergence trigger / prune / embedding enqueue). Since v4
 * `492771aff` (P4.D222) the function is the once-per-process memo over
 * `reconcileHelpDocs` — the full sync, the section backfill, and the enqueue
 * of every incomplete doc — so the scenarios below exercise that instead; the
 * `calls` field drives the memo's multi-call arms, and each line carries
 * `logs: {reconciled, reconcileFailed}`.
 *
 * Drives v4's REAL `ensureHelpDocsSynced()` end to end against a REAL encrypted
 * database: the trigger's `helpDocsDivergeFromDisk` (private — exercised
 * through its caller, not imported), `syncHelpDocs`, and
 * `enqueueMissingHelpDocEmbeddings`. Five scenarios share ONE committed help
 * tree (`fixtures/help-ensure/help/`) and vary only the seed, so each isolates
 * a single trigger path.
 *
 * ## Why jest and not tsx
 *
 * `enqueueJob()` calls `ensureProcessorRunning()` AFTER inserting its row, and
 * that resolves v4's job-child entry relative to `process.cwd()`. This case
 * MUST chdir (v4 captures `HELP_DIR = join(process.cwd(), 'help')` at module
 * load), so the resolution throws — and `enqueueMissingHelpDocEmbeddings`
 * swallows it by design, silently truncating the corpus after one job. Only a
 * module mock fixes that, so this is the `[[jest-real-db-oracle]]` recipe.
 * `QUILLTAP_JOB_CHILD=1` is NOT the lever: it puts the whole data layer into
 * child mode ("Cannot append write … outside of a job scope").
 *
 * jest's `resetModules` + re-import is also what makes one scenario per
 * SCENARIO possible in one process: re-loading `help-doc-sync` re-captures
 * `HELP_DIR` from the current cwd.
 *
 * Emits one line per scenario:
 *   { kind: 'ensure', scenario, helpDocs: [...], jobs: [...] }
 * Minted ids never reach the corpus: doc rows report `path` (+ `<minted>` for a
 * non-seeded id), and jobs resolve their payload `entityId` to the doc's PATH.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixtures:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_ENSURE_DIR=/tmp/qt-ensure \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-help-ensure-fixture.ts
 *   TMPO=/tmp/qt-ensure-oracle; rm -rf $TMPO; mkdir -p $TMPO/cases $TMPO/fixtures
 *   cp $V5/harness/oracle/cases/help-sync-ensure.test.ts $TMPO/cases/
 *   cp $V5/harness/oracle/fixtures/help-ensure.json $TMPO/fixtures/
 *   cp -R $V5/harness/oracle/fixtures/help-ensure $TMPO/fixtures/
 *   PATH=$N:$PATH QT_FIXTURE_ENSURE_DIR=/tmp/qt-ensure \
 *   QT_ORACLE_OUT=/tmp/oracle-help-ensure.ndjson \
 *     npx jest --silent --testTimeout=120000 --roots $TMPO/cases -- help-sync-ensure
 */

import { describe, it } from '@jest/globals';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import * as fs from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedDoc {
  id: string;
  path: string;
}

interface SeedChunk {
  id: string;
}

interface Scenario {
  name: string;
  seedProfile: boolean;
  helpDocs: SeedDoc[];
  seedChunks?: SeedChunk[];
  /** P4.D222 — the multi-call gate arms; absent = one call. */
  calls?: 'concurrent-then-later' | 'fail-once-then-retry';
}

interface Spec {
  testPepperBase64: string;
  seedSentinel: string;
  scenarios: Scenario[];
}

// P4.D222 — the fail-once-then-retry plant (the Rust side runs the same SQL).
const FAIL_TRIGGER_CREATE =
  'CREATE TRIGGER "p4d222_fail_section_insert" BEFORE INSERT ON "help_doc_chunks" ' +
  "BEGIN SELECT RAISE(ABORT, 'planted reconcile failure'); END";
const FAIL_TRIGGER_DROP = 'DROP TRIGGER "p4d222_fail_section_insert"';

// Captured BEFORE any chdir — the cipher driver path and the jest module
// registry both resolve from the v4 project root.
const V4_ROOT = process.cwd();

describe('help-doc-sync: ensureHelpDocsSynced', () => {
  it('records the trigger / prune / enqueue corpus', async () => {
    const here = dirname(fileURLToPath(import.meta.url));
    const spec = JSON.parse(
      fs.readFileSync(join(here, '..', 'fixtures', 'help-ensure.json'), 'utf8'),
    ) as Spec;

    const outPath = process.env.QT_ORACLE_OUT;
    if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
    const fixtureDir = process.env.QT_FIXTURE_ENSURE_DIR;
    if (!fixtureDir) throw new Error('QT_FIXTURE_ENSURE_DIR must be set');

    const helpTreeRoot = join(here, '..', 'fixtures', 'help-ensure');
    const lines: string[] = [];

    for (const scenario of spec.scenarios) {
      const fixtureMain = join(fixtureDir, `qt-ensure-${scenario.name}-main.db`);
      if (!fs.existsSync(fixtureMain)) throw new Error(`missing fixture ${fixtureMain}`);

      const scratch = fs.mkdtempSync(join(tmpdir(), `qt-ensure-oracle-${scenario.name}-`));
      fs.mkdirSync(join(scratch, 'data'), { recursive: true });
      const workMain = join(scratch, 'ensure-main.db');
      fs.copyFileSync(fixtureMain, workMain);

      process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
      process.env.SQLITE_PATH = workMain;
      process.env.QUILLTAP_DATA_DIR = scratch;
      delete process.env.SQLITE_WAL_MODE;
      process.env.LOG_LEVEL = 'error';

      jest.resetModules();
      // HELP_DIR is captured at module load from process.cwd(), and resetModules
      // above guarantees the fresh import below re-captures it.
      process.chdir(helpTreeRoot);

      const cipherDriverPath = join(
        V4_ROOT,
        'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
      );
      jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
      // Defeat jest.setup's global DB mocks — we want the REAL data layer.
      jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
      jest.doMock('@/lib/repositories/factory', () =>
        jest.requireActual('@/lib/repositories/factory'),
      );
      // The one mock that is NOT about defeating jest.setup: see the header.
      jest.doMock('@/lib/background-jobs/processor', () => {
        const actual = jest.requireActual('@/lib/background-jobs/processor');
        return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
      });

      const { initializeDatabase, closeDatabase, rawQuery } = await import(
        '@/lib/database/manager'
      );
      const { ensureHelpDocsSynced } = await import('@/lib/help/help-doc-sync');
      const { logger } = await import('@/lib/logger');

      // The sync + enqueue are best-effort BY DESIGN: they swallow errors into a
      // logger.error and carry on. That is correct v4 behavior and the port
      // reproduces it — but it also means a broken HARNESS yields a corpus that
      // looks legitimate. Fail loudly instead of pinning an artifact.
      const swallowed: string[] = [];
      const realError = logger.error.bind(logger);
      (logger as unknown as { error: (...a: unknown[]) => void }).error = (...args: unknown[]) => {
        const msg = String(args[0] ?? '');
        if (msg.includes('[HelpDocSync]')) swallowed.push(JSON.stringify(args));
        return (realError as (...a: unknown[]) => void)(...args);
      };
      // P4.D222 (v4 `492771aff`) — the gate swallows a failed reconcile into
      // logger.WARN (`Help doc reconcile failed; …`), never block help from
      // loading. Watch it too, or a reconcile that silently failed would pin as
      // legitimate behavior. The ONE scenario that plants a failure counts it
      // instead (and must see exactly one).
      const logs = { reconciled: 0, reconcileFailed: 0 };
      const realWarn = logger.warn.bind(logger);
      (logger as unknown as { warn: (...a: unknown[]) => void }).warn = (...args: unknown[]) => {
        const msg = String(args[0] ?? '');
        if (msg.includes('[HelpDocSync] Help doc reconcile failed')) {
          logs.reconcileFailed++;
          if (scenario.calls !== 'fail-once-then-retry') swallowed.push(JSON.stringify(args));
        } else if (msg.includes('[HelpDocSync]') && msg.includes('failed')) {
          swallowed.push(JSON.stringify(args));
        }
        return (realWarn as (...a: unknown[]) => void)(...args);
      };
      const realInfo = logger.info.bind(logger);
      (logger as unknown as { info: (...a: unknown[]) => void }).info = (...args: unknown[]) => {
        if (String(args[0] ?? '') === '[HelpDocSync] Help docs reconciled') logs.reconciled++;
        return (realInfo as (...a: unknown[]) => void)(...args);
      };

      await initializeDatabase();
      if (scenario.calls === 'concurrent-then-later') {
        // Two callers race the same run, then a third arrives after it settled.
        await Promise.all([ensureHelpDocsSynced(), ensureHelpDocsSynced()]);
        await ensureHelpDocsSynced();
      } else if (scenario.calls === 'fail-once-then-retry') {
        // The first run fails on a REAL database (no mock): a planted trigger
        // aborts every section insert, so the backfill's `replaceForDoc` throws
        // out of the reconcile. (Renaming the table away does NOT work in v4 —
        // the repository's `getCollection` recreates a missing table on first
        // access.) Dropped, the next call must start a fresh run.
        await rawQuery(FAIL_TRIGGER_CREATE, []);
        await ensureHelpDocsSynced();
        await rawQuery(FAIL_TRIGGER_DROP, []);
        await ensureHelpDocsSynced();
      } else {
        await ensureHelpDocsSynced();
      }

      if (swallowed.length > 0) {
        throw new Error(
          `[${scenario.name}] ensureHelpDocsSynced swallowed ${swallowed.length} error(s) — the ` +
            `corpus would be a harness artifact, not v4 behavior:\n${swallowed.join('\n')}`,
        );
      }

      const docRows =
        (await rawQuery<Array<Record<string, unknown>>>(
          'SELECT id, path, title, hex(embedding) AS embeddingHex, updatedAt FROM help_docs ORDER BY path ASC',
          [],
        )) ?? [];

      const seededIds = new Set(scenario.helpDocs.map((d) => d.id));
      const pathById = new Map<string, string>();
      for (const r of docRows) pathById.set(String(r.id), String(r.path));

      const helpDocs = docRows.map((r) => ({
        id: seededIds.has(String(r.id)) ? String(r.id) : '<minted>',
        path: String(r.path),
        // The seeded rows carry a deliberately-different title ("Seeded …"), so
        // "did the sync rewrite this row" is visible without minting anything.
        title: String(r.title),
        hasEmbedding: String(r.embeddingHex ?? '') !== '',
        updatedAt: String(r.updatedAt) === spec.seedSentinel ? '<sentinel>' : '<ts>',
      }));

      const jobRows =
        (await rawQuery<Array<Record<string, unknown>>>(
          'SELECT type, status, priority, maxAttempts, payload, userId FROM background_jobs',
          [],
        )) ?? [];

      const jobs = jobRows
        .map((r) => {
          const payload = typeof r.payload === 'string' ? JSON.parse(r.payload) : r.payload;
          const entityId = String(payload?.entityId ?? '');
          return {
            type: String(r.type),
            status: String(r.status),
            priority: Number(r.priority),
            maxAttempts: Number(r.maxAttempts),
            entityType: String(payload?.entityType ?? ''),
            // A created doc's id is minted on BOTH sides — path is the identity.
            entityPath: pathById.get(entityId) ?? `<unknown:${entityId}>`,
            profileId: String(payload?.profileId ?? ''),
            userId: String(r.userId),
          };
        })
        .sort((a, b) => (a.entityPath < b.entityPath ? -1 : a.entityPath > b.entityPath ? 1 : 0));

      // P4.D77 (v4 `24633026`) — the section chunks. On the early-return path
      // this is the BACKFILL's output; on the diverged path it is the sync's.
      // Chunk ids are minted on both sides, so the identity is
      // (doc PATH, chunkIndex); a SEEDED id is reported literally, which is how
      // "the backfill left the existing rows alone" is visible at all.
      const seededChunkIds = new Set((scenario.seedChunks ?? []).map((c) => c.id));
      const chunkRows =
        (await rawQuery<Array<Record<string, unknown>>>(
          'SELECT id, docId, chunkIndex, heading, content, hex(embedding) AS embeddingHex, ' +
            'updatedAt FROM help_doc_chunks',
          [],
        )) ?? [];
      const chunks = chunkRows
        .map((r) => ({
          id: seededChunkIds.has(String(r.id)) ? String(r.id) : '<minted>',
          docPath: pathById.get(String(r.docId)) ?? `<unknown:${String(r.docId)}>`,
          chunkIndex: Number(r.chunkIndex),
          heading: r.heading == null ? null : String(r.heading),
          content: String(r.content),
          hasEmbedding: String(r.embeddingHex ?? '') !== '',
          updatedAt: String(r.updatedAt) === spec.seedSentinel ? '<sentinel>' : '<ts>',
        }))
        .sort((a, b) =>
          a.docPath < b.docPath ? -1 : a.docPath > b.docPath ? 1 : a.chunkIndex - b.chunkIndex,
        );

      lines.push(
        JSON.stringify({ kind: 'ensure', scenario: scenario.name, helpDocs, jobs, chunks, logs }),
      );

      (logger as unknown as { error: unknown }).error = realError;
      (logger as unknown as { warn: unknown }).warn = realWarn;
      (logger as unknown as { info: unknown }).info = realInfo;
      await closeDatabase();
      process.chdir(V4_ROOT);
    }

    fs.writeFileSync(outPath, lines.join('\n') + '\n');
  });
});
