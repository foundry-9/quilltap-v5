/**
 * Tier-3 fixture builder for the memory processor (v4 `processTurnForMemory`).
 *
 * Bakes the SEED state both differential sides start from, across BOTH
 * databases: the participant characters (via v4's REAL
 * `repos.characters.create` — pinned ids, full vault provisioning in the
 * mount-index DB), one `Others/<subject>.md` canon file in Aria's vault (via
 * the real `linkDocumentContent` — NOT `writeDatabaseDocument`, whose
 * `QUILLTAP_JOB_CHILD=1` skip-switch breaks `initializeDatabase`; see the
 * document-store oracle notes), the gate-band seed memories + their vectors on
 * `memories.embedding` (as in the memory-gate fixture; P4.D288 — v4
 * `af48f6cc2`, the `83334b9a5` pin: the per-character vector index entries
 * are gone with the mirror), and the
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
  pronouns?: { subject: string; object: string; possessive: string };
  othersFiles: Array<{ path: string; content: string }>;
}
// P4.D296: the extraction-turn family reads chats from the DB (v4's
// `buildExtractionContext` + `runTurnExtraction`), so the seed grows the
// profiles, the chat-settings row and the chats with stamped messages.
interface ProfileSpec {
  id: string;
  name: string;
  provider: string;
  modelName: string;
  isCheap: boolean;
  isDangerousCompatible: boolean;
}
interface ChatSpec {
  id: string;
  chatType?: string;
  /** Written after the messages (the fold catch-ups read them off the row). */
  summaryAnchorMessageIds?: string[];
  lastMessageAt?: string;
  messageCount?: number;
  /** Seat status overrides (`isParticipantPresent` reads them). */
  statuses?: Record<string, string>;
  participants: Array<{ id: string; characterId: string; controlledBy: string; connectionProfileId?: string }>;
  messages: Array<{ id: string; role: string; content: string; participantId: string | null; createdAt: string }>;
}
interface SeedMemory {
  id: string;
  characterId: string;
  /** P4.D296: a per-turn fragment the episode pass links (`findByCharacterAndSourceMessageIds`). */
  sourceMessageId?: string;
  chatId?: string;
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
  connectionProfiles?: ProfileSpec[];
  chatSettings?: { cheapLLMSettings: Record<string, unknown> };
  chats?: ChatSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  // P4.D296: the extraction families share this builder; each names its own
  // spec (default: the turn grain's).
  const spec = JSON.parse(
    readFileSync(join(here, process.env.QT_FIXTURE_SPEC ?? 'memory-processor-tier3.json'), 'utf8')
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
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
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
        pronouns: c.pronouns ?? null,
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

  // Seed memories: gate-band rows (pinned seed timestamp + vector on the row's
  // own `embedding` — P4.D288, v4 `af48f6cc2` at the `83334b9a5` pin: the
  // `vector_entries` mirror is gone) and future-dated rate-limit ballast (no
  // vector).
  let seededMemories = 0;
  let seededVectors = 0;
  for (const seed of spec.seedMemories) {
    const ts = seed.createdAt === 'future' ? spec.futureTimestamp : spec.seedTimestamp;
    await repos.memories.create(
      {
        characterId: seed.characterId,
        aboutCharacterId: null,
        chatId: seed.chatId ?? null,
        projectId: null,
        content: seed.content,
        summary: seed.summary,
        keywords: [],
        tags: [],
        importance: 0.5,
        embedding: seed.vector ? new Float32Array(seed.vector) : null,
        source: 'AUTO',
        witnessedContext: null,
        sourceMessageId: seed.sourceMessageId ?? null,
        lastAccessedAt: null,
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: [],
        reinforcedImportance: 0.5,
      } as never,
      { id: seed.id, createdAt: ts, updatedAt: ts }
    );
    seededMemories++;
    if (seed.vector) seededVectors++;
  }

  // P4.D296: profiles, the chat-settings row, the chats with pinned ids and
  // stamped messages (every window anchors to these — no Date pinning).
  for (const p of spec.connectionProfiles ?? []) {
    await repos.connections.create(
      {
        userId: spec.userId,
        name: p.name,
        provider: p.provider,
        modelName: p.modelName,
        isCheap: p.isCheap,
        isDangerousCompatible: p.isDangerousCompatible,
      } as never,
      { id: p.id }
    );
  }
  if (spec.chatSettings) {
    await repos.chatSettings.create(
      { userId: spec.userId, cheapLLMSettings: spec.chatSettings.cheapLLMSettings } as never,
      {
        id: '5e000000-0000-4000-8000-00000000c5e7',
        createdAt: spec.seedTimestamp,
        updatedAt: spec.seedTimestamp,
      }
    );
  }
  for (const chat of spec.chats ?? []) {
    await repos.chats.create(
      {
        userId: spec.userId,
        title: `Fixture ${chat.id}`,
        chatType: chat.chatType ?? undefined,
        participants: chat.participants.map((p) => ({
          id: p.id,
          type: 'CHARACTER',
          characterId: p.characterId,
          controlledBy: p.controlledBy,
          connectionProfileId: p.connectionProfileId ?? null,
          ...(chat.statuses?.[p.id] ? { status: chat.statuses[p.id] } : {}),
          createdAt: spec.seedTimestamp,
          updatedAt: spec.seedTimestamp,
        })),
      } as never,
      { id: chat.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
    for (const m of chat.messages) {
      await repos.chats.addMessage(chat.id, {
        type: 'message',
        id: m.id,
        role: m.role,
        content: m.content,
        opaqueContent: m.content,
        attachments: [],
        createdAt: m.createdAt,
        participantId: m.participantId,
        systemSender: null,
        systemKind: null,
        targetParticipantIds: null,
      } as never);
    }
    const rowExtras: Record<string, unknown> = {};
    if (chat.summaryAnchorMessageIds) rowExtras.summaryAnchorMessageIds = chat.summaryAnchorMessageIds;
    if (chat.lastMessageAt) rowExtras.lastMessageAt = chat.lastMessageAt;
    if (chat.messageCount !== undefined) rowExtras.messageCount = chat.messageCount;
    if (Object.keys(rowExtras).length > 0) await repos.chats.update(chat.id, rowExtras as never);
  }
  if ((spec.chats ?? []).length > 0) {
    // The lazily created tables the extraction path reads: the cursor table
    // (its repository's own DDL, through a real write then a clear) and
    // `instance_settings` (fresh_schema.json's DDL — v4 creates it only on a
    // migrated instance; memory note `builder-fixtures-lack-instance-settings`).
    const first = spec.chats![0].id;
    await repos.memoryExtractionCursors.setCursor(first, 'turn', spec.chats![0].messages[0].id);
    await repos.memoryExtractionCursors.clearForChat(first);
    // `memories` too (its repository's own DDL through a real read): the Rust
    // writer expects the table a booted instance carries.
    await repos.memories.findById('00000000-0000-4000-8000-000000000000');
    const { rawQuery } = await import('@/lib/database/manager');
    await rawQuery('CREATE TABLE IF NOT EXISTS "instance_settings" (\n  "key" TEXT PRIMARY KEY,\n  "value" TEXT NOT NULL\n)');
    // `background_jobs` (v4 creates it on the first enqueue): a real enqueue,
    // then the row deleted — v4's exact DDL, zero rows.
    const { enqueueJob } = await import('@/lib/background-jobs/queue-service');
    const throwaway = await enqueueJob(spec.userId, 'LLM_LOG_CLEANUP' as never, {} as never);
    await rawQuery('DELETE FROM "background_jobs" WHERE "id" = ?', [throwaway]);
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built memory-processor fixture: ${outMain} + ${outMount} ` +
      `(${spec.characters.length} characters, ${othersFiles} canon files, ` +
      `${seededMemories} seed memories, ${seededVectors} vectors, ${(spec.chats ?? []).length} chats)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-processor fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
