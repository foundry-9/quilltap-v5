/**
 * Tier-3 fixture builder for the memory processor (v4 `processTurnForMemory`).
 *
 * Bakes the SEED state both differential sides start from, across BOTH
 * databases: the participant characters (via v4's REAL
 * `repos.characters.create` — pinned ids, full vault provisioning in the
 * mount-index DB), one `Others/<subject>.md` canon file in Aria's vault (via
 * the real `linkDocumentContent` — NOT `writeDatabaseDocument`, whose
 * `QUILLTAP_JOB_CHILD=1` skip-switch breaks `initializeDatabase`; see the
 * document-store oracle notes), the gate-band seed memories + their
 * per-character vector index entries (as in the memory-gate fixture), and the
 * FUTURE-dated rate-limit ballast rows (a 2099 `createdAt` is always "created
 * in the last hour" regardless of wall clock, so `countCreatedSince` is
 * deterministic).
 *
 * Both the jest oracle and the Rust test then copy BOTH fixture files and run
 * only the processor — which mints new ids/timestamps — so the seed rows are
 * byte-identical on both sides and only the processor's own effect differs.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-memory-processor-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-memory-processor-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-memory-processor-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CharacterSpec {
  id: string;
  name: string;
  identity?: string;
  description?: string;
  manifesto?: string;
  personality?: string;
  controlledBy: string;
  othersFiles: Array<{ path: string; content: string }>;
}
interface SeedMemory {
  id: string;
  characterId: string;
  content: string;
  summary: string;
  vector: number[] | null;
  createdAt: 'seed' | 'future';
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  futureTimestamp: string;
  characters: CharacterSpec[];
  seedMemories: SeedMemory[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'memory-processor-tier3.json'), 'utf8')
  ) as Spec;

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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-memory-processor-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { DocMountFileLinksRepository } = await import(
    '@/lib/database/repositories/doc-mount-file-links.repository'
  );
  const { sha256OfString } = await import('@/lib/utils/sha256');
  const { VectorIndicesRepository } = await import(
    '@/lib/database/repositories/vector-indices.repository'
  );

  await initializeDatabase();
  const repos = getRepositories();

  // Materialize the mount-index content tables (hand-written repos whose DDL
  // v4 emits at startup elsewhere; the vault scaffold needs them).
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) {
      midb.exec(sql);
    }
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

  // Observer canon files (Others/<name>.md) into the owning vault.
  const links = new DocMountFileLinksRepository();
  let othersFiles = 0;
  for (const c of spec.characters) {
    if (c.othersFiles.length === 0) continue;
    const raw = await repos.characters.findByIdRaw(c.id);
    const mountId = raw?.characterDocumentMountPointId;
    if (!mountId) throw new Error(`character ${c.name} has no vault mount point`);
    for (const f of c.othersFiles) {
      const rel = f.path;
      await links.linkDocumentContent({
        mountPointId: mountId,
        relativePath: rel,
        fileName: rel.includes('/') ? rel.slice(rel.lastIndexOf('/') + 1) : rel,
        folderId: null,
        fileType: 'markdown',
        content: f.content,
        contentSha256: sha256OfString(f.content),
        plainTextLength: f.content.length,
        fileSizeBytes: Buffer.byteLength(f.content, 'utf-8'),
      });
      othersFiles++;
    }
  }

  // Seed memories: gate-band rows (pinned seed timestamp + vector entry) and
  // future-dated rate-limit ballast (no vector).
  const vectors = new VectorIndicesRepository();
  let seededMemories = 0;
  let seededVectors = 0;
  for (const seed of spec.seedMemories) {
    const ts = seed.createdAt === 'future' ? spec.futureTimestamp : spec.seedTimestamp;
    await repos.memories.create(
      {
        characterId: seed.characterId,
        aboutCharacterId: null,
        chatId: null,
        projectId: null,
        content: seed.content,
        summary: seed.summary,
        keywords: [],
        tags: [],
        importance: 0.5,
        embedding: null,
        source: 'AUTO',
        witnessedContext: null,
        sourceMessageId: null,
        lastAccessedAt: null,
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: [],
        reinforcedImportance: 0.5,
      } as never,
      { id: seed.id, createdAt: ts, updatedAt: ts }
    );
    seededMemories++;

    if (seed.vector) {
      await vectors.saveMeta(seed.characterId, seed.vector.length);
      await vectors.addEntry({
        id: seed.id,
        characterId: seed.characterId,
        embedding: new Float32Array(seed.vector),
      });
      seededVectors++;
    }
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built memory-processor fixture: ${outMain} + ${outMount} ` +
      `(${spec.characters.length} characters, ${othersFiles} canon files, ` +
      `${seededMemories} seed memories, ${seededVectors} vectors)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-processor fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
