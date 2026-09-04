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

// The two stores this route writes into (the builder pins both).
const UPLOADS_MP = '80000000-0000-4000-8000-000000000001';
const LANTERN_MP = '80000000-0000-4000-8000-000000000002';

/** A real 1x1 PNG — sharp must be able to decode it for the WebP arm to mean
 *  anything (a synthetic buffer would take `convertToWebP`'s catch instead). */
const PNG_1X1 = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==',
  'base64',
);
/** An SVG passes through `convertToWebP` untouched, so its bytes — and
 *  therefore its sha — are comparable BETWEEN the two implementations. */
const SVG_BYTES = Buffer.from(
  '<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"></svg>',
  'utf8',
);

interface UploadFile {
  name: string;
  type: string;
  bytes: Buffer;
}

/** A multipart POST, the shape v4's `request.formData()` reads. */
function mockMultipartRequest(
  url: string,
  fields: { file?: UploadFile; [k: string]: unknown },
): unknown {
  const fd = new FormData();
  for (const [k, v] of Object.entries(fields)) {
    if (k === 'file' && v) {
      const f = v as UploadFile;
      fd.append('file', new File([f.bytes], f.name, { type: f.type }));
    } else if (v != null) {
      fd.append(k, String(v));
    }
  }
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'multipart/form-data; boundary=x' }),
    formData: async () => fd,
    json: async () => ({}),
  };
}

function mockJsonPost(url: string, body: unknown): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    formData: async () => {
      throw new Error('not multipart');
    },
    json: jest.fn().mockResolvedValue(body),
  };
}

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'GET',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

/**
 * The canned `fetch` the import-from-URL leg sees. v4 imports `node-fetch` at
 * the top of `lib/images-v2.ts`, so the mock is installed per case (after
 * `jest.resetModules`) and reads this slot — the same canned-wire-per-row shape
 * the P4.D138 HuggingFace family uses.
 */
let cannedFetch: { status: number; statusText: string; contentType: string; body: Buffer } = {
  status: 200,
  statusText: 'OK',
  contentType: 'image/png',
  body: Buffer.alloc(0),
};

function applyMocks(spec: Spec): void {
  // `node-fetch` is a dependency of the v4 CHECKOUT, not of this /tmp-staged
  // case, so a bare specifier does not resolve here — and a `{virtual: true}`
  // mock would register under a different key than the one `images-v2.ts`
  // resolves. Mock the RESOLVED path instead, from the checkout (cwd).
  const nodeFetchPath = require.resolve('node-fetch', { paths: [process.cwd()] });
  jest.doMock(nodeFetchPath, () => ({
    __esModule: true,
    default: async () => ({
      ok: cannedFetch.status >= 200 && cannedFetch.status < 300,
      status: cannedFetch.status,
      statusText: cannedFetch.statusText,
      headers: {
        get: (k: string) =>
          k.toLowerCase() === 'content-type' ? cannedFetch.contentType : null,
      },
      arrayBuffer: async () =>
        cannedFetch.body.buffer.slice(
          cannedFetch.body.byteOffset,
          cannedFetch.body.byteOffset + cannedFetch.body.byteLength,
        ),
    }),
  }));
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

const imagesUpload = (fields: { file?: UploadFile; [k: string]: unknown }) => async () =>
  respond(
    await (await loadRoute('@/app/api/v1/images/route')).POST(
      mockMultipartRequest(IMAGES, fields),
    ),
  );

/** The `?action=` fall-through probe: v4 uploads on ANY non-'generate' value. */
const imagesUploadWithAction = (
  action: string,
  fields: { file?: UploadFile; [k: string]: unknown },
) =>
  async () =>
    respond(
      await (await loadRoute('@/app/api/v1/images/route')).POST(
        mockMultipartRequest(`${IMAGES}?action=${action}`, fields),
      ),
    );

const imagesImport = (body: unknown) => async () =>
  respond(
    await (await loadRoute('@/app/api/v1/images/route')).POST(mockJsonPost(IMAGES, body)),
  );

/** A POST whose content-type is neither JSON nor multipart. */
const imagesPostBadContentType = () => async () =>
  respond(
    await (await loadRoute('@/app/api/v1/images/route')).POST({
      method: 'POST',
      url: IMAGES,
      nextUrl: new URL(IMAGES),
      headers: new Headers({ 'Content-Type': 'text/plain' }),
      json: async () => ({}),
    } as never),
  );

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
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const main = getRawDatabase() as unknown as {
    prepare: (s: string) => { all: () => unknown };
  };
  const mount = getRawMountIndexDatabase() as unknown as {
    prepare: (s: string) => { all: (...a: unknown[]) => unknown };
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
    // The MOUNT side. v4 writes the blob BEFORE `repos.files.create`
    // validates, so a create that throws leaves the bytes behind — an orphan
    // this dump makes visible instead of assumed. `sha256` here is what the
    // within-tree "the row's sha names the bytes we stored" check reads.
    // `sha256`/`fileSizeBytes` live on doc_mount_files (content-addressed);
    // the link row carries the path and the ORIGINAL mime. Joined so one row
    // shows both halves.
    links: mount
      .prepare(
        'SELECT l.relativePath, l.fileName, l.originalMimeType, f.sha256, f.fileSizeBytes, ' +
          'f.fileType FROM doc_mount_file_links l JOIN doc_mount_files f ON f.id = l.fileId ' +
          // Scoped to the two stores this route writes: every character mint
          // scaffolds a vault full of empty .md files, which would swamp the
          // dump and make the ingest rows unfindable.
          'WHERE l.mountPointId IN (?, ?) ORDER BY l.relativePath',
      )
      .all(UPLOADS_MP, LANTERN_MP),
  };
}

