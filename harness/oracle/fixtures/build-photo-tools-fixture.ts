/**
 * Fixture builder for the photo-trio tool-handler differential (W4.9b; v4
 * `lib/tools/handlers/doc-edit/photo-handlers.ts` — keep_image / list_images /
 * attach_image).
 *
 * Bakes ONE shared starting state across the TWO databases (main + mount-index)
 * so BOTH v4 and the Rust port read/mutate a copy of byte-identical vault state;
 * each oracle/harness case runs on its own fresh copy (keep_image WRITES). Steps:
 *
 *   1. Two characters (Aurora, Basil — BOTH `systemTransparency: true`) with full
 *      provisioned vaults, via the REAL repos.characters.create (each mints a
 *      linked vault mount). Their minted vault ids are echoed to the .meta.json.
 *   2. A "Quilltap Uploads" mount + the `instance_settings.userUploadsMountPointId`
 *      key, so `ingestImageBuffer` (→ writeUserUploadToMountStore) can store bytes.
 *   3. The spec.images ingested via the REAL ingestImageBuffer — which mints its
 *      OWN fileId (crypto.randomUUID; it does NOT honour img.fileId) and may
 *      transcode raster → WebP. So we map realFileIdByKey[key] = entry.id, patch
 *      the generation metadata onto the FileEntry (ingest drops those fields), and
 *      read the STORED bytes back (readImageBuffer) into realBytesByKey[key] so the
 *      Rust canned FileBytesStore returns EXACTLY the bytes v4 will hash.
 *   4. The spec.bakedPhotos baked via the REAL saveImageToAlbum (Date frozen to
 *      each entry's spec keptAt so the minted keptAt is pinned) into vault A|B.
 *      The returned {relativePath, linkId, keptAt} → bakedByKey[imageKey].
 *   5. Each baked photo's markdown chunks have their embedding overwritten to
 *      spec.photoChunkVectors["<vault>:<imageKey>"] (docMountChunks.updateEmbedding),
 *      so the list_images semantic branch is deterministic (no model call).
 *   6. A chat (spec.chatId) with participants A/B + the user participant and
 *      spec.sceneState (so a keep snapshots the scene), plus a SECOND chat
 *      (spec.chatMalformedId) with spec.sceneStateMalformed (fails SceneStateSchema)
 *      to bank the malformed-scene placeholder path.
 *
 * The minted char-vault ids / real fileIds / stored bytes / baked link info are
 * echoed to a JSON sidecar (QT_FIXTURE_PHOTO_MAIN + '.meta.json') the oracle case
 * + the Rust harness both read.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_PHOTO_MAIN=/tmp/qt-photo-main.db QT_FIXTURE_PHOTO_MOUNT=/tmp/qt-photo-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-photo-tools-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface ImageSpec {
  key: string;
  fileId: string;
  bytes: number[];
  mimeType: string;
  originalFilename: string;
  generationPrompt: string;
  generationRevisedPrompt: string;
  generationModel: string;
  width: number;
  height: number;
}
interface BakedPhoto {
  vault: 'A' | 'B';
  imageKey: string;
  caption: string;
  tags: string[];
  keptAt: string;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  user: Record<string, unknown>;
  charAId: string;
  charBId: string;
  charA: { name: string };
  charB: { name: string };
  chatId: string;
  chatMalformedId: string;
  participantAId: string;
  participantBId: string;
  userParticipantId: string;
  sceneState: Record<string, unknown>;
  sceneStateMalformed: Record<string, unknown>;
  images: ImageSpec[];
  bakedPhotos: BakedPhoto[];
  cannedEmbeddings: Record<string, number[]>;
  cannedFailures: string[];
  photoChunkVectors: Record<string, number[]>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'photo-tools.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_PHOTO_MAIN;
  const mountOut = process.env.QT_FIXTURE_PHOTO_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_PHOTO_MAIN and QT_FIXTURE_PHOTO_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-photo-fixture-build-'));
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
  const { CharacterSchema, ChatMetadataSchema, FileEntrySchema } = await import('@/lib/schemas/types');
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
  const { ingestImageBuffer, readImageBuffer } = await import('@/lib/images-v2');
  const { saveImageToAlbum } = await import('@/lib/photos/save-image-to-album');
  const { sha256OfBuffer } = await import('@/lib/utils/sha256');

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('chats', ChatMetadataSchema);
  await ensureCollection('files', FileEntrySchema);

  // Materialize the mount-index DDL up front (repos.characters.create scaffolds
  // vaults through linkDocumentContent, which needs these tables; ingest +
  // saveImageToAlbum write the store tables directly). See
  // [[document-store-oracle-gotchas]].
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
  // doc_mount_blobs is hand-written by v4 (its `data BLOB` column is deliberately
  // absent from the schema, so generateDDL omits it). Touch it once so the repo
  // creates its own correct table (CREATE TABLE IF NOT EXISTS) — the blob store
  // linkBlobContent (saveImageToAlbum) needs it.
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');

  // 1. User.
  await repos.users.create(spec.user as never, {
    id: spec.userId,
    createdAt: TS,
    updatedAt: TS,
  } as never);

  // 2. Characters (BOTH transparent — a keep on B's vault is visible to A). Mints
  //    + links each vault.
  await repos.characters.create(
    { name: spec.charA.name, userId: spec.userId, systemTransparency: true, controlledBy: 'llm' } as never,
    { id: spec.charAId, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.characters.create(
    { name: spec.charB.name, userId: spec.userId, systemTransparency: true, controlledBy: 'llm' } as never,
    { id: spec.charBId, createdAt: TS, updatedAt: TS } as never,
  );
  const charARaw = await repos.characters.findByIdRaw(spec.charAId);
  const charBRaw = await repos.characters.findByIdRaw(spec.charBId);
  const charAVault = charARaw?.characterDocumentMountPointId as string | null;
  const charBVault = charBRaw?.characterDocumentMountPointId as string | null;
  if (!charAVault || !charBVault) throw new Error('character vault(s) not minted');
  const vaultForKey = (v: 'A' | 'B'): string => (v === 'A' ? charAVault : charBVault);
  const attributionForKey = (v: 'A' | 'B'): { name: string; id: string; role: 'character' } =>
    v === 'A'
      ? { name: spec.charA.name, id: spec.charAId, role: 'character' }
      : { name: spec.charB.name, id: spec.charBId, role: 'character' };

  // 3. Provision the "Quilltap Uploads" mount + settings key so ingestImageBuffer
  //    (→ writeUserUploadToMountStore) has somewhere to store bytes.
  const uploadsMountPointId = 'e0000000-0000-4000-8000-0000000000ff';
  await repos.docMountPoints.create(
    {
      name: 'Quilltap Uploads',
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
    { id: uploadsMountPointId, createdAt: TS, updatedAt: TS },
  );
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'userUploadsMountPointId',
    uploadsMountPointId,
  ]);

  // 4. Ingest the images. ingestImageBuffer mints its OWN fileId and (for raster
  //    inputs) transcodes to WebP — so record the REAL fileId + STORED bytes (read
  //    back) + the stored-byte sha, and patch the generation metadata (ingest drops
  //    those fields) so the baked photos' markdown carries them.
  const realFileIdByKey: Record<string, string> = {};
  const realBytesByKey: Record<string, number[]> = {};
  const realShaByKey: Record<string, string> = {};
  for (const img of spec.images) {
    const entry = await ingestImageBuffer({
      buffer: Buffer.from(img.bytes),
      originalFilename: img.originalFilename,
      mimeType: img.mimeType,
      userId: spec.userId,
    });
    await repos.files.update(entry.id, {
      generationPrompt: img.generationPrompt,
      generationRevisedPrompt: img.generationRevisedPrompt,
      generationModel: img.generationModel,
      width: img.width,
      height: img.height,
    } as never);
    const stored = await readImageBuffer(entry.id);
    realFileIdByKey[img.key] = entry.id;
    realBytesByKey[img.key] = Array.from(stored);
    realShaByKey[img.key] = sha256OfBuffer(stored);
    process.stderr.write(
      `ingested ${img.key}: fileId=${entry.id} storedBytes=${stored.length} sha=${realShaByKey[img.key].slice(0, 12)}…\n`,
    );
  }

  // 5. Bake the photos via the REAL saveImageToAlbum, freezing Date to each spec
  //    keptAt so the minted keptAt (new Date().toISOString()) is pinned.
  const bakedByKey: Record<string, { vault: 'A' | 'B'; relativePath: string; linkId: string; keptAt: string }> = {};
  const RealDate = Date;
  for (const baked of spec.bakedPhotos) {
    const fixedIso = baked.keptAt;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    global.Date = class extends RealDate {
      constructor(...a: unknown[]) {
        if (a.length === 0) {
          super(fixedIso);
        } else {
          // @ts-expect-error forward variadic args to the real Date ctor
          super(...a);
        }
      }
      static now(): number {
        return RealDate.parse(fixedIso);
      }
    } as unknown as DateConstructor;
    try {
      const saved = await saveImageToAlbum({
        mountPointId: vaultForKey(baked.vault),
        fileId: realFileIdByKey[baked.imageKey],
        caption: baked.caption,
        tags: baked.tags,
        chatId: null,
        attribution: attributionForKey(baked.vault),
      });
      bakedByKey[baked.imageKey] = {
        vault: baked.vault,
        relativePath: saved.relativePath,
        linkId: saved.linkId,
        keptAt: saved.keptAt,
      };
      process.stderr.write(
        `baked ${baked.vault}:${baked.imageKey}: link=${saved.linkId} path=${saved.relativePath} keptAt=${saved.keptAt}\n`,
      );
    } finally {
      global.Date = RealDate;
    }
  }

  // 6. Seed each baked photo's chunk embedding to its spec vector (list_images
  //    semantic branch). Resolve the link by (mount, relativePath) → its chunks.
  for (const baked of spec.bakedPhotos) {
    const vecKey = `${baked.vault}:${baked.imageKey}`;
    const vec = spec.photoChunkVectors[vecKey];
    if (!vec) throw new Error(`no photoChunkVectors entry for ${vecKey}`);
    const info = bakedByKey[baked.imageKey];
    const mountPointId = vaultForKey(baked.vault);
    const link = await repos.docMountFileLinks.findByMountPointAndPath(mountPointId, info.relativePath);
    if (!link) throw new Error(`no link for baked ${vecKey} at ${info.relativePath}`);
    const chunks = await repos.docMountChunks.findByLinkId(link.id);
    if (chunks.length === 0) throw new Error(`no chunks for baked ${vecKey} (link ${link.id})`);
    for (const chunk of chunks) {
      await repos.docMountChunks.updateEmbedding(chunk.id, new Float32Array(vec));
    }
    process.stderr.write(`seeded ${chunks.length} chunk vector(s) for ${vecKey}\n`);
  }

  // 7. Chats. The primary chat carries a valid sceneState (a keep snapshots it);
  //    the malformed chat carries an invalid sceneState (location: number) → the
  //    keep markdown emits the malformed-scene placeholder.
  const mkParticipant = (id: string, characterId: string | null, controlledBy: string) => ({
    id,
    type: 'CHARACTER',
    characterId,
    controlledBy,
    displayOrder: 0,
    isActive: true,
    status: 'active',
    hasHistoryAccess: false,
    createdAt: TS,
    updatedAt: TS,
  });
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Photo Tools Fixture',
      chatType: 'salon',
      avatarGenerationEnabled: false,
      allowCrossCharacterVaultReads: true,
      sceneState: spec.sceneState,
      // Two CHARACTER participants (both LLM). A `characterId: null` "user"
      // participant is rejected by ChatParticipant's `characterId: z.string()`
      // (see [[chat-participant-fixture-shape]]); the photo corpus doesn't need a
      // user participant — charA (acting) + charB (transparent peer, present) drive
      // the Shared-Vaults peer visibility.
      participants: [
        mkParticipant(spec.participantAId, spec.charAId, 'llm'),
        mkParticipant(spec.participantBId, spec.charBId, 'llm'),
      ],
    } as never,
    { id: spec.chatId, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Photo Tools Fixture (malformed scene)',
      chatType: 'salon',
      avatarGenerationEnabled: false,
      allowCrossCharacterVaultReads: true,
      sceneState: spec.sceneStateMalformed,
      participants: [mkParticipant(spec.participantAId, spec.charAId, 'llm')],
    } as never,
    { id: spec.chatMalformedId, createdAt: TS, updatedAt: TS } as never,
  );

  closeMountIndexSQLiteClient();
  await closeDatabase();

  const meta = { charAVault, charBVault, realFileIdByKey, realBytesByKey, realShaByKey, bakedByKey };
  writeFileSync(mainOut + '.meta.json', JSON.stringify(meta));
  process.stderr.write(
    `built photo-tools fixtures: main=${mainOut} mount=${mountOut} charAVault=${charAVault} charBVault=${charBVault}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`photo-tools fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
