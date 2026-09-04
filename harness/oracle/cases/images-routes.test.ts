/**
 * @jest-environment node
 *
 * P4.73 — the `/api/v1/images` COLLECTION route ORACLE. Drives v4's REAL
 * `app/api/v1/images/route.ts` (and the `[id]` DELETE arm) over a FRESH copy of
 * the committed images fixture per case, emitting each response (status + body
 * + post-mutation table dumps) so the Rust port (`api::images::*`) diffs
 * byte-for-byte.
 *
 * Only the seams the Rust harness also neutralizes are mocked (the auth session
 * + the startup gate); the DB stack and every storage bridge are `doMock`ed to
 * the REAL modules past `jest.setup` — this family's whole point is that the
 * ingest and delete legs touch real rows.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-images-routes-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/images-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/images-collection.json" "$TMPO/fixtures/"
 *   cp "$V5W/crates/quilltap-web/tests/fixtures/images-main.db"  /tmp/qt-imgcol-main.db
 *   cp "$V5W/crates/quilltap-web/tests/fixtures/images-mount.db" /tmp/qt-imgcol-mount.db
 *   cp "$V5W/crates/quilltap-web/tests/fixtures/images-main.db.meta.json" /tmp/qt-imgcol-main.db.meta.json
 *   cd ~/source/quilltap-server
 *   TZ=UTC QT_FIXTURE_IMGCOL_MAIN=/tmp/qt-imgcol-main.db \
 *   QT_FIXTURE_IMGCOL_MOUNT=/tmp/qt-imgcol-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-images-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- images-routes
 */

import { copyFileSync, mkdtempSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import * as fs from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  userIdB: string;
}

// The pinned ids the fixture bakes (kept in lockstep with
// `harness/oracle/fixtures/build-images-collection-fixture.ts`).
const CHAR_TAG = 'c1000000-0000-4000-8000-000000000003';
const THEME_TAG = 'ee000000-0000-4000-8000-000000000001';
const F_TAGGED = 'f0000000-0000-4000-8000-000000000001';
const F_INUSE = 'f0000000-0000-4000-8000-000000000005';
const F_ORPHAN = 'f0000000-0000-4000-8000-000000000006';
const F_PLAIN = 'f0000000-0000-4000-8000-000000000007';
const F_NOKEY_INUSE = 'f0000000-0000-4000-8000-00000000000a';
const F_DOC = 'f0000000-0000-4000-8000-000000000009';
const MISSING = 'f0000000-0000-4000-8000-00000000dead';

const IMAGES = 'http://localhost/api/v1/images';

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'GET',
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
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
  );
  // The ingest + delete legs read and write REAL blobs; jest.setup stubs the
  // storage manager and both bridges, which would make every arm vacuous.
  jest.doMock('@/lib/file-storage/manager', () =>
    jest.requireActual('@/lib/file-storage/manager'),
  );
  jest.doMock('@/lib/file-storage/user-uploads-bridge', () =>
    jest.requireActual('@/lib/file-storage/user-uploads-bridge'),
  );
  jest.doMock('@/lib/file-storage/lantern-store-bridge', () =>
    jest.requireActual('@/lib/file-storage/lantern-store-bridge'),
  );
  jest.doMock('@/lib/file-storage/project-store-bridge', () =>
    jest.requireActual('@/lib/file-storage/project-store-bridge'),
  );
  jest.doMock('@/lib/mount-index/store-file', () =>
    jest.requireActual('@/lib/mount-index/store-file'),
  );
  jest.doMock('@/lib/files/tag-inheritance', () =>
    jest.requireActual('@/lib/files/tag-inheritance'),
  );
  jest.doMock('@/lib/mount-index/mount-chunk-cache', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/mount-index/mount-chunk-cache'),
    invalidateMountPoint: jest.fn(),
  }));
  jest.doMock('@/lib/mount-index/embedding-scheduler', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/mount-index/embedding-scheduler'),
    enqueueEmbeddingJobsForMountPoint: jest.fn().mockResolvedValue(undefined),
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

async function loadRoute(
  path: string,
): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
  return (await import(path)) as never;
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

const imagesGet = (q: string) => async () =>
  respond(await (await loadRoute('@/app/api/v1/images/route')).GET(mockRequest(`${IMAGES}${q}`)));

const imageDelete = (id: string) => async () =>
  respond(
    await (await loadRoute('@/app/api/v1/images/[id]/route')).DELETE(
      mockRequest(`${IMAGES}/${id}`),
      { params: Promise.resolve({ id }) },
    ),
  );

/**
 * The post-mutation state every write case diffs. `files` carries the columns
 * the route can move; `characters` is where the DELETE route's orphan cleanup
 * lands (`defaultImageId` cleared, `avatarOverrides` filtered).
 */
async function dumpTables(): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const main = getRawDatabase() as unknown as {
    prepare: (s: string) => { all: () => unknown };
  };
  return {
    files: main
      .prepare(
        'SELECT id, userId, sha256, originalFilename, mimeType, size, width, height, source, ' +
          'category, linkedTo, tags, description, generationPrompt, generationModel, ' +
          'generationRevisedPrompt, storageKey, fileStatus FROM files ORDER BY id',
      )
      .all(),
    characters: main
      .prepare('SELECT id, defaultImageId, avatarOverrides FROM characters ORDER BY id')
      .all(),
  };
}

