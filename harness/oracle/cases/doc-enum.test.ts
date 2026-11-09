/**
 * @jest-environment node
 *
 * ORACLE for the doc-edit ENUMERATION tool handlers (doc_grep + doc_list_files;
 * v4 lib/tools/handlers/doc-edit-handler.ts `executeDocEditTool` +
 * `formatDocEditResults`), ported to
 * quilltap_core::tools::doc_edit::{execute_doc_edit_tool, format_doc_edit_results}.
 *
 * Drives v4's REAL `executeDocEditTool` (the handler function called directly, no
 * HTTP/auth) against a REAL fixture DB. The whole DB stack is doMocked to the REAL
 * modules (past jest.setup's global mocks) plus the real
 * better-sqlite3-multiple-ciphers cipher binding, so store provisioning + the
 * vault read overlay run genuinely. See [[jest-real-db-oracle]].
 *
 * grep/list are READ-only handlers, so the Librarian / reindex / trigger /
 * embedding-scheduler side effects never fire; they are jest.mock'd to no-ops here
 * to stay harmless (matching the Rust port, which omits them).
 *
 * Ops run in a SINGLE module graph on ONE fixture copy, in order (no state
 * mutates — the reads are independent). Emits ONE NDJSON line:
 *   { case, ops: [{ name, tool, output, formatted }, ...] }
 * No table dump — nothing mutates.
 *
 * Run (Node 24, from the v4 checkout). STAGE this case OUTSIDE `.claude/` — v4's
 * jest ignores those paths in BOTH testPathIgnorePatterns and modulePathIgnorePatterns,
 * so `--roots` into a worktree matches ZERO tests, leaves the previous NDJSON in place,
 * and the Rust family then passes against a stale oracle (that is how the P4.32
 * stale-RED stayed invisible). The jest filter is ANCHORED for the same reason.
 *   N=~/.nvm/versions/node/v24.13.1/bin ; W=<this worktree>
 *   STAGE=/tmp/qt-oracle-stage-doc-enum
 *   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
 *   cp $W/harness/oracle/cases/doc-enum.test.ts $STAGE/harness/oracle/cases/
 *   cp $W/harness/oracle/fixtures/doc-enum.json $STAGE/harness/oracle/fixtures/
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_DEN_MAIN=/tmp/qt-den-main.db QT_FIXTURE_DEN_MOUNT=/tmp/qt-den-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-doc-enum-fixture.ts
 *   QT_FIXTURE_DEN_MAIN=/tmp/qt-den-main.db QT_FIXTURE_DEN_MOUNT=/tmp/qt-den-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-doc-enum.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=240000 \
 *       --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "doc-enum\.test\.ts$"
 *
 * The fixture pair is NOT committed — the builder MINTS it (fresh UUIDs every run) —
 * so rebuild, regenerate, then `cargo test` against that SAME build, in that order.
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Op {
  name: string;
  tool: string;
  args: Record<string, unknown>;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  characterId: string;
  projectId: string;
  chatId: string;
  ops: Op[];
}


// === P4.D380 === (v4 `a3d53eae1`, round ruling R-2 (A); C1's second re-aim):
// the document core's `afterDocumentWrite` chain runs REAL and AWAITED. The
// `@/lib/doc-edit` barrel's `reindexSingleFile` (the core's one import of it) is
// a recorder that calls the real function and keeps its promise; before each op
// is recorded the chain is drained (every pending promise settled, then a run of
// macrotask turns for the `.then` hops, the dynamic `link-groups` import and the
// `Promise.all`), and the op carries `afterWrite` — the chokepoint's DEBUG lines
// (`afterDocumentWrite: scheduling store refresh` `{mountPointId,
// relativePath}`) it logged. The embedding scheduler stays seamed (no job host
// in the oracle).
const P4D380_PENDING: Promise<unknown>[] = [];
const P4D380_LINES: Array<{ message: string; fields: Record<string, unknown> }> = [];
const P4D380_AFTER_WRITE = 'afterDocumentWrite: scheduling store refresh';
async function p4d380Drain(): Promise<void> {
  for (let round = 0; round < 3; round++) {
    await Promise.allSettled(P4D380_PENDING.splice(0));
    for (let i = 0; i < 25; i++) await new Promise((r) => setImmediate(r));
  }
}
async function p4d380AfterWrite(): Promise<Array<{ mountPointId: unknown; relativePath: unknown }>> {
  await p4d380Drain();
  const out = P4D380_LINES.filter((l) => l.message === P4D380_AFTER_WRITE).map((l) => ({
    mountPointId: l.fields.mountPointId,
    relativePath: l.fields.relativePath,
  }));
  P4D380_LINES.length = 0;
  return out;
}
/** Record every line the singleton logger writes (children share its transports). */
async function p4d380Splice(): Promise<void> {
  const { logger } = await import('@/lib/logger');
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { message: string; context?: Record<string, unknown> };
      P4D380_LINES.push({ message: data.message, fields: data.context ?? {} });
    },
  });
  P4D380_LINES.length = 0;
}
// === end P4.D380 ===

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'doc-enum.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_DEN_MAIN;
  const mountFixture = process.env.QT_FIXTURE_DEN_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_DEN_MAIN and QT_FIXTURE_DEN_MOUNT must point at the seed fixtures from build-doc-enum-fixture.ts',
    );
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-den-oracle-'));
  try {
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  // P4.D380: DEBUG passes the level gate (the chokepoint's line is a DEBUG).
  process.env.LOG_LEVEL = 'debug';

  // Restore the REAL DB stack (past jest.setup's global mocks) + the real cipher
  // driver. doMock is runtime (not hoisted), so it runs now and wins over
  // jest.setup.
  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories'),
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
  );
  jest.doMock('@/lib/embedding/embedding-service', () =>
    jest.requireActual('@/lib/embedding/embedding-service'),
  );

  // Seamed side effects → no-ops (documented Rust seams; matches the port). grep/
  // list never fire these, but keep the mocks harmless anyway.
  jest.doMock('@/lib/services/librarian-notifications/writer', () => {
    const actual = jest.requireActual('@/lib/services/librarian-notifications/writer');
    return {
      __esModule: true,
      ...actual,
      postLibrarianWriteAnnouncement: async () => undefined,
      postLibrarianDeleteAnnouncement: async () => undefined,
      postLibrarianMoveAnnouncement: async () => undefined,
      postLibrarianFolderAnnouncement: async () => undefined,
      contentHiddenFromCharacters: () => false,
      documentHiddenFromCharacters: () => false,
    };
  });
  // P4.32 — the reindex module runs for REAL (RULED 2026-08-04). Its one export
  // `reindexSingleFile` IS v4's database-store chunk pass: `writeDatabaseDocument`
  // AWAITS it (`lib/mount-index/database-store.ts:148`) to chunk the bytes it just
  // wrote — the P4.6BK chunk-on-write v5 also performs. Mocking it to a no-op
  // therefore silenced v4's OWN chunking, and `chunkCount` diverged 1-vs-0 on every
  // write that lands in a database store. Kept as an explicit requireActual (rather
  // than a plain deletion) so a future global mock in `jest.setup` cannot re-silence
  // it invisibly — which is exactly how this red hid. The embedding-enqueue seam is
  // a DIFFERENT module and stays mocked below (`reindexSingleFile` never enqueues:
  // "Embedding jobs are NOT enqueued here").
  jest.doMock('@/lib/doc-edit/reindex-file', () =>
    jest.requireActual('@/lib/doc-edit/reindex-file'),
  );

  // …but the tool-level document chain stays seamed. v4 `a3d53eae1` moved the
  // doc-tool reindex into the document core: every write goes through
  // `writeDocument` -> `afterDocumentWrite` (`lib/documents/core/write.ts`),
  // which fires `reindexSingleFile(...).then(reindexLinkGroupSiblings)
  // .then(enqueue+refreshStats)` UN-AWAITED (the old `triggerReindexIfNeeded`
  // export of `doc-edit/shared` is gone, so the previous seam was inert and
  // 15 real calls reached the chain in the P4.D361 probe). The core imports
  // `reindexSingleFile` from the `@/lib/doc-edit` BARREL, so a never-settling
  // `reindexSingleFile` there stops the whole chain (no link-group siblings,
  // no embedding enqueue, no `refreshStats`) while `database-store`'s AWAITED
  // chunk pass (a direct `doc-edit/reindex-file` import) stays real. Its
  // writes would otherwise race the dump and make the FILESYSTEM branch mint
  // mount-index rows v5 never writes. (P4.D380 replaces this with awaiting the
  // chain, R-2 (A).)
  jest.doMock('@/lib/doc-edit', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/doc-edit'),
    // === P4.D380 === the real chain, recorded (see `p4d380Drain`).
    reindexSingleFile: (...a: unknown[]) => {
      const real = (jest.requireActual('@/lib/doc-edit') as {
        reindexSingleFile: (...x: unknown[]) => Promise<unknown>;
      }).reindexSingleFile;
      const p = real(...a);
      P4D380_PENDING.push(p);
      return p;
    },
    // === end P4.D380 ===
  }));
  jest.doMock('@/lib/mount-index/embedding-scheduler', () => ({
    __esModule: true,
    enqueueEmbeddingJobsForMountPoint: () => undefined,
  }));

  // Fresh copy of BOTH fixture DBs.
  const work = mkdtempSync(join(scratch, 'run-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { executeDocEditTool, formatDocEditResults } = await import(
    '@/lib/tools/handlers/doc-edit-handler'
  );

  await initializeDatabase();
  await p4d380Splice(); // P4.D380

  const ctx = {
    chatId: spec.chatId,
    userId: spec.userId,
    projectId: spec.projectId,
    characterId: spec.characterId,
  };

  const outLines: string[] = [];
  try {
    const opResults: Array<Record<string, unknown>> = [];
    for (const op of spec.ops) {
      const result = await executeDocEditTool(op.tool, op.args, ctx as never);
      const formatted = formatDocEditResults(op.tool, result);
      opResults.push({ name: op.name, tool: op.tool, output: result, formatted, afterWrite: await p4d380AfterWrite() });
    }
    outLines.push(JSON.stringify({ case: 'doc-enum', ops: opResults }));
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }

  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`doc-enum oracle wrote ${outPath} (${spec.ops.length} ops)\n`);
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

test('doc-enum oracle', async () => {
  await main();
});
