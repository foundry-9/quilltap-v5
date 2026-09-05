/**
 * Tier-3 SEED fixture builder for the Carina query engine
 * (v4 `runCarinaQuery`, lib/services/carina/carina.service.ts).
 *
 * Bakes the SEED both differential sides start from, across BOTH databases:
 *   - the connection profiles (none `isDefault`, so `findDefault` returns null and
 *     the Carina profile chain falls through to the first webSearch-capable
 *     provider — OPENAI — except `Prof`, which carries a `defaultConnectionProfileId`
 *     and resolves ANTHROPIC via chain step 1);
 *   - the answerer + asker characters (via v4's REAL `repos.characters.create` —
 *     pinned ids, full vault provisioning in the mount-index DB), including a
 *     name-collision pair of two "Bob"s (the older one NOT `canBeCarina`);
 *   - `Sage`'s one seed memory + its vector index entry (the memory-recall case);
 *   - the chat with three CHARACTER participants (Alice, a user-controlled
 *     "Operator" persona, and a plain-LLM "Bystander");
 *   - one baked prior Carina exchange for `Echo` (via v4's REAL
 *     `postCarinaResponse`, so the prior-exchange-replay case has continuity).
 *
 * Both the jest oracle and the Rust test copy BOTH fixture files and run only the
 * query — which mints new ids/timestamps — so the seed rows are byte-identical on
 * both sides and only the query's own effect differs.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-carina-query-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-carina-query-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-carina-query-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CharSpec {
  id: string;
  name: string;
  canBeCarina: boolean;
  controlledBy: string;
  identity?: string;
  description?: string;
  title?: string;
  aliases?: string[];
  pronouns?: { subject: string; object: string; possessive: string };
  scenarios?: unknown[];
  systemPrompts?: unknown[];
  defaultConnectionProfileId?: string;
}
interface ProfileSpec {
  id: string;
  name: string;
  provider: string;
  modelName: string;
  isDefault?: boolean;
}
interface SeedMemory {
  id: string;
  characterId: string;
  /**
   * Whose life the memory describes (v4 `d883a5ee1`, bug 122). Absent = the
   * owner's own. A store keyed on `characterId` alone holds both, so the
   * answerer's recall must be able to carry an about-someone-else row.
   */
  aboutCharacterId?: string | null;
  content: string;
  summary: string;
  importance: number;
  vector: number[];
  createdAt: string;
}
interface PriorExchange {
  answererId: string;
  participantId: string | null;
  question: string;
  answer: string;
}
interface ParticipantSeed {
  id: string;
  characterId: string;
  controlledBy: string;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  connectionProfiles: ProfileSpec[];
  characters: CharSpec[];
  chat: { id: string; title: string; participants: ParticipantSeed[] };
  seedMemories: SeedMemory[];
  priorExchanges: PriorExchange[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'carina-query-tier3.json'), 'utf8')) as Spec;

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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-carina-query-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
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
    GroupCharacterMemberSchema,
    GroupDocMountLinkSchema,
    ProjectDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');
  // P4.D103: the standing-instructions substrate (store-backed projects/groups
  // plus the membership join).
  const { GroupSchema } = await import('@/lib/schemas/group.types');
  const { ProjectSchema } = await import('@/lib/schemas/project.types');
  const { BackgroundJobSchema } = await import('@/lib/schemas/job.types');

  await initializeDatabase();
  await ensureCollection('groups', GroupSchema);
  await ensureCollection('projects', ProjectSchema);
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
    ['group_character_members', GroupCharacterMemberSchema],
    ['group_doc_mount_links', GroupDocMountLinkSchema],
    ['project_doc_mount_links', ProjectDocMountLinkSchema],
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

  // Materialize the main-DB background_jobs table (v4 auto-creates it on first
  // enqueue; the Rust port does not own DDL, so its fixture copy needs it).
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const maindb = getRawDatabase();
  if (!maindb) throw new Error('main DB handle unavailable');
  for (const sql of generateDDL('background_jobs', BackgroundJobSchema as never)) {
    maindb.exec(sql);
  }

  // Connection profiles (pinned ids; none isDefault).
  for (const p of spec.connectionProfiles) {
    await repos.connections.create(
      {
        userId: spec.userId,
        name: p.name,
        provider: p.provider,
        modelName: p.modelName,
        isDefault: p.isDefault ?? false,
      } as never,
      { id: p.id }
    );
  }

  // Characters, oldest-first (the two "Bob"s rely on creation order), full vault
  // provisioning, pinned ids.
  for (const c of spec.characters) {
    const { id, ...rest } = c;
    await repos.characters.create(
      {
        ...rest,
        userId: spec.userId,
      } as never,
      { id }
    );
  }

  // Sage's seed memories + their vector index entries.
  const vectors = new VectorIndicesRepository();
  for (const m of spec.seedMemories) {
    await repos.memories.create(
      {
        characterId: m.characterId,
        aboutCharacterId: m.aboutCharacterId ?? null,
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
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: [],
        reinforcedImportance: m.importance,
      } as never,
      { id: m.id, createdAt: m.createdAt, updatedAt: m.createdAt }
    );
    await vectors.saveMeta(m.characterId, m.vector.length);
    await vectors.addEntry({
      id: m.id,
      characterId: m.characterId,
      embedding: new Float32Array(m.vector),
    });
  }

  // The chat with participants, pinned id + timestamps.
  const participants = spec.chat.participants.map((p) => ({
    id: p.id,
    type: 'CHARACTER',
    characterId: p.characterId,
    controlledBy: p.controlledBy,
    createdAt: spec.seedTimestamp,
    updatedAt: spec.seedTimestamp,
  }));
  // P4.D103: the instructed project the chat belongs to + the instructed group
  // ALICE alone belongs to. See the corpus's `standingInstructions._comment`.
  {
    const si = spec.standingInstructions;
    const project = await repos.projects.create(
      { userId: spec.userId, name: si.projectName, instructions: si.projectInstructions } as never,
      { id: si.projectId, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp } as never
    );
    if (project.id !== si.projectId) throw new Error(`project id drift: ${project.id}`);
    const group = await repos.groups.create(
      { name: si.groupName, instructions: si.groupInstructions } as never,
      { id: si.groupId, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp } as never
    );
    if (group.id !== si.groupId) throw new Error(`group id drift: ${group.id}`);
    await repos.groupCharacterMembers.addMember(si.groupId, si.groupMemberCharacterId);
  }

  await repos.chats.create(
    {
      userId: spec.userId,
      title: spec.chat.title,
      participants,
      projectId: spec.standingInstructions.projectId,
    } as never,
    { id: spec.chat.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
  );

  // Bake the prior Carina exchange(s) via v4's REAL postCarinaResponse.
  const { postCarinaResponse } = await import('@/lib/services/carina/writer');
  for (const px of spec.priorExchanges) {
    const posted = await postCarinaResponse({
      chatId: spec.chat.id,
      answer: px.answer,
      answererId: px.answererId,
      question: px.question,
      participantId: px.participantId,
      whisper: false,
      askerParticipantId: null,
    });
    if (!posted) throw new Error(`failed to bake prior exchange for ${px.answererId}`);
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built carina-query fixture: ${outMain} + ${outMount} ` +
      `(${spec.characters.length} characters, ${spec.connectionProfiles.length} profiles, ` +
      `${spec.seedMemories.length} memories, ${spec.priorExchanges.length} prior exchanges)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`carina-query fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
