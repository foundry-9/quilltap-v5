/**
 * The committed Document Mode server-surface fixture builder (P4.6w deliverable) —
 * the shared test-pepper substrate for `documents_routes_equivalence` (the chat-
 * scoped + standalone route differentials).
 *
 * Bakes, via v4's REAL repositories + `linkDocumentContent` (all ids/timestamps
 * pinned so both differential sides read identical rows — reads diff with no
 * remap; only the open/write/rename mutations mint fresh ids/mtimes, blanked by
 * the harness):
 *   - one user (the FIXTURE_USER; the web edge rewrites it to SINGLE_USER_ID),
 *   - one CHARACTER (Mnemo) with `systemTransparency: true` + a provisioned vault
 *     (V1 = its characterDocumentMountPointId) holding one .md,
 *   - one project (Atelier) with an official database store (OS1) holding a .md,
 *   - the singleton "Quilltap General" database store (GEN, storeType 'documents')
 *     holding three .md files + one PNG blob (the resolve-document image gate);
 *     `instance_settings.generalMountPointId = GEN`,
 *   - one project↔GEN doc-mount link (the accessible-stores project-links branch),
 *   - TWO chats owned by the user: CHAT_A (Mnemo participant, documentMode
 *     'split') with open + inactive + project-scope `chat_documents` rows across
 *     scopes; CHAT_B clean (no rows),
 *   - `chat_documents` rows under STANDALONE_CHAT_ID (the cross-chat recents +
 *     the standalone project-scope filter + the current-chat-wins dedupe).
 *
 * Regenerate (Node 24, from the v4 checkout) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_DOC_MAIN=$W/crates/quilltap-web/tests/fixtures/documents-main.db \
 *   QT_FIXTURE_DOC_MOUNT=$W/crates/quilltap-web/tests/fixtures/documents-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-documents-web-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join, basename as pathBasename } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
}

// ── Pinned ids (shared verbatim with the Rust differential) ─────────────────
const MNEMO = 'a1000000-0000-4000-8000-000000000001';
const PROJECT = 'b1000000-0000-4000-8000-000000000001';
const GEN = 'd0000000-0000-4000-8000-00000000000e'; // the Quilltap General mount
const CHAT_A = 'c1000000-0000-4000-8000-00000000000a';
const CHAT_B = 'c1000000-0000-4000-8000-00000000000b';
const PART_A = 'e1000000-0000-4000-8000-00000000000a';
const STANDALONE = '00000000-0000-0000-0000-000000000000';
// chat_documents rows.
const R1 = 'f1000000-0000-4000-8000-000000000001'; // CHAT_A active, document_store/GEN, notes/alpha.md
const R2 = 'f1000000-0000-4000-8000-000000000002'; // CHAT_A inactive, document_store/GEN, notes/beta.md
const R3 = 'f1000000-0000-4000-8000-000000000003'; // CHAT_A active, project, brief.md
const S1 = 'f1000000-0000-4000-8000-000000000011'; // STANDALONE, document_store/GEN, notes/gamma.md
const S2 = 'f1000000-0000-4000-8000-000000000012'; // STANDALONE, project, brief.md (standalone filter drops)
const S3 = 'f1000000-0000-4000-8000-000000000013'; // STANDALONE, document_store/GEN, notes/alpha.md (dedupe: R1 wins)

function detectFileType(rel: string): 'markdown' | 'txt' | 'json' | 'jsonl' {
  const lower = rel.toLowerCase();
  if (lower.endsWith('.json')) return 'json';
  if (lower.endsWith('.jsonl') || lower.endsWith('.ndjson')) return 'jsonl';
  if (lower.endsWith('.md') || lower.endsWith('.markdown')) return 'markdown';
  return 'txt';
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'documents-web.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_DOC_MAIN;
  const mountOut = process.env.QT_FIXTURE_DOC_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_DOC_MAIN and QT_FIXTURE_DOC_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-doc-web-fixture-'));
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
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { ChatDocumentSchema } = await import('@/lib/schemas/chat-document.types');
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
  await ensureCollection('chat_documents', ChatDocumentSchema);

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

  const mainDb = getRawDatabase();
  if (!mainDb) throw new Error('main DB handle unavailable');
  mainDb.exec(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );

  const repos = getRepositories();

  // 1. User.
  await repos.users.create(
    { username: 'friday', name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Character (Mnemo) with a provisioned vault; systemTransparency: true.
  await repos.characters.create(
    { name: 'Mnemo', userId: spec.userId, systemTransparency: true } as never,
    { id: MNEMO, createdAt: TS, updatedAt: TS } as never,
  );
  const mnemo = await repos.characters.findById(MNEMO);
  const V1 = (mnemo as { characterDocumentMountPointId?: string })?.characterDocumentMountPointId;
  if (!V1) throw new Error('Mnemo has no characterDocumentMountPointId');

  // 3. Project (Atelier) with an official store (OS1).
  await repos.projects.create(
    { name: 'Atelier', userId: spec.userId } as never,
    { id: PROJECT, createdAt: TS, updatedAt: TS } as never,
  );
  const project = await repos.projects.findById(PROJECT);
  const OS1 = (project as { officialMountPointId?: string })?.officialMountPointId;
  if (!OS1) throw new Error('Atelier has no officialMountPointId');

  // 4. The "Quilltap General" database store (GEN) + instance setting.
  await repos.docMountPoints.create(
    {
      name: 'Quilltap General',
      basePath: '',
      mountType: 'database',
      storeType: 'documents',
      includePatterns: [],
      excludePatterns: [],
      enabled: true,
      scanStatus: 'idle',
      conversionStatus: 'idle',
      fileCount: 0,
      chunkCount: 0,
      totalSizeBytes: 0,
    } as never,
    { id: GEN, createdAt: TS, updatedAt: TS } as never,
  );
  mainDb
    .prepare('INSERT OR REPLACE INTO "instance_settings" ("key","value") VALUES (?,?)')
    .run('generalMountPointId', GEN);

  // 5. A project↔GEN doc-mount link (accessible-stores project-links branch).
  await repos.projectDocMountLinks.create(
    { projectId: PROJECT, mountPointId: GEN } as never,
    { id: 'a2000000-0000-4000-8000-000000000001', createdAt: TS, updatedAt: TS } as never,
  );

  // 6. Corpus files via the REAL linkDocumentContent.
  const seedFile = async (mountPointId: string, rel: string, content: string): Promise<void> => {
    await repos.docMountFileLinks.linkDocumentContent({
      mountPointId,
      relativePath: rel,
      fileName: pathBasename(rel),
      folderId: null,
      fileType: detectFileType(rel),
      content,
      contentSha256: sha256OfString(content),
      plainTextLength: content.length,
      fileSizeBytes: Buffer.byteLength(content, 'utf-8'),
    });
  };
  await seedFile(GEN, 'notes/alpha.md', '# Alpha\n\nThe alpha document in the general store.\n');
  await seedFile(GEN, 'notes/beta.md', '# Beta\n\nThe beta document.\n');
  await seedFile(GEN, 'notes/gamma.md', '# Gamma\n\nThe gamma document.\n');
  await seedFile(OS1, 'brief.md', '# Brief\n\nThe project brief.\n');
  await seedFile(V1, 'self-note.md', "# Self\n\nMnemo's own note.\n");

  // 7. Chats.
  const mkParticipant = (id: string, characterId: string) => ({
    id,
    type: 'CHARACTER',
    characterId,
    controlledBy: 'llm',
    status: 'active',
    createdAt: TS,
    updatedAt: TS,
  });
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Document Chat',
      participants: [mkParticipant(PART_A, MNEMO)],
      chatType: 'salon',
      documentMode: 'split',
      contextSummary: null,
      avatarGenerationEnabled: false,
    } as never,
    { id: CHAT_A, createdAt: TS, updatedAt: TS },
  );
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Clean Chat',
      participants: [],
      chatType: 'salon',
      documentMode: 'normal',
      contextSummary: null,
      avatarGenerationEnabled: false,
    } as never,
    { id: CHAT_B, createdAt: TS, updatedAt: TS },
  );
  // Materialize the chat_messages table (a read lazily creates it) so the
  // Librarian open/save/rename/delete announcements can append a message row on
  // both differential sides (the writer opens the raw DB and needs the table).
  await repos.chats.getMessageCount(CHAT_A);
  await repos.chats.getMessageCount(CHAT_B);

  // 8. chat_documents rows (pinned ids + staggered updatedAt for recents order).
  const mkDoc = async (
    id: string,
    chatId: string,
    filePath: string,
    scope: string,
    mountPoint: string | null,
    isActive: boolean,
    updatedAt: string,
  ) => {
    await repos.chatDocuments.create(
      {
        chatId,
        filePath,
        scope,
        mountPoint,
        displayTitle: pathBasename(filePath),
        isActive,
      } as never,
      { id, createdAt: TS, updatedAt } as never,
    );
  };
  // CHAT_A: two open (alpha active, brief active) + one inactive (beta).
  await mkDoc(R1, CHAT_A, 'notes/alpha.md', 'document_store', 'Quilltap General', true, '2026-03-05T00:00:00.000Z');
  await mkDoc(R2, CHAT_A, 'notes/beta.md', 'document_store', 'Quilltap General', false, '2026-03-04T00:00:00.000Z');
  await mkDoc(R3, CHAT_A, 'brief.md', 'project', null, true, '2026-03-03T00:00:00.000Z');
  // STANDALONE recents (newest-first by updatedAt): gamma, alpha(dup of R1), brief(project).
  await mkDoc(S1, STANDALONE, 'notes/gamma.md', 'document_store', 'Quilltap General', false, '2026-03-06T00:00:00.000Z');
  await mkDoc(S2, STANDALONE, 'brief.md', 'project', null, false, '2026-03-07T00:00:00.000Z');
  await mkDoc(S3, STANDALONE, 'notes/alpha.md', 'document_store', 'Quilltap General', false, '2026-03-08T00:00:00.000Z');

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built documents-web fixture: main=${mainOut} mount=${mountOut} (V1=${V1}, OS1=${OS1}, GEN=${GEN})\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`documents-web fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
