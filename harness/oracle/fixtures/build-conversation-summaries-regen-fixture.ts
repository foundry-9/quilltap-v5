/**
 * P4.43 unit 7 fixture builder — conversation-summaries regeneration (v4
 * `lib/background-jobs/handlers/regenerate-conversation-summaries.ts`).
 *
 * Materializes the committed `conversation-summaries-regen-{main,mount}.db` family
 * from `conversation-summaries-regen.json`, using v4's OWN repos so the schema +
 * vault provisioning are production-identical:
 *   - MAIN db: three vault-provisioned `characters` (repos.characters.create),
 *     five `chats` (repos.chats.create — pinned ids, `contextSummary` +
 *     `compactionGeneration` set), each mirrored chat's `chat_messages` via the
 *     REAL repos.chats.addMessage, and an empty `background_jobs` (ensureCollection);
 *   - MOUNT-INDEX db: the document-store tables via v4's generated DDL (the Rust
 *     core issues no DDL), plus the lazily-created `doc_mount_blobs`.
 *
 * Run from the v4 checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server   (or a pinned worktree)
 *   QT_CSR_MAIN=$V5/crates/quilltap-web/tests/fixtures/conversation-summaries-regen-main.db \
 *   QT_CSR_MOUNT=$V5/crates/quilltap-web/tests/fixtures/conversation-summaries-regen-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-conversation-summaries-regen-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CharacterSpec {
  id: string;
  name: string;
  controlledBy: string;
}
interface ParticipantSpec {
  id: string;
  characterId: string;
  controlledBy: string;
}
interface MessageSpec {
  id: string;
  role: string;
  content: string;
  createdAt: string;
  participantId: string;
}
interface ChatSpec {
  id: string;
  title: string;
  contextSummary: string | null;
  compactionGeneration: number;
  participants: ParticipantSpec[];
  messages: MessageSpec[];
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  characters: CharacterSpec[];
  chats: ChatSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'conversation-summaries-regen.json'), 'utf8')
  ) as Spec;

  const mainOut = process.env.QT_CSR_MAIN;
  const mountOut = process.env.QT_CSR_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_CSR_MAIN and QT_CSR_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-csr-fixture-build-'));
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
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
  } = await import('@/lib/schemas/mount-index.types');
  const { BackgroundJobSchema } = await import('@/lib/schemas/job.types');

  await initializeDatabase();
  const repos = getRepositories();
  const TS = spec.seedTimestamp;

  // background_jobs must exist (empty) so the enqueue + status ops write/dump it.
  await ensureCollection('background_jobs', BackgroundJobSchema as never);

  // MOUNT-INDEX db: materialize every store table the create/provision/write path
  // touches, via v4's own generated DDL (idempotent).
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) {
      midb.exec(sql);
    }
  }
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

  // Three characters, each with a fully provisioned vault (pinned id).
  for (const c of spec.characters) {
    await repos.characters.create(
      {
        name: c.name,
        userId: spec.userId,
        aliases: [],
        controlledBy: c.controlledBy,
      } as never,
      { id: c.id }
    );
  }

  // Five chats (pinned ids/timestamps), each carrying contextSummary +
  // compactionGeneration, and the mirrored chats' messages.
  for (const chat of spec.chats) {
    const participants = chat.participants.map((p) => ({
      id: p.id,
      type: 'CHARACTER',
      characterId: p.characterId,
      controlledBy: p.controlledBy,
      status: 'active',
      createdAt: TS,
      updatedAt: TS,
    }));
    await repos.chats.create(
      {
        userId: spec.userId,
        title: chat.title,
        participants,
        chatType: 'salon',
        contextSummary: chat.contextSummary,
        compactionGeneration: chat.compactionGeneration,
      } as never,
      { id: chat.id, createdAt: TS, updatedAt: TS }
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
    // Force chat_messages into existence for chats with no messages too.
    await repos.chats.getMessageCount(chat.id);
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built conversation-summaries-regen fixtures: main=${mainOut} mount=${mountOut} ` +
      `(${spec.characters.length} characters, ${spec.chats.length} chats)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`conversation-summaries-regen fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
