/**
 * P4.73 — the fixture builder for `images_routes_equivalence` (v4's
 * `/api/v1/images` COLLECTION route + the `[id]` DELETE arm).
 *
 * No existing committed pair carries what this family needs at once: the
 * measurement at ordering found tagged IMAGE rows only in the photos fixture, a
 * provisioned **Lantern Backgrounds** store only in the image-generation and
 * story-background ones, and a **Quilltap Uploads** store in a third set — and
 * NONE has characters whose `defaultImageId` / `avatarOverrides` point at those
 * images, without which the list's `_count` pair and the DELETE's in-use refusal
 * are both vacuous. So this builder is trimmed to THIS family's question rather
 * than grown from a sibling (memory note
 * `a-new-fixture-built-from-a-grown-shared-fixture`).
 *
 * Bakes, through v4's REAL repositories (every id and timestamp pinned, so both
 * sides read byte-identical rows and only the ingest/generate mutations mint):
 *
 *   1. TWO users — USER_A (= v5 `SINGLE_USER_ID`, the owner) and USER_B (the
 *      ownership arm the list must exclude).
 *   2. Characters: CHAR_DEF (`defaultImageId` → F_INUSE), CHAR_OVR
 *      (`avatarOverrides` → F_INUSE twice, in two chats — so the count is 2 and
 *      not 1), and CHAR_TAGGED, whose ID is used as a TAG on F_TAGGED so the
 *      list's derived `CHARACTER` tagType has something to derive.
 *   3. IMAGE files covering every list arm: tagged / untagged, an IMPORTED row
 *      whose `description` is the `url` the projection echoes, a GENERATED row
 *      carrying `generationPrompt`/`generationModel`, a row with NO `storageKey`
 *      (the `filepath` falls back to the bare filename), a USER_B row, a
 *      non-IMAGE row (excluded by category), and F_BADSHA — a row whose `sha256`
 *      is 8 chars, which v4's `findByFilter` DROPS via `FileEntrySchema`, so the
 *      list must not show it.
 *   4. Real stored bytes: F_INUSE and F_ORPHAN both look identical in `files`,
 *      but only F_INUSE's `storageKey` resolves to a real `doc_mount_blobs` row.
 *      That is the whole discriminator for the DELETE route's orphan branch —
 *      one is refused `Image is in use`, the other cleans up the references and
 *      deletes.
 *   5. Both stores: "Quilltap Uploads" (the ingest target, `images/`) and
 *      "Lantern Backgrounds" (the generate target, `tool/`), each with its
 *      `instance_settings` pointer.
 *   6. Connection profiles + synthetic api keys for the generate leg, and one
 *      per-user `chat_settings` row the corpus patches per case.
 *
 * SAFETY: every `key_value` is a SYNTHETIC placeholder — never a real API key.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_IMGCOL_MAIN=/tmp/qt-imgcol-main.db
 *   QT_FIXTURE_IMGCOL_MOUNT=/tmp/qt-imgcol-mount.db
 *   QT_FIXTURE_IMGCOL_MAIN=$QT_FIXTURE_IMGCOL_MAIN QT_FIXTURE_IMGCOL_MOUNT=$QT_FIXTURE_IMGCOL_MOUNT \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-images-collection-fixture.ts
 */

import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface ProfileSpec {
  id: string;
  name: string;
  provider: string;
  modelName: string;
  isDefault: boolean;
  isDangerousCompatible: boolean;
  apiKeyId?: string;
  tags: string[];
}
interface KeySpec {
  id: string;
  provider: string;
  name: string;
  keyValue: string;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  userIdB: string;
  user: Record<string, unknown>;
  userB: Record<string, unknown>;
  uploadsMountPointId: string;
  lanternMountPointId: string;
  connectionProfiles: ProfileSpec[];
  apiKeys: KeySpec[];
  chatSettings: { id: string; dangerousContent: Record<string, unknown> };
}

