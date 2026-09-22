/**
 * @jest-environment node
 *
 * ORACLE for the doc-edit BLOB tool handlers (v4
 * lib/tools/handlers/doc-edit-handler.ts `executeDocEditTool` +
 * `formatDocEditResults`, routing to blob-handlers.ts), ported to
 * quilltap_core::tools::doc_edit::{execute_doc_edit_tool, format_doc_edit_results}.
 *
 * Drives v4's REAL `executeDocEditTool` (called directly, no HTTP/auth) against a
 * REAL fixture DB. The whole DB stack is doMocked to the REAL modules (past
 * jest.setup's global mocks) plus the real better-sqlite3-multiple-ciphers cipher
 * binding, so store provisioning + the blob content/link split run genuinely.
 * See [[jest-real-db-oracle]].
 *
 * TRANSCODE SEAM: `transcodeToWebP` is jest.mock'd to a PASSTHROUGH (store the raw
 * bytes unchanged, storedMimeType = the input mime, sha256 = sha256(raw)) for the
 * op sequence. The mock is the WHOLE module's, so it also silences
 * `linkBlobContent`'s normalization (`normalize-blob-image.ts` imports
 * `transcodeToWebP` from the same module); the Rust family runs the same op
 * sequence with no encoder wired, which passes the junk bytes through on both
 * sides. `normaliseBlobRelativePath` stays REAL both sides.
 *
 * P4.104 — THE IMAGE PASS (line 3). A second run over a FRESH fixture copy
 * flips `__qtRealTranscode`, so the mocked module delegates to the REAL
 * `transcodeToWebP` (real sharp) — v4's pre-transcode AND its normalization —
 * and writes the DECODABLE 240×170 `photo.png` seed through `doc_write_blob`.
 * The comparand is D19 (`../lib/blob-image-facts`): the tool output with its
 * encoder-specific `sha256`/`size_bytes` blanked, and the stored row's
 * `imageFacts` — never the WebP bytes. P4.32: `@/lib/doc-edit/reindex-file` is REAL — and this family is
 * unmoved because the blob handlers never invoke it at all (no
 * `triggerReindexIfNeeded`, no `writeDatabaseDocument`; only transcode +
 * `linkBlobContent`), NOT because the pass short-circuits on the file type;
 * the TOOL-level `triggerReindexIfNeeded` + embedding scheduler stay no-op seams.
 *
 * Ops run in a SINGLE module graph on ONE fixture copy, in order (state
 * accumulates). After every op, dumps the three store tables. Emits two NDJSON
 * lines:
 *   line 1: { case, ops: [{ name, tool, output, formatted }, ...] }
 *   line 2: { case, dumps: { blobs, fileLinks, files, chatMessages } }
 *
 * Run (Node 24, from the v4 checkout). STAGE this case OUTSIDE `.claude/` — v4's
 * jest ignores those paths in BOTH testPathIgnorePatterns and modulePathIgnorePatterns,
 * so `--roots` into a worktree matches ZERO tests, leaves the previous NDJSON in place,
 * and the Rust family then passes against a stale oracle (that is how the P4.32
 * stale-RED stayed invisible). The jest filter is ANCHORED for the same reason.
 *   N=~/.nvm/versions/node/v24.13.1/bin ; W=<this worktree>
 *   STAGE=/tmp/qt-oracle-stage-doc-blob
 *   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
 *   cp $W/harness/oracle/cases/doc-blob.test.ts $STAGE/harness/oracle/cases/
 *   cp $W/harness/oracle/fixtures/doc-blob.json $STAGE/harness/oracle/fixtures/
 *   mkdir -p $STAGE/harness/oracle/lib
 *   cp $W/harness/oracle/lib/blob-image-facts.ts $STAGE/harness/oracle/lib/
 *   cp $W/harness/oracle/fixtures/normalize-blob-image/photo.png $STAGE/harness/oracle/fixtures/
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_DBLOB_MAIN=/tmp/qt-dblob-main.db QT_FIXTURE_DBLOB_MOUNT=/tmp/qt-dblob-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-doc-blob-fixture.ts
 *   QT_FIXTURE_DBLOB_MAIN=/tmp/qt-dblob-main.db QT_FIXTURE_DBLOB_MOUNT=/tmp/qt-dblob-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-doc-blob.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=240000 \
 *       --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "doc-blob\.test\.ts$"
 *
 * The fixture pair is NOT committed — the builder MINTS it (fresh UUIDs every run) —
 * so rebuild, regenerate, then `cargo test` against that SAME build, in that order.
 */

import * as fs from 'fs';
import { createHash } from 'crypto';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { blobImageFacts, sharpMeasure, STORED_BLOB_SELECT } from '../lib/blob-image-facts';

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

