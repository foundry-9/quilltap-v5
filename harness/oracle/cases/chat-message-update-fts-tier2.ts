/**
 * Tier-2 oracle case — `updateMessage` UNDER the FTS triggers (P4.105).
 *
 * Drives v4's REAL `ChatsRepository.updateMessage` over a copy of the triggered
 * seed fixture (`build-chat-message-update-fts-fixture.ts`), one op at a time,
 * and records the STORAGE FORM the triggers write — not just the search
 * results, which a DELETE + re-INSERT leaves unchanged (same tokens under a
 * new rowid), and which is why no family had seen the divergence:
 *
 * - after EVERY op: the `chat_messages_fts_map` (`ftsId`, `messageId`) and the
 *   `chat_messages` base rowids — so each op's movement is attributable;
 * - at the end: the whole `chat_messages` dump (canonical, as the ops family
 *   dumps it), the `chats` dump, the FTS index's own rowids, and the index's
 *   TOKENS per rowid read through a temp `fts5vocab(…, instance)` table (the
 *   index is contentless, so `SELECT content` answers NULL; the vocab table is
 *   the one readable view of what was indexed, and it reads the same on both
 *   engines).
 *
 * NORMALIZATION: NONE. `updateMessage` mints no timestamp and every id is
 * pinned; the rowids and `ftsId`s are the point.
 *
 * Run (Node 24, from the PINNED v4 worktree), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHATMSGUPDATEFTS=/tmp/qt-chatmsgupdatefts-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chat-message-update-fts-tier2.ts \
 *     > /tmp/oracle-chatmsgupdatefts.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  label: string;
  chatId: string;
  messageId: string;
  updates: Record<string, unknown>;
}
interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

/** The readback queries — byte-identical to the Rust test's. */
const MAP_SQL = `SELECT "ftsId", "messageId" FROM "chat_messages_fts_map" ORDER BY "ftsId"`;
const ROWIDS_SQL = `SELECT "id", rowid AS "rid" FROM "chat_messages" ORDER BY "id"`;
const FTS_ROWIDS_SQL = `SELECT rowid AS "rid" FROM "chat_messages_fts" ORDER BY rowid`;
const VOCAB_DDL = `CREATE VIRTUAL TABLE temp."qt_p4105_vocab" USING fts5vocab(main, "chat_messages_fts", instance)`;
const VOCAB_SQL = `SELECT "doc", "offset", "term" FROM temp."qt_p4105_vocab" ORDER BY "doc", "offset"`;

async function dumpTable(rawQuery: (sql: string) => Promise<unknown>, table: string) {
  const columns = (
    (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
  return canonicalizeRows({ table, columns, rawRows, orderBy: 'id' });
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'chat-message-update-fts.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_CHATMSGUPDATEFTS;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_CHATMSGUPDATEFTS must point at the seed fixture from build-chat-message-update-fts-fixture.ts',
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chatmsgupdatefts-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'chatmsgupdatefts-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');

  await initializeDatabase();
  const repo = new ChatsRepository();
  const db = getRawDatabase();
  if (!db) throw new Error('there is no raw SQLite database');

  const snapshot = () => ({
    map: db.prepare(MAP_SQL).all(),
    rowids: db.prepare(ROWIDS_SQL).all(),
  });

  const initial = snapshot();
  const afterEach: Array<{ label: string; returned: boolean } & ReturnType<typeof snapshot>> = [];
  for (const op of spec.ops) {
    const result = await repo.updateMessage(op.chatId, op.messageId, op.updates as never);
    afterEach.push({ label: op.label, returned: result !== null, ...snapshot() });
  }

  db.exec(VOCAB_DDL);
  const ftsRowids = db.prepare(FTS_ROWIDS_SQL).all();
  const vocab = db.prepare(VOCAB_SQL).all();

  const messages = await dumpTable(rawQuery, 'chat_messages');
  const chats = await dumpTable(rawQuery, 'chats');

  await closeDatabase();

  process.stdout.write(
    JSON.stringify({
      case: 'chat-message-update-fts-tier2',
      initial,
      afterEach,
      ftsRowids,
      vocab,
      messages,
      chats,
    }) + '\n',
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-message-update-fts-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