interface CaseSpec {
  name: string;
  run: () => Promise<{ status: number; body: unknown }>;
  dump?: boolean;
  /** Installed into `cannedFetch` before the case runs (import leg only). */
  fetch?: { status: number; statusText: string; contentType: string; body: Buffer };
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

    // ── POST: upload (multipart) ────────────────────────────────────────────
    // A real PNG: `createFile` transcodes it to WebP, so the stored mime and
    // the .webp filename are the POLICY comparands (the BYTES are not — D19).
    {
      name: 'upload_png',
      run: imagesUpload({ file: { name: 'shot.png', type: 'image/png', bytes: PNG_1X1 } }),
      dump: true,
    },
    // SVG passes `convertToWebP` through untouched, so its bytes — and its
    // sha — ARE comparable between the two implementations.
    {
      name: 'upload_svg',
      run: imagesUpload({ file: { name: 'mark.svg', type: 'image/svg+xml', bytes: SVG_BYTES } }),
      dump: true,
    },
    // `validateImageFile` THROWS; nothing wraps it, so the client sees the
    // middleware's flat 500 and the sentence never reaches the wire.
    {
      name: 'upload_disallowed_type',
      run: imagesUpload({ file: { name: 'note.txt', type: 'text/plain', bytes: Buffer.from('x') } }),
      dump: true,
    },
    // Over the 10 MB cap — the same thrown-then-500 shape.
    {
      name: 'upload_oversize',
      run: imagesUpload({
        file: { name: 'big.png', type: 'image/png', bytes: Buffer.alloc(10 * 1024 * 1024 + 1) },
      }),
      dump: true,
    },
    { name: 'upload_no_file', run: imagesUpload({}), dump: true },
    {
      name: 'upload_tags_bad_json',
      run: imagesUpload({
        file: { name: 'shot.png', type: 'image/png', bytes: PNG_1X1 },
        tags: '{oops',
      }),
      dump: true,
    },
    // P4.62(a): NO schema on this leg, so `tags.map(t => t.tagId)` carries the
    // RAW value. `5` lands in the row's `tags` column as a number — which then
    // makes the row fail `FileEntrySchema` and vanish from v4's OWN list.
    {
      name: 'upload_tags_raw_tagid',
      run: imagesUpload({
        file: { name: 'shot.png', type: 'image/png', bytes: PNG_1X1 },
        tags: '[{"tagType":"THEME","tagId":5}]',
      }),
      dump: true,
    },
    // A tag id that IS a string is carried into linkedTo AND the tags column.
    {
      name: 'upload_tags_string_tagid',
      run: imagesUpload({
        file: { name: 'shot.png', type: 'image/png', bytes: PNG_1X1 },
        tags: `[{"tagType":"CHARACTER","tagId":"${CHAR_TAG}"}]`,
      }),
      dump: true,
    },
    // THE FIRST-SHAPE PROBE: `?action=` anything-but-generate still UPLOADS.
    // There is no `withActionDispatch` on this route and so no unknown-action
    // envelope.
    {
      name: 'upload_action_unknown',
      run: imagesUploadWithAction('zzz-not-an-action', {
        file: { name: 'shot.png', type: 'image/png', bytes: PNG_1X1 },
      }),
      dump: true,
    },

