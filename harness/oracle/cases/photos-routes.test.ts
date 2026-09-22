/**
 * @jest-environment node
 *
 * P4.9a USER-PHOTO-GALLERY ORACLE: drives v4's REAL exported service
 * (`lib/photos/user-gallery-service.ts` — `listUserGallery` /
 * `getUserGalleryEntry` / `removeFromUserGallery` / `saveToUserGallery`,
 * imported directly, never reimplemented) AND the REAL route handlers
 * (`app/api/v1/photos/route.ts`, `app/api/v1/photos/[id]/route.ts`) over a
 * FRESH copy of the committed `photos-{main,mount}.db` family per case. Emits
 * each response {status, body} so the Rust port (`api::photos` over
 * `photos::user_gallery_service`) diffs byte-for-byte, key order included.
 *
 * ── THE TWO MOCK BOUNDARIES ──────────────────────────────────────────────────
 *   - `generateEmbeddingForUser` is stubbed to the corpus's canned vectors
 *     (`photos-web.json#cannedEmbeddings`), keyed by the query TEXT — the same
 *     vector the Rust side injects through its canned provider. Everything below
 *     it (searchDocumentChunks, cosine similarity, the literal-phrase boost, the
 *     peak-gate/trail-band filter) runs for real on both sides.
 *   - The file-storage manager is NOT mocked: the save-leg source images were
 *     ingested by the REAL `ingestImageBuffer` at fixture-build time, so
 *     `fileStorageManager.downloadFile` reads genuine stored bytes. The
 *     `*_bytes` cases emit those bytes so the Rust canned `FileBytesStore`
 *     replays them exactly and the computed sha256 agrees.
 *
 * ── CASE COVERAGE (43) ───────────────────────────────────────────────────────
 *   - the ROUTE envelope cases (list / entry / save / delete) — v4's
 *     `successResponse` is the RAW payload and `created` is RAW at 201;
 *   - list: default order (dedup collapse + the linkSummary counts), the three
 *     pagination arms, the four tag-filter arms (incl. the case-insensitive
 *     match and a miss), and the four query arms (two single-winner semantic
 *     queries, the NOISE query that the 0.65 peak gate zeroes out, and a
 *     whitespace-only q that must take the NON-semantic branch);
 *   - the four Zod/`Number()` rejection arms on limit/offset (NaN, fractional,
 *     under-min, over-max) — v4 coerces BEFORE Zod, so each has its own message;
 *   - entry-get: the primary link, a NON-primary link for the same sha, the
 *     link in the DISABLED mount (v4's entry read does not re-check the mount —
 *     an asymmetry with list, pinned deliberately), the non-`photos/` link
 *     (404), and an unknown id (404);
 *   - save: the happy path (MUTATING — 201), the sha256 duplicate, the
 *     not-an-image / not-owned / not-found guards, and the two Zod fileId arms;
 *   - image-info (P4.9a2, `GET /api/v1/images/[id]`): the populated
 *     characterGalleryLinks (both gate legs), the documents-only-linkers empty
 *     array, the minimal UPLOADED payload, the AVATAR+SYSTEM arm, the
 *     blanked-sha 404 (read-side Zod re-validate), the no-ownership-check
 *     quirk, and the two notFound('Image') arms;
 *   - delete: a link-only removal (fileGC false — two hard links survive), the
 *     LAST-link removal (fileGC true), the non-gallery link (400) and an unknown
 *     id (404).
 *
 * ── P4.104: THE DECODABLE SAVE (`save_real_png`) ─────────────────────────────
 * v4's `linkBlobContent` normalizes image bytes to WebP with REAL sharp (bug
 * 159, `186eb09cb`), so a gallery save of a DECODABLE PNG stores `photos/*.webp`
 * / `image/webp`. The fixture's save sources are 1×1 stubs, so this case PLANTS
 * a source on its own per-case copy: the 240×170 `normalize-blob-image`
 * `photo.png` seed linked into the Uploads mount at `images/` through v4's own
 * `linkBlobContent` with `normalizeImages: false` (so the source stays a PNG),
 * plus an IMAGE `files` row (`REAL_PNG_FILE_ID`) whose `mount-blob:` storageKey
 * points at it — the committed `photos-*.db` pair is never edited. The Rust
 * side plants the same `files` row and replays the seed through its canned
 * `FileBytesStore`. The receipt diffs exact (v4 returns the PRE-normalization
 * `relativePath` and the SOURCE sha256); the stored row's D19 `imageFacts`
 * (`../lib/blob-image-facts`) are the image comparand — never the WebP bytes,
 * never the sha's value.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-photos-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/photos-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/photos-web.json" "$TMPO/fixtures/"
 *   mkdir -p "$TMPO/lib"
 *   cp "$V5W/harness/oracle/lib/blob-image-facts.ts"                 "$TMPO/lib/"
 *   cp "$V5W/harness/oracle/fixtures/normalize-blob-image/photo.png" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_PHOTOS_MAIN=$V5W/crates/quilltap-web/tests/fixtures/photos-main.db \
 *   QT_FIXTURE_PHOTOS_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/photos-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-photos.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- photos-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { createHash } from 'node:crypto';
import { blobImageFacts, sharpMeasure, STORED_BLOB_SELECT } from '../lib/blob-image-facts';

/** P4.104: the planted decodable save source (see the header). */
const REAL_PNG_FILE_ID = 'f3000000-0000-4000-8000-000000000001';
const REAL_PNG_CAPTION = 'A real photograph';
const PLANT_TS = '2026-01-01T00:00:00.000Z';

