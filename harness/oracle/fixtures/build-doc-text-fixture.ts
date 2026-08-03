/**
 * Fixture builder for the doc-edit TEXT/MARKDOWN tool-handler differential
 * (W4.1d batch 3b, the FIRST doc-edit differential).
 *
 * Bakes the shared starting state across TWO databases (main + mount-index) so
 * BOTH v4 and the Rust port drive the same doc_* tool handlers against a copy of
 * byte-identical store state:
 *   1. A users row (pinned id).
 *   2. A CHARACTER (Rosalind) with `systemTransparency: true` (so its own vault is
 *      reachable by doc_* tools) + full vault provisioning (REAL
 *      repos.characters.create). V1 = its characterDocumentMountPointId.
 *   3. A project (Atelier) via REAL repos.projects.create. OS1 = its
 *      officialMountPointId (the `scope: 'project'` store).
 *   4. A chat with the character as a CHARACTER participant (controlledBy 'llm').
 *   5. Corpus files: the vaultFiles into V1 and projectFiles into OS1, seeded via
 *      v4's REAL repos.docMountFileLinks.linkDocumentContent (the byte-landing path
 *      every store write uses — content-addressed by SHA-256, folders auto-created).
 *
 * The Librarian / reindex / embedding-scheduler side effects the runtime handlers
 * fire are documented no-op seams (mocked in the oracle case); the fixture does
 * not exercise them.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_DT_MAIN=/tmp/qt-dt-main.db QT_FIXTURE_DT_MOUNT=/tmp/qt-dt-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-doc-text-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join, basename as pathBasename } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface FileSpec {
  relativePath: string;
  content: string;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  characterId: string;
  projectId: string;
  chatId: string;
  characterParticipantId: string;
  character: { name: string };
  project: { name: string };
  vaultFiles: FileSpec[];
  projectFiles: FileSpec[];
}

function detectFileType(rel: string): 'markdown' | 'txt' | 'json' | 'jsonl' {
  const lower = rel.toLowerCase();
  if (lower.endsWith('.json')) return 'json';
  if (lower.endsWith('.jsonl') || lower.endsWith('.ndjson')) return 'jsonl';
  if (lower.endsWith('.md') || lower.endsWith('.markdown')) return 'markdown';
  return 'txt';
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'doc-text.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_DT_MAIN;
  const mountOut = process.env.QT_FIXTURE_DT_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_DT_MAIN and QT_FIXTURE_DT_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-dt-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    ProjectDocMountLinkSchema,
    GroupDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');
  const { sha256OfString } = await import('@/lib/utils/sha256');

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);

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
    ['group_doc_mount_links', GroupDocMountLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
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

  // 1. User.
  await repos.users.create(
    { username: 'doc-tester', name: 'Doc Tester' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Character with a full provisioned vault; systemTransparency: true so its
  //    own vault is reachable by doc_* tools (opaque characters see no vault).
  await repos.characters.create(
    { name: spec.character.name, userId: spec.userId, systemTransparency: true } as never,
    { id: spec.characterId, createdAt: TS, updatedAt: TS } as never,
  );
  const c1 = await repos.characters.findById(spec.characterId);
  if (!c1) throw new Error('character not found after create');
  const v1 = (c1 as { characterDocumentMountPointId?: string }).characterDocumentMountPointId;
  if (!v1) throw new Error('character has no characterDocumentMountPointId');

  // 3. Project with an official store (OS1).
  await repos.projects.create(
    { name: spec.project.name, userId: spec.userId } as never,
    { id: spec.projectId, createdAt: TS, updatedAt: TS } as never,
  );
  const p1 = await repos.projects.findById(spec.projectId);
  if (!p1) throw new Error('project not found after create');
  const os1 = (p1 as { officialMountPointId?: string }).officialMountPointId;
  if (!os1) throw new Error('project has no officialMountPointId');

  // 4. Chat with the character as a CHARACTER participant.
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Doc Text Fixture',
      participants: [
        {
          id: spec.characterParticipantId,
          type: 'CHARACTER',
          characterId: spec.characterId,
          controlledBy: 'llm',
          status: 'active',
          createdAt: TS,
          updatedAt: TS,
        },
      ],
      chatType: 'salon',
      contextSummary: null,
      avatarGenerationEnabled: false,
    } as never,
    { id: spec.chatId, createdAt: TS, updatedAt: TS },
  );
  await repos.chats.getMessageCount(spec.chatId);

  // 5. Corpus files via the REAL linkDocumentContent (folderId derived internally
  //    from relativePath; parent folder rows auto-created).
  const seedFile = async (mountPointId: string, f: FileSpec): Promise<void> => {
    await repos.docMountFileLinks.linkDocumentContent({
      mountPointId,
      relativePath: f.relativePath,
      fileName: pathBasename(f.relativePath),
      folderId: null,
      fileType: detectFileType(f.relativePath),
      content: f.content,
      contentSha256: sha256OfString(f.content),
      plainTextLength: f.content.length,
      fileSizeBytes: Buffer.byteLength(f.content, 'utf-8'),
    });
  };
  for (const f of spec.vaultFiles) await seedFile(v1, f);
  for (const f of spec.projectFiles) await seedFile(os1, f);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built doc-text fixtures: main=${mainOut} mount=${mountOut} (V1=${v1}, OS1=${os1})\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`doc-text fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
