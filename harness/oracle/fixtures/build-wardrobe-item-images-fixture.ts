/**
 * Fixture builder for the wardrobe ITEM-IMAGES tier-2 differential (P4.D263;
 * v4 `lib/wardrobe/item-images.ts`, `7c8572869`).
 *
 * Bakes the shared starting state across TWO databases (main + mount-index):
 *   - two users (the owner + a stranger);
 *   - three characters (REAL `repos.characters.create`, pinned ids): the owner's
 *     live character, the owner's character that is ARCHIVED once its wardrobe
 *     is seeded, and the stranger's character (the ownership-refusal seed);
 *   - a "Quilltap General" store (the `instance_settings` key), a project and a
 *     group, each store-backed with its official store and `Wardrobe/` folder;
 *   - eight wardrobe items across the four tiers (REAL `repos.wardrobe.create`
 *     / `createProjectWardrobeItem`, pinned ids + timestamps);
 *   - every PICTURE through v4's REAL `addWardrobeItemImage` (the bridge's
 *     `linkBlobContent`, the `files` row, the frontmatter pointer) — tiny LOSSY
 *     WebP bytes, which v4's and v5's blob normalizers both pass through
 *     untouched; the picture rows' timestamps are then pinned (one second
 *     apart, seeding order) so "newest first" is unambiguous;
 *   - the planted shapes: a DANGLING pointer (`repos.wardrobe.update`), an
 *     EMPTY-STRING `imageFileId` frontmatter key (P.11 — written with v4's real
 *     `writeDatabaseDocument`, since v4's writer never emits an empty value), a
 *     DOCUMENT `files` row linked to an item, and an IMAGE row whose storage key
 *     does not parse (the carry's unreadable-blob seed).
 *
 * The wear ledger rides in v4's MIGRATION shape (C1 §6) so a booted-instance
 * read never trips on its absence.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WII_MAIN=/tmp/qt-wii-main.db QT_FIXTURE_WII_MOUNT=/tmp/qt-wii-mount.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-wardrobe-item-images-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface ItemSpec {
  id: string;
  home: 'character' | 'general' | 'project' | 'group' | 'archived';
  title: string;
  types: string[];
  imagePrompt?: string;
}
interface PictureSpec {
  item: string;
  kind: 'generated' | 'uploaded' | 'imported';
  webp: number;
  generationPrompt?: string;
  generationModel?: string;
  generationRevisedPrompt?: string;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  user: Record<string, unknown>;
  strangerUserId: string;
  strangerUser: Record<string, unknown>;
  characterId: string;
  archivedCharacterId: string;
  strangerCharacterId: string;
  characterTemplate: Record<string, unknown>;
  character: Record<string, unknown>;
  archivedCharacter: Record<string, unknown>;
  strangerCharacter: Record<string, unknown>;
  projectId: string;
  groupId: string;
  generalMountPointId: string;
  generalStore: { name: string };
  items: Record<string, ItemSpec>;
  pictures: PictureSpec[];
  danglingPointer: { item: string; fileId: string };
  emptyPointer: { item: string };
  unreadablePicture: { item: string; storageKey: string; originalFilename: string };
  documentFile: { item: string; originalFilename: string };
  webp: string[];
}

const PINNED_TS = '2026-02-01T00:00:00.000Z';
const ARCHIVED_AT = '2026-03-01T00:00:00.000Z';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'wardrobe-item-images-tier2.json'), 'utf8'),
  ) as Spec;

  const mainOut = process.env.QT_FIXTURE_WII_MAIN;
  const mountOut = process.env.QT_FIXTURE_WII_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_WII_MAIN and QT_FIXTURE_WII_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wii-fixture-build-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
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
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { FileEntrySchema } = await import('@/lib/schemas/file.types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    ProjectDocMountLinkSchema,
    GroupDocMountLinkSchema,
    GroupCharacterMemberSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  {
    const { WARDROBE_WEAR_STATS_DDL } = await import(
      '@/lib/database/backends/sqlite/wardrobe-wear-stats-ddl'
    );
    const raw = getRawDatabase();
    if (!raw) throw new Error('main DB handle unavailable for the wear ledger');
    for (const sql of WARDROBE_WEAR_STATS_DDL) raw.exec(sql);
  }
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
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
    ['group_doc_mount_links', GroupDocMountLinkSchema],
    ['group_character_members', GroupCharacterMemberSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  // v4's blobs repository creates `doc_mount_blobs` lazily from hand DDL
  // (`doc-mount-blobs.repository.ts:113`); copied verbatim.
  midb.exec(`
    CREATE TABLE IF NOT EXISTS "doc_mount_blobs" (
      "id" TEXT PRIMARY KEY,
      "fileId" TEXT NOT NULL,
      "sha256" TEXT NOT NULL,
      "sizeBytes" INTEGER NOT NULL,
      "storedMimeType" TEXT NOT NULL,
      "data" BLOB NOT NULL,
      "createdAt" TEXT NOT NULL,
      "updatedAt" TEXT NOT NULL,
      FOREIGN KEY ("fileId") REFERENCES "doc_mount_files" ("id") ON DELETE CASCADE
    )
  `);
  midb.exec(
    'CREATE UNIQUE INDEX IF NOT EXISTS "idx_doc_mount_blobs_fileId" ON "doc_mount_blobs" ("fileId")',
  );

  const repos = getRepositories();
  const pin = (id: string) => ({ id, createdAt: PINNED_TS, updatedAt: PINNED_TS }) as never;

  // Users + characters.
  await repos.users.create(spec.user as never, pin(spec.userId));
  await repos.users.create(spec.strangerUser as never, pin(spec.strangerUserId));
  const character = (over: Record<string, unknown>, userId: string) =>
    ({ ...spec.characterTemplate, ...over, userId }) as never;
  await repos.characters.create(character(spec.character, spec.userId), pin(spec.characterId));
  await repos.characters.create(
    character(spec.archivedCharacter, spec.userId),
    pin(spec.archivedCharacterId),
  );
  await repos.characters.create(
    character(spec.strangerCharacter, spec.strangerUserId),
    pin(spec.strangerCharacterId),
  );

  // Quilltap General.
  await repos.docMountPoints.create(
    {
      name: spec.generalStore.name,
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
    pin(spec.generalMountPointId),
  );
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    spec.generalMountPointId,
  ]);

  // Project + group, each with its official store and `Wardrobe/` folder.
  await repos.projects.create({ name: 'Images Project' } as never, pin(spec.projectId));
  await repos.groups.create({ name: 'Images Group' } as never, pin(spec.groupId));
  const projectMp = (await repos.projects.findByIdRaw(spec.projectId))?.officialMountPointId as string;
  const groupMp = (await repos.groups.findByIdRaw(spec.groupId))?.officialMountPointId as string;
  if (!projectMp || !groupMp) throw new Error('project/group official store not minted');
  const { ensureProjectWardrobeFolder } = await import('@/lib/mount-index/project-wardrobe');
  const { ensureGroupWardrobeFolder } = await import('@/lib/mount-index/group-wardrobe');
  await ensureProjectWardrobeFolder(projectMp);
  await ensureGroupWardrobeFolder(groupMp);

  // Items.
  const { createProjectWardrobeItem } = await import(
    '@/lib/database/repositories/vault-overlay/wardrobe-writes'
  );
  const body = (item: ItemSpec, characterId: string | null) => ({
    characterId,
    title: item.title,
    description: null,
    imagePrompt: item.imagePrompt ?? null,
    types: item.types,
    componentItemIds: [],
    appropriateness: null,
    isDefault: false,
    replace: false,
    migratedFromClothingRecordId: null,
    archivedAt: null,
  });
  for (const item of Object.values(spec.items)) {
    if (item.home === 'character' || item.home === 'archived' || item.home === 'general') {
      const owner =
        item.home === 'character'
          ? spec.characterId
          : item.home === 'archived'
            ? spec.archivedCharacterId
            : null;
      await repos.wardrobe.create(body(item, owner) as never, pin(item.id));
    } else {
      await createProjectWardrobeItem(item.home === 'project' ? projectMp : groupMp, {
        id: item.id,
        ...body(item, null),
        createdAt: PINNED_TS,
        updatedAt: PINNED_TS,
      } as never);
    }
  }

  // Pictures — v4's REAL addWardrobeItemImage.
  const { resolveWardrobeItemHome, addWardrobeItemImage } = await import(
    '@/lib/wardrobe/item-images'
  );
  const homeOf = async (key: string) => {
    const item = spec.items[key];
    const [scope, containerId] =
      item.home === 'character'
        ? ['character', spec.characterId]
        : item.home === 'archived'
          ? ['character', spec.archivedCharacterId]
          : item.home === 'general'
            ? ['general', null]
            : item.home === 'project'
              ? ['project', spec.projectId]
              : ['group', spec.groupId];
    const home = await resolveWardrobeItemHome(
      repos,
      spec.userId,
      scope as never,
      containerId as string | null,
      item.id,
    );
    if (!home) throw new Error(`no home for ${key}`);
    return home;
  };
  for (const p of spec.pictures) {
    await addWardrobeItemImage(repos, await homeOf(p.item), {
      userId: spec.userId,
      kind: p.kind,
      content: Buffer.from(spec.webp[p.webp], 'base64'),
      contentType: 'image/webp',
      width: null,
      height: null,
      generationPrompt: p.generationPrompt ?? null,
      generationModel: p.generationModel ?? null,
      generationRevisedPrompt: p.generationRevisedPrompt ?? null,
    });
  }

  // The planted shapes.
  await repos.wardrobe.update(
    spec.items[spec.danglingPointer.item].id,
    { imageFileId: spec.danglingPointer.fileId } as never,
    spec.characterId,
  );
  {
    // An EMPTY-STRING `imageFileId` key — v4's writer never emits one, so the
    // document is rewritten through v4's real database-store writer.
    const hatId = spec.items[spec.emptyPointer.item].id;
    const vaultMp = (await repos.characters.findByIdRaw(spec.characterId))
      ?.characterDocumentMountPointId as string;
    const docs = midb
      .prepare(
        `SELECT l.relativePath AS relativePath, d.content AS content
           FROM doc_mount_file_links l JOIN doc_mount_documents d ON d.fileId = l.fileId
          WHERE l.mountPointId = ?`,
      )
      .all(vaultMp) as Array<{ relativePath: string; content: string }>;
    const hat = docs.find((d) => d.content.includes(`id: ${hatId}`));
    if (!hat) throw new Error('hat document not found');
    const planted = hat.content.replace(/^createdAt:/m, 'imageFileId: ""\ncreatedAt:');
    if (planted === hat.content) throw new Error('hat frontmatter has no createdAt line');
    const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
    await writeDatabaseDocument(vaultMp, hat.relativePath, planted);
  }
  await repos.files.create(
    {
      userId: spec.userId,
      sha256: 'a'.repeat(64),
      originalFilename: spec.unreadablePicture.originalFilename,
      mimeType: 'image/webp',
      size: 10,
      width: null,
      height: null,
      linkedTo: [spec.items[spec.unreadablePicture.item].id],
      source: 'UPLOADED',
      category: 'IMAGE',
      generationPrompt: null,
      generationModel: null,
      generationRevisedPrompt: null,
      description: null,
      tags: [],
      storageKey: spec.unreadablePicture.storageKey,
      projectId: null,
      folderPath: null,
    } as never,
    { id: '7a8b9cad-0001-4000-8000-000000000001' } as never,
  );
  await repos.files.create(
    {
      userId: spec.userId,
      sha256: 'b'.repeat(64),
      originalFilename: spec.documentFile.originalFilename,
      mimeType: 'text/plain',
      size: 11,
      width: null,
      height: null,
      linkedTo: [spec.items[spec.documentFile.item].id],
      source: 'UPLOADED',
      category: 'DOCUMENT',
      generationPrompt: null,
      generationModel: null,
      generationRevisedPrompt: null,
      description: null,
      tags: [],
      storageKey: null,
      projectId: null,
      folderPath: null,
    } as never,
    { id: '7a8b9cad-0002-4000-8000-000000000002' } as never,
  );

  // Pin every `files` row's timestamps — one second apart, in seeding order.
  {
    const raw = getRawDatabase();
    if (!raw) throw new Error('main DB handle unavailable');
    const rows = raw.prepare('SELECT id FROM files ORDER BY rowid').all() as Array<{ id: string }>;
    const base = Date.parse(PINNED_TS);
    rows.forEach((r, i) => {
      const ts = new Date(base + (i + 1) * 1000).toISOString();
      raw.prepare('UPDATE files SET createdAt = ?, updatedAt = ? WHERE id = ?').run(ts, ts, r.id);
    });
  }

  // Archive the archived character LAST (its wardrobe and picture seeded).
  await rawQuery('UPDATE "characters" SET "archivedAt" = ? WHERE "id" = ?', [
    ARCHIVED_AT,
    spec.archivedCharacterId,
  ]);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(`built wardrobe-item-images fixtures: main=${mainOut} mount=${mountOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`wardrobe-item-images fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
