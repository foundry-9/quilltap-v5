/**
 * The committed chat-gallery fixture builder (P4.D174 deliverable) — the
 * test-pepper substrate the `chat_gallery_equivalence` differential and its
 * oracle case (`harness/oracle/cases/chat-gallery.test.ts`) both read.
 *
 * Bakes, via v4's REAL repositories + store-write helpers (every id and
 * timestamp pinned so both sides read identical rows), one conversation that
 * exercises every arm of `lib/photos/chat-gallery.ts`:
 *
 *   1. Five characters. ALDA (llm, vault) wears a chat avatar and owns a
 *      superseded `avatarOverrides` repaint for this chat; her standing
 *      portrait is a vault LINK. BRAN (llm, vault) has a portrait whose bytes
 *      are the repaint's bytes — the pass-3 `hasSha` skip. CORA (user persona,
 *      vault) authors the inline-Markdown message. DELL is a `removed`
 *      participant with a portrait (excluded by `participantCharacterIds`).
 *      ELIN's vault pointer names a mount point that does not exist (the
 *      broken-vault fail-soft: the overlay drops the row, so ELIN contributes
 *      no portrait and no vault for relative references).
 *   2. A "Quilltap General" database mount holding `generated/old-backdrop.webp`
 *      (the superseded background's PATH signal), `misc/inline-abs.webp` (the
 *      absolute-blob-URL inline reference), `docs/diagram.png` (a non-`photos/`
 *      store attach → `attachment` with `idKind: 'link'`), `photos/twin.webp`
 *      (the sha twin of a `files` row — the `messageId`-forwarding arm) and
 *      `library/notes.md` (a native-text document: no blob, so the shared walk
 *      surfaces it for `chatFilesList` and the gallery drops it on the mime
 *      check).
 *   3. Ten `files` rows linked to the chat, one per classifier arm: the current
 *      background (`chats.storyBackgroundImageId`), the superseded background
 *      (the `generated/` path), the worn avatar (`chats.characterAvatars`), the
 *      superseded repaint (`avatarOverrides`), an `images/history/` avatar
 *      whose character comes from `tags`, a `/character-avatars/` avatar whose
 *      character comes from `linkedTo`, a GENERATED image attached to a
 *      message, a plain upload, the sha twin and a non-image (filtered out).
 *      NOT a row with an unparseable `createdAt` — see the comment at the seed
 *      site: v4's repository drops such a row on its Zod schema, so the NaN
 *      comparator is unreachable through v4's own reader.
 *   4. Six messages: the GENERATED image's announcement, a kept-photo re-show,
 *      a store attach + the twin link + the native-text document, CORA's
 *      inline-Markdown prose (a relative vault path, an absolute blob URL, a
 *      `data:` URL, an off-site URL, an already-rolled file URL, a missing file
 *      URL, a non-image extension and some other absolute app path), and a
 *      SYSTEM-role message whose prose names a resolvable vault image (skipped).
 *
 * The minted vault mount ids + link ids are echoed to a JSON sidecar
 * (QT_FIXTURE_CG_MAIN + '.meta.json') that the oracle case and the Rust
 * differential both read.
 *
 * Regenerate (Node 24, from the v4 checkout) — the sweep driver runs this:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   TZ=UTC \
 *   QT_FIXTURE_CG_MAIN=$W/crates/quilltap-web/tests/fixtures/chat-gallery-main.db \
 *   QT_FIXTURE_CG_MOUNT=$W/crates/quilltap-web/tests/fixtures/chat-gallery-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-chat-gallery-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
}

// ── Pinned entity ids (shared verbatim with the oracle case + the Rust test) ──
const ALDA = 'a1000000-0000-4000-8000-000000000001';
const BRAN = 'a1000000-0000-4000-8000-000000000002';
const CORA = 'a1000000-0000-4000-8000-000000000003';
const DELL = 'a1000000-0000-4000-8000-000000000004';
const ELIN = 'a1000000-0000-4000-8000-000000000005';

const GENERAL_MP = '83000000-0000-4000-8000-000000000001';
/** No `doc_mount_points` row — ELIN's vault pointer is repointed here. */
const DEAD_MP = '83000000-0000-4000-8000-0000000000de';

