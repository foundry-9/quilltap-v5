/**
 * Tier-3 SEED fixture builder for the memory-pipeline job handlers
 * (P4.6bj units 2–3: v4 `handleMemoryExtraction` + `handleContextSummary`).
 *
 * Bakes the SEED both differential sides start from, across BOTH databases:
 *   - the two connection profiles (the ANTHROPIC current profile every payload
 *     names, plus a cheap OPENAI profile `getCheapLLMProvider(PROVIDER_CHEAPEST)`
 *     selects for the cheap-LLM tasks);
 *   - one chat_settings row (cheapLLMSettings + dangerousContentSettings
 *     AUTO_ROUTE — the CONTEXT_SUMMARY chain's mode gate is exercised ON);
 *   - the three characters (via v4's REAL `repos.characters.create` — pinned
 *     ids, full vault provisioning, pronouns for deterministic CONTEXT labels);
 *   - the chats (pinned participants; the two CONTEXT_SUMMARY chats reference
 *     NONEXISTENT characterIds so the fold's vault mirror/refresh no-op — the
 *     ctx-summary family's unprovisioned-character idiom);
 *   - every chat message through the same real `repos.chats.addMessage` with
 *     PINNED ids + createdAt (the transcript walk, the fold's [YYYY-MM-DD]
 *     prefixes, and the extraction CLOCK all anchor to these — no Date pinning
 *     needed anywhere in the family).
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<the v5 worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-mpj-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-mpj-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-memory-pipeline-jobs-fixture.ts
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
}
interface MessageSeed {
  id: string;
  role: string;
  content: string;
  participantId: string | null;
  createdAt: string;
}
interface ChatSpec {
  id: string;
  chatType?: string;
  participants: Array<{ id: string; characterId: string; controlledBy: string }>;
  messages: MessageSeed[];
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  chatSettings: {
    cheapLLMSettings: { strategy: string; fallbackToLocal: boolean };
    dangerMode: string;
  };
  connectionProfiles: ProfileSpec[];
  characters: CharSpec[];
  chats: ChatSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'memory-pipeline-jobs-tier3.json'), 'utf8')
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mpj-build-'));
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
    for (const sql of generateDDL(name, schema as never)) {
      midb.exec(sql);
    }
  }

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
      id: '5e000000-0000-4000-8000-00000000c5e7',
      createdAt: spec.seedTimestamp,
      updatedAt: spec.seedTimestamp,
    }
  );

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
  }

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
        systemSender: null,
        systemKind: null,
        targetParticipantIds: null,
      } as never);
    }
  }

  // Materialize the LAZILY-created main tables the handlers write to — v4
  // creates them on first write, but the Rust side expects them present.
  // `memories` + the V2 vector tables via a real seed row (an ORTHOGONAL
  // vector, e1, so the extraction candidates' e0 embeddings never land in its
  // reinforce band); `background_jobs` via a real enqueue that is then deleted
  // (v4's exact DDL, zero rows).
  const vectors = new VectorIndicesRepository();
  await repos.memories.create(
    {
      characterId: spec.characters[0].id,
      aboutCharacterId: null,
      chatId: null,
      projectId: null,
      content: 'Ada once catalogued every gear in the shop.',
      summary: 'the gear catalogue',
      keywords: [],
      tags: [],
      importance: 0.4,
      embedding: null,
      source: 'AUTO',
      witnessedContext: null,
      sourceMessageId: null,
      lastAccessedAt: null,
      reinforcementCount: 1,
      lastReinforcedAt: null,
      relatedMemoryIds: [],
      reinforcedImportance: 0.4,
    } as never,
    {
      id: 'f0000000-0000-4000-8000-00000000f001',
      createdAt: spec.seedTimestamp,
      updatedAt: spec.seedTimestamp,
    }
  );
  await vectors.saveMeta(spec.characters[0].id, 8);
  await vectors.addEntry({
    id: 'f0000000-0000-4000-8000-00000000f001',
    characterId: spec.characters[0].id,
    embedding: new Float32Array([0, 1, 0, 0, 0, 0, 0, 0]),
  });
  const { enqueueJob } = await import('@/lib/background-jobs/queue-service');
  const throwawayJobId = await enqueueJob(spec.userId, 'LLM_LOG_CLEANUP' as never, {} as never);
  const mainDb = getRawDatabase();
  if (!mainDb) throw new Error('main DB handle unavailable');
  mainDb.prepare('DELETE FROM "background_jobs" WHERE "id" = ?').run(throwawayJobId);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built memory-pipeline-jobs fixture: ${outMain} + ${outMount} ` +
      `(${spec.characters.length} characters, ${spec.connectionProfiles.length} profiles, ` +
      `${spec.chats.length} chats)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-pipeline-jobs fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
