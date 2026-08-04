/**
 * @jest-environment node
 *
 * ORACLE for the doc-edit DOCUMENT-UI tool handlers (v4
 * lib/tools/handlers/doc-edit/document-ui-handlers.ts: doc_open_document,
 * doc_close_document, doc_focus), ported to
 * quilltap_core::tools::doc_edit::document_ui.
 *
 * Drives v4's REAL `executeDocEditTool` (called directly, no HTTP/auth) against a
 * REAL fixture DB. The whole DB stack is doMocked to the REAL modules (past
 * jest.setup's global mocks) plus the real better-sqlite3-multiple-ciphers cipher
 * binding, so store provisioning + the vault read overlay run genuinely. See
 * [[jest-real-db-oracle]].
 *
 * W4.6c: the Librarian open announcement + documentHiddenFromCharacters gate are now
 * LIVE (the REAL writer module) — the chat_messages open-announcement rows land on
 * both sides and are diffed (the open is an actual message, so it also bumps the
 * chat's updatedAt). P4.32: `@/lib/doc-edit/reindex-file` is REAL; the seams left
 * are the TOOL-level `triggerReindexIfNeeded` + the embedding-scheduler, jest.mock'd
 * to no-ops so they don't perturb the DB (matching the Rust port).
 *
 * Ops run in a SINGLE module graph on ONE fixture copy, in order (state
 * accumulates). After every op, dumps `chat_documents` + a `chats` subset (both
 * MAIN db). Emits two NDJSON lines:
 *   line 1: { case, ops: [{ name, tool, output, formatted }, ...] }
 *   line 2: { case, dumps: { chatDocuments, chats, chatMessages } }
 *
 * Run (Node 24, from the v4 checkout). STAGE this case OUTSIDE `.claude/` — v4's
 * jest ignores those paths in BOTH testPathIgnorePatterns and modulePathIgnorePatterns,
 * so `--roots` into a worktree matches ZERO tests, leaves the previous NDJSON in place,
 * and the Rust family then passes against a stale oracle (that is how the P4.32
 * stale-RED stayed invisible). The jest filter is ANCHORED for the same reason.
 *   N=~/.nvm/versions/node/v24.13.1/bin ; W=<this worktree>
 *   STAGE=/tmp/qt-oracle-stage-doc-ui
 *   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
 *   cp $W/harness/oracle/cases/doc-ui.test.ts $STAGE/harness/oracle/cases/
 *   cp $W/harness/oracle/fixtures/doc-ui.json $STAGE/harness/oracle/fixtures/
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_DUI_MAIN=/tmp/qt-dui-main.db QT_FIXTURE_DUI_MOUNT=/tmp/qt-dui-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-doc-ui-fixture.ts
 *   QT_FIXTURE_DUI_MAIN=/tmp/qt-dui-main.db QT_FIXTURE_DUI_MOUNT=/tmp/qt-dui-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-doc-ui.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=240000 \
 *       --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "doc-ui\.test\.ts$"
 *
 * The fixture pair is NOT committed — the builder MINTS it (fresh UUIDs every run) —
 * so rebuild, regenerate, then `cargo test` against that SAME build, in that order.
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

// BLOBs -> lowercase hex, nulls explicit, everything else as-is; rows sorted by
// `orderBy` (code-unit string order) so it matches the Rust dump order.
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
    fs.readFileSync(join(here, '..', 'fixtures', 'doc-ui.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_DUI_MAIN;
  const mountFixture = process.env.QT_FIXTURE_DUI_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_DUI_MAIN and QT_FIXTURE_DUI_MOUNT must point at the seed fixtures from build-doc-ui-fixture.ts',
    );
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-dui-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // Restore the REAL DB stack (past jest.setup's global mocks) + the real cipher
  // driver. doMock is runtime (not hoisted), so it wins over jest.setup.
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

  // W4.6c: the open-document Librarian announcement + documentHiddenFromCharacters
  // suppression gate are now LIVE — use the REAL writer module so the
  // chat_messages open-announcement rows land on both sides (the Rust port posts
  // the same rows after the sync write closure). The open announcement is an actual
  // `type:'message'` event, so it also bumps the chat's `updatedAt` (both sides).
  // Reindex/embedding stay seamed below.
  jest.doMock('@/lib/services/librarian-notifications/writer', () =>
    jest.requireActual('@/lib/services/librarian-notifications/writer'),
  );
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

  // …but `triggerReindexIfNeeded` — the TOOL-level fire-and-forget trigger — stays
  // seamed. It is a SEPARATE, still-standing v5 deferral (`tools/doc_edit/shared.rs`
  // header: "the port omits them"), and unlike `writeDatabaseDocument`'s awaited
  // chunk pass it is NOT awaited by anything: `triggerReindexIfNeeded` kicks off
  // `reindexSingleFile(...).then(reindexLinkGroupSiblings).then(enqueue+refreshStats)`
  // and returns, so its writes race the dump. Leaving it live would (a) make the
  // FILESYSTEM branch mint mount-index link + chunk rows v5 never writes — the
  // deferral's measured blast radius, see the P4.32 lane record — and (b) make every
  // family timing-dependent. Seaming just this one export keeps the mock exactly as
  // wide as the deferral, instead of also silencing v4's own chunk-on-write.
  jest.doMock('@/lib/tools/handlers/doc-edit/shared', () => {
    const actual = jest.requireActual('@/lib/tools/handlers/doc-edit/shared');
    return {
      __esModule: true,
      ...actual,
      triggerReindexIfNeeded: async () => undefined,
    };
  });
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

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
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
    outLines.push(JSON.stringify({ case: 'doc-ui', ops: opResults }));

    // Dump chat_documents (all cols) + a chats subset from the MAIN db via
    // rawQuery. Order chat_documents by the stable (filePath) key so the
    // positional-UUID remap assigns identical tokens on both sides (the minted
    // `id` is random and must not be the sort key).
    const cdColumns = (
      (await rawQuery('PRAGMA table_info(chat_documents)')) as Array<{ name: string }>
    ).map((c) => c.name);
    const cdRows = (await rawQuery('SELECT * FROM chat_documents')) as Array<
      Record<string, unknown>
    >;
    const chatRows = (await rawQuery(
      'SELECT id, documentMode, updatedAt FROM chats',
    )) as Array<Record<string, unknown>>;
    // W4.6c: the Librarian open-announcement rows live in the MAIN db. Order by
    // `content` (remap-invariant — no minted uuid/timestamp in the persona body).
    const cmColumns = (
      (await rawQuery('PRAGMA table_info(chat_messages)')) as Array<{ name: string }>
    ).map((c) => c.name);
    const cmRows = (await rawQuery('SELECT * FROM chat_messages')) as Array<
      Record<string, unknown>
    >;

    const dumps = {
      chatDocuments: canonicalizeRows({
        table: 'chat_documents',
        columns: cdColumns,
        rawRows: cdRows,
        orderBy: 'filePath',
      }),
      chats: canonicalizeRows({
        table: 'chats',
        columns: ['id', 'documentMode', 'updatedAt'],
        rawRows: chatRows,
        orderBy: 'id',
      }),
      chatMessages: canonicalizeRows({
        table: 'chat_messages',
        columns: cmColumns,
        rawRows: cmRows,
        orderBy: 'content',
      }),
    };
    outLines.push(JSON.stringify({ case: 'doc-ui', dumps }));
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }

  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`doc-ui oracle wrote ${outPath} (${spec.ops.length} ops)\n`);
}

test('doc-ui oracle', async () => {
  await main();
});