    // ── POST: import from URL (JSON) ────────────────────────────────────────
    {
      name: 'import_ok',
      run: imagesImport({ url: 'https://example.invalid/pics/photo.png' }),
      fetch: { status: 200, statusText: 'OK', contentType: 'image/png', body: PNG_1X1 },
      dump: true,
    },
    // The URL path's last segment carries no dot → the mime's subtype is
    // appended (`images-v2.ts:295`).
    {
      name: 'import_no_dot_filename',
      run: imagesImport({ url: 'https://example.invalid/pics/portrait' }),
      fetch: { status: 200, statusText: 'OK', contentType: 'image/png', body: PNG_1X1 },
      dump: true,
    },
    // The dotless-path arm above is VACUOUS on its own: `createFile`
    // transcodes the PNG and renames it to `.webp`, which is what
    // `replaceExtensionWithWebP` would also produce from a bare `portrait` —
    // so appending the extension or not lands in the same place (a mutation
    // dropping the rule stayed green). An SVG passes through untouched, so the
    // filename v4 built is the filename that survives.
    //
    // It also pins a v4 quirk: `contentType.split('/')[1]` of `image/svg+xml`
    // is `svg+xml`, so the stored name really is `portrait.svg+xml`.
    {
      name: 'import_no_dot_svg',
      run: imagesImport({ url: 'https://example.invalid/pics/portrait' }),
      fetch: {
        status: 200,
        statusText: 'OK',
        contentType: 'image/svg+xml',
        body: SVG_BYTES,
      },
      dump: true,
    },
    {
      name: 'import_not_ok',
      run: imagesImport({ url: 'https://example.invalid/missing.png' }),
      fetch: { status: 404, statusText: 'Not Found', contentType: 'image/png', body: Buffer.alloc(0) },
      dump: true,
    },
    {
      name: 'import_wrong_content_type',
      run: imagesImport({ url: 'https://example.invalid/page.html' }),
      fetch: { status: 200, statusText: 'OK', contentType: 'text/html', body: Buffer.from('<html>') },
      dump: true,
    },
    {
      name: 'import_oversize',
      run: imagesImport({ url: 'https://example.invalid/huge.png' }),
      fetch: {
        status: 200,
        statusText: 'OK',
        contentType: 'image/png',
        body: Buffer.alloc(10 * 1024 * 1024 + 1),
      },
      dump: true,
    },
    // `importFromUrlSchema` IS schema-checked (unlike the multipart leg), so a
    // wrong-typed tagId refuses with the middleware's `Validation error`.
    {
      name: 'import_tags_raw_tagid',
      run: imagesImport({
        url: 'https://example.invalid/pics/photo.png',
        tags: [{ tagType: 'THEME', tagId: 5 }],
      }),
      fetch: { status: 200, statusText: 'OK', contentType: 'image/png', body: PNG_1X1 },
      dump: true,
    },
    { name: 'import_bad_url', run: imagesImport({ url: 'not-a-url' }), dump: true },

    // Neither JSON nor multipart.
    { name: 'post_bad_content_type', run: imagesPostBadContentType(), dump: true },

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
  if (c.fetch) cannedFetch = c.fetch;

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
