/**
 * Tier-3 SEED fixture builder — the answer-confirmation service (W4.3; v4
 * `finalizeMessageResponse` + `answer-confirmation.service.ts`).
 *
 * Bakes the seed state both differential sides start from, across BOTH databases:
 *   - one character (Aurora) via v4's REAL `repos.characters.create` (pinned id,
 *     full vault provisioning — the finalizer's `calculateNextSpeaker` reads the
 *     responder's `talkativeness` through the vault-overlaid `findById`);
 *   - one `chat_settings` row (cheapLLM present, autoDetectRng OFF; the per-case
 *     answer-confirmation `enabled` is overridden in the oracle, not stored);
 *   - one project (Project ON) via `repos.projects.create` with
 *     `answerConfirmationOverride: 'ON'` in its store — for the project-tier gate
 *     cases (chat OFF beats project ON; project ON with global false);
 *   - the corpus chats via `repos.chats.create` (pinned ids/timestamps, ONE
 *     CHARACTER participant each — the user is a `userParticipantId`), each with a
 *     seeded Commonplace Book whisper (systemSender 'commonplaceBook',
 *     targetParticipantIds [participantId]) when the case wants a whisper, via
 *     `repos.chats.addMessages`.
 *
 * Both the jest oracle and the Rust test copy BOTH fixture files and run only the
 * finalizer sequence (which mints new ids/timestamps), so the seed rows are
 * byte-identical on both sides and only the finalizer's effect differs.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-ac-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-ac-mount.db \
 *     $N/npx tsx <this worktree>/harness/oracle/fixtures/build-answer-confirmation-fixture.ts
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
interface DialogueSpec {
  id: string;
  role: 'USER' | 'ASSISTANT';
  content?: string;
  /** Content assembled from repeated units (large / non-ASCII transcripts). */
  contentRepeat?: Array<{ unit: string; times: number }>;
  /** When true, the row carries the call's participantId (name-resolvable). */
  participant?: boolean;
  /** A Staff whisper (dropped by buildRecentConversationContext). */
  systemSender?: string;
  /** isSilentMessage: true (dropped by buildRecentConversationContext). */
  silent?: boolean;
}
interface CallSpec {
  name: string;
  chatId: string;
  participantId: string;
  assistantMessageId: string;
  whisperMessageId: string;
  projectId: string | null;
  chatOverride: 'ON' | 'OFF' | null;
  impersonated: boolean;
  dangerous?: boolean;
  whisper?: string | null;
  whisperRepeat?: { unit: string; times: number };
  /** Prior real-dialogue rows seeded BEFORE the whisper (a7b1398d scene cases). */
  dialogue?: DialogueSpec[];
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  chatSettingsId: string;
  characters: CharacterSpec[];
  project: { id: string; name: string; answerConfirmationOverride: 'ON' | 'OFF' };
  calls: CallSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'answer-confirmation-tier3.json'), 'utf8')
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ac-fixture-'));
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
  const TS = spec.seedTimestamp;

  // Materialize the mount-index content tables (hand-written repos whose DDL v4
  // emits at startup elsewhere; the vault scaffold + project store need them).
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

  // Character (full vault provisioning, pinned id + talkativeness).
  for (const c of spec.characters) {
    await repos.characters.create(
      {
        name: c.name,
        userId: spec.userId,
        aliases: [],
        controlledBy: c.controlledBy,
        talkativeness: c.talkativeness,
      } as never,
      { id: c.id }
    );
  }

  // Chat settings (cheapLLM present, autoDetectRng OFF; per-case global override
  // is applied in the oracle, not stored).
  await repos.chatSettings.create(
    {
      userId: spec.userId,
      cheapLLMSettings: { strategy: 'PROVIDER_CHEAPEST', fallbackToLocal: false },
      autoDetectRng: false,
      answerConfirmationSettings: { enabled: false },
      dangerousContentSettings: { mode: 'AUTO_ROUTE' },
    } as never,
    { id: spec.chatSettingsId }
  );

  // Project with answerConfirmationOverride:'ON' (the project-tier gate).
  await repos.projects.create(
    {
      name: spec.project.name,
      userId: spec.userId,
      answerConfirmationOverride: spec.project.answerConfirmationOverride,
    } as never,
    { id: spec.project.id, createdAt: TS, updatedAt: TS } as never
  );

  const character = spec.characters[0];

  // Chats + their seeded Commonplace whisper messages.
  for (const call of spec.calls) {
    const participants = [
      {
        id: call.participantId,
        type: 'CHARACTER',
        characterId: character.id,
        controlledBy: call.impersonated ? 'user' : 'llm',
        status: 'active',
        createdAt: TS,
        updatedAt: TS,
      },
    ];
    await repos.chats.create(
      {
        userId: spec.userId,
        title: `Fixture ${call.name}`,
        participants,
        chatType: 'salon',
        projectId: call.projectId ?? undefined,
        answerConfirmationOverride: call.chatOverride ?? undefined,
        impersonatingParticipantIds: call.impersonated ? [call.participantId] : [],
        isDangerousChat: call.dangerous === true ? true : undefined,
      } as never,
      { id: call.chatId, createdAt: TS, updatedAt: TS }
    );

    // Seed the prior-dialogue rows (a7b1398d — the recent-conversation context
    // the re-affirmation pass anchors to). Strictly increasing createdAt so both
    // sides read the same order; the whisper (below) keeps the seed timestamp so
    // it sorts first (buildRecentConversationContext drops it by systemSender
    // regardless; findLatestCommonplaceWhisper scans backward by position).
    if (call.dialogue && call.dialogue.length > 0) {
      const rows = call.dialogue.map((d, i) => {
        const content =
          d.contentRepeat != null
            ? d.contentRepeat.map((p) => p.unit.repeat(p.times)).join('')
            : d.content ?? '';
        const row: Record<string, unknown> = {
          id: d.id,
          type: 'message',
          role: d.role,
          content,
          createdAt: `2020-01-01T00:01:${String(i + 1).padStart(2, '0')}.000Z`,
          attachments: [],
        };
        if (d.participant) row.participantId = call.participantId;
        if (d.systemSender) row.systemSender = d.systemSender;
        if (d.silent) row.isSilentMessage = true;
        return row;
      });
      await repos.chats.addMessages(call.chatId, rows as never);
    }

    // Seed a Commonplace Book whisper for this participant when the case wants one.
    const whisperText =
      call.whisperRepeat != null
        ? call.whisperRepeat.unit.repeat(call.whisperRepeat.times)
        : call.whisper ?? null;
    if (whisperText != null) {
      await repos.chats.addMessages(
        call.chatId,
        [
          {
            id: call.whisperMessageId,
            type: 'message',
            role: 'ASSISTANT',
            content: whisperText,
            participantId: call.participantId,
            systemSender: 'commonplaceBook',
            targetParticipantIds: [call.participantId],
            createdAt: TS,
            attachments: [],
          },
        ] as never
      );
    } else {
      await repos.chats.getMessageCount(call.chatId);
    }
  }

  // Force background_jobs into existence (empty) so the Rust port — which owns no
  // DDL — can INSERT the enqueue rows (v4 creates the table lazily).
  await repos.backgroundJobs.findByUserId(spec.userId, 'PENDING');

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built answer-confirmation fixture: ${outMain} + ${outMount} ` +
      `(${spec.characters.length} characters, ${spec.calls.length} chats)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`answer-confirmation fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
