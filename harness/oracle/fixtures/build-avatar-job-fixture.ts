/**
 * Fixture builder for the W4.9c CHARACTER_AVATAR_GENERATION differential.
 *
 * Bakes ONE shared two-database starting state (main + mount-index) via v4's REAL
 * repos so BOTH v4 and the Rust port read/mutate a copy of byte-identical state;
 * each case runs on its own fresh copy (the handler WRITES the vault store tables
 * + files + chats + characters + a chat_messages notification row).
 *
 * Steps:
 *   1. Users (A: OFF danger; B: AUTO_ROUTE for the post-hoc reroute case).
 *   2. Characters with provisioned vaults (REAL repos.characters.create) +
 *      patched physicalDescriptions (avatar prompt reads the tiers).
 *   3. Image profiles (a primary, a no-key one, a blocked/uncensored pair for B).
 *   4. Per-user chat settings.
 *   5. A provisioned "Quilltap General" `database` store + the
 *      instance_settings.generalMountPointId pointer + a >600-char
 *      aurora-aesthetics.md seeded via REAL linkDocumentContent (the aesthetic
 *      preamble case).
 *   6. Wardrobe items via the REAL public repos.wardrobe.create (full vault
 *      projection), pinned ids/timestamps.
 *   7. One chat per case (a single CHARACTER participant + a seeded equippedOutfit
 *      + alertCharactersOfLanternImages:true so the Lantern notification posts).
 *
 * Minted character-vault ids are echoed to a JSON sidecar (QT_FIXTURE_AVATAR_MAIN
 * + '.meta.json') for reference.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; WT=<this worktree root>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_AVATAR_MAIN=/tmp/qt-avatar-main.db QT_FIXTURE_AVATAR_MOUNT=/tmp/qt-avatar-mount.db \
 *     $N/node --import tsx $WT/harness/oracle/fixtures/build-avatar-job-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { createHash } from 'node:crypto';

interface CharSpec {
  id: string;
  userId: string;
  name: string;
  pronouns?: Record<string, unknown>;
  physicalDescription?: Record<string, unknown>;
}
interface ItemSpec {
  id: string;
  characterId: string;
  title: string;
  types: string[];
  description?: string;
  imagePrompt?: string;
}
interface ChatSpec {
  id: string;
  userId: string;
  characterId: string;
  imageProfileId: string;
  equipped: Record<string, string[]>;
  equippedSlotsOverride?: Record<string, string[]>;
  expectWrite: boolean;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  generalMountPointId: string;
  auroraAesthetic: string;
  apiKeys: Record<string, string>;
  users: Array<Record<string, unknown> & { id: string }>;
  characters: CharSpec[];
  wardrobeItems: ItemSpec[];
  imageProfiles: Array<Record<string, unknown> & { id: string }>;
  chatSettings: Array<Record<string, unknown> & { id: string }>;
  chats: Record<string, ChatSpec>;
}

function pathBasename(p: string): string {
  return p.includes('/') ? p.slice(p.lastIndexOf('/') + 1) : p;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'avatar-job.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_AVATAR_MAIN;
  const mountOut = process.env.QT_FIXTURE_AVATAR_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_AVATAR_MAIN and QT_FIXTURE_AVATAR_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-avatar-fixture-build-'));
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
  // Touch doc_mount_blobs so the hand-written repo creates its own table.
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');

  // 1. Users.
  for (const u of spec.users) {
    const { id, ...data } = u;
    await repos.users.create(data as never, { id, createdAt: TS, updatedAt: TS } as never);
  }

  // 2. Characters (provisioned vaults) + physicalDescription patch.
  const charVaults: Record<string, string | null> = {};
  for (const c of spec.characters) {
    await repos.characters.create(
      { name: c.name, userId: c.userId, controlledBy: 'llm', ...(c.pronouns ? { pronouns: c.pronouns } : {}) } as never,
      { id: c.id, createdAt: TS, updatedAt: TS } as never,
    );
    if (c.physicalDescription) {
      await repos.characters.update(c.id, { physicalDescription: c.physicalDescription } as never);
    }
    const raw = await repos.characters.findByIdRaw(c.id);
    charVaults[c.id] = (raw?.characterDocumentMountPointId as string | null) ?? null;
    if (!charVaults[c.id]) throw new Error(`character vault not minted for ${c.id}`);
  }

  // 3. Image profiles.
  for (const p of spec.imageProfiles) {
    const { id, userId, ...data } = p as Record<string, unknown> & { id: string; userId: string };
    await repos.imageProfiles.create({ userId, ...data } as never, { id, createdAt: TS, updatedAt: TS });
  }

  // 4. Chat settings (per user).
  for (const cs of spec.chatSettings) {
    const { id, userId, ...data } = cs as Record<string, unknown> & { id: string; userId: string };
    await repos.chatSettings.create({ userId, ...data } as never, { id, createdAt: TS, updatedAt: TS });
  }

  // 5. Quilltap General store + the settings pointer + aurora-aesthetics.md.
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
    { id: spec.generalMountPointId, createdAt: TS, updatedAt: TS },
  );
  await rawQuery('CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)');
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    spec.generalMountPointId,
  ]);
  {
    const content = spec.auroraAesthetic;
    await repos.docMountFileLinks.linkDocumentContent({
      mountPointId: spec.generalMountPointId,
      relativePath: 'aurora-aesthetics.md',
      fileName: 'aurora-aesthetics.md',
      folderId: null,
      fileType: 'markdown',
      content,
      contentSha256: createHash('sha256').update(content, 'utf-8').digest('hex'),
      plainTextLength: content.length,
      fileSizeBytes: Buffer.byteLength(content, 'utf-8'),
    } as never);
  }

  // 6. Wardrobe items (REAL public repos.wardrobe.create, full vault projection).
  for (const item of spec.wardrobeItems) {
    await repos.wardrobe.create(
      {
        characterId: item.characterId,
        title: item.title,
        description: item.description ?? null,
        imagePrompt: item.imagePrompt ?? null,
        types: item.types,
        componentItemIds: [],
        appropriateness: null,
        isDefault: false,
        replace: false,
        migratedFromClothingRecordId: null,
        archivedAt: null,
      } as never,
      { id: item.id, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // 7. Chats (one CHARACTER participant + a seeded equippedOutfit; the Lantern
  //    notification gate on).
  let pIdx = 0;
  const mkParticipant = (characterId: string, name: string) => ({
    id: `fbaa0000-0000-4000-8000-${String(++pIdx).padStart(12, '0')}`,
    type: 'CHARACTER',
    characterId,
    characterName: name,
    controlledBy: 'llm',
    displayOrder: 0,
    isActive: true,
    status: 'active',
    hasHistoryAccess: false,
    createdAt: TS,
    updatedAt: TS,
  });
  const charName = (id: string): string => spec.characters.find((c) => c.id === id)!.name;
  // v4's stored equippedOutfit always carries all four slots as arrays; a partial
  // map makes resolveEquippedOutfitForCharacter throw ("slots.X is not iterable").
  const SLOTS = ['top', 'bottom', 'footwear', 'accessories'];
  const fullSlots = (m: Record<string, string[]>): Record<string, string[]> => {
    const out: Record<string, string[]> = {};
    for (const s of SLOTS) out[s] = m[s] ?? [];
    return out;
  };
  for (const [, chat] of Object.entries(spec.chats)) {
    await repos.chats.create(
      {
        userId: chat.userId,
        title: 'Avatar Fixture',
        chatType: 'salon',
        alertCharactersOfLanternImages: true,
        participants: [mkParticipant(chat.characterId, charName(chat.characterId))],
        equippedOutfit: { [chat.characterId]: fullSlots(chat.equipped) },
      } as never,
      { id: chat.id, createdAt: TS, updatedAt: TS } as never,
    );
    await repos.chats.getMessages(chat.id);
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  writeFileSync(mainOut + '.meta.json', JSON.stringify({ charVaults }));
  process.stderr.write(`built avatar-job fixtures: main=${mainOut} mount=${mountOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`avatar-job fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
