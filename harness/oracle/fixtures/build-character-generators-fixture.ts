/**
 * P4.9K2 (also read by P4.9K1's optimizer family) — the committed
 * `character-generators-{main,mount}.db` fixture builder: the substrate for
 * `character_wizard_tier3_equivalence`, `ai_import_tier3_equivalence` and
 * `character_optimizer_tier3_equivalence`.
 *
 * Bakes, via v4's REAL repositories (every id pinned except the vault-side
 * mount-point / link / blob ids, which are MINTED but baked into the committed
 * .db, so both differential sides read the identical rows):
 *   - one user (the FIXTURE_USER — the same id the characters pair uses),
 *   - two api keys + two connection profiles: an OPENAI_COMPATIBLE "Local Mock"
 *     that does NOT accept images (`supportsImageUpload: false`) and an OPENAI
 *     "Vision Mock" that does — the wizard's primary/vision split,
 *   - one DEFAULT embedding profile (so v4's `isEmbeddingAvailable` is true
 *     and the optimizer's semantic-search arm is reachable),
 *   - an "Uploads" database-backed mount point holding four blobs, each also a
 *     legacy `files` row whose `storageKey` is the `mount-blob:` key — the
 *     shape `fileStorageManager.downloadFile` and v5's `download_file` both
 *     read WITHOUT a disk tree: two text files (a `.md` and a `.txt` — the
 *     wizard's `document` source / the import's `sourceFileIds`), one PNG (the
 *     `gallery`/`upload` source), one opaque binary (the extractor's
 *     `[Binary file: …]` placeholder arm),
 *   - Mira — a rich character: every prose field, aliases, pronouns, 2 system
 *     prompts (1 default), 2 scenarios (1 default), 2 wardrobe items, a
 *     physical description, and TEN memories: eight about herself with
 *     `reinforcementCount >= 2` (one of them created a year earlier for the
 *     date-window arms; one with NO vector entry so the semantic-search arm
 *     can exclude something), one about herself with count 1 (filtered out),
 *     one ABOUT Nix (excluded — the self-reference rule), the rest with a
 *     4-dim vector in the index,
 *   - Nix — a bare character with no memories (the "not enough" arm).
 *
 * Regenerate (Node 24, from the v4 checkout) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CG_MAIN=$V5W/crates/quilltap-web/tests/fixtures/character-generators-main.db \
 *   QT_FIXTURE_CG_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/character-generators-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-character-generators-fixture.ts
 */

import * as zlib from 'node:zlib';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SystemPromptSpec { name: string; content: string; isDefault: boolean; makeDefault?: boolean }
interface ScenarioSpec { title: string; content: string; makeDefault?: boolean }
interface WardrobeSpec { title: string; description?: string; imagePrompt?: string; types: string[]; isDefault?: boolean }
interface CharacterSpec {
  id: string;
  name: string;
  title?: string;
  identity?: string;
  description?: string;
  manifesto?: string;
  personality?: string;
  firstMessage?: string;
  exampleDialogues?: string;
  aliases?: string[];
  pronouns?: { subject: string; object: string; possessive: string } | null;
  controlledBy: string;
  talkativeness: number;
  connectionProfileId: string;
  systemPrompts?: SystemPromptSpec[];
  scenarios?: ScenarioSpec[];
  wardrobe?: WardrobeSpec[];
  physicalDescription?: Record<string, unknown>;
}
interface FileSpec {
  id: string;
  path: string;
  originalFilename: string;
  mimeType: string;
  category: string;
  textBody?: string;
  png?: boolean;
  hexBody?: string;
}
interface MemorySpec {
  id: string;
  characterId: string;
  aboutCharacterId: string | null;
  content: string;
  summary: string;
  importance: number;
  reinforcementCount: number;
  createdAt: string;
  vector: number[] | null;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  apiKeys: Array<Record<string, unknown>>;
  connectionProfiles: Array<Record<string, unknown>>;
  embeddingProfiles: Array<Record<string, unknown>>;
  uploadsMountPointId: string;
  files: FileSpec[];
  characters: CharacterSpec[];
  memories: MemorySpec[];
}

