/**
 * @jest-environment node
 *
 * P4.D204 FTS5 index ORACLE — v4's REAL
 * `lib/database/backends/sqlite/chat-message-fts.ts` (`f45a517a9`) driven
 * against a REAL SQLite connection over the shared script
 * `harness/oracle/fixtures/chat-message-fts.json`.
 *
 * The scenarios are v4's OWN test cases from
 * `__tests__/unit/lib/database/backends/sqlite/chat-message-fts.test.ts`,
 * rebuilt AS DATA so both implementations run the same operations against the
 * same reduced `chat_messages` DDL (which lives in the corpus, not in either
 * side's source) — plus four this port adds: the whole schema absent, an
 * identical-text rewrite, a rebuild that crosses v4's 500-row batch boundary,
 * and the extra role arms (`TOOL`, `ASSISTANT`, a NULL role).
 *
 * What matters is the PLUMBING the triggers do, which no mock can stand in
 * for: that an insert indexes, a delete retires, a genuine edit retokenizes, a
 * RE-ENCODE (the compression backfill) does not, and that the eligibility
 * filter keeps the index to the rows search will ever return.
 *
 * After every step (every OBSERVED step — see `quiet`) each side records:
 *   - `missing`      — `missingChatMessageFtsObjects`;
 *   - `eligible` / `indexed` — the two counts the boot reconciler compares;
 *   - `map`          — every `chat_messages_fts_map` row, by `ftsId`, so a
 *                      stable index identity is a comparand and not a story;
 *   - `contents`     — per message id, the STORAGE FORM (`text` / `blob` /
 *                      `null`) and `qt_text()` of the cell, so an encoding
 *                      change is visible as an encoding change;
 *   - `searches`     — every corpus `MATCH` expression's hit list, ordered
 *                      `createdAt DESC` (v4's own probe query);
 *   - `ftsDataRows`  — `COUNT(*)` of the FTS5 shadow table
 *                      `chat_messages_fts_data`. This is the ONLY field that
 *                      can see the `_au` trigger's `WHEN qt_text(new) IS NOT
 *                      qt_text(old)` guard do its job: a delete-and-reinsert
 *                      of the same rowid with the same terms is otherwise
 *                      observationally identical, so without this the guard is
 *                      a performance comment rather than a tested invariant.
 *                      It is a MEASUREMENT: if the two SQLite builds' fts5
 *                      internals disagree, it is dropped with that recorded.
 *   - `error`        — the step's own thrown message, or null. The no-UDF
 *                      scenario's whole point is the message text.
 *   - `rebuild` / `progress` — the `ChatMessageFtsRebuildResult` (minus the
 *                      nondeterministic `durationMs`) and the per-batch
 *                      callback arguments.
 *   - `stale`        — (P4.D363, v4 `902c6efe9`) the `stale` step op's
 *                      `staleChatMessageFtsTriggers` answer, in v4's
 *                      `sqlite_master` order (its SQL has no ORDER BY).
 *   - `master`       — (P4.D363) the `master` step op's re-dump of the five
 *                      objects' `sqlite_master` rows, so the text a STALE
 *                      REPLACE leaves on disk is a comparand (the per-scenario
 *                      dump is taken before the steps).
 *   - `logs`         — (P4.D363) every line the step sent to the bare
 *                      `@/lib/logger` (which this module logs through):
 *                      level, message, context. The comparand for
 *                      `Replacing outdated chat message FTS triggers` and the
 *                      rebuild's three debug lines. Captured with a spy, not
 *                      the transports, because v4's logger filters `debug`
 *                      below its INFO default before any transport sees it.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores
 * .claude/ paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=<this worktree root>
 *   TMPO=/tmp/qt-chat-message-fts-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/chat-message-fts.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/chat-message-fts.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-chat-message-fts.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "chat-message-fts\.test\.ts$"
 */

import { describe, it, jest } from '@jest/globals';
import * as fs from 'fs';
import path from 'path';

import {
  chatMessageFtsEligibilitySql,
  chatMessageFtsObjectNames,
  countEligibleChatMessages,
  countIndexedChatMessages,
  ensureChatMessageFtsSchema,
  missingChatMessageFtsObjects,
  rebuildChatMessageFtsIndex,
  staleChatMessageFtsTriggers,
  CHAT_MESSAGE_FTS_SCHEMA_STATEMENTS,
} from '@/lib/database/backends/sqlite/chat-message-fts';
import { logger } from '@/lib/logger';
import { registerTextCodecFunction } from '@/lib/database/backends/sqlite/text-codec-function';
import { textToBlob } from '@/lib/database/text-compression';

