/**
 * @jest-environment node
 *
 * P4.6y MOUNT-INDEX route-surface ORACLE (the indexing family: scan / reindex /
 * embed / semantic-search): drives v4's REAL mount-point action-dispatch
 * handlers (`app/api/v1/mount-points/[id]/route.ts` POST) + the collection
 * route's `?action=semantic-search` over a FRESH copy of the committed
 * mounts-{main,mount}.db fixture per case, emitting `{status, body, tables}` —
 * the RAW eight-table dump (mount: points/files/links/documents/folders/
 * chunks/blobs; main: background_jobs). Normalization (timestamps, minted-UUID
 * remap, fs mtimes, extraction-error text) happens ONCE, in the Rust test, and
 * is applied to BOTH sides' raw dumps.
 *
 * The fs/obsidian mounts carry the sentinel basePath `__MOUNTS_FS_TREE__`;
 * each case rewrites it to a per-case temp COPY of the committed
 * `mounts-fs-tree/` (nothing writes the committed tree).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-mount-index-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/mount-index.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/mounts.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MOUNTS_MAIN=$V5W/crates/quilltap-web/tests/fixtures/mounts-main.db \
 *   QT_FIXTURE_MOUNTS_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/mounts-mount.db \
 *   QT_MOUNTS_FS_TREE=$V5W/crates/quilltap-web/tests/fixtures/mounts-fs-tree \
 *   QT_ORACLE_OUT=/tmp/oracle-mount-index.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- mount-index
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, cpSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

const MP_DB = 'b6000000-0000-4000-8000-000000000001';
const MP_FS = 'b6000000-0000-4000-8000-000000000003';
const MP_OBS = 'b6000000-0000-4000-8000-000000000004';
const BOGUS = 'b6000000-0000-4000-8000-0000000000ee';

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
    text: jest.fn().mockResolvedValue(body === undefined ? '' : JSON.stringify(body)),
  };
}

function applyMocks(spec: Spec): void {
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
  // jest.setup cans generateEmbeddingForUser to a 3-dim test vector — un-mock
  // so semantic-search embeds queries through the REAL builtin TF-IDF path.
  jest.doMock('@/lib/embedding/embedding-service', () =>
    jest.requireActual('@/lib/embedding/embedding-service'),
  );
  // Keep the in-process job dispatcher OFF: enqueued EMBEDDING_GENERATE rows
  // must stay PENDING in the dump (the W4.7e2 mock-ensureProcessorRunning
  // recipe) — otherwise the drain lets v4 run the jobs and embed the chunks.
  jest.doMock('@/lib/background-jobs/host/processor-host', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/background-jobs/host/processor-host'),
    ensureProcessorRunning: () => {},
  }));
  // The watcher pulls chokidar (ESM); stub the seams (P4.6p recipe).
  jest.doMock('@/lib/mount-index/watcher', () => ({
    __esModule: true,
    attachMountPoint: async () => {},
    detachMountPoint: async () => {},
    refreshMountPoint: async () => {},
  }));
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: spec.userId } }),
  }));
  jest.doMock('@/lib/startup/startup-state', () => {
    const actual = jest.requireActual('@/lib/startup/startup-state');
    return {
      __esModule: true,
      ...actual,
      startupState: {
        ...actual.startupState,
        isReady: () => true,
        waitForReady: async () => true,
        isPepperResolved: () => true,
        getPepperState: () => 'resolved',
        getPhase: () => 'ready',
        isLockedMode: () => false,
      },
    };
  });
}

// ── raw table dumps (identical column lists in the Rust side) ───────────────

const MOUNT_TABLES: Record<string, string> = {
  points:
    'SELECT id, name, basePath, mountType, storeType, includePatterns, excludePatterns, enabled, ' +
    'lastScannedAt, scanStatus, lastScanError, conversionStatus, conversionError, fileCount, ' +
    'chunkCount, totalSizeBytes, createdAt, updatedAt FROM doc_mount_points ORDER BY id',
  files:
    'SELECT id, sha256, fileSizeBytes, fileType, source, createdAt, updatedAt ' +
    'FROM doc_mount_files ORDER BY sha256, source',
  links:
    'SELECT id, fileId, mountPointId, relativePath, fileName, folderId, originalFileName, ' +
    'originalMimeType, description, descriptionUpdatedAt, conversionStatus, conversionError, ' +
    'plainTextLength, extractedText, extractedTextSha256, extractionStatus, extractionError, ' +
    'chunkCount, allowEmbed, allowCharacterRead, allowCharacterWrite, lastModified, createdAt, ' +
    'updatedAt, linkGroupId FROM doc_mount_file_links ORDER BY mountPointId, relativePath',
  documents:
    'SELECT id, fileId, content, contentSha256, plainTextLength, createdAt, updatedAt ' +
    'FROM doc_mount_documents ORDER BY contentSha256',
  folders:
    'SELECT id, mountPointId, parentId, name, path, createdAt, updatedAt ' +
    'FROM doc_mount_folders ORDER BY mountPointId, path',
  chunks:
    'SELECT c.id, c.linkId, c.mountPointId, c.chunkIndex, c.content, c.tokenCount, ' +
    'c.headingContext, (CASE WHEN c.embedding IS NULL THEN NULL ELSE length(c.embedding) END) ' +
    'AS embeddingLength, c.createdAt, c.updatedAt, l.relativePath AS sortPath ' +
    'FROM doc_mount_chunks c LEFT JOIN doc_mount_file_links l ON l.id = c.linkId ' +
    'ORDER BY c.mountPointId, sortPath, c.chunkIndex',
  blobs:
    'SELECT id, fileId, sha256, sizeBytes, storedMimeType, length(data) AS dataLength, ' +
    'createdAt, updatedAt FROM doc_mount_blobs ORDER BY sha256',
};

const JOBS_SQL =
  'SELECT id, userId, type, status, payload, priority, attempts, maxAttempts, lastError, ' +
  'scheduledAt, startedAt, completedAt, createdAt, updatedAt FROM background_jobs ORDER BY payload';

async function dumpTables(): Promise<Record<string, unknown>> {
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const midb = getRawMountIndexDatabase() as never as {
    prepare: (s: string) => { all: () => unknown[] };
  };
  const main = getRawDatabase() as never as { prepare: (s: string) => { all: () => unknown[] } };
  const out: Record<string, unknown> = {};
  for (const [name, sql] of Object.entries(MOUNT_TABLES)) {
    out[name] = midb.prepare(sql).all();
  }
  out.jobs = main.prepare(JOBS_SQL).all();
  return out;
}

interface CaseSpec {
  name: string;
  run: () => Promise<{ status: number; body: unknown; tables?: unknown }>;
  // === P4.D381 ===
  /** In-case plant on this case's fs-tree copy (before any module loads). */
  plant?: (treeWork: string) => void;
  /** Undo the plant before the copy is removed (modes restored). */
  unplant?: (treeWork: string) => void;
  // === /P4.D381 ===
}

