/**
 * P4.9a fixture builder (lane A) — the committed test-pepper substrate for the
 * user-photo-gallery differential (`photos_routes_equivalence`). Everything is
 * baked through v4's REAL repositories + the REAL `linkBlobContent` store
 * write, so the DDL, the content-addressed dedup, and the blob rows are
 * production-identical on both differential sides.
 *
 * ── TWO FILES, ONE FAMILY ────────────────────────────────────────────────────
 *   - `photos-main.db`  — users (the owner + a stranger, for the ownership arm),
 *                         characters (whose creates mint the two vaults), and
 *                         the `files` rows the SAVE leg reads.
 *   - `photos-mount.db` — the mount-index sibling: five mount points, the
 *                         `photos/` links, their blobs, and the pinned-embedding
 *                         chunks the semantic branch searches.
 *
 * ── WHAT THE CORPUS STAGES (the listUserGallery arms) ────────────────────────
 * Four DISTINCT images live under `photos/` in ENABLED mounts, so the default
 * listing is exactly four entries, newest-primary-first:
 *
 *   sha A "dawn flight"   — THREE hard links: Aria's vault (T5, the primary),
 *                           Bramwell's vault (T3), Quilltap Uploads (T1). Plus a
 *                           FOURTH link at `notes/reference.md` in Aria's vault
 *                           which is NOT under `photos/`: it must not create a
 *                           second entry, must still appear in the linkSummary
 *                           (with `isPhotoAlbum: false`), and is the subject of
 *                           the delete route's "not a gallery entry" arm.
 *                           → the dedup collapse AND the linkSummary count 4.
 *   sha B "covenant"      — one link, Quilltap Uploads (T4); caption + two tags
 *                           + a generation prompt (the excerpt arm) + a REVISED
 *                           prompt (so the markdown carries two `##` sections).
 *   sha C "wardrobe"      — one link, the project store (T2); NO caption, NO
 *                           tags — the null/empty projection arms.
 *   sha D "lantern"       — one link, Aria's vault (T0); the LAST-LINK GC arm:
 *                           deleting it drops the file row and the blob.
 *   sha E "hidden"        — one link in a DISABLED mount: `findEnabled` must
 *                           drop it entirely (it is invisible to list, but
 *                           `getUserGalleryEntry` still resolves it — v4's entry
 *                           read does NOT re-check the mount, a real asymmetry
 *                           the differential pins).
 *
 * The two vaults are minted by the REAL `repos.characters.create`, so their
 * `storeType` is genuinely `'character'` — that is what drives the Vault-vs-Album
 * badge the SPA renders off `linkSummary.linkers[].mountStoreType`.
 *
 * ── THE SAVE LEG'S `files` ROWS ──────────────────────────────────────────────
 *   - SAVE_OK      — a real `ingestImageBuffer` image (fresh bytes) → the happy
 *                    path (and the only case that MUTATES, so it runs on its own
 *                    copy and its minted ids are remapped harness-side).
 *   - SAVE_DUPE    — a real ingest of the SAME bytes as sha A → v4's
 *                    "already saved" 400, because sha A is already linked under
 *                    `photos/` in the Quilltap Uploads mount.
 *   - SAVE_NOT_IMG — category ATTACHMENT + `text/plain` → the "not an image" 400.
 *   - SAVE_NOT_OWN — an IMAGE owned by the STRANGER → the "not owned" 400.
 *   - (a nonexistent id)                              → the "Image not found" 400.
 *
 * ── THE SEMANTIC BRANCH ──────────────────────────────────────────────────────
 * Each `photos/` link gets ONE chunk whose embedding is PINNED in
 * `photos-web.json` (`chunkEmbeddings`), and the oracle stubs
 * `generateEmbeddingForUser` to the matching `cannedEmbeddings` entry (the Rust
 * side injects the identical vector through its canned provider). The three
 * query vectors are chosen to exercise all three gate outcomes:
 *   - "dawn flight" → only sha A clears the trail band (a single-winner query);
 *   - "covenant"    → only sha B;
 *   - "asdfasdf"    → every score sits at the noise floor, BELOW the 0.65 peak
 *                     gate, so v4 returns ZERO entries (the peak-gate arm).
 * The real ranking still runs on both sides — the literal-phrase boost included
 * — so the differential, not this comment, is what proves the scores agree.
 *
 * ── PINNED TIMESTAMPS ────────────────────────────────────────────────────────
 * `linkBlobContent` stamps `createdAt` with a LIVE clock, and the dedup
 * tie-break reads exactly that column, so every link's `createdAt`/`updatedAt`
 * is re-pinned by raw UPDATE after staging (step 8). The minted link ids are
 * echoed to the `.meta.json` sidecar both the oracle case and the Rust
 * differential read.
 *
 * Regenerate (Node 24, from the v4 checkout) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_PHOTOS_MAIN=$W/crates/quilltap-web/tests/fixtures/photos-main.db \
 *   QT_FIXTURE_PHOTOS_MOUNT=$W/crates/quilltap-web/tests/fixtures/photos-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-photos-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  otherUserId: string;
  seedTimestamp: string;
  cannedEmbeddings: Record<string, number[]>;
  chunkEmbeddings: Record<string, number[]>;
}

// ── Pinned entity ids (shared verbatim with the oracle + the Rust harness) ───
const ARIA = 'a1000000-0000-4000-8000-000000000001'; // vault → storeType 'character'
const BRAMWELL = 'a1000000-0000-4000-8000-000000000002';

const UPLOADS_MP = 'b0000000-0000-4000-8000-0000000000ff';
const PROJECT_MP = 'b0000000-0000-4000-8000-000000000001';
const DISABLED_MP = 'b0000000-0000-4000-8000-00000000dead';

const SAVE_NOT_IMG = 'f1000000-0000-4000-8000-000000000001';
const SAVE_NOT_OWN = 'f1000000-0000-4000-8000-000000000002';

// The image-info rows (P4.9a2 — `GET /api/v1/images/{id}`). Each pins one arm
// of the route: LINKED carries sha A (so its characterGalleryLinks resolve
// through the two vault links, and NOT the Uploads/documents or notes/
// non-album links); DOC carries sha B (linkers exist, none pass the
// character+isPhotoAlbum gate → empty array); PLAIN/AVATAR/NOSHA pin the
// UPLOADED/SYSTEM source remaps, the AVATAR category pass, and the
// invalid-sha row v4's read-side Zod re-validate turns into a 404.
const IMG_LINKED = 'f2000000-0000-4000-8000-000000000001';
const IMG_DOC = 'f2000000-0000-4000-8000-000000000002';
const IMG_PLAIN = 'f2000000-0000-4000-8000-000000000003';
const IMG_AVATAR = 'f2000000-0000-4000-8000-000000000004';
const IMG_NOSHA = 'f2000000-0000-4000-8000-000000000005';
/** A non-character tag id — derives `tagType: 'THEME'` on the wire. */
const THEME_TAG = 'e7000000-0000-4000-8000-000000000077';
/** The two avatar-override chat ids Bramwell's `_count` arm carries. */
const OVERRIDE_CHAT_1 = 'c1000000-0000-4000-8000-000000000001';
const OVERRIDE_CHAT_2 = 'c1000000-0000-4000-8000-000000000002';

