/**
 * Shared characters-server fixture builder (P4.6f deliverable #14) — the
 * committed test-pepper substrate for the whole characters-server-surface
 * differential set (`characters_reads_equivalence` / `characters_mutations_
 * equivalence`) AND the sibling P4.6g Characters-SPA Playwright e2e.
 *
 * Bakes, via v4's REAL repositories (all ids/timestamps pinned; vault link /
 * blob / mount-point ids are MINTED but baked into the committed .db, so both
 * differential sides read the identical rows — reads diff with no remap):
 *   - one user (the FIXTURE_USER; the web e2e rewrites it to SINGLE_USER_ID),
 *   - one OPENAI_COMPATIBLE connection profile + its api key + one image profile,
 *   - one legacy IMAGE `files` row (Dax's avatar),
 *   - two tags (Adventure, Mystery),
 *   - five characters exercising every list-DTO branch:
 *       Aria   — llm, favorite, canBeCarina, a VAULT avatar (writeCharacterAvatar
 *                ToVault), tags, 2 system prompts (1 default) + defaultSystemPromptId,
 *                2 scenarios (1 default) + defaultScenarioId, 2 wardrobe items,
 *                plugin data, defaultPartnerId → Cleo,
 *       Bram   — llm, npc,
 *       Cleo   — user-controlled (Aria's default partner),
 *       Dax    — llm, a LEGACY-file avatar,
 *       Echo   — a BROKEN-vault character (mount rows dropped) for the
 *                `findByIdRaw` path,
 *   - one SOLO chat (Aria + Cleo) exclusive to Aria (the cascade-delete branch),
 *   - two memories sourced from an Aria assistant message.
 *
 * Regenerate (Node 24, from the v4 checkout) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARACTERS_MAIN=$W/crates/quilltap-web/tests/fixtures/characters-main.db \
 *   QT_FIXTURE_CHARACTERS_MOUNT=$W/crates/quilltap-web/tests/fixtures/characters-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-characters-fixture.ts
 */

import * as zlib from 'node:zlib';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SystemPromptSpec { name: string; content: string; isDefault: boolean; makeDefault?: boolean }
interface ScenarioSpec { title: string; content: string; makeDefault?: boolean }
interface WardrobeSpec { title: string; description?: string; imagePrompt?: string; types: string[]; isDefault?: boolean }
interface PluginDataSpec { pluginName: string; data: unknown }
interface CharacterSpec {
  id: string;
  name: string;
  title?: string;
  description?: string;
  personality?: string;
  aliases?: string[];
  controlledBy: string;
  talkativeness: number;
  connectionProfileId: string;
  isFavorite?: boolean;
  npc?: boolean;
  canBeCarina?: boolean;
  tags?: string[];
  defaultImageId?: string;
  vaultAvatar?: { filename: string };
  defaultPartnerName?: string;
  systemPrompts?: SystemPromptSpec[];
  scenarios?: ScenarioSpec[];
  wardrobe?: WardrobeSpec[];
  pluginData?: PluginDataSpec[];
  breakVault?: boolean;
}
interface ParticipantSpec { id: string; character: string; controlledBy: string; connectionProfileId?: string }
interface MessageSpec { id: string; role: string; content: string; participantId: string | null; provider?: string; modelName?: string }
interface ChatSpec { id: string; title: string; chatType: string; tags?: string[]; participants: ParticipantSpec[]; messages: MessageSpec[] }
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  connectionProfiles: Array<Record<string, unknown>>;
  apiKeys: Array<Record<string, unknown>>;
  imageProfiles: Array<Record<string, unknown>>;
  files: Array<Record<string, unknown>>;
  tags: Array<{ id: string; name: string }>;
  characters: CharacterSpec[];
  chats: ChatSpec[];
  memories: Array<Record<string, unknown>>;
}

/** A valid 1x1 truecolor PNG (deflate) — decodable by sharp's transcode-to-WebP
 * on the vault-avatar write. Built via the PNG chunk algorithm (v4's own). */
