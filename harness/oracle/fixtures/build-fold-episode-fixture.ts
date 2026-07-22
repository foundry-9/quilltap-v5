/**
 * Tier-3 fixture builder for the fold-time episode pass (v4
 * `runFoldEpisodePass`, lib/memory/fold-episode-pass.ts).
 *
 * Bakes the SEED state both differential sides start from, across BOTH
 * databases: the participant characters (via v4's REAL
 * `repos.characters.create` — pinned ids, full vault provisioning in the
 * mount-index DB), the chats whose participant rosters decide which characters
 * form the episode memory, and the per-turn FRAGMENT memories (pinned ids +
 * `sourceMessageId`s inside the folded window) the pass links the consolidated
 * episode to. No vector entries are seeded: the pass's gate writes must land as
 * plain INSERTs, and an empty per-character store is exactly that.
 *
 * Both the jest oracle and the Rust test then copy BOTH fixture files and run
 * only the pass — which mints new ids/timestamps — so the seed rows are
 * byte-identical on both sides and only the pass's own effect differs.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-fold-episode-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-fold-episode-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-fold-episode-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CharacterSpec {
  id: string;
  name: string;
  identity?: string;
  controlledBy: string;
}
interface ChatSpec {
  id: string;
  title: string;
  participants: Array<{
    id: string;
    characterId: string;
    controlledBy: string;
    status: string;
  }>;
}
interface SeedMemory {
  id: string;
  characterId: string;
  content: string;
  summary: string;
  sourceMessageId: string;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  characters: CharacterSpec[];
  chats: ChatSpec[];
  seedMemories: SeedMemory[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'fold-episode-tier3.json'), 'utf8')) as Spec;

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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-fold-episode-build-'));
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

  await initializeDatabase();
  const repos = getRepositories();

  // Materialize the mount-index content tables (as the memory-processor builder
  // does — the vault scaffold the character create runs needs them).
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

  // The vector tables the gate touches. v4's repos create their table lazily on
  // first use, so a fixture that seeds no vectors would leave `vector_indices`
  // absent — the oracle's own gate run would then create it while the Rust side
  // (which never emits DDL at runtime) would fail the open. Emit the DDL here so
  // BOTH sides start from the same empty tables.
  const maindb = (await import('@/lib/database/backends/sqlite/client')).getRawDatabase?.();
  // NOTE the v2 (BLOB-backed) schemas — `VectorIndexSchema` / `VectorEntrySchema`
  // are the legacy shapes and would emit a `vector_entries` without
  // `characterId`, which the real `addEntry` then fails on.
  const { VectorIndexMetaSchema, VectorEntryRowSchema } = await import(
    '@/lib/schemas/vector-indices.types'
  );
  if (!maindb) throw new Error('main DB handle unavailable');
  for (const [name, schema] of [
    ['vector_indices', VectorIndexMetaSchema],
    ['vector_entries', VectorEntryRowSchema],
  ] as Array<[string, unknown]>) {
    for (const sql of generateDDL(name, schema as never)) {
      maindb.exec(sql);
    }
  }

  for (const c of spec.characters) {
    await repos.characters.create(
      {
        name: c.name,
        userId: spec.userId,
        identity: c.identity ?? null,
        controlledBy: c.controlledBy,
      } as never,
      { id: c.id }
    );
  }

  for (const chat of spec.chats) {
    const participants = chat.participants.map((p) => ({
      id: p.id,
      type: 'CHARACTER',
      characterId: p.characterId,
      controlledBy: p.controlledBy,
      status: p.status,
      createdAt: spec.seedTimestamp,
      updatedAt: spec.seedTimestamp,
    }));
    await repos.chats.create(
      { userId: spec.userId, title: chat.title, participants } as never,
      { id: chat.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  for (const seed of spec.seedMemories) {
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
        occurredAt: null,
        narrativeTime: null,
        entities: [],
        kind: 'semantic',
        sourceMessageId: seed.sourceMessageId,
        lastAccessedAt: null,
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: [],
        reinforcedImportance: 0.5,
      } as never,
      { id: seed.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built fold-episode fixture: ${outMain} + ${outMount} (${spec.characters.length} characters, ` +
      `${spec.chats.length} chats, ${spec.seedMemories.length} fragment memories)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fold-episode fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
