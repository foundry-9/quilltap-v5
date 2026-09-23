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
 * ## The two venues (P4.D204)
 *
 * `f45a517a9` rewrote `searchMessagesGlobal` over an FTS5 index with a `LIKE`
 * fallback, and WHICH PATH a query takes changes what it finds — `walk` stops
 * matching *sidewalk*, `cafe` starts matching *café*. So every read runs TWICE,
 * against two fixtures the builder makes from the same spec: one WITHOUT the
 * FTS objects (every query falls back, including the runtime fallback v4 takes
 * when the FTS query throws `no such table`) and one WITH them, built by v4's
 * own `ensureChatMessageFtsSchema` + `rebuildChatMessageFtsIndex`.
 *
 * Each venue gets its own copy of its fixture, its own replaces and its own
 * post-replace dump — and, on the FTS venue, the `chat_messages_fts_map` dump
 * plus a post-replace search, so the `_au` trigger firing on v5's own UPDATE is
 * in the comparand too.
 *
 * ONE VENUE PER INVOCATION, deliberately: `@/lib/database/manager` holds the
 * open database in module state, so a second `initializeDatabase()` in the same
 * process would go on reading the FIRST venue's file. Two processes, two lines,
 * appended.
 *
 * Run (Node 24, from the v4 checkout), AFTER building BOTH fixtures:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_VENUE=plain QT_FIXTURE_CHSEARCH=/tmp/qt-chsearch-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-search.ts > /tmp/oracle-chsearch.ndjson
 *   QT_VENUE=fts QT_FIXTURE_CHSEARCH=/tmp/qt-chsearch-fixture-fts.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-search.ts >> /tmp/oracle-chsearch.ndjson
 *   QT_VENUE=poisoned QT_FIXTURE_CHSEARCH=/tmp/qt-chsearch-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-search.ts >> /tmp/oracle-chsearch.ndjson
 *
 * THE POISONED VENUE (P4.105). `searchMessagesGlobal` is a `safeQuery(…, [])`
 * in v4 — FALLBACK mode, so a query that throws logs `Failed to search
 * messages globally` and answers `[]`. Neither of the other venues can reach
 * that arm: a missing INDEX is already caught by the FTS path's own
 * try/catch, which falls back to the `LIKE` scan and answers. Only a failure
 * of the `LIKE` scan itself escapes to `safeQuery` — so this venue renames
 * `chat_messages` out from under both shapes (on the per-run COPY of the
 * plain fixture, after `initializeDatabase()`) and runs `poisonedReads`: one
 * `fts` plan (the FTS query throws, warns, then the `LIKE` throws) and one
 * `fallback` plan (straight to the `LIKE`, which throws).
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
  postReplaceReads: ReadOp[];
  poisonedReads: ReadOp[];
}

/** The poisoned venue's plant — byte-identical to the Rust test's. */
const POISON_SQL = `ALTER TABLE "chat_messages" RENAME TO "chat_messages_p4105_poisoned"`;

