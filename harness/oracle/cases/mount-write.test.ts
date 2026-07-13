/**
 * @jest-environment node
 *
 * P4.6y MOUNT-WRITE route-surface ORACLE (the ingest family: the item-route
 * PUT through `storeMountFile`, the `?action=write-file` byte-preserving verb,
 * and the blobs collection/item routes): drives v4's REAL handlers over a
 * FRESH copy of the committed mounts fixture per case, emitting
 * `{status, body, tables}` (the RAW eight-table dump). Normalization happens
 * ONCE, in the Rust test (`tests/mount_common/mod.rs`).
 *
 * The fire-and-forget post-write chain (reindex + embedding enqueue + stats)
 * runs REAL and is drained (setImmediate x60) before the dump — the in-process
 * job dispatcher stays mocked OFF so enqueued rows stay PENDING.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-mount-write-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/mount-write.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/mounts.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MOUNTS_MAIN=$V5W/crates/quilltap-web/tests/fixtures/mounts-main.db \
 *   QT_FIXTURE_MOUNTS_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/mounts-mount.db \
 *   QT_MOUNTS_FS_TREE=$V5W/crates/quilltap-web/tests/fixtures/mounts-fs-tree \
 *   QT_ORACLE_OUT=/tmp/oracle-mount-write.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- mount-write
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

function jsonRequest(url: string, body?: unknown, method = 'POST'): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
    text: jest.fn().mockResolvedValue(body === undefined ? '' : JSON.stringify(body)),
  };
}

function multipartRequest(
  url: string,
  fields: Record<string, string>,
  file: { name: string; type: string; bytes: Buffer } | null,
): unknown {
  const form = new FormData();
  for (const [k, v] of Object.entries(fields)) form.append(k, v);
  if (file) {
    form.append(
      'file',
      new File([new Uint8Array(file.bytes)], file.name, { type: file.type }),
    );
  }
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'multipart/form-data; boundary=x' }),
    formData: jest.fn().mockResolvedValue(form),
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
  jest.doMock('@/lib/embedding/embedding-service', () =>
    jest.requireActual('@/lib/embedding/embedding-service'),
  );
  jest.doMock('@/lib/background-jobs/host/processor-host', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/background-jobs/host/processor-host'),
    ensureProcessorRunning: () => {},
  }));
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
    'updatedAt FROM doc_mount_file_links ORDER BY mountPointId, relativePath',
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

interface RunCtx {
  /** intro.md's stored lastModified as epoch ms (the expected-mtime happy path). */
  introMtimeMs: number;
}