// ── Pinned entity ids (shared verbatim with the Rust differential) ──────────
const CHAR_DEF = 'c1000000-0000-4000-8000-000000000001'; // defaultImageId → F_INUSE
const CHAR_OVR = 'c1000000-0000-4000-8000-000000000002'; // avatarOverrides → F_INUSE ×2
const CHAR_TAG = 'c1000000-0000-4000-8000-000000000003'; // used as a TAG id (→ tagType CHARACTER)

const CHAT_1 = 'cc000000-0000-4000-8000-000000000001';
const CHAT_2 = 'cc000000-0000-4000-8000-000000000002';

const THEME_TAG = 'ee000000-0000-4000-8000-000000000001'; // not a character → tagType THEME

// IMAGE rows, newest first by `createdAt` (the list sorts DESC).
const F_TAGGED = 'f0000000-0000-4000-8000-000000000001'; // tags [CHAR_TAG, THEME_TAG]
const F_IMPORTED = 'f0000000-0000-4000-8000-000000000002'; // IMPORTED, description = the url
const F_GENERATED = 'f0000000-0000-4000-8000-000000000003'; // GENERATED, prompt + model
const F_NOKEY = 'f0000000-0000-4000-8000-000000000004'; // storageKey NULL → filepath = filename
const F_INUSE = 'f0000000-0000-4000-8000-000000000005'; // real bytes; referenced by both chars
const F_ORPHAN = 'f0000000-0000-4000-8000-000000000006'; // dangling key; referenced by CHAR_DEF2
const F_PLAIN = 'f0000000-0000-4000-8000-000000000007'; // untagged, unreferenced (the happy delete)
const F_BADSHA = 'f0000000-0000-4000-8000-000000000008'; // sha256 too short → schema-dropped
const F_DOC = 'f0000000-0000-4000-8000-000000000009'; // category DOCUMENT → not an image
const F_USERB = 'f0000000-0000-4000-8000-0000000000b1'; // USER_B's image → excluded
// storageKey NULL *and* referenced. v4 skips the storage probe entirely when
// there is no key (`[id]/route.ts:150`), leaving `fileExists` FALSE — so this
// row takes the ORPHAN branch and deletes, where a port that treated "no key"
// as "bytes present" would refuse it. Without this row that whole branch of
// the probe is unmeasurable (a mutation flipping it stayed green).
const F_NOKEY_INUSE = 'f0000000-0000-4000-8000-00000000000a';

const CHAR_DEF2 = 'c1000000-0000-4000-8000-000000000004'; // defaultImageId → F_ORPHAN
const CHAR_NOKEY = 'c1000000-0000-4000-8000-000000000005'; // defaultImageId → F_NOKEY_INUSE

// ── P4.76 (the P4.73 review's item (a)): the ARCHIVED-character delete arm ──
// The `{id}` DELETE's orphan cleanup calls `repos.characters.update`, whose
// `validateCharacterArchivePatch` THROWS `CharacterArchivedError` on any patch
// to an archived character that is not the single-key unarchive. `update` uses
// `safeQuery`'s NO-fallback overload, so the throw reaches the route's outer
// catch → `serverError('Failed to delete image')`.
//
// The pair below is what makes that measurable rather than assumed: F_ORPHAN_ARCH
// is referenced by a NORMAL character (created FIRST, so `findByUserId` sees it
// first) and by the ARCHIVED one. v4 clears every `defaultImageId` before it
// touches any `avatarOverrides`, so the run commits the peer's cleared
// `defaultImageId`, throws on the archived row, and NEVER reaches the override
// loop — leaving the peer's override pointing at the image the request tried to
// delete. Without the peer the arm would only prove "it 500s".
const F_ORPHAN_ARCH = 'f0000000-0000-4000-8000-00000000000c';
const CHAR_ARCH_PEER = 'c1000000-0000-4000-8000-000000000006'; // normal; default + override → F_ORPHAN_ARCH
const CHAR_ARCHIVED = 'c1000000-0000-4000-8000-000000000007'; // archivedAt set; default → F_ORPHAN_ARCH