function tinyPng(): Buffer {
  const crc32 = (buf: Buffer): number => {
    let crc = 0xffffffff;
    for (let i = 0; i < buf.length; i++) {
      crc ^= buf[i];
      for (let j = 0; j < 8; j++) crc = crc & 1 ? (crc >>> 1) ^ 0xedb88320 : crc >>> 1;
    }
    return (crc ^ 0xffffffff) >>> 0;
  };
  const chunk = (type: string, data: Buffer): Buffer => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const typeBuf = Buffer.from(type, 'ascii');
    const crcBuf = Buffer.alloc(4);
    crcBuf.writeUInt32BE(crc32(Buffer.concat([new Uint8Array(typeBuf), new Uint8Array(data)])));
    return Buffer.concat([new Uint8Array(len), new Uint8Array(typeBuf), new Uint8Array(data), new Uint8Array(crcBuf)]);
  };
  const sig = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(1, 0); // width
  ihdr.writeUInt32BE(1, 4); // height
  ihdr.writeUInt8(8, 8); // bit depth
  ihdr.writeUInt8(2, 9); // color type RGB
  const raw = Buffer.from([0, 20, 120, 200]); // filter byte + one RGB pixel
  const idat = zlib.deflateSync(raw);
  return Buffer.concat([
    new Uint8Array(sig),
    new Uint8Array(chunk('IHDR', ihdr)),
    new Uint8Array(chunk('IDAT', idat)),
    new Uint8Array(chunk('IEND', Buffer.alloc(0))),
  ]);
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'characters.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_CHARACTERS_MAIN;
  const mountOut = process.env.QT_FIXTURE_CHARACTERS_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_CHARACTERS_MAIN and QT_FIXTURE_CHARACTERS_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-characters-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, getCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
  } = await import('@/lib/schemas/mount-index.types');
  const { writeCharacterAvatarToVault } = await import('@/lib/file-storage/character-vault-bridge');

  await initializeDatabase();
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
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }

  // The `doc_mount_blobs` table is created by its repo's hand-written DDL (it
  // carries a BLOB `data` column the schema translator doesn't model); trigger
  // it with a read so the vault-avatar write has somewhere to land bytes.
  const { DocMountBlobsRepository } = await import(
    '@/lib/database/repositories/doc-mount-blobs.repository'
  );
  await new DocMountBlobsRepository().findByFileId('00000000-0000-4000-8000-000000000000');

  const repos = getRepositories();
  const TS = spec.seedTimestamp;

  // 1. The single user.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Api keys (pinned ids via a direct collection insert), then connections.
  const { ApiKeySchema } = await import('@/lib/schemas/profile.types');
  await ensureCollection('api_keys', ApiKeySchema);
  const apiKeyCol = await getCollection('api_keys');
  for (const key of spec.apiKeys) {
    const row = ApiKeySchema.parse({ ...key, createdAt: TS, updatedAt: TS });
    await apiKeyCol.insertOne(row as never);
  }
  for (const profile of spec.connectionProfiles) {
    const { id, ...rest } = profile as Record<string, unknown>;
    await repos.connections.create({ userId: spec.userId, ...rest } as never, {
      id, createdAt: TS, updatedAt: TS,
    } as never);
  }

  // 3. Image profiles.
  for (const ip of spec.imageProfiles) {
    const { id, ...rest } = ip as Record<string, unknown>;
    await repos.imageProfiles.create({ userId: spec.userId, ...rest } as never, {
      id, createdAt: TS, updatedAt: TS,
    } as never);
  }

  // 4. Legacy image files (Dax's avatar).
  for (const f of spec.files) {
    const { id, ...rest } = f as Record<string, unknown>;
    await repos.files.create({ userId: spec.userId, linkedTo: [], tags: [], ...rest } as never, {
      id, createdAt: TS, updatedAt: TS,
    } as never);
  }

  // 5. Tags.
  for (const t of spec.tags) {
    await repos.tags.create(
      { userId: spec.userId, name: t.name, nameLower: t.name.toLowerCase(), quickHide: false } as never,
      { id: t.id, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // 6. Characters (each mints a linked vault), plus their sub-arrays / vault
  //    avatar / wardrobe / plugin data. Names → ids for partner resolution.
  const idByName = new Map<string, string>();
  for (const c of spec.characters) idByName.set(c.name, c.id);
  for (const c of spec.characters) {
    await repos.characters.create(
      {
        name: c.name,
        userId: spec.userId,
        title: c.title ?? null,
        description: c.description ?? null,
        personality: c.personality ?? null,
        aliases: c.aliases ?? [],
        controlledBy: c.controlledBy,
        talkativeness: c.talkativeness,
        defaultConnectionProfileId: c.connectionProfileId,
        isFavorite: c.isFavorite ?? false,
        npc: c.npc ?? false,
        ...(c.canBeCarina !== undefined ? { canBeCarina: c.canBeCarina } : {}),
        tags: c.tags ?? [],
        ...(c.defaultImageId !== undefined ? { defaultImageId: c.defaultImageId } : {}),
      } as never,
      { id: c.id, createdAt: TS, updatedAt: TS } as never,
    );

    let defaultSystemPromptId: string | undefined;
    for (const sp of c.systemPrompts ?? []) {
      const created = await repos.characters.addSystemPrompt(c.id, {
        name: sp.name, content: sp.content, isDefault: sp.isDefault,
      } as never);
      if (sp.makeDefault && created) defaultSystemPromptId = created.id;
    }
    let defaultScenarioId: string | undefined;
    for (const sc of c.scenarios ?? []) {
      const created = await repos.characters.addScenario(c.id, { title: sc.title, content: sc.content });
      if (sc.makeDefault && created) defaultScenarioId = created.id;
    }
    for (const w of c.wardrobe ?? []) {
      await repos.wardrobe.create({
        characterId: c.id,
        title: w.title,
        description: w.description ?? null,
        imagePrompt: w.imagePrompt ?? null,
        types: w.types,
        componentItemIds: [],
        appropriateness: null,
        isDefault: w.isDefault ?? false,
        replace: false,
        migratedFromClothingRecordId: null,
      } as never);
    }
    for (const pd of c.pluginData ?? []) {
      await repos.characterPluginData.upsert(c.id, pd.pluginName, pd.data);
    }
    let vaultAvatarLinkId: string | undefined;
    if (c.vaultAvatar) {
      const written = await writeCharacterAvatarToVault({
        characterId: c.id,
        kind: 'main',
        filename: c.vaultAvatar.filename,
        content: tinyPng(),
        contentType: 'image/png',
      });
      vaultAvatarLinkId = written.linkId;
    }

    // Slim-column defaults that reference minted sub-array / vault ids.
    const patch: Record<string, unknown> = {};
    if (defaultSystemPromptId) patch.defaultSystemPromptId = defaultSystemPromptId;
    if (defaultScenarioId) patch.defaultScenarioId = defaultScenarioId;
    if (vaultAvatarLinkId) patch.defaultImageId = vaultAvatarLinkId;
    if (c.defaultPartnerName) patch.defaultPartnerId = idByName.get(c.defaultPartnerName) ?? null;
    if (Object.keys(patch).length > 0) await repos.characters.update(c.id, patch as never);
  }

  // 7. Break Echo's vault: drop the mount-point row so findById throws
  //    CharacterVaultUnavailableError while findByIdRaw (slim only) still works.
  for (const c of spec.characters) {
    if (!c.breakVault) continue;
    const raw = await repos.characters.findByIdRaw(c.id);
    const mountId = raw?.characterDocumentMountPointId;
    if (mountId) {
      midb.prepare('DELETE FROM doc_mount_points WHERE id = ?').run(mountId);
    }
  }

  // 8. Chats + messages.
  for (const chat of spec.chats) {
    const participants = chat.participants.map((p) => ({
      id: p.id,
      type: 'CHARACTER',
      characterId: p.character,
      controlledBy: p.controlledBy,
      status: 'active',
      ...(p.connectionProfileId !== undefined ? { connectionProfileId: p.connectionProfileId } : {}),
      createdAt: TS,
      updatedAt: TS,
    }));
    await repos.chats.create(
      {
        userId: spec.userId,
        title: chat.title,
        participants,
        chatType: chat.chatType,
        contextSummary: null,
        ...(chat.tags !== undefined ? { tags: chat.tags } : {}),
      } as never,
      { id: chat.id, createdAt: TS, updatedAt: TS } as never,
    );
    if (chat.messages.length > 0) {
      await repos.chats.addMessages(
        chat.id,
        chat.messages.map((m) => ({
          id: m.id,
          type: 'message',
          role: m.role,
          content: m.content,
          participantId: m.participantId ?? undefined,
          createdAt: TS,
          attachments: [],
          ...(m.provider !== undefined ? { provider: m.provider } : {}),
          ...(m.modelName !== undefined ? { modelName: m.modelName } : {}),
        })) as never,
      );
    }
  }

  // 9. Memories.
  await ensureCollection('memories', (await import('@/lib/schemas/memory.types')).MemorySchema);
  for (const mem of spec.memories) {
    const { id, ...rest } = mem as Record<string, unknown>;
    await repos.memories.create({ userId: spec.userId, keywords: [], ...rest } as never, {
      id, createdAt: TS, updatedAt: TS,
    } as never);
  }

  // Force the empty tables the read/mutation paths touch into existence. The
  // tags usage-count + delete fan-out sweeps `embedding_profiles`, which nothing
  // else seeds — materialize it so both differential sides read the same (empty)
  // table rather than v4 auto-creating it per-case while the Rust raw SQL 404s.
  const { EmbeddingProfileSchema } = await import('@/lib/schemas/profile.types');
  await ensureCollection('embedding_profiles', EmbeddingProfileSchema);
  await repos.backgroundJobs.findByUserId(spec.userId, 'PENDING');
  await repos.vectorIndices.findMetaByCharacterId(spec.characters[0].id);
  await repos.vectorIndices.findEntriesByCharacterId(spec.characters[0].id);
  await repos.groupCharacterMembers.findByCharacterId(spec.characters[0].id);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built characters fixture: main=${mainOut} mount=${mountOut} ` +
      `(${spec.characters.length} characters, ${spec.chats.length} chats)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`characters fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