interface CaseSpec {
  name: string;
  run: (ctx: RunCtx) => Promise<{ status: number; body: unknown }>;
}

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

  const work = mkdtempSync(join(scratch, 'mw-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  const treeWork = join(work, 'fs-tree');
  cpSync(fixtures.tree, treeWork, { recursive: true });
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();
  const { initializePlugins } = await import('@/lib/startup/plugin-initialization');
  await initializePlugins();

  const midb = getRawMountIndexDatabase() as never as {
    prepare: (s: string) => { run: (...a: unknown[]) => unknown; get: (...a: unknown[]) => unknown };
  };
  midb
    .prepare('UPDATE doc_mount_points SET basePath = ? WHERE id IN (?, ?)')
    .run(treeWork, MP_FS, 'b6000000-0000-4000-8000-000000000004');

  const introRow = midb
    .prepare(
      'SELECT lastModified FROM doc_mount_file_links WHERE mountPointId = ? AND relativePath = ?',
    )
    .get(MP_DB, 'notes/intro.md') as { lastModified: string } | undefined;
  const ctx: RunCtx = {
    introMtimeMs: introRow ? new Date(introRow.lastModified).getTime() : 0,
  };

  try {
    const out = await c.run(ctx);
    await drain();
    const tables = await dumpTables();
    return { name: c.name, status: out.status, body: out.body, tables };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

const B = 'http://localhost/api/v1/mount-points';
const idRoute = () => loadRoute('@/app/api/v1/mount-points/[id]/route');
const itemRoute = () => loadRoute('@/app/api/v1/mount-points/[id]/files/[...path]/route');
const blobsRoute = () => loadRoute('@/app/api/v1/mount-points/[id]/blobs/route');
const blobItemRoute = () => loadRoute('@/app/api/v1/mount-points/[id]/blobs/[...path]/route');
const params = (id: string) => ({ params: Promise.resolve({ id }) });
const itemParams = (id: string, path: string[]) => ({ params: Promise.resolve({ id, path }) });

const put = (id: string, path: string[], body: unknown) => async () =>
  respond(
    await (await itemRoute()).PUT(
      jsonRequest(`${B}/${id}/files/${path.join('/')}`, body, 'PUT'),
      itemParams(id, path),
    ),
  );

const GARBAGE_PNG = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10, 9, 9, 9, 9]);
const FAKE_WEBP = Buffer.from('RIFF0000WEBPVP8 fake-but-webp-typed', 'utf8');
const GARBAGE_PDF = Buffer.from('definitely not a pdf either', 'utf8');

function cases(): CaseSpec[] {
  return [
    // ── item-route PUT (storeMountFile ingest) ──
    {
      name: 'put_json_new_md',
      run: put(MP_DB, ['notes', 'fresh.md'], {
        content: '# Fresh\n\nA brand new body line.\n',
      }),
    },
    {
      name: 'put_json_overwrite',
      run: put(MP_DB, ['notes', 'intro.md'], { content: '# Intro v2\n\nRewritten body.\n' }),
    },
    {
      name: 'put_json_conflict',
      run: put(MP_DB, ['notes', 'intro.md'], {
        content: 'x',
        expected_mtime: 12345,
      }),
    },
    {
      name: 'put_json_expected_ok',
      run: async (ctx) =>
        put(MP_DB, ['notes', 'intro.md'], {
          content: '# Intro v3\n\nGuarded rewrite.\n',
          expected_mtime: ctx.introMtimeMs,
        })(),
    },
    {
      name: 'put_fs_new',
      run: put(MP_FS, ['notes', 'created.txt'], { content: 'created on disk\nsecond line\n' }),
    },
    {
      name: 'put_fs_conflict',
      run: put(MP_FS, ['notes', 'alpha.md'], { content: 'x', expected_mtime: 12345 }),
    },
    {
      name: 'put_json_blob_png',
      run: put(MP_DB, ['images', 'new.png'], {
        content: GARBAGE_PNG.toString('base64'),
        encoding: 'base64',
      }),
    },
    {
      name: 'put_pdf',
      run: put(MP_DB, ['docs', 'up.pdf'], {
        content: GARBAGE_PDF.toString('base64'),
        encoding: 'base64',
      }),
    },
    // ── ?action=write-file (byte-preserving) ──
    {
      name: 'write_raw_action',
      run: async () =>
        respond(
          await (await idRoute()).POST(
            multipartRequest(`${B}/${MP_DB}?action=write-file`, { path: 'raw/out.md' }, {
              name: 'out.md',
              type: 'text/markdown',
              bytes: Buffer.from('# Raw\n\nbyte-preserving write\n', 'utf8'),
            }),
            params(MP_DB),
          ),
        ),
    },
    {
      name: 'write_raw_exists',
      run: async () =>
        respond(
          await (await idRoute()).POST(
            multipartRequest(`${B}/${MP_DB}?action=write-file`, { path: 'notes/intro.md' }, {
              name: 'intro.md',
              type: 'text/markdown',
              bytes: Buffer.from('overwrite attempt', 'utf8'),
            }),
            params(MP_DB),
          ),
        ),
    },
    // ── blobs collection ──
    {
      name: 'blob_upload_png',
      run: async () =>
        respond(
          await (await blobsRoute()).POST(
            multipartRequest(`${B}/${MP_DB}/blobs`, { path: 'images/upload.png', description: 'A caption.' }, {
              name: 'upload.png',
              type: 'image/png',
              bytes: GARBAGE_PNG,
            }),
            params(MP_DB),
          ),
        ),
    },
    {
      name: 'blob_upload_webp',
      run: async () =>
        respond(
          await (await blobsRoute()).POST(
            multipartRequest(`${B}/${MP_DB}/blobs`, { path: 'images/asis.webp' }, {
              name: 'asis.webp',
              type: 'image/webp',
              bytes: FAKE_WEBP,
            }),
            params(MP_DB),
          ),
        ),
    },
    {
      name: 'blob_upload_md',
      run: async () =>
        respond(
          await (await blobsRoute()).POST(
            multipartRequest(`${B}/${MP_DB}/blobs`, { path: 'notes/uploaded.md' }, {
              name: 'uploaded.md',
              type: 'text/markdown',
              bytes: Buffer.from('# Uploaded\n\nvia blobs route\n', 'utf8'),
            }),
            params(MP_DB),
          ),
        ),
    },
    {
      name: 'blob_upload_pdf',
      run: async () =>
        respond(
          await (await blobsRoute()).POST(
            multipartRequest(`${B}/${MP_DB}/blobs`, { path: 'docs/uploaded.pdf' }, {
              name: 'uploaded.pdf',
              type: 'application/pdf',
              bytes: GARBAGE_PDF,
            }),
            params(MP_DB),
          ),
        ),
    },
    {
      name: 'blob_upload_empty',
      run: async () =>
        respond(
          await (await blobsRoute()).POST(
            multipartRequest(`${B}/${MP_DB}/blobs`, { path: 'images/empty.png' }, {
              name: 'empty.png',
              type: 'image/png',
              bytes: Buffer.alloc(0),
            }),
            params(MP_DB),
          ),
        ),
    },
    // ── blobs list ──
    {
      name: 'blobs_list',
      run: async () =>
        respond(await (await blobsRoute()).GET(jsonRequest(`${B}/${MP_DB}/blobs`, undefined, 'GET'), params(MP_DB))),
    },
    {
      name: 'blobs_list_folder',
      run: async () =>
        respond(
          await (await blobsRoute()).GET(
            jsonRequest(`${B}/${MP_DB}/blobs?folder=images`, undefined, 'GET'),
            params(MP_DB),
          ),
        ),
    },
    // ── blob item ──
    {
      name: 'blob_delete',
      run: async () =>
        respond(
          await (await blobItemRoute()).DELETE(
            jsonRequest(`${B}/${MP_DB}/blobs/images/logo.png`, undefined, 'DELETE'),
            itemParams(MP_DB, ['images', 'logo.png']),
          ),
        ),
    },
    {
      name: 'blob_delete_doc_fallback',
      run: async () =>
        respond(
          await (await blobItemRoute()).DELETE(
            jsonRequest(`${B}/${MP_DB}/blobs/notes/intro.md`, undefined, 'DELETE'),
            itemParams(MP_DB, ['notes', 'intro.md']),
          ),
        ),
    },
    {
      name: 'blob_delete_missing',
      run: async () =>
        respond(
          await (await blobItemRoute()).DELETE(
            jsonRequest(`${B}/${MP_DB}/blobs/nope.bin`, undefined, 'DELETE'),
            itemParams(MP_DB, ['nope.bin']),
          ),
        ),
    },
    {
      name: 'blob_patch',
      run: async () =>
        respond(
          await (await blobItemRoute()).PATCH(
            jsonRequest(`${B}/${MP_DB}/blobs/images/logo.png`, { description: 'Recaptioned.' }),
            itemParams(MP_DB, ['images', 'logo.png']),
          ),
        ),
    },
    {
      name: 'blob_patch_missing',
      run: async () =>
        respond(
          await (await blobItemRoute()).PATCH(
            jsonRequest(`${B}/${MP_DB}/blobs/nope.bin`, { description: 'x' }),
            itemParams(MP_DB, ['nope.bin']),
          ),
        ),
    },
  ];
}

describe('mount-write oracle', () => {
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

    const scratch = mkdtempSync(join(tmpdir(), 'qt-mount-write-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    const rows: unknown[] = [];
    for (const c of cases()) {
      rows.push(await runCase(spec, c, scratch, fixtures));
    }
    fs.writeFileSync(outPath, rows.map((r) => JSON.stringify(r)).join('\n') + '\n');
    rmSync(scratch, { recursive: true, force: true });
  }, 300000);
});