// The four photo images + the disabled-mount one. Distinct bytes → distinct
// sha256, which is what the dedup collapse is keyed on.
const PNG_HEAD = [
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
  0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
  0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x62, 0x00, 0x01, 0x00, 0x00,
  0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
  0x42, 0x60, 0x82,
];
/** A distinct byte string per image — the PNG header plus a unique tail. */
function imageBytes(tag: string): Buffer {
  return Buffer.concat([Buffer.from(PNG_HEAD), Buffer.from(`\n${tag}\n`, 'utf-8')]);
}

/** The per-image staging plan. `at` is the PINNED createdAt for every link. */
interface PhotoSpec {
  key: string;
  tag: string;
  mounts: Array<{ mp: () => string; path: string; at: string }>;
  caption: string | null;
  tags: string[];
  generationPrompt: string | null;
  generationRevisedPrompt: string | null;
  /** The chunk embedding key in `photos-web.json`. */
  embedding: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'photos-web.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_PHOTOS_MAIN;
  const mountOut = process.env.QT_FIXTURE_PHOTOS_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_PHOTOS_{MAIN,MOUNT} must point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }
  if (existsSync(mainOut + '.meta.json')) rmSync(mainOut + '.meta.json');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-photos-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema, ChatMetadataSchema, FileEntrySchema } = await import(
    '@/lib/schemas/types'
  );
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    ProjectDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');
  const { buildKeptImageMarkdown, sha256OfString, basenameOfRelativePath } = await import(
    '@/lib/photos/keep-image-markdown'
  );
  const { ensureFolderPath } = await import('@/lib/mount-index/folder-paths');
  const { ingestImageBuffer, readImageBuffer } = await import('@/lib/images-v2');
  const { sha256OfBuffer } = await import('@/lib/utils/sha256');

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('chats', ChatMetadataSchema);
  await ensureCollection('files', FileEntrySchema);

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
    ['project_doc_mount_links', ProjectDocMountLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  const repos = getRepositories();
  // doc_mount_blobs has hand-written DDL (BLOB column) — trigger CREATE via a read.
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');

  // 1. The owner + the stranger (the save leg's ownership arm).
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.users.create(
    { username: 'stranger', email: null, name: 'Stranger' } as never,
    { id: spec.otherUserId, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Two characters — each create MINTS a vault whose storeType is genuinely
  //    'character' (the Vault-vs-Album badge reads it).
  await repos.characters.create(
    { name: 'Aria', userId: spec.userId, controlledBy: 'llm' } as never,
    { id: ARIA, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.characters.create(
    { name: 'Bramwell', userId: spec.userId, controlledBy: 'llm' } as never,
    { id: BRAMWELL, createdAt: TS, updatedAt: TS } as never,
  );
  const ariaVault = (await repos.characters.findByIdRaw(ARIA))
    ?.characterDocumentMountPointId as string | null;
  const bramVault = (await repos.characters.findByIdRaw(BRAMWELL))
    ?.characterDocumentMountPointId as string | null;
  if (!ariaVault || !bramVault) throw new Error('character vault(s) not minted');

  // 3. The three non-vault mounts. DISABLED_MP is `enabled: false` — the
  //    `findEnabled` arm.
  for (const [id, name, enabled] of [
    [UPLOADS_MP, 'Quilltap Uploads', true],
    [PROJECT_MP, 'The Lantern Project', true],
    [DISABLED_MP, 'Retired Album', false],
  ] as Array<[string, string, boolean]>) {
    await repos.docMountPoints.create(
      {
        name,
        basePath: '',
        mountType: 'database',
        storeType: 'documents',
        includePatterns: [],
        excludePatterns: [],
        enabled,
        lastScannedAt: null,
        scanStatus: 'idle',
        lastScanError: null,
        conversionStatus: 'idle',
        conversionError: null,
        fileCount: 0,
        chunkCount: 0,
        totalSizeBytes: 0,
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  }
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'userUploadsMountPointId',
    UPLOADS_MP,
  ]);

  // 4. The photo corpus. Ordered so the WALK order (rowid) differs from the
  //    createdAt order — the dedup tie-break must be reading createdAt, not the
  //    walk, and a fixture where the two agree could not tell the difference.
  const photos: PhotoSpec[] = [
    {
      key: 'A',
      tag: 'dawn-flight',
      mounts: [
        // Uploads FIRST in the walk, but OLDEST — so the primary is the vault link.
        { mp: () => UPLOADS_MP, path: 'photos/dawn-flight-copy.webp', at: '2026-02-01T00:00:01.000Z' },
        { mp: () => ariaVault, path: 'photos/dawn-flight.webp', at: '2026-02-01T00:00:05.000Z' },
        { mp: () => bramVault, path: 'photos/dawn-flight.webp', at: '2026-02-01T00:00:03.000Z' },
      ],
      caption: 'Dawn flight over the Lantern District',
      tags: ['portrait', 'sky'],
      generationPrompt: 'An aeronaut catching the first light above a district of brass lanterns.',
      generationRevisedPrompt: null,
      embedding: 'dawn',
    },
    {
      key: 'B',
      tag: 'covenant-vigil',
      mounts: [{ mp: () => UPLOADS_MP, path: 'photos/covenant-vigil.webp', at: '2026-02-01T00:00:04.000Z' }],
      caption: 'The covenant vigil',
      tags: ['covenant', 'vigil'],
      generationPrompt: 'A vigil kept beneath a covenant stone, rendered in candlelight.',
      generationRevisedPrompt: 'A solemn vigil beneath a weathered covenant stone, candlelit.',
      embedding: 'covenant',
    },
    {
      key: 'C',
      tag: 'wardrobe-study',
      mounts: [{ mp: () => PROJECT_MP, path: 'photos/wardrobe-study.webp', at: '2026-02-01T00:00:02.000Z' }],
      caption: null,
      tags: [],
      generationPrompt: null,
      generationRevisedPrompt: null,
      embedding: 'wardrobe',
    },
    {
      key: 'D',
      tag: 'lone-lantern',
      mounts: [{ mp: () => ariaVault, path: 'photos/lone-lantern.webp', at: '2026-02-01T00:00:00.000Z' }],
      caption: 'A lone lantern',
      tags: ['lantern'],
      generationPrompt: 'One lantern burning at the end of a long pier.',
      generationRevisedPrompt: null,
      embedding: 'lantern',
    },
    {
      key: 'E',
      tag: 'hidden-plate',
      mounts: [{ mp: () => DISABLED_MP, path: 'photos/hidden-plate.webp', at: '2026-02-01T00:00:06.000Z' }],
      caption: 'A retired plate',
      tags: ['retired'],
      generationPrompt: 'A photographic plate retired to a disabled album.',
      generationRevisedPrompt: null,
      embedding: 'lantern',
    },
  ];

  /** linkId → the PINNED createdAt it must carry (applied in step 8). */
  const pinned: Array<[string, string]> = [];
  const linkIds: Record<string, string[]> = {};
  const shas: Record<string, string> = {};
  const chunkRows: Array<{ linkId: string; mountPointId: string; embedding: number[] }> = [];

  for (const photo of photos) {
    const bytes = imageBytes(photo.tag);
    const sha = sha256OfBuffer(bytes);
    shas[photo.key] = sha;
    linkIds[photo.key] = [];
    for (const target of photo.mounts) {
      const mountPointId = target.mp();
      const markdown = buildKeptImageMarkdown({
        generationPrompt: photo.generationPrompt,
        generationRevisedPrompt: photo.generationRevisedPrompt,
        generationModel: 'fixture-model',
        sceneState: null,
        linkedByName: 'You',
        linkedById: spec.userId,
        linkedByRole: 'user',
        tags: photo.tags,
        caption: photo.caption,
        keptAt: target.at,
      });
      const folderId = await ensureFolderPath(mountPointId, 'photos');
      const { link } = await repos.docMountFileLinks.linkBlobContent({
        mountPointId,
        relativePath: target.path,
        fileName: basenameOfRelativePath(target.path),
        folderId,
        originalFileName: `${photo.tag}.png`,
        originalMimeType: 'image/png',
        storedMimeType: 'image/png',
        sha256: sha,
        data: bytes,
        description: photo.caption ?? '',
        extractedText: markdown,
        extractedTextSha256: sha256OfString(markdown),
        extractionStatus: 'converted',
      } as never);
      const linkId = (link as { id: string }).id;
      linkIds[photo.key].push(linkId);
      pinned.push([linkId, target.at]);
      chunkRows.push({
        linkId,
        mountPointId,
        embedding: spec.chunkEmbeddings[photo.embedding],
      });
    }
  }

  // 4b. The NON-photos link on sha A: same bytes, a `notes/` path in Aria's
  //     vault. It must not become a second gallery entry, must still show in the
  //     linkSummary, and is the delete route's "not a gallery entry" subject.
  const notesAt = '2026-02-01T00:00:07.000Z';
  const notesMarkdown = buildKeptImageMarkdown({
    generationPrompt: 'An aeronaut catching the first light above a district of brass lanterns.',
    generationRevisedPrompt: null,
    generationModel: 'fixture-model',
    sceneState: null,
    linkedByName: 'Aria',
    linkedById: ARIA,
    linkedByRole: 'character',
    tags: ['reference'],
    caption: 'Filed as reference, not as an album photo',
    keptAt: notesAt,
  });
  const notesBytes = imageBytes('dawn-flight');
  const { link: notesLink } = await repos.docMountFileLinks.linkBlobContent({
    mountPointId: ariaVault,
    relativePath: 'notes/reference.md',
    fileName: 'reference.md',
    folderId: await ensureFolderPath(ariaVault, 'notes'),
    originalFileName: 'reference.png',
    originalMimeType: 'image/png',
    storedMimeType: 'image/png',
    sha256: sha256OfBuffer(notesBytes),
    data: notesBytes,
    description: '',
    extractedText: notesMarkdown,
    extractedTextSha256: sha256OfString(notesMarkdown),
    extractionStatus: 'converted',
  } as never);
  const notesLinkId = (notesLink as { id: string }).id;
  pinned.push([notesLinkId, notesAt]);

  // 5. The chunks the semantic branch searches — one per photo link, embedding
  //    PINNED from the spec (a plain number[]; the repo writes the Float32 BLOB).
  let chunkSeq = 0;
  for (const row of chunkRows) {
    await repos.docMountChunks.create(
      {
        linkId: row.linkId,
        mountPointId: row.mountPointId,
        chunkIndex: 0,
        content: `Chunk for ${row.linkId}`,
        tokenCount: 8,
        headingContext: null,
        embedding: row.embedding,
      } as never,
      {
        id: `c0000000-0000-4000-8000-${String(++chunkSeq).padStart(12, '0')}`,
        createdAt: TS,
        updatedAt: TS,
      } as never,
    );
  }

  // 6. The save leg's `files` rows. SAVE_OK / SAVE_DUPE go through the REAL
  //    ingest so `fileStorageManager.downloadFile` can read them back — the
  //    oracle drives the unmocked storage manager, and the Rust canned
  //    FileBytesStore replays the STORED bytes the oracle reports.
  const okIngest = await ingestImageBuffer({
    buffer: imageBytes('fresh-arrival'),
    originalFilename: 'fresh-arrival.png',
    mimeType: 'image/png',
    userId: spec.userId,
  });
  const dupeIngest = await ingestImageBuffer({
    buffer: imageBytes('dawn-flight'), // the SAME bytes as sha A → "already saved"
    originalFilename: 'dawn-flight-again.png',
    mimeType: 'image/png',
    userId: spec.userId,
  });
  const okBytes = await readImageBuffer(okIngest.id);
  const dupeBytes = await readImageBuffer(dupeIngest.id);

  await repos.files.create(
    {
      userId: spec.userId,
      linkedTo: [],
      tags: [],
      sha256: 'cccc3333cccc3333cccc3333cccc3333cccc3333cccc3333cccc3333cccc3333',
      originalFilename: 'ledger.txt',
      mimeType: 'text/plain',
      size: 42,
      source: 'UPLOADED',
      category: 'ATTACHMENT',
      storageKey: `${spec.userId}/ledger.txt`,
    } as never,
    { id: SAVE_NOT_IMG, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.files.create(
    {
      userId: spec.otherUserId,
      linkedTo: [],
      tags: [],
      sha256: 'dddd4444dddd4444dddd4444dddd4444dddd4444dddd4444dddd4444dddd4444',
      originalFilename: 'stranger.png',
      mimeType: 'image/png',
      size: 128,
      source: 'UPLOADED',
      category: 'IMAGE',
      storageKey: `${spec.otherUserId}/stranger.png`,
    } as never,
    { id: SAVE_NOT_OWN, createdAt: TS, updatedAt: TS } as never,
  );

  // 6b. The image-info rows (P4.9a2). IMG_LINKED reuses sha A, so the route's
  //     characterGalleryLinks walk resolves the SAME mount links staged in
  //     step 4: Aria's + Bramwell's vault `photos/` links pass the
  //     `character && isPhotoAlbum` gate; the Uploads copy (storeType
  //     'documents') and the `notes/` link (not an album path) must NOT.
  await repos.files.create(
    {
      userId: spec.userId,
      linkedTo: [],
      // ARIA is a character id → tagType CHARACTER; THEME_TAG is not → THEME.
      tags: [ARIA, THEME_TAG],
      sha256: shas.A,
      originalFilename: 'dawn-flight.webp',
      mimeType: 'image/webp',
      size: 2048,
      width: 1024,
      height: 768,
      source: 'GENERATED',
      category: 'IMAGE',
      generationPrompt: 'An aeronaut catching the first light above a district of brass lanterns.',
      generationModel: 'fixture-model',
    } as never,
    { id: IMG_LINKED, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.files.create(
    {
      userId: spec.userId,
      linkedTo: [],
      tags: [THEME_TAG],
      sha256: shas.B,
      originalFilename: 'covenant-vigil.webp',
      mimeType: 'image/webp',
      size: 512,
      source: 'IMPORTED',
      category: 'IMAGE',
    } as never,
    { id: IMG_DOC, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.files.create(
    {
      userId: spec.userId,
      linkedTo: [],
      tags: [],
      sha256: 'eeee5555eeee5555eeee5555eeee5555eeee5555eeee5555eeee5555eeee5555',
      originalFilename: 'plain-upload.png',
      mimeType: 'image/png',
      size: 100,
      source: 'UPLOADED',
      category: 'IMAGE',
    } as never,
    { id: IMG_PLAIN, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.files.create(
    {
      userId: spec.userId,
      linkedTo: [],
      tags: [],
      sha256: 'aaaa7777aaaa7777aaaa7777aaaa7777aaaa7777aaaa7777aaaa7777aaaa7777',
      originalFilename: 'system-avatar.png',
      mimeType: 'image/png',
      size: 256,
      width: 256,
      height: 256,
      source: 'SYSTEM',
      category: 'AVATAR',
    } as never,
    { id: IMG_AVATAR, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.files.create(
    {
      userId: spec.userId,
      linkedTo: [],
      tags: [],
      sha256: 'bbbb8888bbbb8888bbbb8888bbbb8888bbbb8888bbbb8888bbbb8888bbbb8888',
      originalFilename: 'legacy-no-sha.png',
      mimeType: 'image/png',
      size: 64,
      source: 'UPLOADED',
      category: 'IMAGE',
    } as never,
    { id: IMG_NOSHA, createdAt: TS, updatedAt: TS } as never,
  );
  // The legacy-shaped row: blank the hash AFTER the Zod-validated create. An
  // empty string fails `z.string().length(64)` on the READ side, so v4's
  // `findById` (validate inside safeQuery's null-fallback) reports the row
  // ABSENT and the route 404s — the arm the differential pins.
  await rawQuery('UPDATE "files" SET "sha256" = ? WHERE "id" = ?', ['', IMG_NOSHA]);

  // 6c. The `_count` usage pair (route.ts:56-59): Aria uses IMG_LINKED as her
  //     default image (→ charactersUsingAsDefault 1); Bramwell overrides his
  //     avatar with it in two chats (→ chatAvatarOverrides 2). Raw UPDATEs so
  //     the characters' pinned `updatedAt` stamps survive.
  await rawQuery('UPDATE "characters" SET "defaultImageId" = ? WHERE "id" = ?', [
    IMG_LINKED,
    ARIA,
  ]);
  await rawQuery('UPDATE "characters" SET "avatarOverrides" = ? WHERE "id" = ?', [
    JSON.stringify([
      { chatId: OVERRIDE_CHAT_1, imageId: IMG_LINKED },
      { chatId: OVERRIDE_CHAT_2, imageId: IMG_LINKED },
    ]),
    BRAMWELL,
  ]);

  // 7. Force the empty tables the read path touches into existence (the salon
  //    fixture's lesson — the Rust side has no lazy CREATE TABLE).
  await ensureCollection('memories', (await import('@/lib/schemas/memory.types')).MemorySchema);

  // 8. Re-pin every link's timestamps: linkBlobContent stamps a LIVE clock and
  //    the dedup tie-break reads `createdAt` directly.
  for (const [linkId, at] of pinned) {
    midb
      .prepare('UPDATE "doc_mount_file_links" SET "createdAt" = ?, "updatedAt" = ? WHERE "id" = ?')
      .run(at, at, linkId);
  }
  // The doc_mount_files rows carry live stamps too; they don't reach the wire,
  // but pinning them keeps a byte-diff of the fixture itself reproducible.
  midb.prepare('UPDATE "doc_mount_files" SET "createdAt" = ?, "updatedAt" = ?').run(TS, TS);

  const meta = {
    ariaVault,
    bramVault,
    uploadsMp: UPLOADS_MP,
    projectMp: PROJECT_MP,
    disabledMp: DISABLED_MP,
    linkIds,
    shas,
    notesLinkId,
    saveOkFileId: okIngest.id,
    saveOkBytes: Array.from(okBytes),
    saveDupeFileId: dupeIngest.id,
    saveDupeBytes: Array.from(dupeBytes),
    saveNotImageFileId: SAVE_NOT_IMG,
    saveNotOwnedFileId: SAVE_NOT_OWN,
    missingFileId: '00000000-0000-4000-8000-00000000ffff',
    imageInfoLinkedId: IMG_LINKED,
    imageInfoDocLinksId: IMG_DOC,
    imageInfoPlainId: IMG_PLAIN,
    imageInfoAvatarId: IMG_AVATAR,
    imageInfoNoShaId: IMG_NOSHA,
  };
  writeFileSync(mainOut + '.meta.json', JSON.stringify(meta));

  closeMountIndexSQLiteClient();
  await closeDatabase();

  // The raw UPDATEs leave zero-byte TRUNCATE-journal files next to the outputs;
  // only the .db files are the committed fixture.
  for (const out of [mainOut, mountOut]) {
    const j = out + '-journal';
    if (existsSync(j)) rmSync(j);
  }

  process.stderr.write(
    `built photos fixture: ${mainOut} (+${mountOut}) — ${photos.length} images, ` +
      `${pinned.length} links, ${chunkRows.length} chunks; vaults ${ariaVault} / ${bramVault}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`photos fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