/** A dangling `mount-blob:` storage key (no such blob → the bytes are gone). */
function danglingBlobKey(uploads: string, tag: string): string {
  return `mount-blob:${uploads}:deadbeef-0000-4000-8000-${tag.padStart(12, '0')}`;
}

function sha(tag: string): string {
  return (tag.repeat(64) + '0'.repeat(64)).slice(0, 64);
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'images-collection.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_IMGCOL_MAIN;
  const mountOut = process.env.QT_FIXTURE_IMGCOL_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error(
      'QT_FIXTURE_IMGCOL_MAIN and QT_FIXTURE_IMGCOL_MOUNT must point at the .db files',
    );
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-imgcol-fixture-'));
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
  const {
    CharacterSchema,
    ChatMetadataSchema,
    FileEntrySchema,
    BackgroundJobSchema,
  } = await import('@/lib/schemas/types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { ApiKeySchema } = await import('@/lib/schemas/profile.types');
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

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('chats', ChatMetadataSchema);
  await ensureCollection('files', FileEntrySchema);
  await ensureCollection('background_jobs', BackgroundJobSchema);
  await ensureCollection('api_keys', ApiKeySchema);

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
  // doc_mount_blobs has hand-written DDL (its `data BLOB` column is deliberately
  // absent from the schema, so generateDDL omits it). Touch it once so the repo
  // creates its own correct table — the ingest + generate writes need it, and the
  // DELETE route's mount-blob existence probe READS it (without the table, v5
  // errors "no such table" where v4's catch reads it as absent).
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');

  const TS = spec.seedTimestamp;

  // 1. Users.
  await repos.users.create(spec.user as never, {
    id: spec.userId,
    createdAt: TS,
    updatedAt: TS,
  } as never);
  await repos.users.create(spec.userB as never, {
    id: spec.userIdB,
    createdAt: TS,
    updatedAt: TS,
  } as never);

  // 2. The two document stores + their instance_settings pointers.
  const mkDbStore = async (id: string, name: string) =>
    repos.docMountPoints.create(
      {
        name,
        basePath: '',
        mountType: 'database',
        storeType: 'documents',
        includePatterns: [],
        excludePatterns: [],
        enabled: true,
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
  await mkDbStore(spec.uploadsMountPointId, 'Quilltap Uploads');
  await mkDbStore(spec.lanternMountPointId, 'Lantern Backgrounds');
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  for (const [k, v] of [
    ['userUploadsMountPointId', spec.uploadsMountPointId],
    ['lanternBackgroundsMountPointId', spec.lanternMountPointId],
  ] as Array<[string, string]>) {
    await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
      k,
      v,
    ]);
  }

  // 3. The one image whose bytes really exist — F_INUSE. Written through the
  //    REAL link_blob_content so the mount-index rows are production-shaped;
  //    its minted blob id is echoed to the sidecar and pinned into the storage
  //    key below.
  const inUseBytes = Buffer.from('RIFFfixture-webp-bytes-for-F_INUSE', 'utf8');
  const written = await repos.docMountFileLinks.linkBlobContent({
    mountPointId: spec.uploadsMountPointId,
    relativePath: 'images/in-use.webp',
    fileName: 'in-use.webp',
    folderId: null,
    fileType: 'blob',
    originalFileName: 'in-use.webp',
    originalMimeType: 'image/webp',
    storedMimeType: 'image/webp',
    sha256: sha('5'),
    description: '',
    data: inUseBytes,
  } as never);
  const inUseBlobId = (written as { blobId?: string }).blobId;
  if (!inUseBlobId) throw new Error('F_INUSE blob id not minted');
  const inUseStorageKey = `mount-blob:${spec.uploadsMountPointId}:${inUseBlobId}`;
  // The repository content-addresses the blob itself, so the stored sha is the
  // sha of the BYTES, not whatever was passed in. F_INUSE's `files` row must
  // carry that same value or the "the row's sha names the bytes we stored"
  // invariant — which the differential asserts within each tree — would be
  // false for the one seeded row that has real bytes.
  const inUseSha = createHash('sha256').update(inUseBytes).digest('hex');

  // 4. Files.
  type FileSeed = {
    id: string;
    userId?: string;
    filename: string;
    createdAt: string;
    mimeType?: string;
    size?: number;
    width?: number | null;
    height?: number | null;
    source?: string;
    category?: string;
    storageKey?: string | null;
    linkedTo?: string[];
    tags?: string[];
    description?: string | null;
    generationPrompt?: string | null;
    generationModel?: string | null;
    shaTag: string;
    /** Overrides `sha(shaTag)` when the row must name real stored bytes. */
    sha256?: string;
  };
  const mkFile = async (f: FileSeed) => {
    await repos.files.create(
      {
        userId: f.userId ?? spec.userId,
        sha256: f.sha256 ?? sha(f.shaTag),
        originalFilename: f.filename,
        mimeType: f.mimeType ?? 'image/webp',
        size: f.size ?? 2048,
        source: f.source ?? 'UPLOADED',
        category: f.category ?? 'IMAGE',
        linkedTo: f.linkedTo ?? [],
        tags: f.tags ?? [],
        projectId: null,
        folderPath: null,
        storageKey: f.storageKey === undefined ? null : f.storageKey,
        fileStatus: 'ok',
        description: f.description ?? null,
        width: f.width === undefined ? 640 : f.width,
        height: f.height === undefined ? 480 : f.height,
        generationPrompt: f.generationPrompt ?? null,
        generationModel: f.generationModel ?? null,
      } as never,
      { id: f.id, createdAt: f.createdAt, updatedAt: f.createdAt } as never,
    );
  };

  await mkFile({
    id: F_TAGGED,
    filename: 'tagged.webp',
    createdAt: '2026-03-08T00:00:00.000Z',
    tags: [CHAR_TAG, THEME_TAG],
    storageKey: danglingBlobKey(spec.uploadsMountPointId, '1'),
    shaTag: '1',
  });
  await mkFile({
    id: F_IMPORTED,
    filename: 'imported.webp',
    createdAt: '2026-03-07T00:00:00.000Z',
    source: 'IMPORTED',
    description: 'Imported from https://example.invalid/pic.png',
    storageKey: danglingBlobKey(spec.uploadsMountPointId, '2'),
    shaTag: '2',
  });
  await mkFile({
    id: F_GENERATED,
    filename: 'generated.webp',
    createdAt: '2026-03-06T00:00:00.000Z',
    source: 'GENERATED',
    generationPrompt: 'a lantern in the fog',
    generationModel: 'gpt-image-1',
    storageKey: danglingBlobKey(spec.uploadsMountPointId, '3'),
    shaTag: '3',
  });
  // storageKey NULL → v4's `filepath` falls back to the bare originalFilename.
  await mkFile({
    id: F_NOKEY,
    filename: 'legacy-no-key.png',
    createdAt: '2026-03-05T00:00:00.000Z',
    mimeType: 'image/png',
    storageKey: null,
    width: null,
    height: null,
    shaTag: '4',
  });
  await mkFile({
    id: F_INUSE,
    filename: 'in-use.webp',
    createdAt: '2026-03-04T00:00:00.000Z',
    storageKey: inUseStorageKey,
    shaTag: '5',
    sha256: inUseSha,
  });
  await mkFile({
    id: F_ORPHAN,
    filename: 'orphan.webp',
    createdAt: '2026-03-03T00:00:00.000Z',
    storageKey: danglingBlobKey(spec.uploadsMountPointId, '6'),
    shaTag: '6',
  });
  await mkFile({
    id: F_PLAIN,
    filename: 'plain.webp',
    createdAt: '2026-03-02T00:00:00.000Z',
    storageKey: danglingBlobKey(spec.uploadsMountPointId, '7'),
    shaTag: '7',
  });
  await mkFile({
    id: F_NOKEY_INUSE,
    filename: 'no-key-in-use.png',
    createdAt: '2026-03-01T12:00:00.000Z',
    mimeType: 'image/png',
    storageKey: null,
    width: null,
    height: null,
    shaTag: 'a',
  });
  await mkFile({
    id: F_DOC,
    filename: 'notes.txt',
    createdAt: '2026-03-01T00:00:00.000Z',
    mimeType: 'text/plain',
    category: 'DOCUMENT',
    storageKey: danglingBlobKey(spec.uploadsMountPointId, '9'),
    width: null,
    height: null,
    shaTag: '9',
  });
  await mkFile({
    id: F_ORPHAN_ARCH,
    filename: 'orphan-archived.webp',
    createdAt: '2026-02-28T12:00:00.000Z',
    storageKey: danglingBlobKey(spec.uploadsMountPointId, 'c'),
    shaTag: 'c',
  });
  await mkFile({
    id: F_USERB,
    userId: spec.userIdB,
    filename: 'other-user.webp',
    createdAt: '2026-02-28T00:00:00.000Z',
    storageKey: danglingBlobKey(spec.uploadsMountPointId, 'b'),
    shaTag: 'b',
  });

  // F_BADSHA is inserted RAW: its 8-char `sha256` is what `FileEntrySchema`
  // rejects, and `repos.files.create` would refuse to write it. On disk it is a
  // perfectly ordinary IMAGE row — the list must still omit it, because v4's
  // `findByFilter` re-validates every row it reads and drops the failures
  // (`base.repository.ts:277-285`).
  await rawQuery(
    'INSERT INTO files (id, userId, sha256, originalFilename, mimeType, size, width, height, ' +
      'isPlainText, linkedTo, source, category, generationPrompt, generationModel, ' +
      'generationRevisedPrompt, description, tags, projectId, folderPath, storageKey, ' +
      'fileStatus, createdAt, updatedAt) ' +
      'VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)',
    [
      F_BADSHA,
      spec.userId,
      'deadbeef',
      'bad-sha.webp',
      'image/webp',
      2048,
      640,
      480,
      null,
      '[]',
      'UPLOADED',
      'IMAGE',
      null,
      null,
      null,
      null,
      '[]',
      null,
      null,
      danglingBlobKey(spec.uploadsMountPointId, '8'),
      'ok',
      '2026-02-27T00:00:00.000Z',
      '2026-02-27T00:00:00.000Z',
    ],
  );

  // 5. Chats (the avatar-override arm needs two, so the override COUNT is 2).
  for (const id of [CHAT_1, CHAT_2]) {
    await repos.chats.create(
      {
        userId: spec.userId,
        title: `Chat ${id.slice(-1)}`,
        participants: [],
        tags: [],
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
    await repos.chats.getMessages(id);
  }

  // 6. Characters. CHAR_TAG exists only so F_TAGGED's first tag id resolves to a
  //    character and the list's derived tagType reads CHARACTER for it and THEME
  //    for its sibling — the one thing that distinguishes the two branches.
  await repos.characters.create(
    {
      name: 'Default Owner',
      userId: spec.userId,
      controlledBy: 'llm',
      defaultImageId: F_INUSE,
    } as never,
    { id: CHAR_DEF, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.characters.create(
    {
      name: 'Override Owner',
      userId: spec.userId,
      controlledBy: 'llm',
      avatarOverrides: [
        { chatId: CHAT_1, imageId: F_INUSE },
        { chatId: CHAT_2, imageId: F_INUSE },
      ],
    } as never,
    { id: CHAR_OVR, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.characters.create(
    { name: 'Tag Target', userId: spec.userId, controlledBy: 'llm' } as never,
    { id: CHAR_TAG, createdAt: TS, updatedAt: TS } as never,
  );
  // CHAR_DEF2 references F_ORPHAN through BOTH mechanisms, so the DELETE
  // route's orphan-cleanup arm exercises both of its branches: clearing
  // `defaultImageId` AND filtering `avatarOverrides`. With only the first, the
  // override rewrite would never run and the arm would be half vacuous. The
  // second override (on F_INUSE) must SURVIVE the filter — otherwise a port
  // that cleared the whole array would pass.
  await repos.characters.create(
    {
      name: 'Orphan Owner',
      userId: spec.userId,
      controlledBy: 'llm',
      defaultImageId: F_ORPHAN,
      avatarOverrides: [
        { chatId: CHAT_1, imageId: F_ORPHAN },
        { chatId: CHAT_2, imageId: F_INUSE },
      ],
    } as never,
    { id: CHAR_DEF2, createdAt: TS, updatedAt: TS } as never,
  );

  await repos.characters.create(
    {
      name: 'Key-less Owner',
      userId: spec.userId,
      controlledBy: 'llm',
      defaultImageId: F_NOKEY_INUSE,
    } as never,
    { id: CHAR_NOKEY, createdAt: TS, updatedAt: TS } as never,
  );

  // 6b. P4.76 item (a): the archived-character delete arm (see the constants).
  await repos.characters.create(
    {
      name: 'Archive Peer',
      userId: spec.userId,
      controlledBy: 'llm',
      defaultImageId: F_ORPHAN_ARCH,
      avatarOverrides: [{ chatId: CHAT_1, imageId: F_ORPHAN_ARCH }],
    } as never,
    { id: CHAR_ARCH_PEER, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.characters.create(
    {
      name: 'Archived Owner',
      userId: spec.userId,
      controlledBy: 'llm',
      defaultImageId: F_ORPHAN_ARCH,
    } as never,
    { id: CHAR_ARCHIVED, createdAt: TS, updatedAt: TS } as never,
  );
  // Archived by RAW update: `repos.characters.update` would run the very guard
  // this row exists to arm, and archiving through the service would prune the
  // vault (§4.2a) — irrelevant here and slow. The column is the whole fact.
  await rawQuery('UPDATE characters SET archivedAt = ? WHERE id = ?', [
    '2026-04-01T00:00:00.000Z',
    CHAR_ARCHIVED,
  ]);

  // 7. Connection profiles + their SYNTHETIC api keys (pinned ids — a minted
  //    `createApiKey` id would differ between the two fixture copies).
  for (const k of spec.apiKeys) {
    await rawQuery(
      'INSERT INTO api_keys (id, userId, label, provider, key_value, isActive, lastUsed, createdAt, updatedAt) ' +
        'VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)',
      [k.id, spec.userId, k.name, k.provider, k.keyValue, 1, null, TS, TS],
    );
  }
  for (const p of spec.connectionProfiles) {
    const { id, ...data } = p;
    await repos.connections.create({ userId: spec.userId, ...data } as never, {
      id,
      createdAt: TS,
      updatedAt: TS,
    } as never);
  }

  // 8. The per-user chat_settings row (the corpus patches `dangerousContent`
  //    per case rather than seeding a second user).
  await repos.chatSettings.create(
    { userId: spec.userId, dangerousContent: spec.chatSettings.dangerousContent } as never,
    { id: spec.chatSettings.id, createdAt: TS, updatedAt: TS } as never,
  );

  closeMountIndexSQLiteClient();
  await closeDatabase();

  // The one value neither side can pin in the spec: the blob id the REAL
  // link_blob_content minted for F_INUSE (its storage key is what makes the
  // "the bytes still exist" probe answer true).
  writeFileSync(mainOut + '.meta.json', JSON.stringify({ inUseBlobId, inUseStorageKey }));
  process.stderr.write(
    `built images-collection fixtures: main=${mainOut} mount=${mountOut} inUseBlobId=${inUseBlobId}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`images-collection fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
