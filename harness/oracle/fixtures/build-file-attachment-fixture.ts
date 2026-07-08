/**
 * Fixture builder for the chat file/attachment tier-3 differential (W4.4b; v4
 * `lib/services/chat-message/context-builder.service.ts` `loadAndProcessFiles` +
 * `lib/chat/file-attachment-fallback.ts` + `lib/chat-files-v2.ts`
 * `loadChatFilesForLLM`).
 *
 * Bakes ONE shared starting state across TWO databases (main + mount-index) via
 * v4's REAL repos, so both v4 and the Rust port read a copy of byte-identical
 * state. Steps:
 *   1. User + a character (for the chat participant) + a chat.
 *   2. Two connection profiles in the DB (an OPENAI vision describer + an OLLAMA
 *      uncensored describer), and a chat-settings row referencing both as the
 *      image-description / uncensored-image-description profiles.
 *   3. The `files` rows (text / images / zip / reuse-tier images / a
 *      missing-bytes image) via the REAL `repos.files.create` with the pinned
 *      ids from the spec, `linkedTo: [chatId]` and a `storageKey`.
 *   4. A Scriptorium mount file (a blob) via the REAL `linkBlobContent`, so the
 *      mount-path branch of `loadChatFilesForLLM` resolves — its minted link id
 *      is echoed to the `.meta.json` sidecar.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_FILE_ATTACH_MAIN=/tmp/qt-fa-main.db QT_FIXTURE_FILE_ATTACH_MOUNT=/tmp/qt-fa-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-file-attachment-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface FileSpec {
  key: string;
  id: string;
  originalFilename: string;
  mimeType: string;
  category: string;
  fsmBytes?: number[];
  fsmBytesUtf8?: string;
  fsmMissing?: boolean;
  generationPrompt?: string;
  generationRevisedPrompt?: string;
  description?: string;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  user: Record<string, unknown>;
  chatId: string;
  participantAId: string;
  charId: string;
  charName: string;
  chatSettingsId: string;
  descProfileId: string;
  uncensoredProfileId: string;
  connectionProfiles: Array<Record<string, unknown>>;
  files: FileSpec[];
  mount: {
    mountPointId: string;
    relativePath: string;
    fileName: string;
    storedMimeType: string;
    data: number[];
  };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'file-attachment.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_FILE_ATTACH_MAIN;
  const mountOut = process.env.QT_FIXTURE_FILE_ATTACH_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_FILE_ATTACH_MAIN and QT_FIXTURE_FILE_ATTACH_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-fa-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema, ChatMetadataSchema, FileEntrySchema } = await import('@/lib/schemas/types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { ConnectionProfileSchema } = await import('@/lib/schemas/profile.types');
  const { ChatSettingsSchema } = await import('@/lib/schemas/settings.types');
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
  const { sha256OfBuffer } = await import('@/lib/utils/sha256');

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('chats', ChatMetadataSchema);
  await ensureCollection('files', FileEntrySchema);
  await ensureCollection('connectionProfiles', ConnectionProfileSchema);
  await ensureCollection('chatSettings', ChatSettingsSchema);

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
  // Force doc_mount_blobs to exist (hand-written table; not in generateDDL).
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');

  // 1. User + character + chat.
  await repos.users.create(spec.user as never, { id: spec.userId, createdAt: TS, updatedAt: TS } as never);
  await repos.characters.create(
    { name: spec.charName, userId: spec.userId, controlledBy: 'llm' } as never,
    { id: spec.charId, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Connection profiles + chat settings.
  for (const cp of spec.connectionProfiles) {
    await repos.connections.create(
      { userId: spec.userId, ...cp } as never,
      { id: cp.id as string, createdAt: TS, updatedAt: TS } as never,
    );
  }
  await repos.chatSettings.create(
    {
      userId: spec.userId,
      imageDescriptionProfileId: spec.descProfileId,
      uncensoredImageDescriptionProfileId: spec.uncensoredProfileId,
    } as never,
    { id: spec.chatSettingsId, createdAt: TS, updatedAt: TS } as never,
  );

  // 3. Files linked to the chat.
  for (const f of spec.files) {
    await repos.files.create(
      {
        userId: spec.userId,
        sha256: (`sha-${f.key}`).padEnd(64, '0'),
        originalFilename: f.originalFilename,
        mimeType: f.mimeType,
        size: (f.fsmBytes?.length ?? (f.fsmBytesUtf8 ? Buffer.byteLength(f.fsmBytesUtf8) : 0)),
        linkedTo: [spec.chatId],
        source: 'UPLOADED',
        category: f.category,
        generationPrompt: f.generationPrompt ?? null,
        generationRevisedPrompt: f.generationRevisedPrompt ?? null,
        description: f.description ?? null,
        tags: [],
        storageKey: `storage/${f.key}`,
        fileStatus: 'ok',
      } as never,
      { id: f.id, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // 4. Chat (with the single character participant).
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'File Attachment Fixture',
      chatType: 'salon',
      participants: [
        {
          id: spec.participantAId,
          type: 'CHARACTER',
          characterId: spec.charId,
          controlledBy: 'llm',
          displayOrder: 0,
          isActive: true,
          status: 'active',
          hasHistoryAccess: false,
          createdAt: TS,
          updatedAt: TS,
        },
      ],
    } as never,
    { id: spec.chatId, createdAt: TS, updatedAt: TS } as never,
  );

  // 5. Provision the Scriptorium mount + a blob file (the mount-path branch).
  await repos.docMountPoints.create(
    {
      name: 'Scriptorium',
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
    { id: spec.mount.mountPointId, createdAt: TS, updatedAt: TS },
  );
  const blobData = Buffer.from(spec.mount.data);
  const linkResult = await repos.docMountFileLinks.linkBlobContent({
    mountPointId: spec.mount.mountPointId,
    relativePath: spec.mount.relativePath,
    fileName: spec.mount.fileName,
    folderId: null,
    originalFileName: spec.mount.fileName,
    originalMimeType: spec.mount.storedMimeType,
    storedMimeType: spec.mount.storedMimeType,
    sha256: sha256OfBuffer(blobData),
    data: blobData,
  });

  closeMountIndexSQLiteClient();
  await closeDatabase();

  const meta = {
    mountLinkId: linkResult.link.id,
    mountPointId: spec.mount.mountPointId,
  };
  writeFileSync(mainOut + '.meta.json', JSON.stringify(meta));
  process.stderr.write(
    `built file-attachment fixtures: main=${mainOut} mount=${mountOut} mountLinkId=${linkResult.link.id}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`file-attachment fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
