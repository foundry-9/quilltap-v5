/**
 * The committed avatar-rolls fixture builder (P4.D185 deliverable) — the
 * test-pepper substrate the `avatar_rolls_tier2_equivalence` differential and
 * its oracle case (`harness/oracle/cases/avatar-rolls-tier2.test.ts`) both read.
 *
 * Bakes, via v4's REAL repositories + store-write helpers (every id and
 * timestamp pinned so both sides read identical rows), one character's roll
 * collection covering every arm of `lib/photos/avatar-rolls-service.ts`:
 *
 *   1. Three characters. ROLF (vault) is the subject; his `defaultImageId` is
 *      the ALBUM LINK of a kept roll — the post-Phase-3 pointer shape, so
 *      `isPortrait` has to reach it through `albumLink.linkId` and not through
 *      `files.id`. SAGE (vault) owns one roll of her own (the tag scoping arm)
 *      and her `defaultImageId` is a LEGACY `files.id` — the other half of the
 *      portrait tolerance, and the arm `deleteAvatarRoll` clears. TALL has NO
 *      vault (his `characterDocumentMountPointId` is nulled after creation),
 *      which is the `has no linked database-backed vault` refusal.
 *   2. ROLF's five rolls, newest first by `createdAt`:
 *        `roll-new`     — one link, `images/history/`, not in the album. The
 *                         plain arm, and the one two chats are wearing.
 *        `roll-kept`    — TWO links over one blob: `images/history/` and the
 *                         album copy `photos/`. "Already kept" (the idempotent
 *                         save), the portrait, and the delete that must leave
 *                         the album copy standing.
 *        `roll-album`   — its ONLY link is the album copy. `rollLink` is null,
 *                         so the display URL falls to the album link and a
 *                         delete drops no link at all.
 *        `roll-nolink`  — a keyed `files` row whose sha is in no mount index:
 *                         both links null, so the URL falls back to
 *                         `buildLegacyFileUrl`.
 *        `roll-legacy`  — a PRE-VAULT roll: its link lives in a project store
 *                         under `character-avatars/`, and its `storageKey`
 *                         names that mount, which is what scopes `rollLink`
 *                         when a blob is linked from more than one mount.
 *      Plus two negatives: SAGE's roll (tagged to her, so ROLF's listing must
 *      not carry it), an unkeyed IMAGE tagged to ROLF (`generationKey` NULL —
 *      a picture, not a roll), and one roll tagged to the VAULT-LESS TALL, which
 *      is the only way to reach the `has no linked database-backed vault` rung
 *      of the error ladder (a vault-less character with no roll refuses one rung
 *      earlier, on the roll lookup).
 *   3. Three chats. Two wear `roll-new` through `characterAvatars` (so
 *      `usedInChatCount` is a COUNT, not a boolean, and the delete scrub has
 *      two rows to clear); the third wears nothing. ROLF also carries an
 *      `avatarOverrides` entry naming `roll-new` — the second pointer
 *      `deleteAvatarRoll` filters.
 *
 * The `generationKey`s are derived by v4's REAL `lib/wardrobe/avatar-cache.ts`
 * (`deriveAvatarCacheKey` for the v1 rows, `deriveLegacyAvatarCacheKey` for the
 * pre-cache `roll-legacy`), so the column holds the shapes production writes
 * rather than invented literals. Nothing in this lane READS the value — the
 * membership test is `generationKey IS NOT NULL` — but a fixture that lies
 * about its own shape is how a later reader learns the wrong thing.
 *
 * The minted vault mount ids + link ids are echoed to a JSON sidecar
 * (QT_FIXTURE_AR_MAIN + '.meta.json') that the oracle case and the Rust
 * differential both read.
 *
 * Regenerate (Node 24, from a v4 checkout pinned at the lane's target):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd /tmp/qt-v4-pin-<lane>-31436bae4
 *   TZ=UTC \
 *   QT_FIXTURE_AR_MAIN=$W/crates/quilltap-web/tests/fixtures/avatar-rolls-main.db \
 *   QT_FIXTURE_AR_MOUNT=$W/crates/quilltap-web/tests/fixtures/avatar-rolls-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-avatar-rolls-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  keptAt: string;
}

// ── Pinned entity ids (shared verbatim with the oracle case + the Rust test) ──
const ROLF = 'a1000000-0000-4000-8000-000000000001';
const SAGE = 'a1000000-0000-4000-8000-000000000002';
const TALL = 'a1000000-0000-4000-8000-000000000003';

/** A project store — where a pre-vault roll's bytes live. */
const PROJECT_MP = '83000000-0000-4000-8000-000000000001';

