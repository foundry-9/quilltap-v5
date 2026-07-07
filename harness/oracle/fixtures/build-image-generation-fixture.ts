/**
 * Fixture builder for the W4.9a image-generation differential (v4
 * `lib/tools/handlers/image-generation-handler.ts` executeImageGenerationTool +
 * the avatar-trigger `lib/wardrobe/avatar-generation.ts`).
 *
 * Bakes ONE shared two-database starting state (main + mount-index) via v4's REAL
 * repos so BOTH v4 and the Rust port read/mutate a copy of byte-identical state;
 * each case runs on its own fresh copy (image generation WRITES `files` + the
 * store tables). Steps:
 *
 *   1. A user + one character (Aurora) with a full provisioned vault, via the REAL
 *      repos.characters.create (mints a linked vault mount). The character's
 *      `physicalDescription` is patched on so placeholder resolution + appearance
 *      resolution have tiers to read; its minted vault id is echoed to the sidecar.
 *   2. Two image profiles (a normal default + an uncensored isDangerousCompatible
 *      one) via repos.imageProfiles.create.
 *   3. One default connection profile via repos.connections.create (the cheap-LLM
 *      selection candidate).
 *   4. Three chat-settings rows (OFF / AUTO_ROUTE / DETECT_ONLY) via
 *      repos.chatSettings.create.
 *   5. The chats, one per case (each with a single CHARACTER participant = Aurora,
 *      + a controlledBy:'user' peer participant so {{me}}/{{user}} resolve). The
 *      appearance/sanitize cases carry recent messages (added via
 *      repos.chats.addMessages) so the appearance-resolution trivial-skip is
 *      bypassed and the cheap-LLM resolve/sanitize calls fire.
 *   6. A PROVISIONED "Lantern Backgrounds" `database` mount + the
 *      `instance_settings.lanternBackgroundsMountPointId` key so the REAL
 *      getLanternBackgroundsStore + writeLanternBackgroundToMountStore land the
 *      generated image bytes in a real store (byte-diffable).
 *
 * The minted char-vault id is echoed to a JSON sidecar (QT_FIXTURE_IMGGEN_MAIN +
 * '.meta.json') the oracle case + the Rust harness both read.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   WT=<this worktree root>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_IMGGEN_MAIN=/tmp/qt-imggen-main.db QT_FIXTURE_IMGGEN_MOUNT=/tmp/qt-imggen-mount.db \
 *     $N/node --import tsx $WT/harness/oracle/fixtures/build-image-generation-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface ChatSpec {
  id: string;
  op: 'generate' | 'avatar';
  chatType?: string;
  avatarGenerationEnabled?: boolean;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  user: Record<string, unknown>;
  charAId: string;
  charA: { name: string };
  lanternMountPointId: string;
  profileId: string;
  imageProfiles: Array<Record<string, unknown> & { id: string }>;
  connectionProfiles: Array<Record<string, unknown> & { id: string }>;
  chatSettingsOff: Record<string, unknown> & { id: string };
  auroraPhysicalDescription: Record<string, unknown>;
  chats: Record<string, ChatSpec>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'image-generation.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_IMGGEN_MAIN;
  const mountOut = process.env.QT_FIXTURE_IMGGEN_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_IMGGEN_MAIN and QT_FIXTURE_IMGGEN_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-imggen-fixture-build-'));
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
  const { CharacterSchema, ChatMetadataSchema, FileEntrySchema, BackgroundJobSchema } = await import('@/lib/schemas/types');
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

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('chats', ChatMetadataSchema);
  await ensureCollection('files', FileEntrySchema);
  await ensureCollection('background_jobs', BackgroundJobSchema);

  // Materialize the mount-index DDL up front (repos.characters.create scaffolds
  // vaults through linkDocumentContent, which needs these tables; the Lantern
  // store write lands bytes in doc_mount_blobs). See [[document-store-oracle-gotchas]].
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
  // creates its own correct table (CREATE TABLE IF NOT EXISTS) — link_blob_content
  // (the Lantern store write) needs it.
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');

  // 1. User + character (Aurora) with a provisioned vault.
  await repos.users.create(spec.user as never, { id: spec.userId, createdAt: TS, updatedAt: TS } as never);
  await repos.characters.create(
    { name: spec.charA.name, userId: spec.userId, controlledBy: 'llm' } as never,
    { id: spec.charAId, createdAt: TS, updatedAt: TS } as never,
  );
  // Patch a physicalDescription onto Aurora (placeholder + appearance resolution
  // read its tiers). update mints updatedAt, but the fixture is a fresh baseline;
  // both sides read the same value.
  await repos.characters.update(spec.charAId, {
    physicalDescription: {
      id: 'ddddffff-ffff-4fff-8fff-ffffffffffff',
      ...spec.auroraPhysicalDescription,
    },
  } as never);
  const charARaw = await repos.characters.findByIdRaw(spec.charAId);
  const charAVault = (charARaw?.characterDocumentMountPointId as string | null) ?? null;
  if (!charAVault) throw new Error('character vault not minted');

  // 2. Image profiles.
  for (const p of spec.imageProfiles) {
    const { id, ...data } = p;
    await repos.imageProfiles.create({ userId: spec.userId, ...data } as never, {
      id, createdAt: TS, updatedAt: TS,
    });
  }

  // 3. Connection profile(s).
  for (const cp of spec.connectionProfiles) {
    const { id, ...data } = cp;
    await repos.connections.create({ userId: spec.userId, ...data } as never, {
      id, createdAt: TS, updatedAt: TS,
    });
  }

  // 4. Chat settings (the single OFF row for the one corpus user; chatSettings is
  //    per-user via findByUserId, so the danger mode is fixed OFF — the Concierge
  //    classify/reroute paths are a tracked deferral proven by the W4.2 differentials).
  {
    const { id, ...data } = spec.chatSettingsOff;
    await repos.chatSettings.create({ userId: spec.userId, ...data } as never, {
      id, createdAt: TS, updatedAt: TS,
    });
  }

  // 5. Chats. Each has Aurora (LLM) + a user-controlled peer participant so
  //    {{me}} (caller) and {{user}} resolve. The caller for the tool is Aurora.
  const participantAId = 'fbaa0000-0000-4000-8000-000000000001';
  const participantUserId = 'fbaa0000-0000-4000-8000-000000000002';
  const mkParticipant = (id: string, characterId: string, controlledBy: string, name: string) => ({
    id,
    type: 'CHARACTER',
    characterId,
    characterName: name,
    controlledBy,
    displayOrder: 0,
    isActive: true,
    status: 'active',
    hasHistoryAccess: false,
    createdAt: TS,
    updatedAt: TS,
  });
  for (const [key, chat] of Object.entries(spec.chats)) {
    const extra: Record<string, unknown> = {};
    if (chat.chatType) extra.chatType = chat.chatType;
    if (chat.avatarGenerationEnabled !== undefined) extra.avatarGenerationEnabled = chat.avatarGenerationEnabled;
    await repos.chats.create(
      {
        userId: spec.userId,
        title: `ImgGen ${key}`,
        chatType: (chat.chatType as string) ?? 'salon',
        participants: [
          mkParticipant(participantAId, spec.charAId, 'llm', spec.charA.name),
          mkParticipant(participantUserId, spec.charAId, 'user', 'You'),
        ],
        ...extra,
      } as never,
      { id: chat.id, createdAt: TS, updatedAt: TS } as never,
    );
    // Materialize chat_messages lazily so the Rust read of an empty chat works.
    await repos.chats.getMessages(chat.id);
  }

  // 6. Provision the Lantern Backgrounds mount + the instance_settings pointer.
  await repos.docMountPoints.create(
    {
      name: 'Lantern Backgrounds',
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
    { id: spec.lanternMountPointId, createdAt: TS, updatedAt: TS },
  );
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'lanternBackgroundsMountPointId',
    spec.lanternMountPointId,
  ]);

  closeMountIndexSQLiteClient();
  await closeDatabase();

  const meta = { charAVault };
  writeFileSync(mainOut + '.meta.json', JSON.stringify(meta));
  process.stderr.write(
    `built image-generation fixtures: main=${mainOut} mount=${mountOut} charAVault=${charAVault}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`image-generation fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
