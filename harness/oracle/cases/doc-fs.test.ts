/**
 * @jest-environment node
 *
 * ORACLE for the doc-edit HOST-FILESYSTEM tool branches (P4.6bg unit 4). Drives
 * v4's REAL `executeDocEditTool` over the `general` scope, filesystem-backed
 * mounts, and the legacy `<filesDir>/<projectId>` project fallback, against a copy
 * of the doc-fs fixture DBs + a temp fs tree BOTH differential sides materialize
 * identically under a CANONICAL scratch root (macOS /var → /private/var), so
 * `safeRealpath` + `verifyPathIsWithinBase` see the same structure. The fs-mount
 * store's sentinel basePath is rewritten to `<scratch>/mount` per side.
 *
 * The whole DB stack is doMocked to the REAL modules (past jest.setup's global
 * mocks) + the real cipher binding. The Librarian announcement / reindex /
 * embedding side effects are documented no-op seams — mocked here and NOT posted on
 * the Rust side — so `chat_messages` stays untouched and is not dumped. Each op
 * carries its own `ctx` (some fs branches run standalone with no project/character,
 * some address the project-linked fs store). After the ops it dumps the resulting
 * fs tree (files/ + mount/ + outside/, byte-for-byte) and the empty
 * doc_mount_documents / doc_mount_file_links (proving the fs ops never wrote DB
 * rows). Emits two NDJSON lines:
 *   line 1: { case, ops: [{ name, tool, output, formatted }, ...] }
 *   line 2: { case, dumps: { tree, documents, fileLinks } }
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_DFS_MAIN=/tmp/qt-dfs-main.db QT_FIXTURE_DFS_MOUNT=/tmp/qt-dfs-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-doc-fs-fixture.ts
 *   QT_FIXTURE_DFS_MAIN=/tmp/qt-dfs-main.db QT_FIXTURE_DFS_MOUNT=/tmp/qt-dfs-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-doc-fs.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- doc-fs
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join, relative } from 'node:path';
import {
  mkdtempSync,
  mkdirSync,
  copyFileSync,
  existsSync,
  rmSync,
  realpathSync,
  writeFileSync,
  symlinkSync,
  readdirSync,
  readFileSync,
} from 'node:fs';
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
  ctx: { characterId?: string; projectId?: string; operatorOverride?: boolean };
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  charAId: string;
  chatId: string;
  fsStore: string;
  legacyProjectId: string;
  ops: Op[];
}

interface TreeEntry {
  path: string;
  kind: 'file' | 'dir' | 'symlink';
  content?: string;
}

/**
 * Materialize the identical host-filesystem tree both sides build under the
 * canonical scratch root. Returns `<root>/mount` (the fs-mount base):
 *   <root>/files/_general/existing.md          (a general read/list seed)
 *   <root>/files/<legacyProjectId>/draft.md    (legacy-fallback seed)
 *   <root>/mount/docs/note.md                  (fs-mount read target)
 *   <root>/mount/escape -> <root>/outside      (fs-mount symlink escape)
 *   <root>/outside/secret.md                   (escape destination)
 */
function materializeTree(root: string, legacyProjectId: string): string {
  const general = join(root, 'files', '_general');
  const legacy = join(root, 'files', legacyProjectId);
  const mount = join(root, 'mount');
  const outside = join(root, 'outside');
  mkdirSync(general, { recursive: true });
  mkdirSync(legacy, { recursive: true });
  mkdirSync(join(mount, 'docs'), { recursive: true });
  mkdirSync(outside, { recursive: true });
  writeFileSync(join(general, 'existing.md'), '# existing general\n');
  writeFileSync(join(legacy, 'draft.md'), '# draft\n\ndraft body\n');
  writeFileSync(join(mount, 'docs', 'note.md'), '# note\n\nnote body\n');
  writeFileSync(join(outside, 'secret.md'), 'secret\n');
  symlinkSync(outside, join(mount, 'escape'));
  return mount;
}

const UUID_RE = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/g;
/** Collapse every UUID to a constant so the sort order is remap-invariant (the
 * new-blank filenames are random per side; the legacy dir uuid is pinned). */
function sortKey(path: string): string {
  return path.replace(UUID_RE, '<uuid>');
}

