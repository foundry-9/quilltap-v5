/**
 * Tier-2 SEED fixture builder — the stale-chat asset collapse (W4.8, v4
 * `lib/background-jobs/maintenance/collapse-stale-chat-assets.ts`).
 *
 * Bakes the PRE-SWEEP state across the main + mount-index DBs, through v4's REAL
 * repos where possible (a character with a `defaultImageId`, two chats, chat
 * messages) plus direct raw INSERTs for the `files` rows and the mount-index
 * album/vault link (the sweep only READS those, and pinned shas make the
 * protection deterministic). Neither the oracle nor the Rust test mutates this
 * seed: both COPY it and run `collapseStaleChatAssets(now)` on the copy.
 *
 * The fixture covers every skip-reason branch + the fresh-chat guard:
 *   - `stale-chat` (updatedAt far past, an OLD played message) with five
 *     GENERATED/IMAGE files linked to it:
 *       bg-current       → kept ('current': = chat.storyBackgroundImageId)
 *       avatar-current   → kept ('current': = a chat.characterAvatars imageId)
 *       album-protected  → kept ('album-or-vault-link': its sha has a photos/ link)
 *       char-ref         → kept ('character-reference': = character.defaultImageId)
 *       unprotected      → DELETED (no protection)
 *   - `fresh-chat` (a RECENT played message) with one GENERATED/IMAGE file
 *     `fresh-gen` — must SURVIVE (an active chat is never touched).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MAIN=/tmp/qt-maint-main.db \
 *   QT_FIXTURE_MOUNT=/tmp/qt-maint-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-maintenance-sweep-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  now: string;
  staleUpdatedAt: string;
  freshCreatedAt: string;
  seedTs: string;
  characterId: string;
  characterMountPointId: string;
  shas: {
    bgCurrent: string;
    avatarCurrent: string;
    albumProtected: string;
    charRef: string;
    unprotected: string;
    freshGen: string;
  };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'maintenance-sweep.json'), 'utf8')) as Spec;

  const outMain = process.env.QT_FIXTURE_MAIN;
  const outMount = process.env.QT_FIXTURE_MOUNT;
  if (!outMain || !outMount) {
    throw new Error('QT_FIXTURE_MAIN and QT_FIXTURE_MOUNT must be set');
  }
  for (const base of [outMain, outMount]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = base + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-maint-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, ensureCollection, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const { UserSchema, CharacterSchema, FileEntrySchema } = await import('@/lib/schemas/types');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    ProjectDocMountLinkSchema,
    GroupDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('files', FileEntrySchema);

  // Materialize the mount-index store tables (the ports never issue CREATE TABLE),
  // so the character-vault provisioning + the raw album-link INSERTs both work.
  const midb0 = getRawMountIndexDatabase();
  if (!midb0) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
    ['project_doc_mount_links', ProjectDocMountLinkSchema],
    ['group_doc_mount_links', GroupDocMountLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb0.exec(sql);
  }

  const repos = getRepositories();
  const ts = spec.seedTs;

  await repos.users.create(
    { username: 'owner', name: 'Owner' } as never,
    { id: spec.userId, createdAt: ts, updatedAt: ts } as never,
  );

  // File ids (pinned).
  const F = {
    bgCurrent: 'f0000000-0000-4000-8000-0000000000b1',
    avatarCurrent: 'f0000000-0000-4000-8000-0000000000b2',
    albumProtected: 'f0000000-0000-4000-8000-0000000000b3',
    charRef: 'f0000000-0000-4000-8000-0000000000b4',
    unprotected: 'f0000000-0000-4000-8000-0000000000b5',
    freshGen: 'f0000000-0000-4000-8000-0000000000b6',
  };
  const STALE_CHAT = 'c0000000-0000-4000-8000-0000000000c1';
  const FRESH_CHAT = 'c0000000-0000-4000-8000-0000000000c2';

  // A character whose defaultImageId protects the `char-ref` file. Created via the
  // REAL repo so it provisions its vault (unused by the sweep, but faithful).
  await repos.characters.create(
    {
      userId: spec.userId,
      name: 'Aria',
      controlledBy: 'llm',
      defaultImageId: F.charRef,
    } as never,
    { id: spec.characterId, createdAt: ts, updatedAt: ts },
  );

  // Two chats via the REAL repo (pinned ids/timestamps). The stale chat carries
  // the current story-background + a characterAvatars map; both chats have one
  // participant so the schema `.refine()` (>=1) passes.
  const participant = (id: string) => ({
    id,
    type: 'CHARACTER',
    characterId: spec.characterId,
    controlledBy: 'llm',
    displayOrder: 0,
    isActive: true,
    status: 'active',
    createdAt: ts,
    updatedAt: ts,
  });

  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Stale Chat',
      chatType: 'salon',
      participants: [participant('d0000000-0000-4000-8000-0000000000d1')],
      storyBackgroundImageId: F.bgCurrent,
      characterAvatars: {
        [spec.characterId]: {
          imageId: F.avatarCurrent,
          generatedAt: ts,
          afterMessageCount: 1,
        },
      },
    } as never,
    { id: STALE_CHAT, createdAt: ts, updatedAt: spec.staleUpdatedAt },
  );

  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Fresh Chat',
      chatType: 'salon',
      participants: [participant('d0000000-0000-4000-8000-0000000000d2')],
    } as never,
    { id: FRESH_CHAT, createdAt: ts, updatedAt: spec.staleUpdatedAt },
  );

  // Materialize chat_messages and add played messages: the fresh chat gets a
  // RECENT participant message (so getLastPlayedMessageAt makes it non-stale); the
  // stale chat gets an OLD one (so its lastPlayed is far in the past → stale).
  await repos.chats.getMessageCount(STALE_CHAT);
  await rawQuery(
    `INSERT INTO chat_messages (id, chatId, type, role, content, systemSender, createdAt) VALUES (?, ?, 'message', 'ASSISTANT', ?, NULL, ?)`,
    ['m0000000-0000-4000-8000-0000000000e1', STALE_CHAT, 'old line', spec.staleUpdatedAt],
  );
  await rawQuery(
    `INSERT INTO chat_messages (id, chatId, type, role, content, systemSender, createdAt) VALUES (?, ?, 'message', 'ASSISTANT', ?, NULL, ?)`,
    ['m0000000-0000-4000-8000-0000000000e2', FRESH_CHAT, 'fresh line', spec.freshCreatedAt],
  );

  // The GENERATED/IMAGE files, via direct raw INSERTs (pinned shas, storageKey
  // NULL so deleteFileCompletely's storage delete is a no-op-warn — the FsSeam).
  const insertFile = async (
    id: string,
    sha256: string,
    linkedTo: string[],
    source: string,
    category: string,
  ) => {
    await rawQuery(
      `INSERT INTO files (id, userId, sha256, originalFilename, mimeType, size, isPlainText, linkedTo, source, category, storageKey, fileStatus, createdAt, updatedAt)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      [
        id,
        spec.userId,
        sha256,
        `${id}.webp`,
        'image/webp',
        1024,
        0,
        JSON.stringify(linkedTo),
        source,
        category,
        null,
        'ok',
        ts,
        ts,
      ],
    );
  };
  await insertFile(F.bgCurrent, spec.shas.bgCurrent, [STALE_CHAT], 'GENERATED', 'IMAGE');
  await insertFile(F.avatarCurrent, spec.shas.avatarCurrent, [STALE_CHAT], 'GENERATED', 'IMAGE');
  await insertFile(F.albumProtected, spec.shas.albumProtected, [STALE_CHAT], 'GENERATED', 'IMAGE');
  await insertFile(F.charRef, spec.shas.charRef, [STALE_CHAT], 'GENERATED', 'IMAGE');
  await insertFile(F.unprotected, spec.shas.unprotected, [STALE_CHAT], 'GENERATED', 'IMAGE');
  await insertFile(F.freshGen, spec.shas.freshGen, [FRESH_CHAT], 'GENERATED', 'IMAGE');

  // The mount-index album link protecting `album-protected`: a doc_mount_files row
  // whose sha256 == albumProtected's sha, plus a link into a `photos/` folder.
  // Direct raw INSERTs on the mount-index DB (the sweep only reads them). Seed via
  // a real store so the tables exist with the right shape.
  const mountDb = getRawMountIndexDatabase();
  if (!mountDb) throw new Error('mount-index DB not available');

  const MP = '20000000-0000-4000-8000-0000000000f1';
  const DMF = '20000000-0000-4000-8000-0000000000f2';
  const LINK = '20000000-0000-4000-8000-0000000000f3';
  // A 'documents' store with a photos/ link → isPhotoAlbum protects the bytes.
  mountDb
    .prepare(
      `INSERT INTO doc_mount_points (id, name, basePath, mountType, storeType, includePatterns, excludePatterns, enabled, createdAt, updatedAt)
       VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?, ?)`,
    )
    .run(MP, 'Album Store', '', 'database', 'documents', '[]', '[]', ts, ts);
  mountDb
    .prepare(
      `INSERT INTO doc_mount_files (id, sha256, fileSizeBytes, fileType, source, createdAt, updatedAt)
       VALUES (?, ?, ?, ?, ?, ?, ?)`,
    )
    .run(DMF, spec.shas.albumProtected, 1024, 'blob', 'database', ts, ts);
  mountDb
    .prepare(
      `INSERT INTO doc_mount_file_links (id, fileId, mountPointId, relativePath, fileName, lastModified, createdAt, updatedAt)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
    )
    .run(LINK, DMF, MP, 'photos/album.webp', 'album.webp', ts, ts, ts);

  await closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(`built maintenance-sweep fixture: main=${outMain} mount=${outMount}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`maintenance-sweep fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
