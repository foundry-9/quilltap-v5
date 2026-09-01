/**
 * Fixture builder for the W4.9c STORY_BACKGROUND_GENERATION differential.
 *
 * Bakes ONE shared two-database starting state (main + mount-index) via v4's REAL
 * repos so BOTH v4 and the Rust port read/mutate a copy of byte-identical state;
 * each case runs on its own fresh copy.
 *
 * Steps:
 *   1. A user.
 *   2. A participant character (Fern) with a provisioned vault + physicalDescription
 *      + a depiction-guidelines.md seeded into its vault (the Ariel Clause) + a
 *      wardrobe item; a NON-participant user character (Zelda) with a physical
 *      description (for the missing-character enumeration append).
 *   3. Image profiles (OPENAI + GROK) + two connection profiles (a cheap OPENAI
 *      default + an OLLAMA one used as the imagePromptProfileId uncensored retry
 *      selection).
 *   4. One OFF chat-settings row (imagePromptProfileId = the OLLAMA profile).
 *   5. A provisioned Quilltap General store carrying lantern/aurora-aesthetics.md +
 *      a provisioned Lantern Backgrounds store + a project P (backgroundDisplayMode
 *      latest_chat) whose official store carries its OWN lantern/aurora-aesthetics.md.
 *   6. One chat per case (recent messages seeded via addMessages; one carries a
 *      fresh scene state; one belongs to project P; [decd8ef9] some are marked
 *      `isDangerousChat` — their per-case danger settings are patched onto the
 *      copy by each side, not baked here).
 *
 * Minted vault / project-store ids are echoed to a JSON sidecar for reference.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; WT=<this worktree root>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_STORY_MAIN=/tmp/qt-story-main.db QT_FIXTURE_STORY_MOUNT=/tmp/qt-story-mount.db \
 *     $N/node --import tsx $WT/harness/oracle/fixtures/build-story-background-job-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { createHash } from 'node:crypto';

interface CharSpec {
  id: string;
  name: string;
  participant: boolean;
  pronouns?: Record<string, unknown>;
  physicalDescription?: Record<string, unknown>;
  depictionGuidelines?: boolean;
  wardrobe?: Array<{ id: string; title: string; types: string[]; imagePrompt?: string }>;
}
interface ChatSpec {
  id: string;
  characterIds: string[];
  imageProfileId: string;
  recentMessages?: string[];
  sceneState?: Record<string, unknown>;
  projectId?: string;
  /** [decd8ef9] Seeds the chat's Concierge classification label. */
  isDangerousChat?: boolean;
  /**
   * [P4.D141 / v4 `60e3c4a0a`] Seeds the operator override. `'UNCENSORED'` is
   * the four-state feature's motivating regression: the operator asserts the
   * chat spicy, the global mode is OFF, and the prompt must still go out candid.
   */
  conciergeOverride?: 'OFF' | 'UNCENSORED';
  /**
   * [decd8ef9] The per-case `chat_settings.dangerousContentSettings` bag. NOT
   * baked in here (one row, one user): both sides UPDATE it on their own fresh
   * copy before the case runs. Declared so the shape lives in one place.
   */
  dangerousContentSettings?: Record<string, unknown>;
  expectWrite: boolean;
  expectDerive: boolean;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  user: Record<string, unknown>;
  generalMountPointId: string;
  lanternMountPointId: string;
  projectId: string;
  projectName: string;
  generalLanternAesthetic: string;
  generalAuroraAesthetic: string;
  projectLanternAesthetic: string;
  projectAuroraAesthetic: string;
  depictionGuidelinesX: string;
  characters: CharSpec[];
  imageProfiles: Array<Record<string, unknown> & { id: string }>;
  connectionProfiles: Array<Record<string, unknown> & { id: string }>;
  chatSettings: Record<string, unknown> & { id: string };
  chats: Record<string, ChatSpec>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'story-background-job.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_STORY_MAIN;
  const mountOut = process.env.QT_FIXTURE_STORY_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_STORY_MAIN and QT_FIXTURE_STORY_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-story-fixture-build-'));
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
  const { CharacterSchema, ChatMetadataSchema, FileEntrySchema, BackgroundJobSchema, ProjectSchema, FolderSchema } = await import('@/lib/schemas/types');
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
  await ensureCollection('projects', ProjectSchema);
  // The project storage branch's legacy `folders` find-or-create needs the table;
  // v4 auto-creates it on first repo access, the Rust port requires it to exist.
  await ensureCollection('folders', FolderSchema);

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
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');

  const seedTextFile = async (mountPointId: string, relativePath: string, content: string): Promise<void> => {
    await repos.docMountFileLinks.linkDocumentContent({
      mountPointId,
      relativePath,
      fileName: relativePath.includes('/') ? relativePath.slice(relativePath.lastIndexOf('/') + 1) : relativePath,
      folderId: null,
      fileType: 'markdown',
      content,
      contentSha256: createHash('sha256').update(content, 'utf-8').digest('hex'),
      plainTextLength: content.length,
      fileSizeBytes: Buffer.byteLength(content, 'utf-8'),
    } as never);
  };

  // 1. User.
  await repos.users.create(spec.user as never, { id: spec.userId, createdAt: TS, updatedAt: TS } as never);

  // 2. Characters + physicalDescription + depiction-guidelines.md + wardrobe.
  const charVaults: Record<string, string | null> = {};
  for (const c of spec.characters) {
    await repos.characters.create(
      { name: c.name, userId: spec.userId, controlledBy: 'llm', ...(c.pronouns ? { pronouns: c.pronouns } : {}) } as never,
      { id: c.id, createdAt: TS, updatedAt: TS } as never,
    );
    if (c.physicalDescription) {
      await repos.characters.update(c.id, { physicalDescription: c.physicalDescription } as never);
    }
    const raw = await repos.characters.findByIdRaw(c.id);
    const vault = (raw?.characterDocumentMountPointId as string | null) ?? null;
    charVaults[c.id] = vault;
    if (!vault) throw new Error(`character vault not minted for ${c.id}`);
    if (c.depictionGuidelines) {
      await seedTextFile(vault, 'depiction-guidelines.md', spec.depictionGuidelinesX);
    }
    for (const w of c.wardrobe ?? []) {
      await repos.wardrobe.create(
        {
          characterId: c.id,
          title: w.title,
          description: null,
          imagePrompt: w.imagePrompt ?? null,
          types: w.types,
          componentItemIds: [],
          appropriateness: null,
          isDefault: false,
          replace: false,
          migratedFromClothingRecordId: null,
          archivedAt: null,
        } as never,
        { id: w.id, createdAt: TS, updatedAt: TS } as never,
      );
    }
  }

  // [40319484] `gcOrphanedFileRow` runs inside every content-addressed rewrite
  // and deletes from `doc_mount_blobs` unconditionally, so a mount index that
  // lacks the table now throws `no such table` on the SECOND write to any path.
  // v4's blobs repository creates it lazily from hand-written DDL
  // (`doc-mount-blobs.repository.ts:113`); copied verbatim so the fixture carries
  // exactly the table a real instance has (it is in `fresh_schema.json` too).
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

  // 3. Image profiles + connection profiles.
  for (const p of spec.imageProfiles) {
    const { id, ...data } = p;
    await repos.imageProfiles.create({ userId: spec.userId, ...data } as never, { id, createdAt: TS, updatedAt: TS });
  }
  for (const cp of spec.connectionProfiles) {
    const { id, ...data } = cp;
    await repos.connections.create({ userId: spec.userId, ...data } as never, { id, createdAt: TS, updatedAt: TS });
  }

  // 4. Chat settings.
  {
    const { id, ...data } = spec.chatSettings;
    await repos.chatSettings.create({ userId: spec.userId, ...data } as never, { id, createdAt: TS, updatedAt: TS });
  }

  // 5a. Quilltap General store + aesthetics.
  await repos.docMountPoints.create(
    { name: 'Quilltap General', basePath: '', mountType: 'database', storeType: 'documents', includePatterns: [], excludePatterns: [], enabled: true, lastScannedAt: null, scanStatus: 'idle', lastScanError: null, conversionStatus: 'idle', conversionError: null, fileCount: 0, chunkCount: 0, totalSizeBytes: 0 } as never,
    { id: spec.generalMountPointId, createdAt: TS, updatedAt: TS },
  );
  await rawQuery('CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)');
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', ['generalMountPointId', spec.generalMountPointId]);
  await seedTextFile(spec.generalMountPointId, 'lantern-aesthetics.md', spec.generalLanternAesthetic);
  await seedTextFile(spec.generalMountPointId, 'aurora-aesthetics.md', spec.generalAuroraAesthetic);

  // 5b. Lantern Backgrounds store.
  await repos.docMountPoints.create(
    { name: 'Lantern Backgrounds', basePath: '', mountType: 'database', storeType: 'documents', includePatterns: [], excludePatterns: [], enabled: true, lastScannedAt: null, scanStatus: 'idle', lastScanError: null, conversionStatus: 'idle', conversionError: null, fileCount: 0, chunkCount: 0, totalSizeBytes: 0 } as never,
    { id: spec.lanternMountPointId, createdAt: TS, updatedAt: TS },
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', ['lanternBackgroundsMountPointId', spec.lanternMountPointId]);

  // 5c. Project P (latest_chat) + its official store aesthetics.
  await repos.projects.create(
    { name: spec.projectName, userId: spec.userId, backgroundDisplayMode: 'latest_chat' } as never,
    { id: spec.projectId, createdAt: TS, updatedAt: TS } as never,
  );
  const project = await repos.projects.findById(spec.projectId);
  const projectStore = (project as { officialMountPointId?: string })?.officialMountPointId;
  if (!projectStore) throw new Error('project has no officialMountPointId');
  await seedTextFile(projectStore, 'lantern-aesthetics.md', spec.projectLanternAesthetic);
  await seedTextFile(projectStore, 'aurora-aesthetics.md', spec.projectAuroraAesthetic);

  // 6. Chats.
  let pIdx = 0;
  const charName = (id: string): string => spec.characters.find((c) => c.id === id)!.name;
  const mkParticipant = (characterId: string) => ({
    id: `fb5a0000-0000-4000-8000-${String(++pIdx).padStart(12, '0')}`,
    type: 'CHARACTER',
    characterId,
    characterName: charName(characterId),
    controlledBy: 'llm',
    displayOrder: 0,
    isActive: true,
    status: 'active',
    hasHistoryAccess: false,
    createdAt: TS,
    updatedAt: TS,
  });
  let msgIdx = 0;
  for (const [, chat] of Object.entries(spec.chats)) {
    const extra: Record<string, unknown> = {};
    if (chat.projectId) extra.projectId = chat.projectId;
    if (chat.isDangerousChat) extra.isDangerousChat = true;
    if (chat.conciergeOverride) extra.conciergeOverride = chat.conciergeOverride;
    if (chat.sceneState) extra.sceneState = chat.sceneState;
    // Equip Fern's Green Cloak so the appearance-resolution wardrobe path is
    // exercised (all four slots present — v4 requires the full slot map).
    const equippedOutfit: Record<string, Record<string, string[]>> = {};
    for (const cid of chat.characterIds) {
      const cs = spec.characters.find((c) => c.id === cid);
      const top = (cs?.wardrobe ?? []).filter((w) => w.types.includes('top')).map((w) => w.id);
      equippedOutfit[cid] = { top, bottom: [], footwear: [], accessories: [] };
    }
    extra.equippedOutfit = equippedOutfit;
    await repos.chats.create(
      {
        userId: spec.userId,
        title: `STORYCASE:${Object.keys(spec.chats).find((k) => spec.chats[k].id === chat.id)}`,
        chatType: 'salon',
        alertCharactersOfLanternImages: true,
        participants: chat.characterIds.map(mkParticipant),
        contextSummary: null,
        ...extra,
      } as never,
      { id: chat.id, createdAt: TS, updatedAt: TS } as never,
    );
    if (chat.recentMessages && chat.recentMessages.length > 0) {
      const events = chat.recentMessages.map((content, i) => ({
        id: `e5a00000-0000-4000-8000-${String(++msgIdx).padStart(12, '0')}`,
        type: 'message',
        role: i % 2 === 0 ? 'USER' : 'ASSISTANT',
        content,
        createdAt: `2020-01-02T00:00:0${i}.000Z`,
        attachments: [],
      }));
      await repos.chats.addMessages(chat.id, events as never);
    } else {
      await repos.chats.getMessages(chat.id);
    }
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  writeFileSync(mainOut + '.meta.json', JSON.stringify({ charVaults, projectStore }));
  process.stderr.write(`built story-background-job fixtures: main=${mainOut} mount=${mountOut} projectStore=${projectStore}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`story-background-job fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
