/**
 * Fixture builder for the W4.1d4 search/introspection tool differential
 * (request_full_context / project_info / help_search / search).
 *
 * Bakes ONE shared state across the TWO databases (main + mount-index) so BOTH v4
 * and the Rust port read it identically; each oracle/harness case runs on its own
 * fresh copy (search + request_full_context WRITE):
 *   - Char A (responding, owned by the user) + Char B (roster-only), via the REAL
 *     repos.characters.create — each mints a linked vault mount.
 *   - Char A's vault gets a Knowledge/ file (via linkDocumentContent) + a chunk
 *     with an embedding (so the self-vault knowledge tier + qtap://self URI fire).
 *   - A rich project P (roster [A,B], description, instructions, a colour) + a
 *     minimal project, via the REAL repos.projects.create — each mints a
 *     "Project Files: <name>" store. P's store gets a Docs/ file (document hit), a
 *     Knowledge/ file (knowledge hit), each with a chunk+embedding, and a blob.
 *   - The Quilltap General singleton (id persisted into MAIN instance_settings)
 *     with a Knowledge/ file + chunk (global tier).
 *   - A chat scoped to Char A (participant A) with projectId=P + userId=U (for the
 *     conversation-search scope + project chatCount + request_full_context write),
 *     and a minimal second chat, via the REAL repos.chats.create.
 *   - Char A memories + per-character vector index (memory search).
 *   - conversation_chunks for the chat (conversation search).
 *   - help_docs with embeddings (help search).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TMP_MAIN=/tmp/qt-search-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-search-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-search-tools-fixture.ts
 *
 * The minted char-vault + project-store ids are echoed to a JSON sidecar
 * (QT_FIXTURE_TMP_MAIN + '.meta.json') that the oracle + Rust test read.
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface StoreFile {
  relativePath: string;
  fileType: string;
  content: string;
}
interface StoreBlob {
  relativePath: string;
  bytes: number[];
}
interface DocChunk {
  id: string;
  store: 'project' | 'character' | 'global';
  relativePath: string;
  chunkIndex: number;
  headingContext: string | null;
  content: string;
  vector: number[];
}
interface SeedMemory {
  id: string;
  content: string;
  summary: string;
  importance: number;
  reinforcedImportance: number;
  source: string;
  /** Episodic spine fields (P4.d13 unit 6). */
  occurredAt?: string | null;
  narrativeTime?: string | null;
  kind?: string;
  aboutCharacterId?: string | null;
  chatId?: string | null;
  vector: number[];
}
interface ConvChunk {
  id: string;
  interchangeIndex: number;
  content: string;
  participantNames: string[];
  messageIds: string[];
  vector: number[];
}
interface HelpDoc {
  id: string;
  title: string;
  path: string;
  url: string;
  content: string;
  vector: number[];
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  charAId: string;
  charBId: string;
  generalMountPointId: string;
  generalStoreName: string;
  projectId: string;
  projectName: string;
  projectDescription: string;
  projectInstructions: string;
  minimalProjectId: string;
  minimalProjectName: string;
  chatAId: string;
  chatMinimalId: string;
  seedTimestamp: string;
  characterA: Record<string, unknown>;
  characterB: Record<string, unknown>;
  projectStoreFiles: StoreFile[];
  projectStoreBlobs: StoreBlob[];
  characterKnowledgeFiles: StoreFile[];
  globalKnowledgeFiles: StoreFile[];
  docChunks: DocChunk[];
  memories: SeedMemory[];
  conversationChunks: ConvChunk[];
  helpDocs: HelpDoc[];
  helpDocChunks: HelpDocChunk[];
}

/** P4.D77 — one section row of a help document (v4 `24633026`). */
interface HelpDocChunk {
  id: string;
  docId: string;
  chunkIndex: number;
  heading: string | null;
  content: string;
  vector: number[];
}

const PINNED_TS = '2026-02-01T00:00:00.000Z';

