/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the Carina markup runner
 * (v4 `runCarinaMarkupQuery`, lib/services/carina/markup-runner.ts).
 *
 * Drives v4's REAL `runCarinaMarkupQuery` over the committed corpus
 * (harness/oracle/fixtures/carina-runner-tier3.json) against the REAL fixture DB,
 * with ONLY the two seams the runner reaches stubbed:
 *
 *   - `runCarinaQuery` (from `./carina.service`) — the isolated query engine
 *     (name resolution + model + tool loop) is unported (wave 4 tool subsystem +
 *     unported resolvers/commonplace/Brahma), so it is the injected seam. The
 *     mock resolves each call to the corpus `outcome`: an `ok` outcome with
 *     `post:true` posts a REAL Carina message through v4's REAL `postCarinaResponse`
 *     (so `chat_messages` fills byte-for-byte) and fires the caller's `onPosted`
 *     exactly as the real query does, then returns `{ ok: true, ... }`; an `err`
 *     outcome returns `{ ok: false, error }`; a `throw` outcome throws (so the
 *     runner's outer catch is exercised). The RECORDED query args
 *     (characterName / question / whisper / askerParticipantId / operatorInitiated)
 *     are emitted so the Rust side pins the same seam inputs.
 *   - `postProsperoCarinaError` (from `@/lib/services/prospero-notifications/writer`)
 *     — the post-office subsystem (wave 4+). Mocked to a recorder: it captures
 *     the exact `{ chatId, kind, characterName, detail, whisper, askerParticipantId }`
 *     and posts nothing.
 *
 * Everything else — `parseCarinaQuery`, the runner's control flow, the callback
 * wiring, the REAL `postCarinaResponse` + `repos.chats.addMessage` — is v4's REAL
 * code against the REAL fixture DB.
 *
 * The runner-observable trace (ordered callback fires + seam invocations) is
 * emitted per call as `kind: "trace"`; `chat_messages` is dumped once at the end.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-carina-runner.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-carina-runner-fixture.ts
 *   QT_FIXTURE_CARINA=/tmp/qt-carina-runner.db \
 *   QT_ORACLE_OUT=/tmp/oracle-carina-runner.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- carina-runner-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}
function canonicalizeRows(opts: {
  table: string;
  columns: string[];
  rawRows: Array<Record<string, unknown>>;
  orderBy?: string;
}): { table: string; columns: string[]; rows: Array<Record<string, unknown>> } {
  const { table, columns, rawRows, orderBy = 'id' } = opts;
  const rows = rawRows
    .map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
      return out;
    })
    .sort((a, b) => {
      const av = String(a[orderBy] ?? '');
      const bv = String(b[orderBy] ?? '');
      return av < bv ? -1 : av > bv ? 1 : 0;
    });
  return { table, columns, rows };
}

interface CarinaErrorSpec {
  kind: 'not-found' | 'no-profile' | 'llm-failed';
  characterName?: string | null;
  detail?: string | null;
}
type Outcome =
  | { kind: 'ok'; answer: string; post: boolean; whisper?: boolean }
  | { kind: 'err'; error: CarinaErrorSpec }
  | { kind: 'throw'; message: string };
