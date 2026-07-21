/**
 * Tier-3 fixture builder for the buildContext capstone (v4 `buildContext`,
 * lib/chat/context-manager.ts:477).
 *
 * Bakes the SEED state both differential sides start from, across BOTH
 * databases: the responding character with a full vault (via v4's REAL
 * `repos.characters.create`, pinned id), its `Knowledge/*.md` files (via the real
 * `linkDocumentContent`) plus the `doc_mount_chunks` rows with embeddings
 * directly (linkDocumentContent doesn't chunk/embed), the gate-band seed memory +
 * its per-character vector index (`saveMeta` + `addEntry`), and a bare chat row
 * (via `repos.chats.create`) so the internal `getMessages` reads have a table.
 *
 * Both the jest oracle and the Rust test then copy BOTH fixture files. buildContext
 * mints no persistent rows on the read side (its writes are the seamed whisper
 * posts, all mocked/recorded), so the seed rows are byte-identical on both sides.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-build-context-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-build-context-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-context-tier3-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface KnowledgeFile {
  path: string;
  fileType: string;
  content: string;
}
interface KnowledgeChunk {
  id: string;
  path: string;
  chunkIndex: number;
  headingContext: string | null;
  content: string;
  vector: number[];
}
interface CharacterSpec {
  id: string;
  name: string;
  identity?: string;
  description?: string;
  manifesto?: string;
  personality?: string;
  controlledBy: string;
  knowledgeFiles: KnowledgeFile[];
  knowledgeChunks: KnowledgeChunk[];
}
interface SeedMemory {
  id: string;
  characterId: string;
  aboutCharacterId?: string | null;
  content: string;
  summary: string;
  importance: number;
  /** Targeting-tag keywords (episodic arm: `past` / `scope: wide` / context words). */
  keywords?: string[];
  /** Episodic spine fields (P4.d13 retro arm). */
  occurredAt?: string | null;
  narrativeTime?: string | null;
  kind?: string;
  entities?: string[];
  /** Per-character vector-index entry (null → not indexed). */
  vector: number[] | null;
  /**
   * The memory row's own `embedding` column (the entity-anchor rescue path
   * reads it when the memory is NOT in the vector index).
   */
  embeddingColumn?: number[] | null;
}
interface ChatSpec {
  id: string;
  type: string;
  compactionGeneration: number;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  characters: CharacterSpec[];
  seedMemories: SeedMemory[];
  chat: ChatSpec;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'build-context-tier3.json'), 'utf8')) as Spec;

  const outMain = process.env.QT_FIXTURE_OUT;
  const outMount = process.env.QT_FIXTURE_MOUNT_OUT;
  if (!outMain || !outMount) {
    throw new Error('QT_FIXTURE_OUT and QT_FIXTURE_MOUNT_OUT must point at the fixture .db files');
  }
  for (const base of [outMain, outMount]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = base + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-build-context-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { DocMountFileLinksRepository } = await import(
    '@/lib/database/repositories/doc-mount-file-links.repository'
  );
  const { DocMountChunksRepository } = await import(
    '@/lib/database/repositories/doc-mount-chunks.repository'
  );
  const { sha256OfString } = await import('@/lib/utils/sha256');
  const { VectorIndicesRepository } = await import(
    '@/lib/database/repositories/vector-indices.repository'
  );
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  const repos = getRepositories();

  // Materialize the mount-index content tables (the vault scaffold needs them).
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }

  // Characters with full vault provisioning, pinned ids.
  for (const c of spec.characters) {
    await repos.characters.create(
      {
        name: c.name,
        userId: spec.userId,
        identity: c.identity ?? null,
        description: c.description ?? null,
        manifesto: c.manifesto ?? null,
        personality: c.personality ?? null,
        controlledBy: c.controlledBy,
      } as never,
      { id: c.id }
    );
  }

  // Knowledge files into the responding character's vault + chunks with embeddings.
  const links = new DocMountFileLinksRepository();
  const chunks = new DocMountChunksRepository();
  let fileCount = 0;
  let chunkCount = 0;
  for (const c of spec.characters) {
    if (c.knowledgeFiles.length === 0) continue;
    const raw = await repos.characters.findByIdRaw(c.id);
    const mountId = raw?.characterDocumentMountPointId;
    if (!mountId) throw new Error(`character ${c.name} has no vault mount point`);
    const linkIdByPath = new Map<string, string>();
    for (const f of c.knowledgeFiles) {
      const rel = f.path;
      const { link } = await links.linkDocumentContent({
        mountPointId: mountId,
        relativePath: rel,
        fileName: rel.includes('/') ? rel.slice(rel.lastIndexOf('/') + 1) : rel,
        folderId: null,
        fileType: f.fileType as never,
        content: f.content,
        contentSha256: sha256OfString(f.content),
        plainTextLength: f.content.length,
        fileSizeBytes: Buffer.byteLength(f.content, 'utf-8'),
      });
      linkIdByPath.set(rel, link.id);
      fileCount++;
    }
    for (const ch of c.knowledgeChunks) {
      const linkId = linkIdByPath.get(ch.path);
      if (!linkId) throw new Error(`no link for chunk ${ch.id} at ${ch.path}`);
      await chunks.create(
        {
          linkId,
          mountPointId: mountId,
          chunkIndex: ch.chunkIndex,
          content: ch.content,
          tokenCount: ch.content.length,
          headingContext: ch.headingContext,
          embedding: new Float32Array(ch.vector),
        } as never,
        { id: ch.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
      );
      chunkCount++;
    }
  }

  // Seed memories + per-character vector index entries.
  const vectors = new VectorIndicesRepository();
  let memoryCount = 0;
  for (const m of spec.seedMemories) {
    await repos.memories.create(
      {
        characterId: m.characterId,
        aboutCharacterId: m.aboutCharacterId ?? null,
        chatId: null,
        projectId: null,
        content: m.content,
        summary: m.summary,
        keywords: m.keywords ?? [],
        tags: [],
        importance: m.importance,
        embedding: m.embeddingColumn ?? null,
        source: 'AUTO',
        witnessedContext: null,
        sourceMessageId: null,
        lastAccessedAt: null,
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: [],
        reinforcedImportance: m.importance,
        // Episodic spine (P4.d13 retro arm).
        occurredAt: m.occurredAt ?? null,
        narrativeTime: m.narrativeTime ?? null,
        kind: m.kind ?? 'semantic',
        entities: m.entities ?? [],
      } as never,
      { id: m.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
    memoryCount++;
    if (m.vector) {
      await vectors.saveMeta(m.characterId, m.vector.length);
      await vectors.addEntry({
        id: m.id,
        characterId: m.characterId,
        embedding: new Float32Array(m.vector),
      });
    }
  }

  // A bare chat row so buildContext's internal getMessages reads have a table.
  await repos.chats.create(
    {
      userId: spec.userId,
      characterId: spec.characters[0].id,
      title: 'Analytical Engine',
      type: spec.chat.type,
      participants: [],
      compactionGeneration: spec.chat.compactionGeneration,
    } as never,
    { id: spec.chat.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
  );

  // Materialize the (empty) chat_messages table: the live whisper posts write
  // through it on BOTH sides, and the Rust writer does not create tables lazily
  // the way v4's collection layer does.
  await repos.chats.getMessages(spec.chat.id);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built build-context fixture: ${outMain} + ${outMount} ` +
      `(${spec.characters.length} chars, ${fileCount} knowledge files, ${chunkCount} chunks, ${memoryCount} memories)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`build-context fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
