/**
 * Fixture builder for the Pascal WARDROBE-EFFECTS differential (P4.D306; v4
 * `lib/pascal/wardrobe-effects.ts`, `f0db8cbb8`).
 *
 * Bakes ONE starting state across main + mount-index so both v4's REAL
 * `applyWardrobeEffects` and v5's `side_effects::apply_effects_for_run` drive
 * the same pool against a copy of byte-identical vault state:
 *   1. A users row (pinned id) and ONE character with a full provisioned vault
 *      (REAL `repos.characters.create`, pinned id — a UUID: the vault overlay's
 *      reads are as strict about character ids as the memory reads).
 *   2. A provisioned "Quilltap General" store (`instance_settings` carries
 *      `generalMountPointId`).
 *   3. The character's wardrobe — v4's own `wardrobe-effects.test.ts` wardrobe:
 *      leaves `shirt` (top) / `trousers` (bottom) / `boots` (footwear), an
 *      archived `coat`, a `ribbon` (hair), a replace-flagged two-slot `cloak`,
 *      and the composite `regalia` = shirt + trousers + boots — seeded through
 *      the REAL public `repos.wardrobe.create` (vault `Wardrobe/*.md`), plus two
 *      General archetypes: a brooch, and a SECOND "Linen Shirt" so a title
 *      lookup must find the character's own first.
 *   4. A chat with the character as its one participant, `avatarGenerationEnabled:
 *      false` (the notify's avatar refresh is a no-op on both sides), and
 *      `background_jobs` materialized so an un-set announcement can enqueue.
 *   5. The wardrobe wear ledger in v4's MIGRATION shape (the P4.D255 rule —
 *      never through the repository).
 *   6. The spec's `toolDefinition` stored as `Tools/regalia.tool.json` in the
 *      character's vault through v4's REAL `storeMountFile` — the v5-only
 *      model-entrance leg's tool (the oracle never reads it).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_PASCAL_WARDROBE_EFFECTS_MAIN=/tmp/qt-pwe-main.db \
 *   QT_FIXTURE_PASCAL_WARDROBE_EFFECTS_MOUNT=/tmp/qt-pwe-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-pascal-wardrobe-effects-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface ItemSpec {
  id: string;
  title: string;
  types: string[];
  replace?: boolean;
  componentItemIds?: string[];
  archived?: boolean;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  user: Record<string, unknown>;
  characterId: string;
  character: { name: string };
  generalMountPointId: string;
  generalStore: { name: string };
  chatId: string;
  participantId: string;
  items: ItemSpec[];
  generalItems: ItemSpec[];
  toolDefinition: Record<string, unknown>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'pascal-wardrobe-effects.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_PASCAL_WARDROBE_EFFECTS_MAIN;
  const mountOut = process.env.QT_FIXTURE_PASCAL_WARDROBE_EFFECTS_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error(
      'QT_FIXTURE_PASCAL_WARDROBE_EFFECTS_MAIN and QT_FIXTURE_PASCAL_WARDROBE_EFFECTS_MOUNT must both point at the .db files to write',
    );
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-pwe-fixture-build-'));
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
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    GroupDocMountLinkSchema,
    GroupCharacterMemberSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  // The wear ledger in v4's MIGRATION shape (P4.D255) — never the repository.
  {
    const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
    const { WARDROBE_WEAR_STATS_DDL } = await import(
      '@/lib/database/backends/sqlite/wardrobe-wear-stats-ddl'
    );
    const raw = getRawDatabase();
    if (!raw) throw new Error('main DB handle unavailable for the wear ledger');
    for (const sql of WARDROBE_WEAR_STATS_DDL) raw.exec(sql);
  }
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
    ['group_doc_mount_links', GroupDocMountLinkSchema],
    ['group_character_members', GroupCharacterMemberSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  // v4's blobs table, verbatim from `build-wardrobe-tools-fixture.ts` (the
  // content-addressed rewrite's GC deletes from it unconditionally).
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
    'CREATE UNIQUE INDEX IF NOT EXISTS "idx_doc_mount_blobs_fileId" ON "doc_mount_blobs" ("fileId")'
  );

  const repos = getRepositories();

  await repos.users.create(spec.user as never, {
    id: spec.userId,
    createdAt: TS,
    updatedAt: TS,
  } as never);
  await repos.characters.create({ name: spec.character.name, userId: spec.userId } as never, {
    id: spec.characterId,
    createdAt: TS,
    updatedAt: TS,
  } as never);

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
    { id: spec.generalMountPointId, createdAt: TS, updatedAt: TS },
  );
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    spec.generalMountPointId,
  ]);

  const seedItem = async (item: ItemSpec, characterId: string | null): Promise<void> => {
    await repos.wardrobe.create(
      {
        characterId,
        title: item.title,
        description: null,
        imagePrompt: null,
        types: item.types,
        componentItemIds: item.componentItemIds ?? [],
        appropriateness: null,
        isDefault: false,
        replace: item.replace ?? false,
        migratedFromClothingRecordId: null,
        archivedAt: item.archived ? TS : null,
      } as never,
      { id: item.id, createdAt: TS, updatedAt: TS } as never,
    );
  };
  for (const item of spec.items) await seedItem(item, spec.characterId);
  for (const item of spec.generalItems) await seedItem(item, null);

  {
    const { storeMountFile } = await import('@/lib/mount-index/store-file');
    const character = (await repos.characters.findById(spec.characterId)) as {
      characterDocumentMountPointId?: string | null;
    } | null;
    const vault = character?.characterDocumentMountPointId;
    if (!vault) throw new Error('the character has no vault');
    await storeMountFile({
      mountPointId: vault,
      relativePath: `Tools/${spec.toolDefinition.name}.tool.json`,
      data: Buffer.from(JSON.stringify(spec.toolDefinition, null, 2), 'utf8'),
      originalMimeType: 'application/json',
      transcodeImages: false,
      extractText: false,
      enqueueEmbedding: false,
      force: true,
    } as never);
  }

  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Wardrobe Effects Fixture',
      participants: [
        {
          id: spec.participantId,
          type: 'CHARACTER',
          characterId: spec.characterId,
          controlledBy: 'llm',
          status: 'active',
          createdAt: TS,
          updatedAt: TS,
        },
      ],
      chatType: 'salon',
      contextSummary: null,
      avatarGenerationEnabled: false,
      equippedOutfit: {},
    } as never,
    { id: spec.chatId, createdAt: TS, updatedAt: TS },
  );
  await repos.chats.getMessageCount(spec.chatId);
  // v4 creates `background_jobs` lazily; force it so an enqueue on either side
  // lands in the same table.
  await repos.backgroundJobs.findByUserId(spec.userId, 'PENDING');

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(`built pascal-wardrobe-effects fixtures: main=${mainOut} mount=${mountOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`pascal-wardrobe-effects fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
