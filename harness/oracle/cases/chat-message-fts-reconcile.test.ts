/**
 * @jest-environment node
 *
 * P4.D204 FTS reconciliation ORACLE — v4's REAL
 * `lib/startup/reconcile-chat-message-fts.ts` (`f45a517a9`) driven against a
 * REAL SQLite connection over the shared corpus
 * `harness/oracle/fixtures/chat-message-fts-reconcile.json`.
 *
 * v4's `reconcileChatMessageFts()` takes no arguments — it reaches for
 * `getRawDatabase()` — so that one module is mocked to hand back the
 * scenario's connection. Everything else is v4's real code: the real DDL
 * module, the real counts, the real rebuild.
 *
 * The scenarios are v4's own five plus two this port adds: an index AHEAD of
 * the transcript (a stale map row the delete trigger never saw, which the
 * count comparison catches in the other direction) and an eligibility mix, so
 * `eligible` is not simply the row count.
 *
 * Each row records:
 *   - `result`  — the whole `ChatMessageFtsReconcileResult`
 *                 (`restored`, `eligible`, `indexed`, `rebuilt`);
 *   - `logs`    — every line the pass emitted, in order, with its LEVEL, its
 *                 message and its context object. All six of v4's lines are
 *                 comparands, and their order is part of the comparand;
 *   - `after`   — the post-pass state: `missing`, both counts, every
 *                 `chat_messages_fts_map` row, and every corpus `MATCH`
 *                 expression's hit list.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores
 * .claude/ paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=<this worktree root>
 *   TMPO=/tmp/qt-chat-message-fts-reconcile-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/chat-message-fts-reconcile.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/chat-message-fts-reconcile.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-chat-message-fts-reconcile.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "chat-message-fts-reconcile\.test\.ts$"
 */

import { describe, it, jest } from '@jest/globals';
import * as fs from 'fs';
import path from 'path';

function loadDriver() {
  // Real binding by absolute root path — a bare or nested require resolves to
  // the jest mock, which returns empty result sets.
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  return require(path.join(process.cwd(), 'node_modules', 'better-sqlite3'));
}

/** The connection `getRawDatabase()` hands back, swapped per scenario. */
let mockRawDb: any = null;

jest.mock('@/lib/database/backends/sqlite/client', () => ({
  getRawDatabase: () => mockRawDb,
}));

interface LogLine {
  level: string;
  message: string;
  context: unknown;
}
const mockLogLines: LogLine[] = [];

jest.mock('@/lib/logging/create-logger', () => ({
  createServiceLogger: () => ({
    debug: (message: string, context?: unknown) =>
      mockLogLines.push({ level: 'debug', message, context: context ?? null }),
    info: (message: string, context?: unknown) =>
      mockLogLines.push({ level: 'info', message, context: context ?? null }),
    warn: (message: string, context?: unknown) =>
      mockLogLines.push({ level: 'warn', message, context: context ?? null }),
    error: (message: string, context?: unknown) =>
      mockLogLines.push({ level: 'error', message, context: context ?? null }),
  }),
}));

interface Step {
  op: string;
  id?: string;
  content?: { kind: string; value?: string };
  type?: string;
  role?: string | null;
}

interface Scenario {
  name: string;
  seed: Step[];
  damage: string[];
  afterDamage?: Step[];
}

interface Corpus {
  chatMessagesDdl: string;
  long: { unit: string; times: number };
  searches: string[];
  scenarios: Scenario[];
}

const corpusPath = path.join(__dirname, '..', 'fixtures', 'chat-message-fts-reconcile.json');
const corpus = JSON.parse(fs.readFileSync(corpusPath, 'utf-8')) as Corpus;
const LONG = corpus.long.unit.repeat(corpus.long.times);

function attempt<T>(f: () => T): T | null {
  try {
    return f();
  } catch {
    return null;
  }
}

describe('P4.D204 chat message FTS reconciliation oracle', () => {
  it('records v4 reconciling each damaged state', async () => {
    const out = process.env.QT_ORACLE_OUT;
    if (!out) throw new Error('set QT_ORACLE_OUT');

    const {
      ensureChatMessageFtsSchema,
      missingChatMessageFtsObjects,
      countEligibleChatMessages,
      countIndexedChatMessages,
      // eslint-disable-next-line @typescript-eslint/no-var-requires
    } = require('@/lib/database/backends/sqlite/chat-message-fts');
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const { registerTextCodecFunction } = require('@/lib/database/backends/sqlite/text-codec-function');
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const { textToBlob } = require('@/lib/database/text-compression');
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const { reconcileChatMessageFts } = require('@/lib/startup/reconcile-chat-message-fts');

    const contentValue = (c: Step['content']): unknown => {
      switch (c?.kind) {
        case 'text':
          return c.value;
        case 'long':
          return LONG;
        case 'longBlob':
          return textToBlob(LONG);
        default:
          return null;
      }
    };

    const runStep = (db: any, step: Step) => {
      if (step.op !== 'insert') throw new Error(`unsupported seed op: ${step.op}`);
      db.prepare(
        'INSERT INTO "chat_messages" ("id","chatId","type","role","content","createdAt") VALUES (?,?,?,?,?,?)',
      ).run(
        step.id,
        'chat-1',
        step.type ?? 'message',
        step.role === undefined ? 'USER' : step.role,
        contentValue(step.content),
        `2026-01-01T00:00:${(step.id as string).slice(-2).padStart(2, '0')}.000Z`,
      );
    };

    const lines: string[] = [];
    const Database = loadDriver();

    for (const scenario of corpus.scenarios) {
      const db = new Database(':memory:');
      mockRawDb = db;
      mockLogLines.length = 0;
      try {
        registerTextCodecFunction(db);
        db.exec(corpus.chatMessagesDdl);
        ensureChatMessageFtsSchema(db);
        for (const step of scenario.seed) runStep(db, step);
        for (const sql of scenario.damage) db.exec(sql);
        for (const step of scenario.afterDamage ?? []) runStep(db, step);

        const result = await reconcileChatMessageFts();

        lines.push(
          JSON.stringify({
            scenario: scenario.name,
            result,
            logs: [...mockLogLines],
            after: {
              missing: attempt(() => missingChatMessageFtsObjects(db)),
              eligible: attempt(() => countEligibleChatMessages(db)),
              indexed: attempt(() => countIndexedChatMessages(db)),
              map: attempt(() =>
                db
                  .prepare(
                    'SELECT "ftsId", "messageId" FROM "chat_messages_fts_map" ORDER BY "ftsId"',
                  )
                  .all(),
              ),
              searches: Object.fromEntries(
                corpus.searches.map((match) => [
                  match,
                  attempt(() =>
                    db
                      .prepare(
                        `SELECT m."id" AS id
                           FROM "chat_messages_fts" f
                           JOIN "chat_messages_fts_map" x ON x."ftsId" = f.rowid
                           JOIN "chat_messages" m         ON m."id" = x."messageId"
                          WHERE "chat_messages_fts" MATCH ?
                          ORDER BY m."createdAt" DESC`,
                      )
                      .all(match)
                      .map((r: { id: string }) => r.id),
                  ),
                ]),
              ),
            },
          }),
        );
      } finally {
        db?.close();
        mockRawDb = null;
      }
    }

    fs.writeFileSync(out, lines.join('\n') + '\n');
  });
});
