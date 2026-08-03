/**
 * Tier-3 SEED fixture builder — the Courier transport (W4.4a4; v4
 * `lib/services/chat-message/courier-transport.service.ts`
 * `dispatchCourierTransport`).
 *
 * Bakes the seed both differential sides start from, across BOTH databases:
 *   - two participant characters via v4's REAL `repos.characters.create` (pinned
 *     ids, full vault provisioning);
 *   - one `chat_settings` row (cheapLLM present, danger DETECT_ONLY);
 *   - one generated-image `files` row (for the attachment-union case);
 *   - the corpus chats via `repos.chats.create`, each seeded with its
 *     `courierCheckpoints` (via `repos.chats.update`) and its post-checkpoint
 *     messages via `repos.chats.addMessages` (per-message `createdAt` /
 *     `targetParticipantIds` / `systemSender`).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-cour-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-cour-mount.db \
 *     $N/npx tsx <cases-copy>/fixtures/build-courier-transport-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CharacterSpec {
  id: string;
  name: string;
  controlledBy: string;
  talkativeness: number;
}
interface ParticipantSpec {
  id: string;
  character: string;
  controlledBy?: string;
}
interface MessageSpec {
  id: string;
  role: string;
  content: string;
  participantId?: string;
  createdAt: string;
  targetParticipantIds?: string[];
  systemSender?: string;
}
interface ChatSpec {
  id: string;
  chatType: string;
  courierCheckpoints?: Record<string, { lastResolvedMessageId: string; resolvedAt: string }>;
  participants: ParticipantSpec[];
  messages: MessageSpec[];
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  characters: CharacterSpec[];
  chatSettings: {
    id: string;
    cheapLLMSettings: { strategy: string; fallbackToLocal: boolean };
    dangerMode: string;
  };
  files: Array<{ id: string; sha256: string; originalFilename: string; mimeType: string; size: number }>;
  chats: ChatSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'courier-transport-tier3.json'), 'utf8')
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cour-fixture-'));
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
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  const repos = getRepositories();

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

  for (const c of spec.characters) {
    await repos.characters.create(
      {
        name: c.name,
        userId: spec.userId,
        controlledBy: c.controlledBy,
        talkativeness: c.talkativeness,
        aliases: [],
      } as never,
      { id: c.id }
    );
  }

  await repos.chatSettings.create(
    {
      userId: spec.userId,
      cheapLLMSettings: spec.chatSettings.cheapLLMSettings,
      dangerousContentSettings: { mode: spec.chatSettings.dangerMode },
    } as never,
    { id: spec.chatSettings.id }
  );

  for (const f of spec.files) {
    await repos.files.create(
      {
        userId: spec.userId,
        sha256: f.sha256,
        originalFilename: f.originalFilename,
        mimeType: f.mimeType,
        size: f.size,
        linkedTo: [],
        source: 'UPLOADED',
        category: 'IMAGE',
        tags: [],
        fileStatus: 'ok',
      } as never,
      { id: f.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  for (const chat of spec.chats) {
    const participants = chat.participants.map((p) => ({
      id: p.id,
      type: 'CHARACTER',
      characterId: p.character,
      controlledBy: p.controlledBy ?? 'llm',
      status: 'active',
      createdAt: spec.seedTimestamp,
      updatedAt: spec.seedTimestamp,
    }));
    await repos.chats.create(
      {
        userId: spec.userId,
        title: `Fixture ${chat.id}`,
        participants,
        chatType: chat.chatType,
      } as never,
      { id: chat.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
    if (chat.courierCheckpoints) {
      // `update` preserves updatedAt unless passed — keep the seed value.
      await repos.chats.update(chat.id, {
        courierCheckpoints: chat.courierCheckpoints,
        updatedAt: spec.seedTimestamp,
      } as never);
    }
    if (chat.messages.length > 0) {
      await repos.chats.addMessages(
        chat.id,
        chat.messages.map((m) => ({
          id: m.id,
          type: 'message',
          role: m.role,
          content: m.content,
          participantId: m.participantId,
          createdAt: m.createdAt,
          attachments: [],
          ...(m.targetParticipantIds ? { targetParticipantIds: m.targetParticipantIds } : {}),
          ...(m.systemSender ? { systemSender: m.systemSender } : {}),
        })) as never
      );
    }
  }

  // Restore the seed updatedAt on chats that got a metadata bump from addMessages
  // (an ASSISTANT/USER message bumps lastMessageAt/updatedAt) so both sides start
  // from identical seed bytes.
  for (const chat of spec.chats) {
    await repos.chats.update(chat.id, { updatedAt: spec.seedTimestamp } as never);
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built courier-transport fixture: ${outMain} + ${outMount} ` +
      `(${spec.characters.length} characters, ${spec.chats.length} chats)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`courier-transport fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
