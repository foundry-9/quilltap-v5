/**
 * SEED fixture builder — the participant resolver (v4
 * `lib/services/chat-message/participant-resolver.service.ts`
 * `resolveRespondingParticipant` / `loadAllParticipantData` / `getRoleplayTemplate`
 * + `lib/chat/connection-resolver.ts`), ported as
 * `quilltap-core::services::participant_resolver`.
 *
 * Builds a two-database fixture (main + a populated mount-index):
 *   - Connection profiles: via v4's REAL `repos.connections.create` (pinned id +
 *     timestamps), so a resolved profile row is byte-comparable across the port.
 *   - Roleplay templates: via v4's REAL `repos.roleplayTemplates.create`.
 *   - Characters: seeded SLIM (null vault mount) via a thin subclass exposing the
 *     protected `_create` — carrying `defaultConnectionProfileId` (a NON-managed
 *     slim column). `talkativeness` is a managed field, so slim chars read back
 *     0.5; the per-participant `talkativeness` override carries the real weights.
 *   - A PROJECT: via v4's REAL `repos.projects.create` (pinned id) with a
 *     `defaultRoleplayTemplateId` property — provisions a full document store in
 *     the mount-index DB (its overlay carries the property). No QUILLTAP_JOB_CHILD
 *     (the reindex runs deterministically; see [[document-store-oracle-gotchas]]).
 *   - Chats: via v4's REAL `repos.chats.create` (pinned ids + participants +
 *     timestamps), some with `roleplayTemplateId` / `projectId` / `imageProfileId`.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-partres-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-partres-mount.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-participant-resolver-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CpSeed {
  id: string;
  name: string;
  provider: string;
  modelName: string;
}
interface RtSeed {
  id: string;
  name: string;
  systemPrompt: string;
}
interface CharSeed {
  id: string;
  name: string;
  controlledBy: 'llm' | 'user';
  defaultConnectionProfileId: string | null;
}
interface ChatSeed {
  id: string;
  title: string;
  roleplayTemplateId?: string;
  projectId?: string;
  imageProfileId?: string;
  participants: Array<Record<string, unknown>>;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  connectionProfiles: CpSeed[];
  roleplayTemplates: RtSeed[];
  project: { id: string; name: string; defaultRoleplayTemplateId: string };
  characters: CharSeed[];
  chats: ChatSeed[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'participant-resolver.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  const outMount = process.env.QT_FIXTURE_MOUNT_OUT;
  if (!out || !outMount) {
    throw new Error('QT_FIXTURE_OUT and QT_FIXTURE_MOUNT_OUT must point at the fixture .db files');
  }
  for (const base of [out, outMount]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = base + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-partres-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
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
  const { CharactersRepository } = await import(
    '@/lib/database/repositories/characters.repository'
  );
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { ChatMessageRowSchema } = await import(
    '@/lib/database/repositories/chats-messages.ops'
  );
  const { ProjectSchema } = await import('@/lib/schemas/project.types');
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

  await initializeDatabase();
  await ensureCollection('characters', CharacterSchema);
  // Materialize the (empty) `chat_messages` table so the port's message read on
  // the multiple-candidate path finds it (v4 lazily ensures it on first
  // message access; the resolver's `getMessages` returns [] on an empty table).
  await ensureCollection('chat_messages', ChatMessageRowSchema);
  // The slim `projects` table (store-resident columns stay NULL/default).
  await ensureCollection('projects', ProjectSchema);

  // Materialize the mount-index store tables + the project link table (the port
  // never issues CREATE TABLE). `doc_mount_chunks` for the post-write reindex.
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

  const repos = getRepositories();

  // Connection profiles.
  for (const cp of spec.connectionProfiles) {
    await repos.connections.create(
      {
        userId: spec.userId,
        name: cp.name,
        provider: cp.provider,
        modelName: cp.modelName,
      } as never,
      { id: cp.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  // Roleplay templates.
  for (const rt of spec.roleplayTemplates) {
    await repos.roleplayTemplates.create(
      { name: rt.name, systemPrompt: rt.systemPrompt } as never,
      { id: rt.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  // Slim characters (null vault mount) via the real protected `_create`.
  class CharactersSqlRepo extends CharactersRepository {
    async createSlim(data: unknown, options: unknown) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      return (this as any)._create(data, options);
    }
  }
  const charRepo = new CharactersSqlRepo();
  for (const c of spec.characters) {
    await charRepo.createSlim(
      {
        userId: spec.userId,
        name: c.name,
        controlledBy: c.controlledBy,
        defaultConnectionProfileId: c.defaultConnectionProfileId,
      },
      { id: c.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  // A real project (provisions a store) carrying `defaultRoleplayTemplateId`.
  await repos.projects.create(
    {
      userId: spec.userId,
      name: spec.project.name,
      defaultRoleplayTemplateId: spec.project.defaultRoleplayTemplateId,
    } as never,
    { id: spec.project.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
  );

  // Chats.
  for (const c of spec.chats) {
    const data: Record<string, unknown> = {
      userId: spec.userId,
      title: c.title,
      participants: c.participants,
    };
    if (c.roleplayTemplateId !== undefined) data.roleplayTemplateId = c.roleplayTemplateId;
    if (c.projectId !== undefined) data.projectId = c.projectId;
    if (c.imageProfileId !== undefined) data.imageProfileId = c.imageProfileId;
    await repos.chats.create(data as never, {
      id: c.id,
      createdAt: spec.seedTimestamp,
      updatedAt: spec.seedTimestamp,
    });
  }

  await closeDatabase();
  await closeMountIndexSQLiteClient();
  process.stderr.write(
    `built participant-resolver fixture: main=${out} mount=${outMount} ` +
      `(${spec.connectionProfiles.length} cp, ${spec.roleplayTemplates.length} rt, ` +
      `${spec.characters.length} chars, 1 project, ${spec.chats.length} chats)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`participant-resolver fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