// === P4.D381: the scan's log lines (v4 bf7a457f6) ===
/** The lines the scan family compares, by message: the scanner's (service
 *  `MountIndex:Scanner`), the route's `[Mount Points v1] Scan completed`, and
 *  the embedding enqueue's two INFOs (for the ORDER: scanner → route →
 *  enqueue). Captured by splicing `logger.transports` at `info`. */
const SCAN_LINES = new Set([
  'Starting mount point scan',
  'Mount point scan completed',
  'Left cloud-evicted files unread; they are indexed once downloaded',
  'Starting database-backed mount point rescan',
  'Database-backed rescan failed',
  'Error processing file during mount scan',
  'Mount point scan failed',
  'Unable to read directory during walk',
  '[Mount Points v1] Scan completed',
  'Enqueuing embedding jobs for mount chunks',
  'Embedding job enqueueing complete',
]);

interface CapturedLine {
  level: string;
  message: string;
  fields: Record<string, unknown>;
}

async function installScanLogCapture(): Promise<CapturedLine[]> {
  const { logger } = await import('@/lib/logger');
  const lines: CapturedLine[] = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as {
        level: string;
        message: string;
        context?: Record<string, unknown>;
        error?: { message: string };
      };
      if (!SCAN_LINES.has(data.message)) return;
      const { service: _s, environment: _e, module: _m, ...fields } = (data.context ?? {}) as Record<
        string,
        unknown
      >;
      if (data.error) fields.error = data.error.message;
      lines.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
    },
  });
  return lines;
}