// A >500-UTF-16-unit body that contains NO literal-phrase match for any query (so
// the chunk's cosine is not boosted); used to prove content truncation to 500 + '…'.
const TRUNCATION_BODY = 'lorem ipsum dolor '.repeat(40).trim();

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'search-tools.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_TMP_MAIN;
  const mountOut = process.env.QT_FIXTURE_TMP_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_TMP_MAIN and QT_FIXTURE_TMP_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-search-fixture-build-'));
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
  const {
    CharacterSchema,
    ProjectSchema,
    ChatMetadataSchema,
    MemorySchema,
    ConversationChunkSchema,
    FileEntrySchema,
  } = await import('@/lib/schemas/types');
  const { HelpDocSchema } = await import('@/lib/schemas/help-doc.types');
  const { HelpDocChunkSchema } = await import('@/lib/schemas/help-doc-chunk.types');
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
  const { DocMountFileLinksRepository } = await import(
    '@/lib/database/repositories/doc-mount-file-links.repository'
  );
  const { DocMountChunksRepository } = await import(
    '@/lib/database/repositories/doc-mount-chunks.repository'
  );
  const { sha256OfString } = await import('@/lib/utils/sha256');

  await initializeDatabase();
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('projects', ProjectSchema);
  await ensureCollection('chats', ChatMetadataSchema);
  await ensureCollection('memories', MemorySchema);
  await ensureCollection('conversation_chunks', ConversationChunkSchema);
  await ensureCollection('help_docs', HelpDocSchema);
  // P4.D77 — the section table help_search now scores.
  await ensureCollection('help_doc_chunks', HelpDocChunkSchema);
  // The `files` table is empty here, but the port's project_info counts it via a
  // scoped SQL COUNT (v4 uses findAll().filter, which tolerates a missing table);
  // materialize it so both sides count 0 instead of the port erroring.
  await ensureCollection('files', FileEntrySchema);

  // Materialize the mount-index DDL up front (repos.characters/projects.create
  // scaffold vaults through linkDocumentContent, which needs these tables). Chunks
  // are seeded directly later, like the knowledge-injector fixture.
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
  // absent from the schema, so generateDDL would omit it). Let the repo create its
  // own correct table (CREATE TABLE IF NOT EXISTS) by touching it once.
  await repos.docMountBlobs.listByMountPoint(spec.generalMountPointId);

  // 1. Characters (mints + links their vaults).
  await repos.characters.create(spec.characterA as never, {
    id: spec.charAId,
    createdAt: PINNED_TS,
    updatedAt: PINNED_TS,
  } as never);
  await repos.characters.create(spec.characterB as never, {
    id: spec.charBId,
    createdAt: PINNED_TS,
    updatedAt: PINNED_TS,
  } as never);
  const charA = await repos.characters.findByIdRaw(spec.charAId);
  const charAVault = charA?.characterDocumentMountPointId as string | null;
  if (!charAVault) throw new Error('character A vault not minted');

  // 2. Projects (each mints a "Project Files: <name>" store).
  await repos.projects.create(
    {
      name: spec.projectName,
      description: spec.projectDescription,
      instructions: spec.projectInstructions,
      characterRoster: [spec.charAId, spec.charBId],
      allowAnyCharacter: true,
    } as never,
    { id: spec.projectId, createdAt: PINNED_TS, updatedAt: PINNED_TS } as never,
  );
  await repos.projects.create(
    {
      name: spec.minimalProjectName,
      description: null,
      instructions: null,
      characterRoster: [],
      allowAnyCharacter: false,
    } as never,
    { id: spec.minimalProjectId, createdAt: PINNED_TS, updatedAt: PINNED_TS } as never,
  );
  const project = await repos.projects.findById(spec.projectId);
  const projectStoreId = project?.officialMountPointId as string | null;
  if (!projectStoreId) throw new Error('project store not minted');

  // 3. The Quilltap General singleton (pinned id) + MAIN instance_settings.
  await repos.docMountPoints.create(
    {
      name: spec.generalStoreName,
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
    { id: spec.generalMountPointId, createdAt: PINNED_TS, updatedAt: PINNED_TS },
  );
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    spec.generalMountPointId,
  ]);

  const links = new DocMountFileLinksRepository();

  // Helper: write a text file into a store via the REAL linkDocumentContent.
  const writeFileTo = async (mountPointId: string, f: StoreFile): Promise<void> => {
    const rel = f.relativePath;
    const content = f.content.replace('LONG_BODY_PLACEHOLDER', TRUNCATION_BODY);
    await links.linkDocumentContent({
      mountPointId,
      relativePath: rel,
      fileName: rel.includes('/') ? rel.slice(rel.lastIndexOf('/') + 1) : rel,
      folderId: null,
      fileType: f.fileType as never,
      content,
      contentSha256: sha256OfString(content),
      plainTextLength: content.length,
      fileSizeBytes: Buffer.byteLength(content, 'utf-8'),
    } as never);
  };

  // 4. Store files.
  for (const f of spec.projectStoreFiles) await writeFileTo(projectStoreId, f);
  for (const f of spec.characterKnowledgeFiles) await writeFileTo(charAVault, f);
  for (const f of spec.globalKnowledgeFiles) await writeFileTo(spec.generalMountPointId, f);

  // 5. Store blobs (via linkBlobContent — creates file + link + blob row).
  for (const b of spec.projectStoreBlobs) {
    const rel = b.relativePath;
    const data = Buffer.from(b.bytes);
    await links.linkBlobContent({
      mountPointId: projectStoreId,
      relativePath: rel,
      fileName: rel.includes('/') ? rel.slice(rel.lastIndexOf('/') + 1) : rel,
      folderId: null,
      originalFileName: rel.includes('/') ? rel.slice(rel.lastIndexOf('/') + 1) : rel,
      originalMimeType: 'application/octet-stream',
      storedMimeType: 'application/octet-stream',
      sha256: 'advisory',
      data,
    } as never);
  }

  // 6. Doc chunks (embeddings direct). Resolve each chunk's linkId by (store, path).
  const storeIdFor = (s: DocChunk['store']): string =>
    s === 'project' ? projectStoreId : s === 'character' ? charAVault : spec.generalMountPointId;
  const chunks = new DocMountChunksRepository();
  for (const c of spec.docChunks) {
    const mountPointId = storeIdFor(c.store);
    const linkRow = await links.findByMountPointAndPath(mountPointId, c.relativePath);
    if (!linkRow) throw new Error(`no link for doc chunk ${c.id} at ${c.store}:${c.relativePath}`);
    const chunkContent = c.content === 'TRUNCATION_BODY' ? TRUNCATION_BODY : c.content;
    await chunks.create(
      {
        linkId: linkRow.id,
        mountPointId,
        chunkIndex: c.chunkIndex,
        content: chunkContent,
        tokenCount: chunkContent.length,
        headingContext: c.headingContext,
        embedding: new Float32Array(c.vector),
      } as never,
      { id: c.id, createdAt: PINNED_TS, updatedAt: PINNED_TS },
    );
  }

  // 7. Chats (real create). Chat A: participant A, projectId=P, userId=U.
  const participantA = {
    id: 'fa000000-0000-4000-8000-000000000001',
    type: 'CHARACTER',
    characterId: spec.charAId,
    controlledBy: 'llm',
    displayOrder: 0,
    isActive: true,
    status: 'active',
    hasHistoryAccess: false,
    createdAt: spec.seedTimestamp,
    updatedAt: spec.seedTimestamp,
  };
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Star Navigation',
      participants: [participantA],
      projectId: spec.projectId,
    } as never,
    { id: spec.chatAId, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp } as never,
  );
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Minimal',
      participants: [
        {
          id: 'fa000000-0000-4000-8000-000000000009',
          type: 'CHARACTER',
          characterId: spec.charBId,
          controlledBy: 'llm',
          displayOrder: 0,
          isActive: true,
          status: 'active',
          hasHistoryAccess: false,
          createdAt: spec.seedTimestamp,
          updatedAt: spec.seedTimestamp,
        },
      ],
    } as never,
    { id: spec.chatMinimalId, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp } as never,
  );

  // 8. Char A memories + per-character vector index.
  const vectors = repos.vectorIndices;
  for (const m of spec.memories) {
    await repos.memories.create(
      {
        characterId: spec.charAId,
        aboutCharacterId: m.aboutCharacterId ?? null,
        chatId: m.chatId ?? null,
        projectId: null,
        content: m.content,
        summary: m.summary,
        keywords: [],
        tags: [],
        importance: m.importance,
        embedding: null,
        source: m.source,
        witnessedContext: null,
        sourceMessageId: null,
        lastAccessedAt: null,
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: [],
        reinforcedImportance: m.reinforcedImportance,
        occurredAt: m.occurredAt ?? null,
        narrativeTime: m.narrativeTime ?? null,
        kind: m.kind ?? 'semantic',
        entities: [],
      } as never,
      { id: m.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp },
    );
    await vectors.saveMeta(spec.charAId, m.vector.length);
    await vectors.addEntry({
      id: m.id,
      characterId: spec.charAId,
      embedding: new Float32Array(m.vector),
    });
  }

  // 9. conversation_chunks for chat A.
  const convRepo = repos.conversationChunks;
  for (const c of spec.conversationChunks) {
    await convRepo.create(
      {
        chatId: spec.chatAId,
        interchangeIndex: c.interchangeIndex,
        content: c.content,
        participantNames: c.participantNames,
        messageIds: c.messageIds,
        embedding: c.vector,
      } as never,
      { id: c.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp },
    );
  }

  // 10. help_docs.
  const helpRepo = repos.helpDocs;
  for (const h of spec.helpDocs) {
    await helpRepo.create(
      {
        title: h.title,
        path: h.path,
        url: h.url,
        content: h.content,
        contentHash: sha256OfString(h.content),
        embedding: h.vector,
      } as never,
      { id: h.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp },
    );
  }

  // 11. help_doc_chunks (P4.D77, v4 `24633026`) — the section rows help_search
  // now scores alongside the whole-document vectors. Each row's arm is
  // documented in the spec's own `_note`.
  const helpChunkRepo = repos.helpDocChunks;
  for (const c of spec.helpDocChunks) {
    await helpChunkRepo.create(
      {
        docId: c.docId,
        chunkIndex: c.chunkIndex,
        heading: c.heading,
        content: c.content,
        embedding: c.vector,
      } as never,
      { id: c.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp },
    );
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  const meta = { charAVault, projectStoreId };
  writeFileSync(mainOut + '.meta.json', JSON.stringify(meta));
  process.stderr.write(
    `built search-tools fixtures: main=${mainOut} mount=${mountOut} charAVault=${charAVault} projectStore=${projectStoreId}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`search-tools fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