const CHAT = 'c1000000-0000-4000-8000-000000000001';
const EMPTY_CHAT = 'c1000000-0000-4000-8000-000000000002';

const P_ALDA = 'e1000000-0000-4000-8000-000000000001';
const P_BRAN = 'e1000000-0000-4000-8000-000000000002';
const P_CORA = 'e1000000-0000-4000-8000-000000000003';
const P_DELL = 'e1000000-0000-4000-8000-000000000004';
const P_ELIN = 'e1000000-0000-4000-8000-000000000005';

const M_GEN = 'd1000000-0000-4000-8000-000000000001';
const M_KEPT = 'd1000000-0000-4000-8000-000000000002';
const M_STORE = 'd1000000-0000-4000-8000-000000000003';
const M_INLINE = 'd1000000-0000-4000-8000-000000000004';
const M_SYSTEM = 'd1000000-0000-4000-8000-000000000005';

const F_BG_CURRENT = 'f1000000-0000-4000-8000-000000000001';
const F_BG_OLD = 'f1000000-0000-4000-8000-000000000002';
const F_AV_WORN = 'f1000000-0000-4000-8000-000000000003';
const F_AV_REPAINT = 'f1000000-0000-4000-8000-000000000004';
const F_AV_HISTORY = 'f1000000-0000-4000-8000-000000000005';
const F_AV_LEGACY = 'f1000000-0000-4000-8000-000000000006';
const F_GEN = 'f1000000-0000-4000-8000-000000000007';
const F_UPLOAD = 'f1000000-0000-4000-8000-000000000008';
const F_TWIN = 'f1000000-0000-4000-8000-000000000009';
const F_NOTES = 'f1000000-0000-4000-8000-00000000000a';
const F_KEPT_SISTER = 'f1000000-0000-4000-8000-00000000000c';
const F_MISSING = '99999999-9999-4999-8999-999999999999';

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
      `chat-gallery fixture must be built under TZ=UTC (getTimezoneOffset=${offset})`,
    );
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'chat-gallery-web.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_CG_MAIN;
  const mountOut = process.env.QT_FIXTURE_CG_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_CG_MAIN and QT_FIXTURE_CG_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }
  if (existsSync(mainOut + '.meta.json')) rmSync(mainOut + '.meta.json');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cg-fixture-'));
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
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
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
  const mkChar = async (id: string, name: string, controlledBy: string): Promise<void> => {
    await repos.characters.create(
      { name, userId: spec.userId, controlledBy } as never,
      { id, createdAt: `2026-04-0${id.slice(-1)}T00:00:00.000Z`, updatedAt: TS } as never,
    );
  };
  await mkChar(ALDA, 'Alda', 'llm');
  await mkChar(BRAN, 'Bran', 'llm');
  await mkChar(CORA, 'Cora', 'user');
  await mkChar(DELL, 'Dell', 'llm');
  await mkChar(ELIN, 'Elin', 'llm');

  const vaultOf = async (id: string): Promise<string> => {
    const raw = await repos.characters.findByIdRaw(id);
    const mp = raw?.characterDocumentMountPointId as string | null;
    if (!mp) throw new Error(`vault not minted for ${id}`);
    return mp;
  };
  const aldaVault = await vaultOf(ALDA);
  const branVault = await vaultOf(BRAN);
  const coraVault = await vaultOf(CORA);
  const dellVault = await vaultOf(DELL);

  // 3. The General store (the non-vault mount every absolute reference names).
  await repos.docMountPoints.create(
    {
      name: 'Quilltap General',
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
    { id: GENERAL_MP, createdAt: TS, updatedAt: TS } as never,
  );
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    GENERAL_MP,
  ]);

  // 4. The store writes. Each blob's sha256 is derived from its BYTES, so a
  //    `files` row that must be the same picture carries the same sha.
  const linkIds: Record<string, string> = {};
  const shas: Record<string, string> = {};
  const writeBlob = async (
    mountPointId: string,
    relativePath: string,
    seed: number,
    mimeType: string,
    createdAt: string,
    key?: string,
  ): Promise<{ linkId: string; sha: string }> => {
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
      originalMimeType: mimeType,
      storedMimeType: mimeType,
      sha256: sha,
      data: bytes,
      description: '',
      extractedText: '',
      extractedTextSha256: null,
      extractionStatus: 'none',
    } as never);
    const linkId = (link as { id: string }).id;
    // Pin the live-clock stamps the store write mints.
    midb
      .prepare('UPDATE "doc_mount_file_links" SET "createdAt" = ?, "updatedAt" = ? WHERE "id" = ?')
      .run(createdAt, createdAt, linkId);
    const k = key ?? relativePath;
    linkIds[k] = linkId;
    shas[k] = sha;
    return { linkId, sha };
  };

  // The superseded background's PATH signal (any mount; the Lantern writes
  // `generated/` in its own store).
  const bgOld = await writeBlob(
    GENERAL_MP,
    'generated/old-backdrop.webp',
    1,
    'image/webp',
    '2026-04-10T00:00:00.000Z',
  );
  // The absolute-blob-URL inline reference.
  const inlineAbs = await writeBlob(
    GENERAL_MP,
    'misc/inline abs.webp',
    2,
    'image/webp',
    '2026-04-11T00:00:00.000Z',
  );
  // A non-`photos/` store file attached to a message → `attachment`, link kind.
  const storeAttach = await writeBlob(
    GENERAL_MP,
    'docs/diagram.png',
    3,
    'image/png',
    '2026-04-12T00:00:00.000Z',
  );
  // The sha twin of `F_TWIN` — attached to a message, so the later sighting
  // hands the pass-1 entry its `messageId`.
  const twin = await writeBlob(
    GENERAL_MP,
    'photos/twin.webp',
    4,
    'image/webp',
    '2026-04-13T00:00:00.000Z',
  );
  // A kept album photo in CORA's vault, re-shown by `attach_image`.
  const kept = await writeBlob(
    coraVault,
    'photos/kept-scene.webp',
    5,
    'image/webp',
    '2026-04-14T00:00:00.000Z',
  );
  // The relative-path inline reference, resolved against CORA's vault.
  const inlineRel = await writeBlob(
    coraVault,
    'images/inline-relative.webp',
    6,
    'image/webp',
    '2026-04-15T00:00:00.000Z',
  );
  // A vault image named only by a SYSTEM message's prose (pass 4 skips SYSTEM).
  await writeBlob(
    coraVault,
    'images/system-only.webp',
    7,
    'image/webp',
    '2026-04-16T00:00:00.000Z',
  );
  // ALDA's standing portrait — a vault LINK, distinct bytes.
  const aldaPortrait = await writeBlob(
    aldaVault,
    'images/avatar.webp',
    8,
    'image/webp',
    '2026-04-17T00:00:00.000Z',
    'aldaPortrait',
  );
  // The repaint's bytes, in ALDA's history folder — the `images/history/` path
  // signal AND the bytes BRAN's portrait shares.
  const repaintHistory = await writeBlob(
    aldaVault,
    'images/history/repaint.webp',
    9,
    'image/webp',
    '2026-04-18T00:00:00.000Z',
    'repaintHistory',
  );
  // A second `images/history/` blob — the path signal for `F_AV_HISTORY`,
  // whose bytes must differ from the repaint's or the collector would dedupe
  // the two `files` rows into one entry.
  const olderHistory = await writeBlob(
    aldaVault,
    'images/history/older.webp',
    11,
    'image/webp',
    '2026-04-18T12:00:00.000Z',
    'olderHistory',
  );
  // BRAN's standing portrait: the SAME bytes as the repaint → pass 3 skips it.
  const branPortrait = await writeBlob(
    branVault,
    'images/avatar.webp',
    9,
    'image/webp',
    '2026-04-19T00:00:00.000Z',
    'branPortrait',
  );
  // DELL's portrait — DELL is a `removed` participant, so this never appears.
  const dellPortrait = await writeBlob(
    dellVault,
    'images/avatar.webp',
    10,
    'image/webp',
    '2026-04-20T00:00:00.000Z',
    'dellPortrait',
  );
  // A native-text document (no blob): the shared walk surfaces it for the file
  // listing; the gallery drops it on the mime check.
  const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
  await writeDatabaseDocument(GENERAL_MP, 'library/notes.md', '# Notes\n\nAttached as text.');
  const notesLink = await repos.docMountFileLinks.findByMountPointAndPath(
    GENERAL_MP,
    'library/notes.md',
  );
  if (!notesLink) throw new Error('native-text document link not visible');
  const notesLinkId = (notesLink as { id: string }).id;
  midb
    .prepare('UPDATE "doc_mount_file_links" SET "createdAt" = ?, "updatedAt" = ? WHERE "id" = ?')
    .run('2026-04-21T00:00:00.000Z', '2026-04-21T00:00:00.000Z', notesLinkId);

  // 5. Portraits + the superseded repaint's ownership.
  await repos.characters.update(ALDA, { defaultImageId: aldaPortrait.linkId } as never);
  await repos.characters.update(BRAN, { defaultImageId: branPortrait.linkId } as never);
  await repos.characters.update(DELL, { defaultImageId: dellPortrait.linkId } as never);
  await repos.characters.update(ALDA, {
    avatarOverrides: [
      { chatId: CHAT, imageId: F_AV_REPAINT, generatedAt: '2026-04-18T00:00:00.000Z' },
    ],
  } as never);
  // ELIN's vault pointer names a mount point that does not exist.
  getRawDatabase()
    .prepare('UPDATE "characters" SET "characterDocumentMountPointId" = ? WHERE "id" = ?')
    .run(DEAD_MP, ELIN);

  // 6. The `files` rows, one per classifier arm.
  const mkFile = async (
    id: string,
    over: Record<string, unknown>,
    createdAt: string,
  ): Promise<void> => {
    await repos.files.create(
      {
        userId: spec.userId,
        linkedTo: [CHAT],
        tags: [],
        source: 'UPLOADED',
        category: 'IMAGE',
        mimeType: 'image/webp',
        size: 128,
        width: 32,
        height: 32,
        ...over,
        ...(over.mimeType === 'text/plain' ? { width: undefined, height: undefined } : {}),
      } as never,
      { id, createdAt, updatedAt: TS } as never,
    );
  };

  await mkFile(
    F_BG_CURRENT,
    {
      sha256: '1111'.repeat(16),
      originalFilename: 'backdrop-now.webp',
      source: 'GENERATED',
      storageKey: `${spec.userId}/backdrop-now.webp`,
    },
    '2026-04-22T00:00:00.000Z',
  );
  await mkFile(
    F_BG_OLD,
    {
      sha256: bgOld.sha,
      originalFilename: 'backdrop-old.webp',
      source: 'GENERATED',
      storageKey: `${spec.userId}/backdrop-old.webp`,
    },
    '2026-04-23T00:00:00.000Z',
  );
  await mkFile(
    F_AV_WORN,
    {
      sha256: '3333'.repeat(16),
      originalFilename: 'alda-worn.webp',
      source: 'GENERATED',
      storageKey: `${spec.userId}/alda-worn.webp`,
    },
    '2026-04-24T00:00:00.000Z',
  );
  await mkFile(
    F_AV_REPAINT,
    {
      sha256: repaintHistory.sha,
      originalFilename: 'alda-repaint.webp',
      source: 'GENERATED',
      storageKey: `${spec.userId}/alda-repaint.webp`,
    },
    '2026-04-25T00:00:00.000Z',
  );
  // Classified only by its `images/history/` linker path; its character comes
  // from `tags` (the avatar job tags the file with the character it painted).
  await mkFile(
    F_AV_HISTORY,
    {
      sha256: olderHistory.sha,
      originalFilename: 'alda-older.webp',
      source: 'GENERATED',
      tags: [CHAT, ALDA],
      storageKey: `${spec.userId}/alda-older.webp`,
    },
    '2026-04-26T00:00:00.000Z',
  );
  // Classified by `folderPath === '/character-avatars/'`; its character comes
  // from `linkedTo`.
  await mkFile(
    F_AV_LEGACY,
    {
      sha256: '6666'.repeat(16),
      originalFilename: 'bran-legacy.webp',
      source: 'UPLOADED',
      folderPath: '/character-avatars/',
      linkedTo: [CHAT, BRAN],
      storageKey: `${spec.userId}/bran-legacy.webp`,
    },
    '2026-04-27T00:00:00.000Z',
  );
  await mkFile(
    F_GEN,
    {
      sha256: '7777'.repeat(16),
      originalFilename: 'generated.webp',
      source: 'GENERATED',
      generationPrompt: 'a lantern-lit alley',
      generationModel: 'mock-image-model',
      storageKey: `${spec.userId}/generated.webp`,
    },
    '2026-04-28T00:00:00.000Z',
  );
  await mkFile(
    F_UPLOAD,
    {
      sha256: '8888'.repeat(16),
      originalFilename: 'upload.png',
      mimeType: 'image/png',
      storageKey: `${spec.userId}/upload.png`,
    },
    '2026-04-29T00:00:00.000Z',
  );
  // Deliberately the SAME `createdAt` as `F_UPLOAD`: v4's comparator has no
  // tie-break, so a tie falls to JS's stable sort and therefore to pass order.
  // Without one, a port could sort UNSTABLY and no case would notice.
  await mkFile(
    F_TWIN,
    {
      sha256: twin.sha,
      originalFilename: 'twin.webp',
      storageKey: `${spec.userId}/twin.webp`,
    },
    '2026-04-29T00:00:00.000Z',
  );
  await mkFile(
    F_NOTES,
    {
      sha256: '9999'.repeat(16),
      originalFilename: 'notes.txt',
      mimeType: 'text/plain',
      category: 'ATTACHMENT',
      size: 42,
      storageKey: `${spec.userId}/notes.txt`,
    },
    '2026-04-30T12:00:00.000Z',
  );
  // The kept photo's `files` sister — same bytes, NOT linked to the chat, so it
  // changes no roll. `saveImageToAlbum` resolves a `doc_mount_file_links.id` to
  // its sister `files` row by sha256 and only falls back to INGESTING the blob
  // when there is none; a real `attach_image` re-show always has one, and the
  // ingest fallback is separately proven by `photo_tools_equivalence`.
  await repos.files.create(
    {
      userId: spec.userId,
      linkedTo: [],
      tags: [],
      sha256: kept.sha,
      originalFilename: 'kept-scene.webp',
      mimeType: 'image/webp',
      size: 128,
      width: 32,
      height: 32,
      source: 'UPLOADED',
      category: 'IMAGE',
      storageKey: `${spec.userId}/kept-scene.webp`,
    } as never,
    { id: F_KEPT_SISTER, createdAt: '2026-04-14T00:00:00.000Z', updatedAt: TS } as never,
  );

  // NOT SEEDED: a row whose `createdAt` is unparseable. The order asked for the
  // NaN-comparator arm; it was MEASURED UNREACHABLE through v4's own reader —
  // `FileEntrySchema.createdAt` is `z.iso.datetime()`, so a `not-a-date` row
  // fails validation and v4's repository DROPS it whole
  // (`findById` → null, `findByLinkedTo` 11 → 10; probed at the pin
  // 2026-09-09). v4's sort therefore never sees a NaN, and a fixture row that
  // v4 cannot surface would only measure the general "v5 does not Zod-validate
  // reads" divergence, which is pre-existing and belongs to no lane here. The
  // Rust side's total order is documented in `EntryCollector::finish`.

  // 7. The chat. ALDA wears `F_AV_WORN`; the wall shows `F_BG_CURRENT`.
  const mkParticipant = (
    id: string,
    characterId: string,
    controlledBy: string,
    status = 'active',
  ) => ({
    id,
    type: 'CHARACTER',
    characterId,
    controlledBy,
    status,
    connectionProfileId: null,
    createdAt: TS,
    updatedAt: TS,
  });
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The Gallery',
      chatType: 'salon',
      contextSummary: null,
      storyBackgroundImageId: F_BG_CURRENT,
      characterAvatars: {
        [ALDA]: {
          imageId: F_AV_WORN,
          generatedAt: '2026-04-24T00:00:00.000Z',
          afterMessageCount: 2,
        },
      },
      activeTypingParticipantId: P_CORA,
      participants: [
        mkParticipant(P_ALDA, ALDA, 'llm'),
        mkParticipant(P_BRAN, BRAN, 'llm'),
        mkParticipant(P_CORA, CORA, 'user'),
        mkParticipant(P_DELL, DELL, 'llm', 'removed'),
        mkParticipant(P_ELIN, ELIN, 'llm'),
      ],
      tags: [],
    } as never,
    { id: CHAT, createdAt: TS, updatedAt: TS } as never,
  );
  // A second chat whose only picture is a standing portrait — the roll that
  // exercises pass 3 alone and leaves six of the seven `counts` keys at 0.
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Portrait Only',
      chatType: 'salon',
      contextSummary: null,
      participants: [mkParticipant('e2000000-0000-4000-8000-000000000001', ALDA, 'llm')],
      tags: [],
    } as never,
    { id: EMPTY_CHAT, createdAt: TS, updatedAt: TS } as never,
  );

  await repos.chats.addMessages(CHAT, [
    {
      id: M_GEN,
      type: 'message',
      role: 'ASSISTANT',
      content: 'Here is the alley.',
      participantId: P_ALDA,
      createdAt: '2026-05-02T00:00:00.000Z',
      attachments: [F_GEN],
    },
    {
      id: M_KEPT,
      type: 'message',
      role: 'ASSISTANT',
      content: 'And the one I kept.',
      participantId: P_ALDA,
      createdAt: '2026-05-03T00:00:00.000Z',
      attachments: [kept.linkId],
    },
    {
      id: M_STORE,
      type: 'message',
      role: 'ASSISTANT',
      content: 'The diagram, the twin and the notes.',
      participantId: P_BRAN,
      createdAt: '2026-05-04T00:00:00.000Z',
      attachments: [storeAttach.linkId, twin.linkId, notesLinkId],
    },
    {
      id: M_INLINE,
      type: 'message',
      role: 'USER',
      content: [
        'Relative: ![rel](images/inline-relative.webp)',
        `Absolute blob: ![abs](/api/v1/mount-points/${GENERAL_MP}/blobs/misc/inline%20abs.webp)`,
        'Data: ![d](data:image/png;base64,iVBORw0KGgo=)',
        'Off-site: ![o](https://example.com/x.png)',
        `Already rolled: ![u](/api/v1/files/${F_UPLOAD})`,
        `Missing: ![m](/api/v1/files/${F_MISSING})`,
        'Not an image: ![n](images/notes.md)',
        'Other app path: ![p](/settings/themes)',
      ].join('\n\n'),
      participantId: P_CORA,
      createdAt: '2026-05-05T00:00:00.000Z',
      attachments: [],
    },
    {
      id: M_SYSTEM,
      type: 'message',
      role: 'SYSTEM',
      content: 'System notice: ![s](images/system-only.webp)',
      participantId: P_CORA,
      createdAt: '2026-05-06T00:00:00.000Z',
      attachments: [],
    },
  ] as never);
  await repos.chats.update(CHAT, { updatedAt: TS } as never);

  // 8. Pin the remaining live-clock stamps the store writes minted.
  midb.prepare('UPDATE "doc_mount_files" SET "createdAt" = ?, "updatedAt" = ?').run(TS, TS);
  midb.prepare('UPDATE "doc_mount_blobs" SET "createdAt" = ?, "updatedAt" = ?').run(TS, TS);
  midb.prepare('UPDATE "doc_mount_folders" SET "createdAt" = ?, "updatedAt" = ?').run(TS, TS);
  midb.prepare('UPDATE "doc_mount_points" SET "lastScannedAt" = NULL').run();

  const meta = {
    aldaVault,
    branVault,
    coraVault,
    dellVault,
    generalMp: GENERAL_MP,
    linkIds,
    shas,
    aldaPortraitLinkId: aldaPortrait.linkId,
    branPortraitLinkId: branPortrait.linkId,
    dellPortraitLinkId: dellPortrait.linkId,
    keptLinkId: kept.linkId,
    storeAttachLinkId: storeAttach.linkId,
    twinLinkId: twin.linkId,
    inlineRelLinkId: inlineRel.linkId,
    inlineAbsLinkId: inlineAbs.linkId,
    olderHistoryLinkId: olderHistory.linkId,
    repaintHistoryLinkId: repaintHistory.linkId,
    notesLinkId,
  };
  writeFileSync(mainOut + '.meta.json', JSON.stringify(meta, null, 2) + '\n');

  await closeDatabase();
  closeMountIndexSQLiteClient();
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(`chat-gallery fixture written: ${mainOut} + ${mountOut}\n`);
}

void main();