/** `scan_fs_in_cloud`: the ONE path v4's `fs.stat` reports `blocks: 0` for
 *  (an evicted placeholder), wrapped over the real `fs/promises`. */
function mockEvictedStat(absolutePath: string): void {
  jest.doMock('fs/promises', () => {
    const actual = jest.requireActual('fs/promises');
    const stat = async (p: string, ...rest: unknown[]) => {
      const st = await actual.stat(p, ...rest);
      // Armed only after the first (indexing) scan — `__qtP4d381Evict`.
      if (p === absolutePath && (globalThis as { __qtP4d381Evict?: boolean }).__qtP4d381Evict) {
        Object.defineProperty(st, 'blocks', { value: 0 });
      }
      return st;
    };
    return { __esModule: true, ...actual, default: { ...actual, stat }, stat };
  });
}
// === /P4.D381 ===


async function loadRoute(
  path: string,
): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
  return (await import(path)) as never;
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

async function drain(): Promise<void> {
  for (let i = 0; i < 60; i++) {
    await new Promise((r) => setImmediate(r));
  }
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string; tree: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'mi-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  const treeWork = join(work, 'fs-tree');
  cpSync(fixtures.tree, treeWork, { recursive: true });
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  // === P4.D381 ===
  process.env.QT_P4D381_TREE = treeWork;
  c.plant?.(treeWork);
  process.env.LOG_LEVEL = 'info';
  const logs = await installScanLogCapture();
  // === /P4.D381 ===

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();
  // Register plugins so the BUILTIN embedding provider resolves (semantic-search).
  const { initializePlugins } = await import('@/lib/startup/plugin-initialization');
  await initializePlugins();

  // Rewrite the sentinel basePath to this case's fs-tree copy.
  const midb = getRawMountIndexDatabase() as never as {
    prepare: (s: string) => { run: (...a: unknown[]) => unknown };
  };
  midb
    .prepare('UPDATE doc_mount_points SET basePath = ? WHERE id IN (?, ?)')
    .run(treeWork, MP_FS, MP_OBS);

  try {
    const out = await c.run();
    await drain();
    const tables = await dumpTables();
    // === P4.D381 ===
    return { name: c.name, status: out.status, body: out.body, tables, logs };
    // === /P4.D381 ===
  } finally {
    c.unplant?.(treeWork); // === P4.D381 ===
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

const B = 'http://localhost/api/v1/mount-points';
const idRoute = () => loadRoute('@/app/api/v1/mount-points/[id]/route');
const collRoute = () => loadRoute('@/app/api/v1/mount-points/route');
const params = (id: string) => ({ params: Promise.resolve({ id }) });

const post = (id: string, action: string, body?: unknown) => async () =>
  respond(
    await (await idRoute()).POST(mockRequest(`${B}/${id}?action=${action}`, body), params(id)),
  );

function cases(spec: Spec): CaseSpec[] {
  void spec;
  return [
    // ── scan (P4.6y unit F) ──
    { name: 'scan_fs', run: post(MP_FS, 'scan') },
    { name: 'scan_obs', run: post(MP_OBS, 'scan') },
    { name: 'scan_db', run: post(MP_DB, 'scan') },
    { name: 'scan_404', run: post(BOGUS, 'scan') },
    // ── reindex / embed / semantic-search (P4.6y unit E) ──
    { name: 'reindex_db_default', run: post(MP_DB, 'reindex', {}) },
    { name: 'reindex_db_force', run: post(MP_DB, 'reindex', { force: true }) },
    { name: 'reindex_db_scoped', run: post(MP_DB, 'reindex', { path: 'notes/', force: true }) },
    { name: 'reindex_404', run: post(BOGUS, 'reindex', {}) },
    { name: 'embed_db_default', run: post(MP_DB, 'embed', {}) },
    {
      name: 'embed_db_scoped_force',
      run: post(MP_DB, 'embed', { path: 'reference.txt', force: true }),
    },
    { name: 'embed_404', run: post(BOGUS, 'embed', {}) },
    {
      name: 'search_basic',
      run: async () =>
        respond(
          await (await collRoute()).POST(
            mockRequest(`${B}?action=semantic-search`, {
              query: 'reference alpha',
              threshold: 0,
              top: 10,
            }),
          ),
        ),
    },
    {
      name: 'search_scoped',
      run: async () =>
        respond(
          await (await collRoute()).POST(
            mockRequest(`${B}?action=semantic-search`, {
              query: 'body line',
              mountPointIds: [MP_DB],
              pathPrefix: 'notes/',
              threshold: 0,
            }),
          ),
        ),
    },
    // === P4.D381 (v4 bf7a457f6): the in-cloud leg + the Tier-2 scan lines ===
    // `notes/alpha.md` (indexed by an earlier scan in the same case) is BOTH
    // reported evicted (`blocks: 0`) AND unreadable (mode 000): a correct port
    // stats first and leaves it `in-cloud`; a read-first port answers EACCES.
    {
      name: 'scan_fs_in_cloud',
      plant: (tree) => mockEvictedStat(join(tree, 'notes', 'alpha.md')),
      run: async () => {
        // First scan indexes the tree; then the plant: evicted + unreadable.
        await post(MP_FS, 'scan')();
        const tree = process.env.QT_P4D381_TREE ?? '';
        (globalThis as { __qtP4d381Evict?: boolean }).__qtP4d381Evict = true;
        fs.chmodSync(join(tree, 'notes', 'alpha.md'), 0o000);
        return post(MP_FS, 'scan')();
      },
      unplant: (tree) => {
        (globalThis as { __qtP4d381Evict?: boolean }).__qtP4d381Evict = false;
        fs.chmodSync(join(tree, 'notes', 'alpha.md'), 0o644);
      },
    },
    // Tier 2 item 16: the same unreadable file WITHOUT the eviction — the
    // per-file WARN + the `errors` entry.
    {
      name: 'scan_fs_unreadable_file',
      run: async () => {
        await post(MP_FS, 'scan')();
        const tree = process.env.QT_P4D381_TREE ?? '';
        fs.chmodSync(join(tree, 'notes', 'alpha.md'), 0o000);
        return post(MP_FS, 'scan')();
      },
      unplant: (tree) => fs.chmodSync(join(tree, 'notes', 'alpha.md'), 0o644),
    },
    // Tier 2 item 16: an unreadable SUBDIRECTORY — the walk WARN.
    {
      name: 'scan_fs_unreadable_dir',
      plant: (tree) => fs.chmodSync(join(tree, 'notes', 'sub'), 0o000),
      run: post(MP_FS, 'scan'),
      unplant: (tree) => fs.chmodSync(join(tree, 'notes', 'sub'), 0o755),
    },
    // === /P4.D381 ===
    {
      name: 'search_bad_body',
      run: async () =>
        respond(
          await (await collRoute()).POST(mockRequest(`${B}?action=semantic-search`, {})),
        ),
    },
  ];
}

describe('mount-index oracle', () => {
  it('emits the differential rows', async () => {
    const here = dirname(fileURLToPath(new URL(`file://${__filename}`).href));
    const spec = JSON.parse(
      fs.readFileSync(join(here, '..', 'fixtures', 'mounts.json'), 'utf8'),
    ) as Spec;
    const fixtures = {
      main: process.env.QT_FIXTURE_MOUNTS_MAIN ?? '',
      mount: process.env.QT_FIXTURE_MOUNTS_MOUNT ?? '',
      tree: process.env.QT_MOUNTS_FS_TREE ?? '',
    };
    for (const [k, v] of Object.entries(fixtures)) {
      if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
    }
    const outPath = process.env.QT_ORACLE_OUT;
    if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

    const scratch = mkdtempSync(join(tmpdir(), 'qt-mount-index-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    const rows: unknown[] = [];
    for (const c of cases(spec)) {
      rows.push(await runCase(spec, c, scratch, fixtures));
    }
    fs.writeFileSync(outPath, rows.map((r) => JSON.stringify(r)).join('\n') + '\n');
    rmSync(scratch, { recursive: true, force: true });
  });
});
