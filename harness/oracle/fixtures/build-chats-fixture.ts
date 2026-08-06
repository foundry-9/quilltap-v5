/**
 * Tier-2 fixture builder — materializes EMPTY `chats` + `chat_messages` tables
 * (Phase-2, the chats repo, sub-unit 1). The op sequence under test
 * (`chats-tier2.json`) starts with its own `create`s, so the seed state is just
 * the schema.
 *
 * Both tables are created by v4's OWN `ensureCollection`, so the DDL (column
 * set/order, affinities) is identical to production by construction. `delete`
 * touches `chat_messages` (cleanup), so that table must exist too.
 *
 * Bug 10 (v4 `3bb664f0`): `delete` now also sweeps `conversation_annotations`.
 * That table is created here too, and SEEDED with rows on the deleted chat
 * (`cccc…`, two rows — must vanish) AND on a surviving chat (`bbbb…`, one row —
 * must remain), so the sweep is proven scoped rather than a blanket wipe. Pinned
 * ids/timestamps so the dump is byte-stable (both sides copy this one built file).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-chats-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chats-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'chats-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chats-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { ChatMetadataSchema } = await import('@/lib/schemas/types');
  const { ChatMessageRowSchema } = await import(
    '@/lib/database/repositories/chats-messages.ops'
  );
  const { ConversationAnnotationSchema } = await import('@/lib/schemas/scriptorium.types');

  await initializeDatabase();
  await ensureCollection('chats', ChatMetadataSchema);
  await ensureCollection('chat_messages', ChatMessageRowSchema);
  await ensureCollection('conversation_annotations', ConversationAnnotationSchema);

  // Bug 10 seed: two annotations on the chat the op sequence DELETEs (cccc…) and
  // one on a chat it only UPDATEs (bbbb…). After the delete, only the bbbb row
  // survives — the sweep is scoped, not a blanket wipe.
  const T = '2026-01-01T00:00:00.000Z';
  const annotations: Array<[string, string, number, string | null, string, string]> = [
    ['anno0000-0000-4000-8000-0000000000c1', 'cccccccc-0000-4000-8000-00000000000c', 0, null, 'Aria', 'swept annotation A'],
    ['anno0000-0000-4000-8000-0000000000c2', 'cccccccc-0000-4000-8000-00000000000c', 1, 'src00000-0000-4000-8000-000000000001', 'Bram', 'swept annotation B'],
    ['anno0000-0000-4000-8000-0000000000b1', 'bbbbbbbb-0000-4000-8000-00000000000b', 0, null, 'Cleo', 'surviving annotation'],
  ];
  for (const [id, chatId, messageIndex, sourceMessageId, characterName, content] of annotations) {
    await rawQuery(
      `INSERT INTO conversation_annotations
         (id, chatId, messageIndex, sourceMessageId, characterName, content, createdAt, updatedAt)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
      [id, chatId, messageIndex, sourceMessageId, characterName, content, T, T],
    );
  }

  await closeDatabase();

  process.stderr.write(`built empty chats fixture: ${out}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