async function dumpTable(
  rawQuery: (sql: string) => Promise<unknown>,
  table: string,
  orderBy = 'id',
) {
  const columns = (
    (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
  return canonicalizeRows({ table, columns, rawRows, orderBy });
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'chats-search.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const venue = process.env.QT_VENUE ?? 'plain';
  if (venue !== 'plain' && venue !== 'fts' && venue !== 'poisoned') {
    throw new Error(`QT_VENUE must be 'plain', 'fts' or 'poisoned', got ${venue}`);
  }
  const fixture = process.env.QT_FIXTURE_CHSEARCH;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_CHSEARCH must point at this venue\'s fixture from build-chats-search-fixture.ts',
    );
  }

  {
    const scratch = mkdtempSync(join(tmpdir(), `qt-chsearch-oracle-${venue}-`));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const work = join(scratch, 'chsearch-work.db');
    copyFileSync(fixture, work);

    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.SQLITE_PATH = work;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    const { initializeDatabase, closeDatabase, rawQuery } = await import(
      '@/lib/database/manager'
    );
    const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');

    await initializeDatabase();
    const repo = new ChatsRepository();

    const runRead = async (op: ReadOp): Promise<unknown> => {
      const searchText = expandSearchText(op.searchText);
      switch (op.kind) {
        case 'countMessagesWithText':
          return repo.countMessagesWithText(op.chatId as string, searchText);
        case 'findMessagesWithText':
          return repo.findMessagesWithText(op.chatId as string, searchText);
        case 'searchMessagesGlobal':
          return repo.searchMessagesGlobal(
            op.chatIds as string[],
            searchText,
            op.limit as number,
          );
        default:
          throw new Error(`unknown read kind: ${(op as { kind: string }).kind}`);
      }
    };

    if (venue === 'poisoned') {
      // P4.109 — THE WARM-UP. `ChatsRepository.ensureMessagesCollection
      // Initialized` runs `CREATE TABLE IF NOT EXISTS "chat_messages"` the
      // FIRST time this repository instance touches the messages collection.
      // Poison before that and v4 silently re-creates an EMPTY table, so
      // count/find answer `0`/`[]` with NO error — the right value for the
      // wrong reason. One read on the SAME instance first; then the rename.
      // (`searchMessagesGlobal` never needed it: it goes through `rawQuery`.)
      await repo.getMessages(spec.poisonedReads.find((op) => op.chatId)?.chatId ?? '');
      await rawQuery(POISON_SQL);

      // P4.109 — the proof the arm FIRED. Every v4 logger is a `Logger`
      // instance (the singleton or a `.child`), so wrapping the prototype
      // records every ERROR/WARN line — before the level check, so
      // `LOG_LEVEL=error` hides nothing — with the two context fields the
      // Rust side compares. A `0` without its line is the warm-up trap.
      const { Logger } = await import('@/lib/logger');
      let opLogs: Array<Record<string, unknown>> = [];
      for (const level of ['error', 'warn'] as const) {
        const original = Logger.prototype[level];
        Logger.prototype[level] = function (this: unknown, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
          const line: Record<string, unknown> = { level, message };
          for (const key of ['chatId', 'chatCount']) {
            if (context && key in context) line[key] = context[key];
          }
          opLogs.push(line);
          return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
        } as never;
      }

      const poisonedReads: Array<{ kind: string; result: unknown; logs: unknown[] }> = [];
      for (const op of spec.poisonedReads) {
        opLogs = [];
        const result = await runRead(op);
        poisonedReads.push({ kind: op.kind, result, logs: opLogs });
      }
      await closeDatabase();
      process.stdout.write(
        JSON.stringify({ case: 'chats-search', venue, poisonedReads }) + '\n',
      );
      process.exit(0);
    }

    // 1) The read methods (reads happen before any mutation).
    const reads: Array<{ kind: string; result: unknown }> = [];
    for (const op of spec.reads) {
      reads.push({ kind: op.kind, result: await runRead(op) });
    }

    // 2) The replace ops (mutate chat_messages; no timestamp touched). On the
    //    FTS venue these fire the `_au` trigger, and one of them pushes its row
    //    over the 512-byte floor so the UPDATE stores a compressed BLOB.
    const replace: Array<{ kind: string; count: number }> = [];
    for (const op of spec.replace) {
      const count = await repo.replaceInMessages(op.chatId, op.searchText, op.replaceText);
      replace.push({ kind: op.kind, count });
    }

    // 3) Reads that can only answer AFTER the replace — the trigger's own proof.
    const postReplaceReads: Array<{ kind: string; result: unknown }> = [];
    for (const op of spec.postReplaceReads) {
      postReplaceReads.push({ kind: op.kind, result: await runRead(op) });
    }

    // 4) Dump the post-replace chat_messages table, and (FTS venue) the map.
    const messages = await dumpTable(rawQuery, 'chat_messages');
    const ftsMap =
      venue === 'fts' ? await dumpTable(rawQuery, 'chat_messages_fts_map', 'ftsId') : null;

    await closeDatabase();

    process.stdout.write(
      JSON.stringify({
        case: 'chats-search',
        venue,
        reads,
        replace,
        postReplaceReads,
        messages,
        ftsMap,
      }) + '\n',
    );
  }

  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats-search oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