// Passthrough transcode seam (Rust has no sharp). Store raw bytes unchanged.
jest.mock('@/lib/mount-index/blob-transcode', () => {
  const { createHash: ch } = require('crypto');
  const actual = jest.requireActual('@/lib/mount-index/blob-transcode');
  return {
    __esModule: true,
    ...actual, // keeps the REAL normaliseBlobRelativePath
    // P4.104: the image pass flips `__qtRealTranscode` to reach REAL sharp.
    transcodeToWebP: async (input: Buffer, mime: string) =>
      (globalThis as { __qtRealTranscode?: boolean }).__qtRealTranscode
        ? actual.transcodeToWebP(input, mime)
        : {
            data: input,
            storedMimeType: mime,
            sizeBytes: input.length,
            sha256: ch('sha256').update(input).digest('hex'),
          },
  };
});

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'doc-blob.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_DBLOB_MAIN;
  const mountFixture = process.env.QT_FIXTURE_DBLOB_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_DBLOB_MAIN and QT_FIXTURE_DBLOB_MOUNT must point at the seed fixtures from build-doc-blob-fixture.ts',
    );
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  // Reference createHash so the import is retained even if only the mock uses it.
  void createHash;

  const scratch = mkdtempSync(join(tmpdir(), 'qt-dblob-oracle-'));
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

  // W4.6c: the blob write/delete Librarian announcements are now LIVE — use the
  // REAL writer module so the chat_messages announcement rows land on both sides
  // (the Rust port posts the same rows after the sync write closure).
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
    outLines.push(JSON.stringify({ case: 'doc-blob', ops: opResults }));

    // Dump the three store tables from the mount-index DB (raw handle).
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
    // Order by remap-invariant keys (content-derived sha / stable relativePath) so
    // the positional-UUID remap assigns identical tokens on both sides.
    //
    // W4.6c: `chat_messages` lives in the MAIN db (via rawQuery), holding the
    // Librarian blob-write / delete announcement rows. Order by `content` (remap-
    // invariant — no minted uuid/timestamp in the persona body).
    const chatMessageCols = (
      (await rawQuery('PRAGMA table_info(chat_messages)')) as Array<{ name: string }>
    ).map((c) => c.name);
    const chatMessageRows = (await rawQuery('SELECT * FROM chat_messages')) as Array<
      Record<string, unknown>
    >;
    const dumps = {
      blobs: dumpTable('doc_mount_blobs', 'sha256'),
      fileLinks: dumpTable('doc_mount_file_links', 'relativePath'),
      files: dumpTable('doc_mount_files', 'sha256'),
      chatMessages: canonicalizeRows({
        table: 'chat_messages',
        columns: chatMessageCols,
        rawRows: chatMessageRows,
        orderBy: 'content',
      }),
    };
    outLines.push(JSON.stringify({ case: 'doc-blob', dumps }));
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }

  // P4.104 — the image pass: a FRESH fixture copy, REAL sharp, one decodable
  // write. The doMocks above survive `resetModules`, so the DB stack is real.
  outLines.push(JSON.stringify(await imagePass(spec, here, scratch, mainFixture, mountFixture)));

  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`doc-blob oracle wrote ${outPath} (${spec.ops.length} ops)\n`);
}

/** The P4.104 image pass's op — shared with the Rust family verbatim. */
const IMAGE_PASS_ARGS = {
  mount_point: 'self',
  path: 'art/real-photo.png',
  original_filename: 'real-photo.png',
  mime_type: 'image/png',
  description: 'a decodable picture',
};

async function imagePass(
  spec: Spec,
  here: string,
  scratch: string,
  mainFixture: string,
  mountFixture: string,
): Promise<Record<string, unknown>> {
  jest.resetModules();
  const work = mkdtempSync(join(scratch, 'image-'));
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
  const { executeDocEditTool } = await import('@/lib/tools/handlers/doc-edit-handler');
  await initializeDatabase();

  const png = fs.readFileSync(join(here, '..', 'fixtures', 'photo.png'));
  const ctx = {
    chatId: spec.chatId,
    userId: spec.userId,
    projectId: spec.projectId,
    characterId: spec.characterId,
  };
  (globalThis as { __qtRealTranscode?: boolean }).__qtRealTranscode = true;
  try {
    const result = (await executeDocEditTool(
      'doc_write_blob',
      { ...IMAGE_PASS_ARGS, data_base64: png.toString('base64') },
      ctx as never,
    )) as Record<string, unknown>;
    // The encoder's own bytes — never comparable across sharp and libwebp (D19):
    // every `sha256` / `size_bytes`, and the byte count `formattedText` quotes.
    const blank = (v: unknown): unknown => {
      if (Array.isArray(v)) return v.map(blank);
      if (v && typeof v === 'object') {
        const o: Record<string, unknown> = {};
        for (const [k, x] of Object.entries(v as Record<string, unknown>)) {
          o[k] =
            k === 'sha256' || k === 'size_bytes'
              ? '<encoder>'
              : typeof x === 'string'
                ? x.replace(/\(\d+ bytes,/g, '(<encoder> bytes,')
                : blank(x);
        }
        return o;
      }
      return v;
    };
    const output = blank(result);
    const midb = getRawMountIndexDatabase() as unknown as {
      prepare: (s: string) => { all: (...a: unknown[]) => unknown };
    };
    const rows = midb
      .prepare(STORED_BLOB_SELECT + "WHERE l.fileName LIKE 'real-photo%' ORDER BY l.relativePath")
      .all() as Parameters<typeof blobImageFacts>[0];
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const sharp = require(require('node:path').join(process.cwd(), 'node_modules/sharp'));
    const imageFacts = await blobImageFacts(rows, png, sharpMeasure(sharp));
    return { case: 'doc-blob-image', output, imageFacts };
  } finally {
    (globalThis as { __qtRealTranscode?: boolean }).__qtRealTranscode = false;
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

test('doc-blob oracle', async () => {
  await main();
});
