/**
 * @jest-environment node
 *
 * ORACLE for the doc-edit FILE-MANAGEMENT tool handlers (v4
 * lib/tools/handlers/doc-edit-handler.ts `executeDocEditTool` +
 * `formatDocEditResults`, dispatching to file-management-handlers.ts:
 * doc_move_file / doc_copy_file / doc_delete_file / doc_create_folder /
 * doc_delete_folder / doc_move_folder), ported to
 * quilltap_core::tools::doc_edit::file_management.
 *
 * Drives v4's REAL `executeDocEditTool` against a REAL fixture DB. The whole DB
 * stack is doMocked to the REAL modules (past jest.setup's global mocks) plus the
 * real better-sqlite3-multiple-ciphers cipher binding, so store provisioning + the
 * vault read/write overlay run genuinely. See [[jest-real-db-oracle]].
 *
 * The handlers' fire-and-forget side effects (Librarian announcements, reindex,
 * embedding-scheduler, chat_documents sync) are documented Rust seams — jest.mock'd
 * to no-ops so they don't perturb the DB (matching the Rust port, which omits them).
 *
 * Ops run in a SINGLE module graph on ONE fixture copy, in order (state
 * accumulates). After all ops, dumps three content tables. Emits two NDJSON lines:
 *   line 1: { case, ops: [{ name, tool, output, formatted }, ...] }
 *   line 2: { case, dumps: { fileLinks, folders, documents } }
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_DFM_MAIN=/tmp/qt-dfm-main.db QT_FIXTURE_DFM_MOUNT=/tmp/qt-dfm-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-doc-fm-fixture.ts
 *   TMPO=/tmp/qt-oracle-dfm; rm -rf $TMPO; mkdir -p $TMPO/cases $TMPO/fixtures
 *   cp $V5/harness/oracle/cases/doc-fm.test.ts $TMPO/cases/
 *   cp $V5/harness/oracle/fixtures/doc-fm.json $TMPO/fixtures/
 *   QT_FIXTURE_DFM_MAIN=/tmp/qt-dfm-main.db QT_FIXTURE_DFM_MOUNT=/tmp/qt-dfm-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-doc-fm.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/cases" -- doc-fm
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

// Inlined canonicalizer (jest can't resolve the `.js` ESM specifier of
// ../lib/tier2.ts). BLOBs -> lowercase hex, nulls explicit, everything else
// as-is, rows sorted by `orderBy` (code-unit string order on the canonicalized
// cell so it matches the Rust dump_table_json_conn order).
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
  orderBy: string;
}): { table: string; columns: string[]; rows: Array<Record<string, unknown>> } {
  const { table, columns, rawRows, orderBy } = opts;
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

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'doc-fm.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_DFM_MAIN;
  const mountFixture = process.env.QT_FIXTURE_DFM_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_DFM_MAIN and QT_FIXTURE_DFM_MOUNT must point at the seed fixtures from build-doc-fm-fixture.ts',
    );
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-dfm-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

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

  // Seamed side effects → no-ops (documented Rust seams; matches the port). The
  // file-management handlers post Move/Copy/Delete/FolderCreated/FolderDeleted
  // announcements and consult documentHiddenFromCharacters — all no-op'd.
  jest.doMock('@/lib/services/librarian-notifications/writer', () => {
    const actual = jest.requireActual('@/lib/services/librarian-notifications/writer');
    return {
      __esModule: true,
      ...actual,
      postLibrarianWriteAnnouncement: async () => undefined,
      postLibrarianDeleteAnnouncement: async () => undefined,
      postLibrarianMoveAnnouncement: async () => undefined,
      postLibrarianCopyAnnouncement: async () => undefined,
      postLibrarianFolderCreatedAnnouncement: async () => undefined,
      postLibrarianFolderDeletedAnnouncement: async () => undefined,
      postLibrarianFolderAnnouncement: async () => undefined,
      contentHiddenFromCharacters: () => false,
      documentHiddenFromCharacters: () => false,
    };
  });
  jest.doMock('@/lib/doc-edit/reindex-file', () => ({
    __esModule: true,
    reindexSingleFile: async () => undefined,
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
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { executeDocEditTool, formatDocEditResults } = await import(
    '@/lib/tools/handlers/doc-edit-handler'
  );

  await initializeDatabase();

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
      opResults.push({ name: op.name, tool: op.tool, output: result, formatted });
    }
    outLines.push(JSON.stringify({ case: 'doc-fm', ops: opResults }));

    // Dump the three content tables from the mount-index DB (raw handle).
    const midb = getRawMountIndexDatabase();
    if (!midb) throw new Error('mount-index DB handle unavailable for dump');
    const dumpTable = (table: string, orderBy: string) => {
      const columns = (
        midb.prepare(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>
      ).map((c) => c.name);
      const rawRows = midb.prepare(`SELECT * FROM ${table}`).all() as Array<
        Record<string, unknown>
      >;
      return canonicalizeRows({ table, columns, rawRows, orderBy });
    };
    // Order by remap-invariant keys. file_links by relativePath, folders by path,
    // documents by contentSha256 (the content-derived key — NOT the random fileId).
    const dumps = {
      fileLinks: dumpTable('doc_mount_file_links', 'relativePath'),
      folders: dumpTable('doc_mount_folders', 'path'),
      documents: dumpTable('doc_mount_documents', 'contentSha256'),
    };
    outLines.push(JSON.stringify({ case: 'doc-fm', dumps }));
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }

  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`doc-fm oracle wrote ${outPath} (${spec.ops.length} ops)\n`);
}

test('doc-fm oracle', async () => {
  await main();
});