interface Spec {
  testPepperBase64: string;
  userId: string;
  otherUserId: string;
  cannedEmbeddings: Record<string, number[]>;
}

interface Meta {
  ariaVault: string;
  bramVault: string;
  uploadsMp: string;
  projectMp: string;
  disabledMp: string;
  linkIds: Record<string, string[]>;
  shas: Record<string, string>;
  notesLinkId: string;
  saveOkFileId: string;
  saveDupeFileId: string;
  saveNotImageFileId: string;
  saveNotOwnedFileId: string;
  missingFileId: string;
  imageInfoLinkedId: string;
  imageInfoDocLinksId: string;
  imageInfoPlainId: string;
  imageInfoAvatarId: string;
  imageInfoNoShaId: string;
}

const LIST_ROUTE = '@/app/api/v1/photos/route';
const ITEM_ROUTE = '@/app/api/v1/photos/[id]/route';
const IMAGE_ROUTE = '@/app/api/v1/images/[id]/route';

function mockRequest(url: string, method = 'GET', body?: unknown): unknown {
  return {
    method,
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
  jest.doMock('@/lib/file-storage/manager', () => jest.requireActual('@/lib/file-storage/manager'));
  jest.doMock('@/lib/file-storage/user-uploads-bridge', () =>
    jest.requireActual('@/lib/file-storage/user-uploads-bridge'),
  );
  jest.doMock('@/lib/mount-index/document-search', () =>
    jest.requireActual('@/lib/mount-index/document-search'),
  );
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
  );
  // ONLY the model boundary is canned; the search + ranking below it are real.
  jest.doMock('@/lib/embedding/embedding-service', () => {
    const actual = jest.requireActual('@/lib/embedding/embedding-service');
    const { EmbeddingError } = actual;
    return {
      __esModule: true,
      ...actual,
      generateEmbeddingForUser: async (text: string) => {
        const vec = spec.cannedEmbeddings[text];
        if (!vec) {
          throw new EmbeddingError(`no canned embedding registered for input: ${text}`);
        }
        return {
          embedding: new Float32Array(vec),
          model: 'canned',
          dimensions: vec.length,
          provider: 'canned',
        };
      },
    };
  });
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

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

const B = 'http://localhost/api/v1';

async function listRoute(qs: string): Promise<{ status: number; body: unknown }> {
  const mod = (await import(LIST_ROUTE)) as { GET: (...a: unknown[]) => Promise<unknown> };
  return respond(await mod.GET(mockRequest(`${B}/photos${qs}`)));
}
async function saveRoute(body: unknown): Promise<{ status: number; body: unknown }> {
  const mod = (await import(LIST_ROUTE)) as { POST: (...a: unknown[]) => Promise<unknown> };
  return respond(await mod.POST(mockRequest(`${B}/photos`, 'POST', body)));
}
async function entryRoute(id: string): Promise<{ status: number; body: unknown }> {
  const mod = (await import(ITEM_ROUTE)) as { GET: (...a: unknown[]) => Promise<unknown> };
  return respond(
    await mod.GET(mockRequest(`${B}/photos/${id}`), { params: Promise.resolve({ id }) }),
  );
}
async function deleteRoute(id: string): Promise<{ status: number; body: unknown }> {
  const mod = (await import(ITEM_ROUTE)) as { DELETE: (...a: unknown[]) => Promise<unknown> };
  return respond(
    await mod.DELETE(mockRequest(`${B}/photos/${id}`, 'DELETE'), {
      params: Promise.resolve({ id }),
    }),
  );
}
/** P4.9a2 — the REAL `GET /api/v1/images/[id]` handler. */
async function imageRoute(id: string): Promise<{ status: number; body: unknown }> {
  const mod = (await import(IMAGE_ROUTE)) as { GET: (...a: unknown[]) => Promise<unknown> };
  return respond(
    await mod.GET(mockRequest(`${B}/images/${id}`), { params: Promise.resolve({ id }) }),
  );
}

interface CaseSpec {
  name: string;
  run: (s: Spec, m: Meta) => Promise<{ status: number; body: unknown; imageFacts?: unknown }>;
}

/** The seed PNG (staged beside the corpus in the mirror's fixtures dir). */
function realPng(): Buffer {
  const here = dirname(fileURLToPath(import.meta.url));
  return readFileSync(join(here, '..', 'fixtures', 'photo.png'));
}

/**
 * P4.104: plant the decodable save source on THIS case's copy — the seed PNG
 * linked into the Uploads mount through v4's own `linkBlobContent` with
 * `normalizeImages: false` (the source must stay a PNG), and an IMAGE `files`
 * row whose `mount-blob:` storageKey resolves to that blob.
 */
async function plantRealPng(s: Spec, m: Meta): Promise<void> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const repos = getRepositories();
  const png = realPng();
  const sha256 = createHash('sha256').update(png).digest('hex');
  const { blobId } = await repos.docMountFileLinks.linkBlobContent({
    mountPointId: m.uploadsMp,
    relativePath: 'images/real-photo.png',
    fileName: 'real-photo.png',
    folderId: null,
    originalFileName: 'real-photo.png',
    originalMimeType: 'image/png',
    storedMimeType: 'image/png',
    sha256,
    data: png,
    normalizeImages: false,
  } as never);
  await repos.files.create(
    {
      userId: s.userId,
      linkedTo: [],
      tags: [],
      sha256,
      originalFilename: 'real-photo.png',
      mimeType: 'image/png',
      size: png.length,
      source: 'UPLOADED',
      category: 'IMAGE',
      storageKey: `mount-blob:${m.uploadsMp}:${blobId}`,
    } as never,
    { id: REAL_PNG_FILE_ID, createdAt: PLANT_TS, updatedAt: PLANT_TS } as never,
  );
}

