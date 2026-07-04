/**
 * Fixture builder — the shared starting DB for the Project Scriptorium tool trio
 * (`read_conversation` / `upsert_annotation` / `delete_annotation`, W4.1d batch 1).
 *
 * Seeds, from the committed spec (`scriptorium-tools.json`), via v4's REAL repos
 * with ids + timestamps pinned:
 *   - chats (`ChatsRepository.create`) — the slim rows carrying `participants` +
 *     `renderedMarkdown` (with `### Message N` / `## Interchange N` headers) the
 *     tools read; one chat is left without rendered markdown.
 *   - conversation annotations (`ConversationAnnotationsRepository.create`).
 *
 * Both the oracle (cases/scriptorium-tools.ts) and the Rust port then run the SAME
 * op sequence over a COPY of this SAME baked fixture. Read ops touch nothing; the
 * write ops (upsert/delete) mint fresh ids/timestamps on each side, so the table
 * dump is compared in the fully-placeholdered natural-key-sorted form (the tool
 * OUTPUT + formatted string carry the created/updated action, byte-exact).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SCRIPTORIUM=/tmp/qt-scriptorium.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-scriptorium-tools-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  chats: Array<Record<string, unknown> & { id: string; createdAt: string; updatedAt: string }>;
  seedAnnotations: Array<{
    id: string;
    chatId: string;
    messageIndex: number;
    sourceMessageId: string | null;
    characterName: string;
    content: string;
    createdAt: string;
    updatedAt: string;
  }>;
}

/** Strip `$comment` keys the spec carries for documentation (Zod is non-strict,
 * but the DDL has no such column, so drop them before create). */
function stripComments<T extends Record<string, unknown>>(o: T): T {
  const clone: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(o)) {
    if (k === '$comment') continue;
    clone[k] = v;
  }
  return clone as T;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'scriptorium-tools.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_SCRIPTORIUM;
  if (!out) {
    throw new Error('QT_FIXTURE_SCRIPTORIUM must point at the fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-scriptorium-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');
  const { ConversationAnnotationsRepository } = await import(
    '@/lib/database/repositories/conversation-annotations.repository'
  );
  const { ConversationAnnotationSchema } = await import('@/lib/schemas/scriptorium.types');

  await initializeDatabase();
  // Ensure the annotations table exists (its schema-derived DDL) before seeding.
  await ensureCollection('conversation_annotations', ConversationAnnotationSchema);

  const chatsRepo = new ChatsRepository();
  for (const c of spec.chats) {
    const { id, createdAt, updatedAt, ...rest } = stripComments(c);
    await chatsRepo.create(rest as never, { id, createdAt, updatedAt });
  }

  const annRepo = new ConversationAnnotationsRepository();
  for (const a of spec.seedAnnotations) {
    const row = stripComments(a);
    await annRepo.create(
      {
        chatId: row.chatId,
        messageIndex: row.messageIndex,
        sourceMessageId: row.sourceMessageId,
        characterName: row.characterName,
        content: row.content,
      } as never,
      { id: row.id, createdAt: row.createdAt, updatedAt: row.updatedAt }
    );
  }

  await closeDatabase();
  process.stderr.write(
    `built scriptorium tools fixture: ${out} (${spec.chats.length} chats, ${spec.seedAnnotations.length} annotations)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`scriptorium tools fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