// Real binding by absolute root path — a bare or nested require resolves to
// the jest mock, which returns empty result sets.
// eslint-disable-next-line @typescript-eslint/no-var-requires
const Database = require(path.join(process.cwd(), 'node_modules', 'better-sqlite3'));

type Content =
  | { kind: 'text'; value: string }
  | { kind: 'long' }
  | { kind: 'longBlob' }
  | { kind: 'null' };

interface Step {
  op: 'ensure' | 'exec' | 'insert' | 'update' | 'delete' | 'rebuild' | 'stale' | 'master';
  sql?: string;
  id?: string;
  content?: Content;
  type?: string;
  /** Absent = 'USER'; explicit null = a NULL role. */
  role?: string | null;
  /** P4.D363: absent = NULL. */
  systemSender?: string;
  /** P4.D363: absent = NULL. */
  customAnnouncer?: string;
}

interface Scenario {
  name: string;
  noUdf?: boolean;
  quiet?: boolean;
  steps: Step[];
}

interface Corpus {
  chatMessagesDdl: string;
  long: { unit: string; times: number };
  searches: string[];
  scenarios: Scenario[];
}

const corpusPath = path.join(__dirname, '..', 'fixtures', 'chat-message-fts.json');
const corpus = JSON.parse(fs.readFileSync(corpusPath, 'utf-8')) as Corpus;
const LONG = corpus.long.unit.repeat(corpus.long.times);

function contentValue(c: Content): unknown {
  switch (c.kind) {
    case 'text':
      return c.value;
    case 'long':
      return LONG;
    case 'longBlob': {
      const blob = textToBlob(LONG);
      if (!Buffer.isBuffer(blob)) {
        throw new Error('corpus expects LONG to compress, but textToBlob returned a string');
      }
      return blob;
    }
    case 'null':
      return null;
  }
}

/** Evaluate `f`, returning `[value, null]` or `[null, message]`. */
function attempt<T>(f: () => T): [T | null, string | null] {
  try {
    return [f(), null];
  } catch (err) {
    return [null, err instanceof Error ? err.message : String(err)];
  }
}

function observe(db: any, scenario: Scenario, extra: Record<string, unknown>) {
  const [missing, missingErr] = attempt(() => missingChatMessageFtsObjects(db));
  const [eligible, eligibleErr] = attempt(() => countEligibleChatMessages(db));
  const [indexed, indexedErr] = attempt(() => countIndexedChatMessages(db));
  const [map, mapErr] = attempt(() =>
    db
      .prepare('SELECT "ftsId", "messageId" FROM "chat_messages_fts_map" ORDER BY "ftsId"')
      .all()
      .map((r: { ftsId: number; messageId: string }) => ({
        ftsId: r.ftsId,
        messageId: r.messageId,
      })),
  );
  const [contents, contentsErr] = attempt(() =>
    db
      .prepare(
        scenario.noUdf
          ? 'SELECT "id", "content" AS raw, NULL AS decoded FROM "chat_messages" ORDER BY "id"'
          : 'SELECT "id", "content" AS raw, qt_text("content") AS decoded FROM "chat_messages" ORDER BY "id"',
      )
      .all()
      .map((r: { id: string; raw: unknown; decoded: string | null }) => ({
        id: r.id,
        kind: r.raw === null ? 'null' : Buffer.isBuffer(r.raw) ? 'blob' : 'text',
        decoded: r.decoded,
      })),
  );
  const searches: Record<string, string[] | null> = {};
  const searchErrors: Record<string, string> = {};
  for (const match of corpus.searches) {
    const [hits, err] = attempt(() =>
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
    );
    searches[match] = hits;
    if (err) searchErrors[match] = err;
  }
  const [ftsDataRows] = attempt(
    () =>
      (db.prepare('SELECT COUNT(*) AS n FROM "chat_messages_fts_data"').get() as { n: number }).n,
  );

  const obsErrors: Record<string, string> = {};
  if (missingErr) obsErrors.missing = missingErr;
  if (eligibleErr) obsErrors.eligible = eligibleErr;
  if (indexedErr) obsErrors.indexed = indexedErr;
  if (mapErr) obsErrors.map = mapErr;
  if (contentsErr) obsErrors.contents = contentsErr;
  for (const [k, v] of Object.entries(searchErrors)) obsErrors[`search:${k}`] = v;

  return {
    ...extra,
    missing,
    eligible,
    indexed,
    map,
    contents,
    searches,
    ftsDataRows,
    obsErrors,
  };
}

/** The five objects' `sqlite_master` rows, in the shape both dumps use. */
function masterRows(db: any) {
  return db
    .prepare(
      `SELECT name, type, sql FROM sqlite_master
        WHERE name IN (${chatMessageFtsObjectNames()
          .map(() => '?')
          .join(',')})
        ORDER BY name`,
    )
    .all(...chatMessageFtsObjectNames());
}

