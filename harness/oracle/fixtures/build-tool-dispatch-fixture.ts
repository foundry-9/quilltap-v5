/**
 * SEED fixture builder — the dispatcher end-to-end differential (W4.1d batch 1).
 *
 * Bakes, across BOTH databases, the seed both differential sides start from:
 *   - one character ("Friday", with an alias) via v4's REAL `repos.characters.create`
 *     (pinned id, full vault provisioning) so the dispatcher's character-NAME
 *     resolution (callingParticipantId → participant.characterId → character.name)
 *     has a real character to resolve;
 *   - two chats via `repos.chats.create` (one with `renderedMarkdown` carrying
 *     `### Message N` headers, one without), each with a CHARACTER participant
 *     referencing Friday;
 *   - one seed annotation (so an upsert on that message is an UPDATE).
 *
 * The oracle then COPIES this seed and drives v4's REAL `executeToolCallWithContext`
 * over the mixed op batch; the Rust test drives the `BuiltInToolRunner` over the
 * same seed. Annotation writes mint fresh ids/timestamps, so the table dump is
 * compared in the placeholdered natural-key-sorted form.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-tooldispatch-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-tooldispatch-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-tool-dispatch-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CharacterSpec {
  id: string;
  name: string;
  controlledBy: string;
  aliases: string[];
}
interface ParticipantSpec {
  id: string;
  character: string;
  status: string;
}
interface ChatSpec {
  id: string;
  renderedMarkdown: string | null;
  participants: ParticipantSpec[];
}
interface AnnotationSpec {
  id: string;
  chatId: string;
  messageIndex: number;
  sourceMessageId: string | null;
  characterName: string;
  content: string;
  createdAt: string;
  updatedAt: string;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  characters: CharacterSpec[];
  chats: ChatSpec[];
  seedAnnotations: AnnotationSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'tool-dispatch.json'), 'utf8')) as Spec;

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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-tooldispatch-fixture-'));
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
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const { ConversationAnnotationSchema } = await import('@/lib/schemas/scriptorium.types');
  const {
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  await ensureCollection('conversation_annotations', ConversationAnnotationSchema);
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

  for (const c of spec.characters) {
    await repos.characters.create(
      { name: c.name, userId: spec.userId, controlledBy: c.controlledBy, aliases: c.aliases } as never,
      { id: c.id }
    );
  }

  for (const chat of spec.chats) {
    const participants = chat.participants.map((p) => ({
      id: p.id,
      type: 'CHARACTER',
      characterId: p.character,
      controlledBy: 'llm',
      status: p.status,
      createdAt: spec.seedTimestamp,
      updatedAt: spec.seedTimestamp,
    }));
    await repos.chats.create(
      {
        userId: spec.userId,
        title: `Fixture ${chat.id}`,
        participants,
        chatType: 'salon',
        renderedMarkdown: chat.renderedMarkdown,
        contextSummary: null,
      } as never,
      { id: chat.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  const annRepo = repos.conversationAnnotations;
  for (const a of spec.seedAnnotations) {
    await annRepo.create(
      {
        chatId: a.chatId,
        messageIndex: a.messageIndex,
        sourceMessageId: a.sourceMessageId,
        characterName: a.characterName,
        content: a.content,
      } as never,
      { id: a.id, createdAt: a.createdAt, updatedAt: a.updatedAt }
    );
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built tool-dispatch fixture: ${outMain} + ${outMount} ` +
      `(${spec.characters.length} characters, ${spec.chats.length} chats, ${spec.seedAnnotations.length} annotations)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`tool-dispatch fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
