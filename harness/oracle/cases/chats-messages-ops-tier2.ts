/**
 * Tier-2 oracle case — the chats messages mutation path (Phase-2, the chats repo —
 * sub-unit 4b: updateMessage / deleteMessagesByIds / clearMessages).
 *
 * P4.D183 widened this to the WHOLE message write funnel — `addMessage`,
 * `addMessages` and the search-and-replace path join the 4b trio — because
 * `chats.transcriptVersion` is DB state and this census is what sees it. The
 * fixture now carries the column (the builder adds it the way v4's migration
 * does), so every bump, and every write that must NOT bump, is a compared
 * cell: add/add-batch/edit/delete/clear/replace move it, a not-found edit and
 * a no-match replace and a plain metadata patch do not.
 *
 * Runs the fixed op sequence (`chats-messages-ops-tier2.json`) via v4's REAL
 * `ChatsRepository` on a copy of the seed fixture, then dumps BOTH the
 * `chat_messages` rows (the row mutations) and the `chats` rows (the metadata
 * side-effects: deleteMessagesByIds recounts messageCount; clearMessages resets
 * messageCount→0 + lastMessageAt→null; updateMessage touches no metadata) canonically.
 *
 * NORMALIZATION SPEC: NONE. The seed's minted `lastMessageAt`/`updatedAt` are baked
 * once and read identically by both sides, and no 4b op mints a new chat timestamp
 * (updateMessage touches no metadata; delete/clear preserve `updatedAt`), so every
 * cell is deterministic.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHATSMSGOPS=/tmp/qt-chatsmsgops-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-messages-ops-tier2.ts > /tmp/oracle-chatsmsgops.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind:
    | 'updateMessage'
    | 'deleteMessagesByIds'
    | 'clearMessages'
    // P4.D183: the rest of v4's message write funnel, plus the two arms that
    // say what must NOT move the counter.
    | 'addMessage'
    | 'addMessages'
    | 'replaceInMessages'
    | 'chatUpdate'
    // P4.109: P4.105's finding 1 — a NULL-content row planted on the per-run
    // copy (only corrupt or hand-edited data carries one; v4 writes `content`
    // through a required Zod string), then a read that must SKIP it.
    | 'plantNullContent'
    // P4.112: the `00c290c9a` unification review's nit — v4's per-row
    // `ChatEventSchema.safeParse` ALSO fails on Zod-only shapes a raw cell can
    // carry (a `role` outside `RoleEnum`, a non-uuid `id`, a `hostEvent` that
    // is not its object shape). One cell planted on the per-run copy.
    // P4.113: + a `createdAt` that fails zod 4.6.5's `z.iso.datetime()` (on a
    // message, a context-summary and a system row) and a non-uuid message
    // `participantId` — two more shapes a raw cell can carry.
    | 'plantCell'
    | 'getMessages';
  chatId: string;
  messageId?: string;
  updates?: Record<string, unknown>;
  messageIds?: string[];
  message?: Record<string, unknown>;
  messages?: Array<Record<string, unknown>>;
  searchText?: string;
  replaceText?: string;
  data?: Record<string, unknown>;
  column?: string;
  value?: string;
}

/** The columns a `plantCell` op may name (a closed set — it is spliced). */
const PLANTABLE_COLUMNS = new Set(['id', 'role', 'hostEvent', 'createdAt', 'participantId']);
interface Spec {
  testPepperBase64: string;
  ops: Op[];
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
  const specPath = join(here, '..', 'fixtures', 'chats-messages-ops-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_CHATSMSGOPS;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_CHATSMSGOPS must point at the seed fixture from build-chats-messages-ops-fixture.ts');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chatsmsgops-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'chatsmsgops-work.db');
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

  // P4.109 — every `updateMessage` op's RETURN (`null` vs the event — here
  // its id) and the ERROR/WARN lines it logged. v4's `updateMessage` is a
  // FALLBACK `safeQuery` answering `null`; the type-mismatch op
  // (`content: 42`) fails `ChatEventSchema.parse` and must log `Failed to
  // update message in chat` — the three healthy/miss ops are its silence legs.
  // Recorded off the `Logger` prototype (singleton and children alike, before
  // the level check), only while an update runs.
  const updateReturns: Array<Record<string, unknown>> = [];
  const reads: Array<Record<string, unknown>> = [];
  let opLogs: Array<Record<string, unknown>> | null = null;
  const { Logger } = await import('@/lib/logger');
  for (const level of ['error', 'warn'] as const) {
    const original = Logger.prototype[level];
    Logger.prototype[level] = function (
      this: unknown,
      message: string,
      context?: Record<string, unknown>,
      ...rest: unknown[]
    ) {
      const line: Record<string, unknown> = { level, message };
      for (const key of ['chatId', 'messageId', 'messageType']) {
        if (context && key in context) line[key] = context[key];
      }
      opLogs?.push(line);
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }

  for (const op of spec.ops) {
    if (op.kind === 'updateMessage') {
      opLogs = [];
      const returned = await repo.updateMessage(
        op.chatId,
        op.messageId as string,
        op.updates as never,
      );
      updateReturns.push({
        messageId: op.messageId,
        returned: returned === null ? null : returned.id,
        logs: opLogs,
      });
      opLogs = null;
    } else if (op.kind === 'deleteMessagesByIds') {
      await repo.deleteMessagesByIds(op.chatId, op.messageIds as string[]);
    } else if (op.kind === 'clearMessages') {
      await repo.clearMessages(op.chatId);
    } else if (op.kind === 'addMessage') {
      await repo.addMessage(op.chatId, op.message as never);
    } else if (op.kind === 'addMessages') {
      await repo.addMessages(op.chatId, op.messages as never);
    } else if (op.kind === 'replaceInMessages') {
      await repo.replaceInMessages(
        op.chatId,
        op.searchText as string,
        op.replaceText as string,
      );
    } else if (op.kind === 'plantNullContent') {
      await rawQuery(
        `UPDATE chat_messages SET content = NULL WHERE id = '${op.messageId as string}'`,
      );
    } else if (op.kind === 'plantCell') {
      const column = op.column as string;
      if (!PLANTABLE_COLUMNS.has(column)) throw new Error(`plantCell: unplantable column ${column}`);
      const value = (op.value as string).replace(/'/g, "''");
      await rawQuery(
        `UPDATE chat_messages SET "${column}" = '${value}' WHERE id = '${op.messageId as string}'`,
      );
    } else if (op.kind === 'getMessages') {
      // v4 `getMessages`: the per-row `ChatEventSchema.safeParse` skips the
      // NULL-content row with WARN `Skipping corrupted chat message` and
      // answers the rest.
      opLogs = [];
      const events = await repo.getMessages(op.chatId);
      reads.push({ chatId: op.chatId, ids: events.map((e) => e.id), logs: opLogs });
      opLogs = null;
    } else if (op.kind === 'chatUpdate') {
      // The arm that says what must NOT move: a plain metadata patch. The
      // counter is outside `ChatMetadataSchema`, so Zod strips it from the
      // validated whole-row rewrite and this write cannot touch it.
      await repo.update(op.chatId, op.data as never);
    } else {
      throw new Error(`unknown op kind: ${String((op as { kind: string }).kind)}`);
    }
  }

  const messages = await dumpTable(rawQuery, 'chat_messages');
  const chats = await dumpTable(rawQuery, 'chats');

  await closeDatabase();

  process.stdout.write(
    JSON.stringify({ case: 'chats-messages-ops-tier2', updateReturns, reads, messages, chats }) + '\n',
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats-messages-ops-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