const CHAT_WEARING_A = 'c1000000-0000-4000-8000-000000000001';
const CHAT_WEARING_B = 'c1000000-0000-4000-8000-000000000002';
const CHAT_BARE = 'c1000000-0000-4000-8000-000000000003';

const F_ROLL_NEW = 'f1000000-0000-4000-8000-000000000001';
const F_ROLL_KEPT = 'f1000000-0000-4000-8000-000000000002';
const F_ROLL_ALBUM = 'f1000000-0000-4000-8000-000000000003';
const F_ROLL_NOLINK = 'f1000000-0000-4000-8000-000000000004';
const F_ROLL_LEGACY = 'f1000000-0000-4000-8000-000000000005';
const F_ROLL_SAGE = 'f1000000-0000-4000-8000-000000000006';
const F_NOT_ROLL = 'f1000000-0000-4000-8000-000000000007';
/** TALL's roll — keyed and tagged, but TALL has no vault, so the save legs
 *  reach the `has no linked database-backed vault` refusal instead of the
 *  roll-not-found one. Without it that ladder rung is unreachable. */
const F_ROLL_TALL = 'f1000000-0000-4000-8000-000000000008';

/** A tiny valid PNG, varied by one byte per image so every sha differs. */
function pngBytes(seed: number): Buffer {
  return Buffer.from([
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x62, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, seed & 0xff,
  ]);
}

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(
      `avatar-rolls fixture must be built under TZ=UTC (getTimezoneOffset=${offset})`,
    );
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'avatar-rolls.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_AR_MAIN;
  const mountOut = process.env.QT_FIXTURE_AR_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_AR_MAIN and QT_FIXTURE_AR_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }
  if (existsSync(mainOut + '.meta.json')) rmSync(mainOut + '.meta.json');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ar-fixture-'));
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
  } = await import('@/lib/schemas/mount-index.types');
  const { sha256OfBuffer } = await import('@/lib/utils/sha256');
  const { basenameOfRelativePath } = await import('@/lib/photos/keep-image-markdown');
  const { ensureFolderPath } = await import('@/lib/mount-index/folder-paths');
  const { buildMountBlobStorageKey } = await import('@/lib/file-storage/project-store-bridge');
  const { deriveAvatarCacheKey, deriveLegacyAvatarCacheKey } = await import(
    '@/lib/wardrobe/avatar-cache'
  );

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
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  const repos = getRepositories();
  // doc_mount_blobs has hand-written DDL (BLOB column) — trigger CREATE via a read.
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');

  // 1. The user.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. The cast. `create` provisions a character vault for each.
  const mkChar = async (id: string, name: string): Promise<void> => {
    await repos.characters.create(
      { name, userId: spec.userId, controlledBy: 'llm' } as never,
      { id, createdAt: `2026-04-0${id.slice(-1)}T00:00:00.000Z`, updatedAt: TS } as never,
    );
  };
  await mkChar(ROLF, 'Rolf');
  await mkChar(SAGE, 'Sage');
  await mkChar(TALL, 'Tall');

  const vaultOf = async (id: string): Promise<string> => {
    const raw = await repos.characters.findByIdRaw(id);
    const mp = raw?.characterDocumentMountPointId as string | null;
    if (!mp) throw new Error(`vault not minted for ${id}`);
    return mp;
  };
  const rolfVault = await vaultOf(ROLF);
  const sageVault = await vaultOf(SAGE);
  const tallVault = await vaultOf(TALL);

  // TALL has NO vault: drop the pointer AND the mount point, so
  // `getCharacterVaultStore` answers null and the save legs refuse.
  await rawQuery('UPDATE "characters" SET "characterDocumentMountPointId" = NULL WHERE "id" = ?', [
    TALL,
  ]);
  midb.prepare('DELETE FROM "doc_mount_file_links" WHERE "mountPointId" = ?').run(tallVault);
  midb.prepare('DELETE FROM "doc_mount_points" WHERE "id" = ?').run(tallVault);

  // 3. A project store — where the pre-vault roll's bytes live.
  await repos.docMountPoints.create(
    {
      name: 'Expedition Project',
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
    { id: PROJECT_MP, createdAt: TS, updatedAt: TS } as never,
  );

  // 4. The store writes. Each blob's sha256 comes from its BYTES, so two links
  //    over "one set of bytes" really are one blob.
  const linkIds: Record<string, string> = {};
  const shas: Record<string, string> = {};
  const blobIds: Record<string, string> = {};
  const writeBlob = async (
    key: string,
    mountPointId: string,
    relativePath: string,
    seed: number,
    createdAt: string,
  ): Promise<{ linkId: string; sha: string; blobId: string }> => {
    const bytes = pngBytes(seed);
    const sha = sha256OfBuffer(bytes);
    const folderId = await ensureFolderPath(
      mountPointId,
      relativePath.includes('/') ? relativePath.slice(0, relativePath.lastIndexOf('/')) : '',
    );
    const { link } = await repos.docMountFileLinks.linkBlobContent({
      mountPointId,
      relativePath,
      fileName: basenameOfRelativePath(relativePath),
      folderId,
      originalFileName: basenameOfRelativePath(relativePath),
      originalMimeType: 'image/webp',
      storedMimeType: 'image/webp',
      sha256: sha,
      data: bytes,
      description: '',
      extractedText: '',
      extractedTextSha256: null,
      extractionStatus: 'none',
    } as never);
    const linkId = (link as { id: string }).id;
    // The storage key names `doc_mount_blobs.id`, which is NOT the link's
    // `fileId` (that is the `doc_mount_files` row the blob hangs off).
    // `readMountBlob` reads `doc_mount_blobs` BY ID, so getting this wrong
    // makes every album save fail with "Mount-blob not found for storageKey".
    const blobRow = midb
      .prepare('SELECT "id" FROM "doc_mount_blobs" WHERE "fileId" = ?')
      .get((link as { fileId: string }).fileId) as { id: string } | undefined;
    if (!blobRow) throw new Error(`no blob row for ${relativePath}`);
    const blobId = blobRow.id;
    // Pin the live-clock stamps the store write mints.
    midb
      .prepare('UPDATE "doc_mount_file_links" SET "createdAt" = ?, "updatedAt" = ? WHERE "id" = ?')
      .run(createdAt, createdAt, linkId);
    linkIds[key] = linkId;
    shas[key] = sha;
    blobIds[key] = blobId;
    return { linkId, sha, blobId };
  };

  // `roll-new`: one link, in the history folder, never kept.
  const rollNew = await writeBlob(
    'rollNew',
    rolfVault,
    'images/history/roll-new.webp',
    1,
    '2026-05-03T00:00:00.000Z',
  );
  // `roll-kept`: the history link…
  const rollKept = await writeBlob(
    'rollKept',
    rolfVault,
    'images/history/roll-kept.webp',
    2,
    '2026-05-02T00:00:00.000Z',
  );
  // …and the album copy of the SAME bytes (two links, one blob) — what "already
  // in the album" means, and what a delete must leave standing.
  const rollKeptAlbum = await writeBlob(
    'rollKeptAlbum',
    rolfVault,
    'photos/roll-kept.webp',
    2,
    '2026-05-02T12:00:00.000Z',
  );
  // `roll-album`: the album copy is its ONLY link.
  const rollAlbum = await writeBlob(
    'rollAlbum',
    rolfVault,
    'photos/album-only.webp',
    3,
    '2026-05-01T00:00:00.000Z',
  );
  // `roll-legacy`: a pre-vault roll in a project store.
  const rollLegacy = await writeBlob(
    'rollLegacy',
    PROJECT_MP,
    'character-avatars/rolf-old.webp',
    5,
    '2026-04-29T00:00:00.000Z',
  );
  // SAGE's own roll (the tag-scoping negative).
  const rollSage = await writeBlob(
    'rollSage',
    sageVault,
    'images/history/sage-roll.webp',
    6,
    '2026-05-04T00:00:00.000Z',
  );
  // An unkeyed picture in ROLF's album — an IMAGE, tagged to him, NOT a roll.
  const notRoll = await writeBlob(
    'notRoll',
    rolfVault,
    'photos/snapshot.webp',
    7,
    '2026-05-05T00:00:00.000Z',
  );

  // 5. The `files` rows. A roll is `generationKey IS NOT NULL` + `category`
  //    IMAGE + the character's tag — no path predicate anywhere.
  const v1Key = (n: number, model: string): string =>
    deriveAvatarCacheKey({
      provider: 'OPENAI',
      imageProfileId: `b0000000-0000-4000-8000-00000000000${n}`,
      params: { prompt: `a portrait, configuration ${n}`, model } as never,
    } as never);

  const mkFile = async (
    id: string,
    over: Record<string, unknown>,
    createdAt: string,
  ): Promise<void> => {
    await repos.files.create(
      {
        userId: spec.userId,
        linkedTo: [],
        tags: [ROLF],
        source: 'GENERATED',
        category: 'IMAGE',
        mimeType: 'image/webp',
        size: 128,
        width: 32,
        height: 32,
        ...over,
      } as never,
      { id, createdAt, updatedAt: TS } as never,
    );
  };

  // ⚠ The INSERT order below is deliberately NOT the read order. v4's
  // `findByTag` carries no `ORDER BY`, so the rows arrive rowid-first and the
  // service's `createdAt` descending sort is what orders them — which is
  // unobservable if the fixture is already stored newest-first. It was, and a
  // mutation that DELETED the sort stayed green until this scramble; the read
  // order is NEW, KEPT, ALBUM, NOLINK, LEGACY, and nothing here matches it.
  // The pre-cache row the collapse migration keyed: a v0 key derived from the
  // only two things such a row records, and a storageKey naming the PROJECT
  // mount its bytes actually live in.
  await mkFile(
    F_ROLL_LEGACY,
    {
      sha256: rollLegacy.sha,
      originalFilename: 'rolf-old.webp',
      generationPrompt: 'an older portrait',
      generationModel: 'dall-e-3',
      generationKey: deriveLegacyAvatarCacheKey({
        modelName: 'dall-e-3',
        prompt: 'an older portrait',
      }),
      storageKey: buildMountBlobStorageKey(PROJECT_MP, rollLegacy.blobId),
    },
    '2026-04-29T00:00:00.000Z',
  );
  await mkFile(
    F_ROLL_ALBUM,
    {
      sha256: rollAlbum.sha,
      originalFilename: 'album-only.webp',
      generationPrompt: 'a portrait, configuration 3',
      generationModel: 'gpt-image-1',
      generationKey: v1Key(3, 'gpt-image-1'),
      storageKey: buildMountBlobStorageKey(rolfVault, rollAlbum.blobId),
    },
    '2026-05-01T00:00:00.000Z',
  );
  await mkFile(
    F_ROLL_NEW,
    {
      sha256: rollNew.sha,
      originalFilename: 'roll-new.webp',
      generationPrompt: 'a portrait, configuration 1',
      generationModel: 'gpt-image-1',
      generationKey: v1Key(1, 'gpt-image-1'),
      storageKey: buildMountBlobStorageKey(rolfVault, rollNew.blobId),
    },
    '2026-05-03T00:00:00.000Z',
  );
  // No mount link anywhere: the sha is in no index, so both links resolve null
  // and the display URL falls back to `buildLegacyFileUrl`.
  await mkFile(
    F_ROLL_NOLINK,
    {
      sha256: 'ab'.repeat(32),
      originalFilename: 'no-link.webp',
      generationPrompt: 'a portrait, configuration 4',
      generationModel: 'gpt-image-1',
      generationKey: v1Key(4, 'gpt-image-1'),
      storageKey: `${spec.userId}/no-link.webp`,
    },
    '2026-04-30T00:00:00.000Z',
  );
  await mkFile(
    F_ROLL_KEPT,
    {
      sha256: rollKept.sha,
      originalFilename: 'roll-kept.webp',
      generationPrompt: 'a portrait, configuration 2',
      generationModel: 'gpt-image-1',
      generationKey: v1Key(2, 'gpt-image-1'),
      storageKey: buildMountBlobStorageKey(rolfVault, rollKept.blobId),
    },
    '2026-05-02T00:00:00.000Z',
  );
  // SAGE's roll — keyed and an image, but tagged to HER.
  await mkFile(
    F_ROLL_SAGE,
    {
      tags: [SAGE],
      sha256: rollSage.sha,
      originalFilename: 'sage-roll.webp',
      generationPrompt: 'a portrait of Sage',
      generationModel: 'gpt-image-1',
      generationKey: v1Key(6, 'gpt-image-1'),
      storageKey: buildMountBlobStorageKey(sageVault, rollSage.blobId),
    },
    '2026-05-04T00:00:00.000Z',
  );
  // Tagged to ROLF, an IMAGE, and NOT a roll: no `generationKey`.
  await mkFile(
    F_NOT_ROLL,
    {
      sha256: notRoll.sha,
      originalFilename: 'snapshot.webp',
      source: 'UPLOADED',
      storageKey: buildMountBlobStorageKey(rolfVault, notRoll.blobId),
    },
    '2026-05-05T00:00:00.000Z',
  );

  // TALL's roll: keyed, tagged to him, no mount link (he has no vault).
  await mkFile(
    F_ROLL_TALL,
    {
      tags: [TALL],
      sha256: 'cd'.repeat(32),
      originalFilename: 'tall-roll.webp',
      generationPrompt: 'a portrait of Tall',
      generationModel: 'gpt-image-1',
      generationKey: v1Key(8, 'gpt-image-1'),
      storageKey: `${spec.userId}/tall-roll.webp`,
    },
    '2026-04-28T00:00:00.000Z',
  );

  // 6. The pointers. ROLF's portrait is the ALBUM LINK of `roll-kept` (the
  //    post-Phase-3 shape); SAGE's is a LEGACY `files.id` (the tolerated shape,
  //    and the one a delete clears). ROLF also carries an `avatarOverrides`
  //    entry naming `roll-new` — the second pointer a delete filters.
  await repos.characters.update(ROLF, {
    defaultImageId: rollKeptAlbum.linkId,
    avatarOverrides: [{ chatId: CHAT_WEARING_A, imageId: F_ROLL_NEW }],
  } as never);
  await repos.characters.update(SAGE, { defaultImageId: F_ROLL_SAGE } as never);

  // 7. The chats. Two wear `roll-new`, so `usedInChatCount` is a count and the
  //    scrub has two rows; the third wears nothing.
  const mkChat = async (
    id: string,
    title: string,
    characterAvatars: Record<string, unknown>,
  ): Promise<void> => {
    await repos.chats.create(
      {
        userId: spec.userId,
        title,
        chatType: 'salon',
        contextSummary: null,
        participants: [
          {
            id: `e1000000-0000-4000-8000-00000000000${id.slice(-1)}`,
            type: 'CHARACTER',
            characterId: ROLF,
            controlledBy: 'llm',
            status: 'active',
            connectionProfileId: null,
            createdAt: TS,
            updatedAt: TS,
          },
        ],
        characterAvatars,
        tags: [],
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  };
  const worn = {
    [ROLF]: { imageId: F_ROLL_NEW, generatedAt: TS, afterMessageCount: 0 },
  };
  await mkChat(CHAT_WEARING_A, 'Wearing A', worn);
  await mkChat(CHAT_WEARING_B, 'Wearing B', worn);
  await mkChat(CHAT_BARE, 'Bare', {});

  // 8. Pin the remaining live-clock stamps the store writes minted.
  midb.prepare('UPDATE "doc_mount_files" SET "createdAt" = ?, "updatedAt" = ?').run(TS, TS);
  midb.prepare('UPDATE "doc_mount_blobs" SET "createdAt" = ?, "updatedAt" = ?').run(TS, TS);
  midb.prepare('UPDATE "doc_mount_folders" SET "createdAt" = ?, "updatedAt" = ?').run(TS, TS);
  midb.prepare('UPDATE "doc_mount_points" SET "lastScannedAt" = NULL').run();

  const meta = {
    rolfVault,
    sageVault,
    projectMp: PROJECT_MP,
    linkIds,
    shas,
    blobIds,
    rollNewLinkId: rollNew.linkId,
    rollKeptLinkId: rollKept.linkId,
    rollKeptAlbumLinkId: rollKeptAlbum.linkId,
    rollAlbumLinkId: rollAlbum.linkId,
    rollLegacyLinkId: rollLegacy.linkId,
    rollSageLinkId: rollSage.linkId,
    notRollLinkId: notRoll.linkId,
  };
  writeFileSync(mainOut + '.meta.json', JSON.stringify(meta, null, 2) + '\n');

  await closeDatabase();
  closeMountIndexSQLiteClient();
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(`avatar-rolls fixture written: ${mainOut} + ${mountOut}\n`);
}

void main();