interface LogLine {
  level: string;
  message: string;
  context: unknown;
}
const logLines: LogLine[] = [];
for (const level of ['debug', 'info', 'warn', 'error'] as const) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (jest.spyOn(logger as any, level) as any).mockImplementation((message: string, context?: unknown) => {
    logLines.push({ level, message, context: context ?? null });
  });
}

describe('P4.D204 chat message FTS oracle', () => {
  it('records v4 driving the corpus script', () => {
    const out = process.env.QT_ORACLE_OUT;
    if (!out) throw new Error('set QT_ORACLE_OUT');
    const lines: string[] = [];

    // The DDL single-sourcing rows: object names and the eligibility fragment
    // at every alias the port uses, recorded rather than transcribed.
    lines.push(
      JSON.stringify({
        row: 'ddl',
        objectNames: chatMessageFtsObjectNames(),
        eligibilitySql: {
          '': chatMessageFtsEligibilitySql(),
          m: chatMessageFtsEligibilitySql('m'),
          mm: chatMessageFtsEligibilitySql('mm'),
        },
        statements: [...CHAT_MESSAGE_FTS_SCHEMA_STATEMENTS],
      }),
    );

    for (const scenario of corpus.scenarios) {
      const db = new Database(':memory:');
      try {
        if (!scenario.noUdf) registerTextCodecFunction(db);
        db.exec(corpus.chatMessagesDdl);
        ensureChatMessageFtsSchema(db);

        // The objects as SQLite itself stored them — compared against v5's own
        // `sqlite_master`, so the comparand is on-disk text, not two
        // transcriptions of the same intent.
        lines.push(
          JSON.stringify({
            row: 'sqliteMaster',
            scenario: scenario.name,
            objects: masterRows(db),
          }),
        );

        for (const [i, step] of scenario.steps.entries()) {
          let error: string | null = null;
          let rebuild: Record<string, number> | null = null;
          let progress: Array<[number, number]> | null = null;
          let stale: string[] | null = null;
          let master: unknown = null;
          logLines.length = 0;

          const [, err] = attempt(() => {
            switch (step.op) {
              case 'ensure':
                ensureChatMessageFtsSchema(db);
                return;
              case 'exec':
                db.exec(step.sql as string);
                return;
              case 'insert':
                // P4.D363: eight columns — the two author columns bound NULL
                // when the step omits them, so a pre-round six-column row is
                // the same row it always was.
                db.prepare(
                  'INSERT INTO "chat_messages" ("id","chatId","type","role","content","systemSender","customAnnouncer","createdAt") VALUES (?,?,?,?,?,?,?,?)',
                ).run(
                  step.id,
                  'chat-1',
                  step.type ?? 'message',
                  step.role === undefined ? 'USER' : step.role,
                  contentValue(step.content as Content),
                  step.systemSender ?? null,
                  step.customAnnouncer ?? null,
                  // v4's own helper: the id's last two digits become the
                  // second, so `createdAt DESC` is a total order.
                  `2026-01-01T00:00:${(step.id as string).slice(-2).padStart(2, '0')}.000Z`,
                );
                return;
              case 'update':
                db.prepare('UPDATE "chat_messages" SET "content" = ? WHERE "id" = ?').run(
                  contentValue(step.content as Content),
                  step.id,
                );
                return;
              case 'delete':
                db.prepare('DELETE FROM "chat_messages" WHERE "id" = ?').run(step.id);
                return;
              case 'rebuild': {
                progress = [];
                const result = rebuildChatMessageFtsIndex(db, (scanned, total) =>
                  (progress as Array<[number, number]>).push([scanned, total]),
                );
                rebuild = {
                  scanned: result.scanned,
                  indexed: result.indexed,
                  total: result.total,
                };
                return;
              }
              case 'stale':
                stale = staleChatMessageFtsTriggers(db);
                return;
              case 'master':
                master = masterRows(db);
                return;
            }
          });
          error = err;

          const observed =
            !scenario.quiet || step.op !== 'insert' || i === scenario.steps.length - 1;
          if (observed) {
            lines.push(
              JSON.stringify(
                observe(db, scenario, {
                  row: 'step',
                  scenario: scenario.name,
                  step: i,
                  op: step.op,
                  error,
                  rebuild,
                  progress,
                  stale,
                  master,
                  logs: [...logLines],
                }),
              ),
            );
          }
        }
      } finally {
        db?.close();
      }
    }

    fs.writeFileSync(out, lines.join('\n') + '\n');
  });
});
