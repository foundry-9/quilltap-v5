/**
 * Mixed differential oracle — v4's `ChatsRepository` search & replace ops
 * (Phase-2, the chats repo — sub-unit 6: countMessagesWithText /
 * findMessagesWithText / searchMessagesGlobal / replaceInMessages).
 *
 * Opens a COPY of the pre-baked seed fixture and drives v4's REAL repository
 * methods. The three read methods (`reads`) emit their results verbatim; the
 * `replace` ops run last (they mutate `chat_messages` without touching any
 * timestamp), and the final `chat_messages` table is dumped canonically. The Rust
 * port (db::chats_search) runs the SAME reads + replaces on its own copy and must
 * produce the same read results, the same replace counts, and the same
 * post-replace `chat_messages` dump — exactly (NO normalization).
 *
 * The over-length search guard is exercised via the sentinel
 * `TOOLONGSEARCHTEXT_REPLACE_AT_RUNTIME`, expanded to 1001 chars (> the v4
 * MAX_SEARCH_QUERY_LENGTH of 1000) on BOTH sides identically.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHSEARCH=/tmp/qt-chsearch-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-search.ts > /tmp/oracle-chsearch.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

/** Sentinel for the >MAX_SEARCH_QUERY_LENGTH guard, expanded identically both sides. */
const TOO_LONG_SENTINEL = 'TOOLONGSEARCHTEXT_REPLACE_AT_RUNTIME';
/** 1001 chars — one over v4's MAX_SEARCH_QUERY_LENGTH (1000). */
function expandSearchText(s: string): string {
  return s === TOO_LONG_SENTINEL ? 'x'.repeat(1001) : s;
}

interface ReadOp {
  kind: 'countMessagesWithText' | 'findMessagesWithText' | 'searchMessagesGlobal';
  chatId?: string;
  chatIds?: string[];
  searchText: string;
  limit?: number;
}
interface ReplaceOp {
  kind: 'replaceInMessages';
  chatId: string;
  searchText: string;
  replaceText: string;
}
interface Spec {
  testPepperBase64: string;
  reads: ReadOp[];
  replace: ReplaceOp[];
}

async function dumpTable(rawQuery: (sql: string) => Promise<unknown>, table: string) {
  const columns = (
    (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
  return canonicalizeRows({ table, columns, rawRows, orderBy: 'id' });
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'chats-search.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_CHSEARCH;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_CHSEARCH must point at the seed fixture from build-chats-search-fixture.ts');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chsearch-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'chsearch-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');

  await initializeDatabase();
  const repo = new ChatsRepository();

  // 1) The read methods (reads happen before any mutation).
  const reads: Array<{ kind: string; result: unknown }> = [];
  for (const op of spec.reads) {
    const searchText = expandSearchText(op.searchText);
    let result: unknown;
    switch (op.kind) {
      case 'countMessagesWithText':
        result = await repo.countMessagesWithText(op.chatId as string, searchText);
        break;
      case 'findMessagesWithText':
        result = await repo.findMessagesWithText(op.chatId as string, searchText);
        break;
      case 'searchMessagesGlobal':
        result = await repo.searchMessagesGlobal(
          op.chatIds as string[],
          searchText,
          op.limit as number
        );
        break;
      default:
        throw new Error(`unknown read kind: ${(op as { kind: string }).kind}`);
    }
    reads.push({ kind: op.kind, result });
  }

  // 2) The replace ops (mutate chat_messages; no timestamp touched).
  const replace: Array<{ kind: string; count: number }> = [];
  for (const op of spec.replace) {
    const count = await repo.replaceInMessages(op.chatId, op.searchText, op.replaceText);
    replace.push({ kind: op.kind, count });
  }

  // 3) Dump the post-replace chat_messages table.
  const messages = await dumpTable(rawQuery, 'chat_messages');

  await closeDatabase();

  process.stdout.write(JSON.stringify({ case: 'chats-search', reads, replace, messages }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats-search oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
