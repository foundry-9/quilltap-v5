/**
 * @jest-environment node
 *
 * Tier-2 ORACLE for Continue Elsewhere — v4's REAL `applyChatContinuation`
 * (`lib/chat/apply-chat-continuation.ts`), driven directly against the REAL
 * two-DB fixture, one FRESH COPY per case.
 *
 * The service is a pure repos-over-DB unit: no LLM, no embeddings, no storage
 * manager, no vault. So nothing is mocked except the four `jest.setup.ts` stubs
 * that would otherwise make a real database impossible (`better-sqlite3` is
 * mapped to a fake driver; `@/lib/database/manager`, `@/lib/database/repositories`
 * and `@/lib/repositories/factory` are stubbed wholesale), plus the background
 * processor, so the in-process job runner cannot race the dumps.
 *
 * The route the feature ships behind (`POST /api/v1/chats` with
 * `continuationFromChatId`) is NOT driven here — its 404 pre-check and its
 * try/catch belong to `chat_create_capstone_equivalence`, which already carries
 * `cs_continuation_bubble_before_replay`. This family is the service.
 *
 * Emits one NDJSON line per case:
 *   { name, result: {replayedMessageCount, hadLibrarianSummary,
 *     postedSourceTailBubble}, tables: {chats, chatMessages}, messageOrder }
 *
 * `messageOrder` is the `rowid`-ordered projection: continuation's contract is
 * POSITIONAL (link bubble → replayed tail → tail bubble in the source), and a
 * key-sorted table dump cannot see it.
 *
 * Run (Node 24, from the v4 checkout; jest ignores `.claude/` paths, so the case
 * is staged in a /tmp mirror):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-continuation-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-continuation-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-chat-continuation-fixture.ts
 *   TMPO=/tmp/qt-continuation-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/chat-continuation-tier2.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/chat-continuation-tier2.json"  "$TMPO/fixtures/"
 *   TZ=UTC QT_FIXTURE_CONT_MAIN=/tmp/qt-continuation-main.db \
 *   QT_FIXTURE_CONT_MOUNT=/tmp/qt-continuation-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-chat-continuation.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "chat-continuation-tier2\.test\.ts$"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CaseSpec {
  name: string;
  source: string;
  destination: string;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  cases: CaseSpec[];
}

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'chat-continuation-tier2.json'), 'utf8'),
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_CONT_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_CONT_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_CONT_MAIN / QT_FIXTURE_CONT_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const lines: string[] = [];

  for (const c of spec.cases) {
    jest.resetModules();
    const cipherDriverPath = require('node:path').join(
      process.cwd(),
      'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
    );
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () =>
      jest.requireActual('@/lib/database/repositories'),
    );
    jest.doMock('@/lib/repositories/factory', () =>
      jest.requireActual('@/lib/repositories/factory'),
    );
    // The in-process job runner must never race the dumps; continuation
    // enqueues nothing, but a transitive import could arm the pump.
    jest.doMock('@/lib/background-jobs/host/processor-host', () => ({
      __esModule: true,
      ...jest.requireActual('@/lib/background-jobs/host/processor-host'),
      ensureProcessorRunning: () => undefined,
    }));

    const scratch = mkdtempSync(join(tmpdir(), 'qt-continuation-oracle-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const workMain = join(scratch, 'main.db');
    const workMount = join(scratch, 'mount.db');
    copyFileSync(fixtureMain, workMain);
    copyFileSync(fixtureMount, workMount);

    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.SQLITE_PATH = workMain;
    process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    const { initializeDatabase, closeDatabase, rawQuery } = await import(
      '@/lib/database/manager'
    );
    const { getRepositories } = await import('@/lib/repositories/factory');
    const { closeMountIndexSQLiteClient } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    const { applyChatContinuation } = await import('@/lib/chat/apply-chat-continuation');

    await initializeDatabase();
    const repos = getRepositories();

    const result = await applyChatContinuation({
      newChatId: c.destination,
      sourceChatId: c.source,
      userId: spec.userId,
      repos,
    } as never);

    const dumpTable = async (table: string) => {
      const columns = ((await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>).map(
        (col) => col.name,
      );
      const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
      const rows = rawRows.map((r) => {
        const out: Record<string, unknown> = {};
        for (const col of columns) out[col] = canonValue(r[col]);
        return out;
      });
      return { table, columns, rows };
    };

    // The insertion-ordered trace: `rowid` is insertion order on both sides
    // (neither side ever deletes a message row), and continuation's contract is
    // positional.
    const messageOrder = (
      (await rawQuery(
        'SELECT "chatId", "type", "role", "systemSender", "systemKind", "content", ' +
          '"participantId", "targetParticipantIds", "opaqueContent", "hostEvent", ' +
          '"provider", "modelName", "tokenCount", "routeTrail" ' +
          'FROM chat_messages ORDER BY rowid',
      )) as Array<Record<string, unknown>>
    ).map((r) => {
      const out: Record<string, unknown> = {};
      for (const k of Object.keys(r)) out[k] = canonValue(r[k]);
      return out;
    });

    const tables = {
      chats: await dumpTable('chats'),
      chatMessages: await dumpTable('chat_messages'),
    };

    lines.push(JSON.stringify({ name: c.name, result, tables, messageOrder }));

    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(scratch, { recursive: true, force: true });
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
}

it('emits the chat-continuation tier-2 oracle', async () => {
  await main();
}, 120000);
