/**
 * @jest-environment node
 *
 * P4.6y REFRESH-PARITY ORACLE (the P4.6w seam-wire proof): the P4.6w
 * documents-routes oracle MOCKED the fire-and-forget document-store refresh,
 * so the chunks-after-a-write behavior was never pinned. This case drives
 * v4's REAL standalone `write-document` over the documents fixture with the
 * refresh chain UNMOCKED and drained, dumping the mount-side state
 * (links / documents / chunks / points / folders) the refresh produces.
 *
 * (The jobs table is deliberately NOT dumped here: the documents fixture
 * carries no embedding profile, so both sides' embed enqueues no-op — v4 by
 * the lazily-created empty profiles collection, v5 by the missing-table
 * best-effort arm. The enqueue parity itself is pinned by the mount-index /
 * mount-write suites over the mounts fixture.)
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-mount-refresh-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/mount-refresh.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/documents-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_DOC_MAIN=$V5W/crates/quilltap-web/tests/fixtures/documents-main.db \
 *   QT_FIXTURE_DOC_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/documents-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-mount-refresh.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- mount-refresh
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

const GEN = 'd0000000-0000-4000-8000-00000000000e';

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
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
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
  );
  // The REFRESH CHAIN RUNS REAL here — that's the point of this oracle. Only
  // the job dispatcher stays off so any enqueued rows would stay PENDING.
  jest.doMock('@/lib/background-jobs/host/processor-host', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/background-jobs/host/processor-host'),
    ensureProcessorRunning: () => {},
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

const DUMP: Record<string, string> = {
  points:
    'SELECT id, name, mountType, storeType, fileCount, chunkCount, totalSizeBytes ' +
    'FROM doc_mount_points ORDER BY id',
  links:
    'SELECT fileId, mountPointId, relativePath, fileName, conversionStatus, plainTextLength, ' +
    'chunkCount, extractionStatus FROM doc_mount_file_links ORDER BY mountPointId, relativePath',
  documents:
    'SELECT content, contentSha256, plainTextLength FROM doc_mount_documents ORDER BY contentSha256',
  chunks:
    'SELECT c.chunkIndex, c.content, c.tokenCount, c.headingContext, ' +
    '(CASE WHEN c.embedding IS NULL THEN NULL ELSE length(c.embedding) END) AS embeddingLength, ' +
    'l.relativePath AS sortPath ' +
    'FROM doc_mount_chunks c LEFT JOIN doc_mount_file_links l ON l.id = c.linkId ' +
    'ORDER BY c.mountPointId, sortPath, c.chunkIndex',
  folders:
    'SELECT mountPointId, parentId IS NULL AS isRoot, name, path FROM doc_mount_folders ' +
    'ORDER BY mountPointId, path',
};

describe('mount-refresh oracle', () => {
  it('emits the refresh-parity row', async () => {
    const here = dirname(fileURLToPath(new URL(`file://${__filename}`).href));
    const spec = JSON.parse(
      fs.readFileSync(join(here, '..', 'fixtures', 'documents-web.json'), 'utf8'),
    ) as Spec;
    const fxMain = process.env.QT_FIXTURE_DOC_MAIN ?? '';
    const fxMount = process.env.QT_FIXTURE_DOC_MOUNT ?? '';
    for (const [k, v] of Object.entries({ fxMain, fxMount })) {
      if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
    }
    const outPath = process.env.QT_ORACLE_OUT;
    if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

    const scratch = mkdtempSync(join(tmpdir(), 'qt-mount-refresh-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const mainWork = join(scratch, 'main.db');
    const mountWork = join(scratch, 'mount.db');
    copyFileSync(fxMain, mainWork);
    copyFileSync(fxMount, mountWork);

    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    jest.resetModules();
    applyMocks(spec);

    const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
    const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    await initializeDatabase();

    const standalone = (await import('@/app/api/v1/documents/route')) as unknown as Record<
      string,
      (...a: unknown[]) => Promise<unknown>
    >;
    const SB = 'http://localhost/api/v1/documents';
    const resp = (await standalone.POST(
      mockRequest(`${SB}?action=write-document`, {
        filePath: 'notes/refresh-case.md',
        scope: 'document_store',
        mountPoint: GEN,
        content: '# Refresh case\n\nThe chunks must exist after this write.\n',
      }),
    )) as { status: number; json: () => Promise<unknown> };
    const status = resp.status;
    const body = await resp.json();

    // Drain the fire-and-forget refresh chain.
    for (let i = 0; i < 120; i++) {
      await new Promise((r) => setImmediate(r));
    }

    const midb = getRawMountIndexDatabase() as never as {
      prepare: (s: string) => { all: () => unknown[] };
    };
    const tables: Record<string, unknown> = {};
    for (const [name, sql] of Object.entries(DUMP)) {
      tables[name] = midb.prepare(sql).all();
    }

    fs.writeFileSync(
      outPath,
      JSON.stringify({ name: 'refresh_after_standalone_write', status, body, tables }) + '\n',
    );

    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(scratch, { recursive: true, force: true });
  });
});