/** A valid 1x1 truecolor PNG (deflate) — the characters builder's own. */
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
  ihdr.writeUInt32BE(1, 0);
  ihdr.writeUInt32BE(1, 4);
  ihdr.writeUInt8(8, 8);
  ihdr.writeUInt8(2, 9);
  const raw = Buffer.from([0, 20, 120, 200]);
  const idat = zlib.deflateSync(raw);
  return Buffer.concat([
    new Uint8Array(sig),
    new Uint8Array(chunk('IHDR', ihdr)),
    new Uint8Array(chunk('IDAT', idat)),
    new Uint8Array(chunk('IEND', Buffer.alloc(0))),
  ]);
}

function fileBytes(f: FileSpec): Buffer {
  if (f.png) return tinyPng();
  if (f.hexBody !== undefined) return Buffer.from(f.hexBody, 'hex');
  return Buffer.from(f.textBody ?? '', 'utf-8');
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'character-generators.json'), 'utf8')) as Spec;

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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-character-generators-fixture-'));
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
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
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
  const { ApiKeySchema, EmbeddingProfileSchema } = await import('@/lib/schemas/profile.types');
  const { MemorySchema } = await import('@/lib/schemas/memory.types');
  const { FileEntrySchema } = await import('@/lib/schemas/types');
  const { BackgroundJobSchema } = await import('@/lib/schemas/job.types');
  const { VectorIndicesRepository } = await import(
    '@/lib/database/repositories/vector-indices.repository'
  );
  const { storeMountFile } = await import('@/lib/mount-index/store-file');
  const { buildMountBlobStorageKey } = await import('@/lib/file-storage/project-store-bridge');
  const { sha256OfBuffer } = await import('@/lib/utils/sha256');

  await initializeDatabase();
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('api_keys', ApiKeySchema);
  await ensureCollection('embedding_profiles', EmbeddingProfileSchema);
  await ensureCollection('memories', MemorySchema);
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
  // `doc_mount_blobs` has hand-written DDL (a BLOB column) — trigger CREATE via a read.
  const { DocMountBlobsRepository } = await import(
    '@/lib/database/repositories/doc-mount-blobs.repository'
  );
  await new DocMountBlobsRepository().findByFileId('00000000-0000-4000-8000-000000000000');

  // Materialize the main-DB background_jobs table (v4 auto-creates it on first
  // enqueue; the Rust port does not own DDL, so its fixture copy needs it).
  const maindb = getRawDatabase();
  if (!maindb) throw new Error('main DB handle unavailable');
  for (const sql of generateDDL('background_jobs', BackgroundJobSchema as never)) {
    maindb.exec(sql);
  }
  // `instance_settings` is read by the wardrobe tier resolver on BOTH sides;
  // v4 fails that read soft while v5's raw SQL would not — materialize the
  // (empty) table so both read the same nothing (the P4.D130 lesson).
  maindb.exec(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );

  const repos = getRepositories();
  const TS = spec.seedTimestamp;

  // 1. The single user.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Api keys (pinned ids via a direct collection insert), then connections.
  const apiKeyCol = await getCollection('api_keys');
  for (const key of spec.apiKeys) {
    const row = ApiKeySchema.parse({ ...key, userId: spec.userId, createdAt: TS, updatedAt: TS });
    await apiKeyCol.insertOne(row as never);
  }
  for (const profile of spec.connectionProfiles) {
    const { id, ...rest } = profile as Record<string, unknown>;
    await repos.connections.create({ userId: spec.userId, ...rest } as never, {
      id, createdAt: TS, updatedAt: TS,
    } as never);
  }

  // 3. The default embedding profile.
  for (const ep of spec.embeddingProfiles) {
    const { id, ...rest } = ep as Record<string, unknown>;
    await repos.embeddingProfiles.create({ userId: spec.userId, ...rest } as never, {
      id, createdAt: TS, updatedAt: TS,
    } as never);
  }

  // 4. The Uploads store + its four blobs, each mirrored as a legacy `files`
  //    row pointing at the mount-blob key (the shape the byte layer reads).
  await repos.docMountPoints.create(
    {
      name: 'Uploads',
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
    { id: spec.uploadsMountPointId, createdAt: TS, updatedAt: TS } as never,
  );
  for (const f of spec.files) {
    const bytes = fileBytes(f);
    const stored = await storeMountFile({
      mountPointId: spec.uploadsMountPointId,
      relativePath: f.path,
      data: bytes,
      originalMimeType: f.mimeType,
      originalFileName: f.originalFilename,
      collisionStrategy: 'unique-suffix',
      treatNativeTextAsDocument: false,
      transcodeImages: false,
      extractText: false,
      enqueueEmbedding: false,
      assetStorage: 'database',
    } as never);
    const blobId = (stored as { blobId?: string }).blobId;
    if (!blobId) throw new Error(`no blob for ${f.path}`);
    await repos.files.create(
      {
        userId: spec.userId,
        sha256: sha256OfBuffer(bytes),
        originalFilename: f.originalFilename,
        mimeType: f.mimeType,
        size: bytes.length,
        source: 'UPLOADED',
        category: f.category,
        storageKey: buildMountBlobStorageKey(spec.uploadsMountPointId, blobId),
        linkedTo: [],
        tags: [],
      } as never,
      { id: f.id, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // 5. Characters (each mints a linked vault), plus their sub-arrays /
  //    wardrobe / physical description.
  for (const c of spec.characters) {
    await repos.characters.create(
      {
        name: c.name,
        userId: spec.userId,
        title: c.title ?? null,
        identity: c.identity ?? null,
        description: c.description ?? null,
        manifesto: c.manifesto ?? null,
        personality: c.personality ?? null,
        firstMessage: c.firstMessage ?? null,
        exampleDialogues: c.exampleDialogues ?? null,
        aliases: c.aliases ?? [],
        pronouns: c.pronouns ?? null,
        controlledBy: c.controlledBy,
        talkativeness: c.talkativeness,
        defaultConnectionProfileId: c.connectionProfileId,
        isFavorite: false,
        npc: false,
        tags: [],
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

    const patch: Record<string, unknown> = {};
    if (defaultSystemPromptId) patch.defaultSystemPromptId = defaultSystemPromptId;
    if (defaultScenarioId) patch.defaultScenarioId = defaultScenarioId;
    if (c.physicalDescription) patch.physicalDescription = c.physicalDescription;
    if (Object.keys(patch).length > 0) await repos.characters.update(c.id, patch as never);
  }

  // 6. Memories + their vector index entries (the carina builder's idiom).
  const vectors = new VectorIndicesRepository();
  for (const m of spec.memories) {
    await repos.memories.create(
      {
        userId: spec.userId,
        characterId: m.characterId,
        aboutCharacterId: m.aboutCharacterId,
        chatId: null,
        projectId: null,
        content: m.content,
        summary: m.summary,
        keywords: [],
        tags: [],
        importance: m.importance,
        embedding: null,
        source: 'AUTO',
        witnessedContext: null,
        sourceMessageId: null,
        lastAccessedAt: null,
        reinforcementCount: m.reinforcementCount,
        lastReinforcedAt: null,
        relatedMemoryIds: [],
        reinforcedImportance: m.importance,
      } as never,
      { id: m.id, createdAt: m.createdAt, updatedAt: m.createdAt } as never,
    );
    // A `vector: null` memory has NO index entry — the one shape that makes
    // the optimizer's semantic-search arm discriminate from "every about-self
    // memory" (a 500-hit search over a nine-entry index matches them all).
    if (m.vector) {
      await vectors.saveMeta(m.characterId, m.vector.length);
      await vectors.addEntry({
        id: m.id,
        characterId: m.characterId,
        embedding: new Float32Array(m.vector),
      });
    }
  }

  // Force the empty tables the read paths touch into existence.
  await repos.backgroundJobs.findByUserId(spec.userId, 'PENDING');
  await repos.vectorIndices.findMetaByCharacterId(spec.characters[1].id);
  await repos.vectorIndices.findEntriesByCharacterId(spec.characters[1].id);
  await repos.groupCharacterMembers.findByCharacterId(spec.characters[0].id);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built character-generators fixture: main=${mainOut} mount=${mountOut} ` +
      `(${spec.characters.length} characters, ${spec.files.length} files, ${spec.memories.length} memories)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`character-generators fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
