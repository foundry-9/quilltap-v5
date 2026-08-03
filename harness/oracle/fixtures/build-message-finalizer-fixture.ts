/**
 * Tier-3 SEED fixture builder — the message finalizer (Phase-3 Unit-3 wave 3,
 * batch 2; v4 `finalizeMessageResponse` / `calculateNextSpeaker`).
 *
 * Bakes the seed state both differential sides start from, across BOTH databases:
 *   - the participant characters via v4's REAL `repos.characters.create` (pinned
 *     ids, full vault provisioning in the mount-index DB — the finalizer's
 *     `calculateNextSpeaker` reads each active character's `talkativeness` through
 *     the vault-overlaid `findById`);
 *   - one `chat_settings` row (danger mode DETECT_ONLY so the danger-classification
 *     enqueue is reachable — the Rust port defers the danger-resolver OFF
 *     short-circuit, so the corpus keeps mode non-OFF for agreement; cheapLLM +
 *     autoDetectRng false + answer-confirmation off);
 *   - the corpus chats via `repos.chats.create` (pinned ids/timestamps, CHARACTER
 *     participants only — v4's `ParticipantTypeEnum` is CHARACTER-only; the user
 *     is a `userParticipantId`, never a participant row), each with its seed
 *     opener message via `repos.chats.addMessages`;
 *   - one generated-image `files` row (for the finalizer's `files.addLink` loop).
 *
 * Both the jest oracle and the Rust test copy BOTH fixture files and run only the
 * finalizer sequence (which mints new ids/timestamps), so the seed rows are
 * byte-identical on both sides and only the finalizer's effect differs.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-mf-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-mf-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-message-finalizer-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CharacterSpec {
  id: string;
  name: string;
  description?: string;
  aliases?: string[];
  controlledBy: string;
  talkativeness: number;
}
interface ParticipantSpec {
  id: string;
  character?: string;
  userParticipant?: boolean;
  controlledBy?: string;
  status?: string;
}
interface MessageSpec {
  id: string;
  role: string;
  content: string;
  participantId: string;
}
interface ChatSpec {
  id: string;
  chatType: string;
  impersonatingParticipantIds?: string[];
  participants: ParticipantSpec[];
  contextSummary: string | null;
  messages: MessageSpec[];
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  characters: CharacterSpec[];
  generatedImages: Array<{ id: string; sha256: string }>;
  chatSettings: {
    id: string;
    cheapLLMSettings: { strategy: string; fallbackToLocal: boolean };
    autoDetectRng: boolean;
    answerConfirmationEnabled: boolean;
    dangerMode: string;
  };
  chats: ChatSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'message-finalizer-tier3.json'), 'utf8')
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mf-fixture-'));
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

  // Characters (full vault provisioning, pinned ids + talkativeness).
  for (const c of spec.characters) {
    await repos.characters.create(
      {
        name: c.name,
        userId: spec.userId,
        description: c.description ?? null,
        aliases: c.aliases ?? [],
        controlledBy: c.controlledBy,
        talkativeness: c.talkativeness,
      } as never,
      { id: c.id }
    );
  }

  // Chat settings (danger DETECT_ONLY, cheapLLM, no autoDetectRng, confirmation off).
  await repos.chatSettings.create(
    {
      userId: spec.userId,
      cheapLLMSettings: spec.chatSettings.cheapLLMSettings,
      autoDetectRng: spec.chatSettings.autoDetectRng,
      answerConfirmationSettings: { enabled: spec.chatSettings.answerConfirmationEnabled },
      dangerousContentSettings: { mode: spec.chatSettings.dangerMode },
    } as never,
    { id: spec.chatSettings.id }
  );

  // One generated-image file row (for the addLink loop).
  for (const img of spec.generatedImages) {
    await repos.files.create(
      {
        userId: spec.userId,
        sha256: img.sha256,
        originalFilename: 'generated.png',
        mimeType: 'image/png',
        size: 1024,
        linkedTo: [],
        source: 'GENERATED',
        category: 'IMAGE',
        tags: [],
        fileStatus: 'ok',
      } as never,
      { id: img.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  // Chats + their opener messages.
  for (const chat of spec.chats) {
    const participants = chat.participants
      .filter((p) => !p.userParticipant) // participants array is CHARACTER-only
      .map((p) => ({
        id: p.id,
        type: 'CHARACTER',
        characterId: p.character!,
        controlledBy: p.controlledBy ?? 'llm',
        status: p.status ?? 'active',
        createdAt: spec.seedTimestamp,
        updatedAt: spec.seedTimestamp,
      }));
    await repos.chats.create(
      {
        userId: spec.userId,
        title: `Fixture ${chat.id}`,
        participants,
        chatType: chat.chatType,
        contextSummary: chat.contextSummary,
        impersonatingParticipantIds: chat.impersonatingParticipantIds ?? [],
      } as never,
      { id: chat.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
    if (chat.messages.length > 0) {
      await repos.chats.addMessages(
        chat.id,
        chat.messages.map((m) => ({
          id: m.id,
          type: 'message',
          role: m.role,
          content: m.content,
          participantId: m.participantId,
          createdAt: spec.seedTimestamp,
          attachments: [],
        })) as never
      );
    } else {
      await repos.chats.getMessageCount(chat.id);
    }
  }

  // Force background_jobs into existence (empty) so the Rust port - which owns
  // no DDL - can INSERT the enqueue rows (v4 creates the table lazily on first
  // repository access).
  await repos.backgroundJobs.findByUserId(spec.userId, 'PENDING');

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built message-finalizer fixture: ${outMain} + ${outMount} ` +
      `(${spec.characters.length} characters, ${spec.chats.length} chats)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`message-finalizer fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
