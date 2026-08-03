/**
 * Tier-3 SEED fixture builder for the Carina memory-extraction job handler
 * (v4 `handleCarinaMemoryExtraction`, lib/background-jobs/handlers/carina-memory-extraction.ts).
 *
 * Bakes the SEED both differential sides start from, across BOTH databases:
 *   - the two connection profiles (the answerer's ANTHROPIC current profile named
 *     by every payload's `connectionProfileId`, plus a cheap OPENAI profile that
 *     `getCheapLLMProvider(PROVIDER_CHEAPEST)` selects for the SELF extraction);
 *   - one chat_settings row for the user (cheapLLMSettings + dangerousContentSettings;
 *     autoHousekeepingSettings defaults to `enabled:false`, so the gate's watermark
 *     check bails without touching background_jobs — no background_jobs table needed);
 *   - the answerer characters (via v4's REAL `repos.characters.create` — pinned ids,
 *     full vault provisioning in the mount-index DB, pronouns so the SELF CONTEXT
 *     footer label renders deterministically);
 *   - Wren's one seed memory + its vector index entry (the reinforce case — its
 *     vector is 0.87*e_0 so a candidate e_0 lands in the REINFORCE band);
 *   - the chats (each with one CHARACTER participant, pinned id + timestamps);
 *   - the posted carina messages: `systemSender:'carina'` / `systemKind:'carina-response'`
 *     ASSISTANT messages carrying `carinaMeta:{answererId, question}` and the answer as
 *     content — byte-identical to what v4's `postCarinaResponse` (lib/services/carina/writer.ts)
 *     builds, but written through the SAME real `repos.chats.addMessage` path with a
 *     PINNED id + createdAt (postCarinaResponse's only nondeterminism is randomUUID/now,
 *     and the payload must name the carina message id — so we pin it and keep the
 *     message SHAPE exactly the writer's).
 *
 * Both the jest oracle and the Rust test copy BOTH fixture files and run only the
 * handler — which mints new ids/timestamps — so the seed rows are byte-identical on
 * both sides and only the handler's own effect differs.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=~/source/quilltap-v5/.claude/worktrees/carina-query-w4-5-a178ff
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-carina-mem-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-carina-mem-mount.db \
 *     $N/npx tsx $W/harness/oracle/fixtures/build-carina-memory-extraction-fixture.ts
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
  description?: string;
  pronouns?: { subject: string; object: string; possessive: string };
}
interface SeedMemory {
  id: string;
  characterId: string;
  content: string;
  summary: string;
  importance: number;
  vector: number[];
  createdAt: string;
}
interface ParticipantSeed {
  id: string;
  characterId: string;
  controlledBy: string;
}
interface ChatSpec {
  id: string;
  participants: ParticipantSeed[];
}
interface CarinaMessage {
  id: string;
  chatId: string;
  answererId: string;
  question: string;
  answer: string;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  carinaMessageTimestamp: string;
  chatSettings: {
    cheapLLMSettings: { strategy: string; fallbackToLocal: boolean };
    dangerMode: string;
  };
  connectionProfiles: ProfileSpec[];
  characters: CharSpec[];
  seedMemories: SeedMemory[];
  chats: ChatSpec[];
  carinaMessages: CarinaMessage[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'carina-memory-extraction-tier3.json'), 'utf8')
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-carina-mem-build-'));
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

  // Materialize the mount-index content tables (hand-written repos whose DDL v4
  // emits at startup elsewhere; the vault scaffold needs them).
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

  // Connection profiles (pinned ids; the cheap OPENAI one is selected for SELF
  // extraction under PROVIDER_CHEAPEST).
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

  // Chat settings for the user (the handler throws without a row). Danger stays
  // benign; autoHousekeepingSettings defaults to enabled:false so no watermark
  // enqueue fires.
  await repos.chatSettings.create(
    {
      userId: spec.userId,
      cheapLLMSettings: spec.chatSettings.cheapLLMSettings,
      dangerousContentSettings: { mode: spec.chatSettings.dangerMode },
    } as never,
    { id: '5e000000-0000-4000-8000-00000000c5e7', createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
  );

  // Answerer characters, full vault provisioning, pinned ids + pronouns.
  for (const c of spec.characters) {
    await repos.characters.create(
      {
        name: c.name,
        userId: spec.userId,
        controlledBy: c.controlledBy,
        identity: c.identity ?? null,
        description: c.description ?? null,
        pronouns: c.pronouns ?? null,
      } as never,
      { id: c.id }
    );
  }

  // Seed memories + vector index entries.
  const vectors = new VectorIndicesRepository();
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
        importance: seed.importance,
        embedding: null,
        source: 'AUTO',
        witnessedContext: null,
        sourceMessageId: null,
        lastAccessedAt: null,
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: [],
        reinforcedImportance: seed.importance,
      } as never,
      { id: seed.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
    await vectors.saveMeta(seed.characterId, seed.vector.length);
    await vectors.addEntry({
      id: seed.id,
      characterId: seed.characterId,
      embedding: new Float32Array(seed.vector),
    });
  }

  // Chats (each with one CHARACTER participant; pinned id + timestamps).
  for (const chat of spec.chats) {
    const participants = chat.participants.map((p) => ({
      id: p.id,
      type: 'CHARACTER',
      characterId: p.characterId,
      controlledBy: p.controlledBy,
      createdAt: spec.seedTimestamp,
      updatedAt: spec.seedTimestamp,
    }));
    await repos.chats.create(
      {
        userId: spec.userId,
        title: `Fixture ${chat.id}`,
        participants,
      } as never,
      { id: chat.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  // Posted carina messages (v4 `postCarinaResponse`'s MessageEvent shape, written
  // through the same real `repos.chats.addMessage`, with a PINNED id + createdAt).
  for (const m of spec.carinaMessages) {
    await repos.chats.addMessage(m.chatId, {
      type: 'message',
      id: m.id,
      role: 'ASSISTANT',
      content: m.answer,
      opaqueContent: m.answer,
      attachments: [],
      createdAt: spec.carinaMessageTimestamp,
      participantId: null,
      systemSender: 'carina',
      systemKind: 'carina-response',
      targetParticipantIds: null,
      carinaMeta: { answererId: m.answererId, question: m.question },
    } as never);
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built carina-memory-extraction fixture: ${outMain} + ${outMount} ` +
      `(${spec.characters.length} characters, ${spec.connectionProfiles.length} profiles, ` +
      `${spec.seedMemories.length} seed memories, ${spec.chats.length} chats, ` +
      `${spec.carinaMessages.length} carina messages)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`carina-memory-extraction fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