/** Recursively dump a directory tree into {path, kind, content?} entries, sorted by
 * a UUID-collapsed key. */
function dumpTree(root: string, label: string): TreeEntry[] {
  const entries: TreeEntry[] = [];
  const walk = (dir: string): void => {
    let dirents: fs.Dirent[];
    try {
      dirents = readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const d of dirents) {
      const full = join(dir, d.name);
      const rel = `${label}/${relative(root, full)}`;
      if (d.isSymbolicLink()) {
        entries.push({ path: rel, kind: 'symlink' });
      } else if (d.isDirectory()) {
        entries.push({ path: rel, kind: 'dir' });
        walk(full);
      } else if (d.isFile()) {
        entries.push({ path: rel, kind: 'file', content: readFileSync(full, 'utf8') });
      }
    }
  };
  if (existsSync(root)) walk(root);
  entries.sort((a, b) => {
    const ak = sortKey(a.path);
    const bk = sortKey(b.path);
    return ak < bk ? -1 : ak > bk ? 1 : 0;
  });
  return entries;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'doc-fs.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_DFS_MAIN;
  const mountFixture = process.env.QT_FIXTURE_DFS_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_DFS_MAIN and QT_FIXTURE_DFS_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  // CANONICAL scratch root so on-disk realpaths share a stable prefix with the
  // fs-mount basePath.
  const scratch = realpathSync(mkdtempSync(join(tmpdir(), 'qt-dfs-oracle-')));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const fsMountBase = materializeTree(scratch, spec.legacyProjectId);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

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

  // The Librarian announcements + the tool-level reindex trigger + embedding are
  // documented seams: mock
  // every poster to a no-op so chat_messages stays untouched (matching the Rust
  // side, which does not post the pending announcement in this differential).
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
      postLibrarianOpenAnnouncement: async () => undefined,
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

  const work = mkdtempSync(join(scratch, 'run-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { executeDocEditTool, formatDocEditResults } = await import(
    '@/lib/tools/handlers/doc-edit-handler'
  );

  await initializeDatabase();

  // Rewrite the fs mount's sentinel basePath to this side's tree.
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB unavailable');
  midb.prepare("UPDATE doc_mount_points SET basePath = ? WHERE mountType = 'filesystem'").run(fsMountBase);

  const sentinelize = (value: unknown): unknown =>
    JSON.parse(JSON.stringify(value).split(scratch).join('__ROOT__'));

  const outLines: string[] = [];
  try {
    const opResults: Array<Record<string, unknown>> = [];
    for (const op of spec.ops) {
      const ctx = {
        chatId: spec.chatId,
        userId: spec.userId,
        projectId: op.ctx.projectId,
        characterId: op.ctx.characterId,
        operatorOverride: op.ctx.operatorOverride,
      };
      const result = await executeDocEditTool(op.tool, op.args, ctx as never);
      const formatted = formatDocEditResults(op.tool, result);
      opResults.push(
        sentinelize({ name: op.name, tool: op.tool, output: result, formatted }) as Record<
          string,
          unknown
        >,
      );
    }
    outLines.push(JSON.stringify({ case: 'doc-fs', ops: opResults }));

    const tree = [
      ...dumpTree(join(scratch, 'files'), 'files'),
      ...dumpTree(join(scratch, 'mount'), 'mount'),
      ...dumpTree(join(scratch, 'outside'), 'outside'),
    ];

    const dumpTable = (table: string, orderBy: string) => {
      const columns = (
        midb.prepare(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>
      ).map((c) => c.name);
      const rawRows = midb.prepare(`SELECT * FROM ${table}`).all() as Array<
        Record<string, unknown>
      >;
      return canonicalizeRows({ table, columns, rawRows, orderBy });
    };
    const dumps = {
      tree,
      documents: dumpTable('doc_mount_documents', 'contentSha256'),
      fileLinks: dumpTable('doc_mount_file_links', 'relativePath'),
    };
    outLines.push(JSON.stringify({ case: 'doc-fs', dumps }));
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }

  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`doc-fs oracle wrote ${outPath} (${spec.ops.length} ops)\n`);
}

test('doc-fs oracle', async () => {
  await main();
});