interface CaseSpec {
  name: string;
  run: () => Promise<{ status: number; body: unknown }>;
  dump?: boolean;
}

function buildCases(): CaseSpec[] {
  return [
    // ── GET: the list projection ────────────────────────────────────────────
    { name: 'list_all', run: imagesGet('') },
    // `?tagId=` is JS-falsy, so it must NOT filter — the same body as list_all.
    { name: 'list_tag_empty', run: imagesGet('?tagId=') },
    // A CHARACTER-id tag: the row survives and its tagType reads CHARACTER.
    { name: 'list_tag_character', run: imagesGet(`?tagId=${CHAR_TAG}`) },
    // A THEME tag on the same row.
    { name: 'list_tag_theme', run: imagesGet(`?tagId=${THEME_TAG}`) },
    // A tag nothing carries → an empty list, not a 404.
    { name: 'list_tag_unmatched', run: imagesGet(`?tagId=${F_TAGGED}`) },
    // v4 reads `searchParams.get` — FIRST wins.
    { name: 'list_tag_duplicated', run: imagesGet(`?tagId=${CHAR_TAG}&tagId=${THEME_TAG}`) },

    // ── DELETE ──────────────────────────────────────────────────────────────
    { name: 'delete_missing', run: imageDelete(MISSING) },
    // Category is neither IMAGE nor AVATAR → v4's `notFound('Image')`.
    { name: 'delete_wrong_category', run: imageDelete(F_DOC) },
    // The bytes exist AND two characters reference it → `Image is in use`.
    { name: 'delete_in_use', run: imageDelete(F_INUSE), dump: true },
    // The bytes are GONE, so the same references are cleaned up and it deletes.
    { name: 'delete_orphaned_cleanup', run: imageDelete(F_ORPHAN), dump: true },
    // Unreferenced → the plain happy path.
    { name: 'delete_ok', run: imageDelete(F_PLAIN), dump: true },
    // NO storageKey at all, and referenced: v4 never probes storage, so
    // `fileExists` stays FALSE and the ORPHAN branch runs. The discriminator
    // for `route.ts:150`'s `if (image.storageKey)` guard.
    { name: 'delete_nokey_in_use', run: imageDelete(F_NOKEY_INUSE), dump: true },
  ];
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'imgcol-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();
  try {
    const out = await c.run();
    const payload: Record<string, unknown> = { name: c.name, status: out.status, body: out.body };
    if (c.dump) payload.tables = await dumpTables();
    return payload;
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON output');
  const fixtureMain = process.env.QT_FIXTURE_IMGCOL_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_IMGCOL_MOUNT;
  if (!fixtureMain || !fixtureMount) {
    throw new Error('QT_FIXTURE_IMGCOL_MAIN and QT_FIXTURE_IMGCOL_MOUNT must be set');
  }

  const specPath = join(__dirname, '..', 'fixtures', 'images-collection.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const scratch = mkdtempSync(join(tmpdir(), 'qt-images-routes-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cases = buildCases();
  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, c, scratch, {
      main: fixtureMain,
      mount: fixtureMount,
    });
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`images-routes oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('images-routes oracle', async () => {
  await main();
});