interface CallSpec {
  name: string;
  text: string;
  askerParticipantId: string | null;
  operatorInitiated: boolean;
  logLabels: { detected: string; failed: string };
  outcome: Outcome;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  chat: { id: string };
  answererId: string;
  answererParticipantId: string;
  askerParticipantId: string;
  calls: CallSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'carina-runner-tier3.json'), 'utf8')
  ) as Spec;

  const fixture = process.env.QT_FIXTURE_CARINA;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_CARINA must point at the seed fixture .db');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-carina-runner-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'carina-main.db');
  copyFileSync(fixture, workMain);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // The current corpus call being served, so the doMocked seams can resolve it.
  let currentCall: CallSpec | null = null;
  // The ordered runner-observable trace for the current call.
  let trace: unknown[] = [];

  // Restore the real DB stack past jest.setup's global mocks; then pin the two
  // seams the runner reaches. doMock is not hoisted, so it runs now and its
  // factories close over the mutable `currentCall` / `trace`.
  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories')
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));

  // The isolated-query seam. Posts a real answer for `ok`+`post`, records nothing
  // internal (the message is proven by the chat_messages dump); the runner-visible
  // trace records the seam INPUTS, deferring the answerer resolution to the corpus.
  jest.doMock('@/lib/services/carina/carina.service', () => ({
    __esModule: true,
    runCarinaQuery: async (queryOpts: {
      userId: string;
      chatId: string;
      characterName: string;
      question: string;
      whisper: boolean;
      askerParticipantId: string | null;
      operatorInitiated?: boolean;
      onPosted?: (m: unknown) => void;
    }) => {
      trace.push({
        t: 'query-invoked',
        characterName: queryOpts.characterName,
        question: queryOpts.question,
        whisper: queryOpts.whisper,
        askerParticipantId: queryOpts.askerParticipantId ?? null,
        operatorInitiated: queryOpts.operatorInitiated === true,
      });
      const outcome = currentCall!.outcome;
      if (outcome.kind === 'throw') {
        throw new Error(outcome.message);
      }
      if (outcome.kind === 'err') {
        return { ok: false, error: outcome.error };
      }
      // ok
      if (!outcome.post) {
        // Should not happen for a real markup call, but the corpus's no_markup
        // case never reaches here (parse returns null). Guard defensively.
        return {
          ok: true,
          answer: outcome.answer,
          messageId: 'unused',
          message: { type: 'message', id: 'unused' },
          answererId: spec.answererId,
          answererName: queryOpts.characterName,
        };
      }
      // Post the answer through v4's REAL postCarinaResponse.
      const { postCarinaResponse } = await import('@/lib/services/carina/writer');
      const posted = await postCarinaResponse({
        chatId: queryOpts.chatId,
        answer: outcome.answer,
        answererId: spec.answererId,
        question: queryOpts.question,
        participantId: spec.answererParticipantId,
        whisper: queryOpts.whisper,
        askerParticipantId: queryOpts.askerParticipantId,
      });
      if (!posted) {
        return {
          ok: false,
          error: { kind: 'llm-failed', detail: 'failed to persist answer', characterName: queryOpts.characterName },
        };
      }
      // Surface live over the caller's stream, exactly as the real query does.
      if (queryOpts.onPosted) {
        try {
          queryOpts.onPosted(posted);
        } catch {
          /* the real query swallows a failed emit */
        }
      }
      return {
        ok: true,
        answer: outcome.answer,
        messageId: posted.id,
        message: posted,
        answererId: spec.answererId,
        answererName: queryOpts.characterName,
      };
    },
  }));

  // The Prospero error seam — a recorder that posts nothing.
  jest.doMock('@/lib/services/prospero-notifications/writer', () => {
    const actual = jest.requireActual('@/lib/services/prospero-notifications/writer');
    return {
      __esModule: true,
      ...actual,
      postProsperoCarinaError: async (args: {
        chatId: string;
        kind: string;
        characterName?: string | null;
        detail?: string | null;
        whisper: boolean;
        askerParticipantId?: string | null;
      }) => {
        trace.push({
          t: 'prospero-error',
          chatId: args.chatId,
          kind: args.kind,
          characterName: args.characterName ?? null,
          detail: args.detail ?? null,
          whisper: args.whisper,
          askerParticipantId: args.askerParticipantId ?? null,
        });
        return null;
      },
    };
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { runCarinaMarkupQuery } = await import('@/lib/services/carina/markup-runner');

  await initializeDatabase();

  const lines: string[] = [];

  for (const call of spec.calls) {
    currentCall = call;
    trace = [];

    await runCarinaMarkupQuery({
      userId: spec.userId,
      chatId: spec.chat.id,
      text: call.text,
      askerParticipantId: call.askerParticipantId,
      operatorInitiated: call.operatorInitiated,
      logLabels: call.logLabels,
      onConsulting: (characterName: string) => {
        trace.push({ t: 'consulting', characterName });
      },
      // Splice a PUBLIC answer — record the id so the Rust side proves the same
      // (whispers must NOT reach this). Ordered relative to query-invoked.
      onPublicAnswer: (msg: { id: string }) => {
        trace.push({ t: 'public-answer', messageId: msg.id });
      },
      // onPosted is seam-internal (passed through to runCarinaQuery); not part of
      // the runner-observable trace.
    });

    lines.push(JSON.stringify({ kind: 'trace', call: call.name, trace }));
  }

  const dumpTable = async (table: string, orderBy: string) => {
    const columns = (
      (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
    ).map((c) => c.name);
    const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };

  // Order by `content` — deterministic (each posted answer text is distinct),
  // unlike the minted `id` / `createdAt` which differ per side.
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chat_messages', 'content')) }));

  await closeDatabase();

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`carina-runner oracle wrote ${outPath}\n`);
}

test('carina-runner tier-3 oracle', async () => {
  await main();
});
