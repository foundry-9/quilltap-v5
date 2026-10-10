/**
 * Tier-3 SEED fixture builder for the fold-grain OTHER pass (P4.D270 — v4
 * `d58548051` `lib/memory/fold-other-pass.ts`, the idle catch-up sweep and the
 * `MEMORY_EXTRACTION` handler's `foldOtherCatchup` branch; pin `01a83539d`).
 *
 * Bakes the SEED both differential sides start from, across BOTH databases,
 * through v4's REAL repositories (the memory-pipeline-jobs builder's idiom):
 *   - the two connection profiles + one chat_settings row;
 *   - six characters with pronouns (pinned ids, full vault provisioning); Dov's
 *     vault carries `Others/Ada.md` + `Commonplace/Ada.md` (the canon the
 *     fold-grain call reads, incl. the Commonplace digest);
 *   - the chats + every message through `repos.chats.addMessage` with PINNED
 *     ids + createdAt; then each chat's `lastMessageAt` / `messageCount` /
 *     watermark / `contextSummary` / `timelineMode` set by a direct UPDATE so
 *     the idle catch-up's windows are exact against the corpus's FIXED `now`;
 *   - the rate-limit ballast (FUTURE-dated memories — always "in the last
 *     hour") for the rate-limited and throttled observers;
 *   - the lazily-created tables the ops write (`memories` + the V2 vector
 *     tables, `background_jobs`, `instance_settings`), created empty;
 *   - the gate-write-failure trigger plant (created LAST, so no seed trips it).
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-fop-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-fop-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-fold-other-pass-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface ProfileSpec {
  id: string;
  name: string;
  provider: string;
  modelName: string;
  isCheap: boolean;
  isDangerousCompatible: boolean;
}
interface CharSpec {
  id: string;
  name: string;
  controlledBy: string;
  identity?: string;
  pronouns?: { subject: string; object: string; possessive: string };
  files: Array<{ path: string; content: string }>;
}
interface MessageSeed {
  id: string;
  role: string;
  content: string;
  participantId: string | null;
  createdAt: string;
  systemSender?: string;
}
interface ChatSpec {
  id: string;
  title: string;
  chatType?: string;
  timelineMode?: string;
  lastMessageAt: string;
  messageCount: number;
  otherExtractionWatermarkMessageId?: string;
  contextSummary?: string;
  participants: Array<{
    id: string;
    characterId: string;
    controlledBy: string;
    connectionProfileId?: string;
  }>;
  messages: MessageSeed[];
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  futureTimestamp: string;
  chatSettings: {
    cheapLLMSettings: { strategy: string; fallbackToLocal: boolean };
    dangerMode: string;
  };
  connectionProfiles: ProfileSpec[];
  characters: CharSpec[];
  chats: ChatSpec[];
  ballast: Array<{ characterId: string; count: number }>;
  plants: Array<{ name: string; content: string; characterId: string; message: string }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'fold-other-pass-tier3.json'), 'utf8')
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-fop-build-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
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
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { DocMountFileLinksRepository } = await import(
    '@/lib/database/repositories/doc-mount-file-links.repository'
  );
  const { sha256OfString } = await import('@/lib/utils/sha256');
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

  for (const p of spec.connectionProfiles) {
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

  await repos.chatSettings.create(
    {
      userId: spec.userId,
      cheapLLMSettings: spec.chatSettings.cheapLLMSettings,
      dangerousContentSettings: { mode: spec.chatSettings.dangerMode },
    } as never,
    {
      id: '5e000000-0000-4000-8000-00000000f0f0',
      createdAt: spec.seedTimestamp,
      updatedAt: spec.seedTimestamp,
    }
  );

  const links = new DocMountFileLinksRepository();
  for (const c of spec.characters) {
    await repos.characters.create(
      {
        name: c.name,
        userId: spec.userId,
        controlledBy: c.controlledBy,
        identity: c.identity ?? null,
        pronouns: c.pronouns ?? null,
      } as never,
      { id: c.id }
    );
    if (c.files.length === 0) continue;
    const raw = await repos.characters.findByIdRaw(c.id);
    const mountId = raw?.characterDocumentMountPointId;
    if (!mountId) throw new Error(`character ${c.name} has no vault mount point`);
    for (const f of c.files) {
      await links.linkDocumentContent({
        mountPointId: mountId,
        relativePath: f.path,
        fileName: f.path.slice(f.path.lastIndexOf('/') + 1),
        folderId: null,
        fileType: 'markdown',
        content: f.content,
        contentSha256: sha256OfString(f.content),
        plainTextLength: f.content.length,
        fileSizeBytes: Buffer.byteLength(f.content, 'utf-8'),
      });
    }
  }

  const mainDb = getRawDatabase();
  if (!mainDb) throw new Error('main DB handle unavailable');
  for (const chat of spec.chats) {
    const participants = chat.participants.map((p) => ({
      id: p.id,
      type: 'CHARACTER',
      characterId: p.characterId,
      controlledBy: p.controlledBy,
      ...(p.connectionProfileId ? { connectionProfileId: p.connectionProfileId } : {}),
      createdAt: spec.seedTimestamp,
      updatedAt: spec.seedTimestamp,
    }));
    await repos.chats.create(
      {
        userId: spec.userId,
        title: chat.title,
        chatType: chat.chatType ?? undefined,
        participants,
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
        systemSender: m.systemSender ?? null,
        systemKind: null,
        targetParticipantIds: null,
      } as never);
    }
    // The catch-up windows are exact against the corpus's FIXED `now`, so the
    // chat-level columns are SET, not left to what addMessage derived.
    mainDb
      .prepare(
        'UPDATE "chats" SET "lastMessageAt" = ?, "messageCount" = ?, ' +
          '"otherExtractionWatermarkMessageId" = ?, "contextSummary" = ?, ' +
          '"timelineMode" = COALESCE(?, "timelineMode"), "updatedAt" = ? WHERE "id" = ?'
      )
      .run(
        chat.lastMessageAt,
        chat.messageCount,
        chat.otherExtractionWatermarkMessageId ?? null,
        chat.contextSummary ?? null,
        chat.timelineMode ?? null,
        spec.seedTimestamp,
        chat.id
      );
  }

  // `memories` + the V2 vector tables + the rate-limit ballast (FUTURE-dated:
  // `countCreatedSince(now - 1h)` counts them whatever the wall clock).
  let ballastN = 0;
  for (const b of spec.ballast) {
    for (let i = 0; i < b.count; i++) {
      ballastN++;
      await repos.memories.create(
        {
          characterId: b.characterId,
          aboutCharacterId: null,
          chatId: null,
          projectId: null,
          content: `Rate-limit ballast ${ballastN}.`,
          summary: `ballast ${ballastN}`,
          keywords: [],
          tags: [],
          importance: 0.3,
          embedding: null,
          source: 'AUTO',
          witnessedContext: null,
          sourceMessageId: null,
          lastAccessedAt: null,
          reinforcementCount: 1,
          lastReinforcedAt: null,
          relatedMemoryIds: [],
          reinforcedImportance: 0.3,
        } as never,
        {
          id: `f0000000-0000-4000-8000-${String(ballastN).padStart(12, '0')}`,
          createdAt: spec.futureTimestamp,
          updatedAt: spec.futureTimestamp,
        }
      );
    }
  }
  const { VectorIndexMetaSchema, VectorEntryRowSchema } = await import(
    '@/lib/schemas/vector-indices.types'
  );
  for (const [name, schema] of [
    ['vector_indices', VectorIndexMetaSchema],
    ['vector_entries', VectorEntryRowSchema],
  ] as Array<[string, unknown]>) {
    for (const sql of generateDDL(name, schema as never)) {
      mainDb.exec(sql);
    }
  }
  // `background_jobs` via a real enqueue that is then deleted (v4's exact DDL,
  // zero rows); `instance_settings` via a real write that is then deleted.
  const { enqueueJob } = await import('@/lib/background-jobs/queue-service');
  const throwawayJobId = await enqueueJob(spec.userId, 'LLM_LOG_CLEANUP' as never, {} as never);
  mainDb.prepare('DELETE FROM "background_jobs" WHERE "id" = ?').run(throwawayJobId);
  // A builder's `initializeDatabase()` never runs the startup version guard,
  // so the k/v table the mode / limits plants write is absent (measured: v4's
  // `writeSetting` throws `no such table`). Created from the DDL
  // `fresh_schema.json` (and v4's `version-guard.ts:202`) carries, EMPTY.
  mainDb.exec(
    'CREATE TABLE IF NOT EXISTS "instance_settings" (\n  "key" TEXT PRIMARY KEY,\n  "value" TEXT NOT NULL\n)'
  );

  const sqlString = (v: string) => `'${v.replace(/'/g, "''")}'`;
  for (const plant of spec.plants) {
    mainDb.exec(
      `CREATE TRIGGER ${plant.name} BEFORE INSERT ON memories ` +
        `WHEN NEW.content = ${sqlString(plant.content)} AND NEW.characterId = ${sqlString(plant.characterId)} ` +
        `BEGIN SELECT RAISE(ABORT, ${sqlString(plant.message)}); END;`
    );
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built fold-other-pass fixture: ${outMain} + ${outMount} ` +
      `(${spec.characters.length} characters, ${spec.connectionProfiles.length} profiles, ` +
      `${spec.chats.length} chats, ${ballastN} ballast memories, ${spec.plants.length} plants)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fold-other-pass fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