/** P4.104: the D19 facts of the gallery row the decodable save wrote. */
async function realPngImageFacts(m: Meta): Promise<unknown> {
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const midb = getRawMountIndexDatabase() as unknown as {
    prepare: (q: string) => { all: (...a: unknown[]) => unknown };
  };
  const rows = midb
    .prepare(
      STORED_BLOB_SELECT +
        "WHERE l.mountPointId = ? AND l.relativePath LIKE 'photos/%-a-real-photograph.%' " +
        'ORDER BY l.relativePath',
    )
    .all(m.uploadsMp) as Parameters<typeof blobImageFacts>[0];
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const sharp = require(require('node:path').join(process.cwd(), 'node_modules/sharp'));
  return blobImageFacts(rows, realPng(), sharpMeasure(sharp));
}

function buildCases(): CaseSpec[] {
  return [
    // ── The stored bytes the Rust canned FileBytesStore must replay ──
    {
      name: 'save_ok_bytes',
      run: async (_s, m) => {
        const { readImageBuffer } = await import('@/lib/images-v2');
        const buf = await readImageBuffer(m.saveOkFileId);
        return { status: 200, body: { base64: Buffer.from(buf).toString('base64') } };
      },
    },
    {
      name: 'save_dupe_bytes',
      run: async (_s, m) => {
        const { readImageBuffer } = await import('@/lib/images-v2');
        const buf = await readImageBuffer(m.saveDupeFileId);
        return { status: 200, body: { base64: Buffer.from(buf).toString('base64') } };
      },
    },

    // ── LIST ──
    { name: 'list_default', run: () => listRoute('') },
    { name: 'list_page_first', run: () => listRoute('?limit=2&offset=0') },
    { name: 'list_page_second', run: () => listRoute('?limit=2&offset=2') },
    { name: 'list_offset_past_end', run: () => listRoute('?offset=99') },
    { name: 'list_limit_nan', run: () => listRoute('?limit=abc') },
    { name: 'list_limit_fraction', run: () => listRoute('?limit=1.5') },
    { name: 'list_limit_over_max', run: () => listRoute('?limit=500') },
    { name: 'list_offset_negative', run: () => listRoute('?offset=-1') },
    { name: 'list_tag_single', run: () => listRoute('?tag=covenant') },
    { name: 'list_tag_case_insensitive', run: () => listRoute('?tag=COVENANT') },
    { name: 'list_tag_multi', run: () => listRoute('?tag=sky&tag=lantern') },
    { name: 'list_tag_miss', run: () => listRoute('?tag=nonesuch') },
    { name: 'list_query_dawn', run: () => listRoute('?q=dawn%20flight') },
    { name: 'list_query_covenant', run: () => listRoute('?q=covenant') },
    { name: 'list_query_noise', run: () => listRoute('?q=asdfasdf') },
    // Whitespace-only q must take the NON-semantic branch (no embedding call —
    // and there is no canned vector for it, so a semantic branch would throw).
    { name: 'list_query_blank', run: () => listRoute('?q=%20%20') },

    // ── ENTRY GET ──
    { name: 'entry_primary', run: (_s, m) => entryRoute(m.linkIds.A[1]) },
    { name: 'entry_non_primary', run: (_s, m) => entryRoute(m.linkIds.A[2]) },
    // The link in the DISABLED mount: invisible to list, but the entry read
    // resolves it — v4 does not re-check the mount here.
    { name: 'entry_disabled_mount', run: (_s, m) => entryRoute(m.linkIds.E[0]) },
    { name: 'entry_not_a_photo', run: (_s, m) => entryRoute(m.notesLinkId) },
    { name: 'entry_unknown', run: () => entryRoute('00000000-0000-4000-8000-000000000bad') },

    // ── SAVE (the happy path MUTATES; every case runs on its own copy) ──
    { name: 'save_ok', run: (_s, m) => saveRoute({ fileId: m.saveOkFileId, caption: 'A fresh arrival', tags: ['new'] }) },
    { name: 'save_duplicate', run: (_s, m) => saveRoute({ fileId: m.saveDupeFileId }) },
    { name: 'save_not_an_image', run: (_s, m) => saveRoute({ fileId: m.saveNotImageFileId }) },
    { name: 'save_not_owned', run: (_s, m) => saveRoute({ fileId: m.saveNotOwnedFileId }) },
    { name: 'save_missing_file', run: (_s, m) => saveRoute({ fileId: m.missingFileId }) },
    { name: 'save_absent_field', run: () => saveRoute({}) },
    { name: 'save_empty_field', run: () => saveRoute({ fileId: '' }) },
    // P4.104: a DECODABLE source — `linkBlobContent` normalizes it to WebP.
    {
      name: 'save_real_png',
      run: async (s, m) => {
        await plantRealPng(s, m);
        const out = await saveRoute({ fileId: REAL_PNG_FILE_ID, caption: REAL_PNG_CAPTION });
        return { ...out, imageFacts: await realPngImageFacts(m) };
      },
    },

    // ── IMAGE INFO (P4.9a2 — read-only; `GET /api/v1/images/[id]`) ──
    // sha A → the two vault links pass the character+isPhotoAlbum gate; the
    // Uploads copy and the notes/ link must not. Also the CHARACTER/THEME
    // tagType split and the _count pair (Aria default + Bramwell's overrides).
    { name: 'imageInfo_linked', run: (_s, m) => imageRoute(m.imageInfoLinkedId) },
    // sha B → linkers exist but all live in 'documents' mounts → empty array.
    { name: 'imageInfo_doc_links_only', run: (_s, m) => imageRoute(m.imageInfoDocLinksId) },
    // An unlinked UPLOADED image — the minimal payload (width/height/generation
    // fields omitted) and the UPLOADED→upload remap.
    { name: 'imageInfo_plain_upload', run: (_s, m) => imageRoute(m.imageInfoPlainId) },
    // category AVATAR passes the guard; source SYSTEM hits the 'upload' fallback.
    { name: 'imageInfo_avatar_system', run: (_s, m) => imageRoute(m.imageInfoAvatarId) },
    // The blanked-sha legacy row: the read-side Zod re-validate fails inside
    // safeQuery's null-fallback, so findById reports it ABSENT → 404.
    { name: 'imageInfo_no_sha', run: (_s, m) => imageRoute(m.imageInfoNoShaId) },
    // The GET has NO ownership check (only auth) — the stranger's IMAGE row
    // resolves, with the counts computed over the SESSION user's roster.
    { name: 'imageInfo_not_owned', run: (_s, m) => imageRoute(m.saveNotOwnedFileId) },
    // category ATTACHMENT → notFound('Image').
    { name: 'imageInfo_not_image', run: (_s, m) => imageRoute(m.saveNotImageFileId) },
    { name: 'imageInfo_unknown', run: (_s, m) => imageRoute(m.missingFileId) },

    // ── DELETE ──
    // A's Uploads link — two hard links survive, so no GC.
    { name: 'delete_link_only', run: (_s, m) => deleteRoute(m.linkIds.A[0]) },
    // D's only link — the file row and blob go with it.
    { name: 'delete_last_link', run: (_s, m) => deleteRoute(m.linkIds.D[0]) },
    { name: 'delete_not_a_photo', run: (_s, m) => deleteRoute(m.notesLinkId) },
    { name: 'delete_unknown', run: () => deleteRoute('00000000-0000-4000-8000-000000000bad') },
  ];
}

async function runCase(
  spec: Spec,
  meta: Meta,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'photos-'));
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
    const out = await c.run(spec, meta);
    // v4's save leg ends with THREE fire-and-forget promises (`refreshStats`,
    // `enqueueEmbeddingJobsForMountPoint`, and the emitted document event) that
    // outlive the awaited response. Without a drain they land AFTER this case's
    // `closeDatabase()` and race the NEXT case's `initializeDatabase()`, whose
    // `readSetting` then throws into `getUserUploadsStore`'s catch — surfacing
    // as a bogus "Quilltap Uploads mount has not been provisioned" on whichever
    // case happens to follow a mutating one. Drain before closing.
    await new Promise((r) => setTimeout(r, 250));
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      ...(out.imageFacts !== undefined ? { imageFacts: out.imageFacts } : {}),
    };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'photos-web.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_PHOTOS_MAIN ?? '',
    mount: process.env.QT_FIXTURE_PHOTOS_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const meta = JSON.parse(readFileSync(fixtures.main + '.meta.json', 'utf8')) as Meta;
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-photos-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const outLines: string[] = [];
  for (const c of buildCases()) {
    const payload = await runCase(spec, meta, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`photos-routes oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('photos-routes oracle', async () => {
  await main();
});
