/**
 * @jest-environment node
 *
 * Tier-3 (mocked model boundaries) ORACLE for the Carina query engine
 * (v4 `runCarinaQuery`, lib/services/carina/carina.service.ts).
 *
 * Drives v4's REAL `runCarinaQuery` over the committed corpus
 * (harness/oracle/fixtures/carina-query-tier3.json) against the REAL two-database
 * fixture, mocking ONLY the model/infra seams the Rust port injects as seams:
 *
 *   - `streamMessage` (the primary stream): per-case scripted sequences popped in
 *     call order + RECORD the exact `provider|model|temperature|messages` canned
 *     key answered (so the Rust `QueuedStreamingProvider` replays them). `buildTools`
 *     is stubbed to an empty slate (its output is invisible to the diff — it never
 *     enters the canned stream key, and the tool loop is driven by the canned
 *     `detectToolCallsInResponse`, not by the slate).
 *   - `detectToolCallsInResponse`: canned by the raw response's `marker` field
 *     (`processToolCalls` stays REAL beneath it — mirrors the Rust `ToolCallDetector`
 *     seam + real `process_tool_calls`).
 *   - `executeToolCallWithContext` (@/lib/chat/tool-executor): canned per-call by
 *     `name|JSON.stringify(args)|callId` (mirrors the Rust `CannedToolRunner`).
 *   - `generateEmbeddingForUser`: the corpus's canned vectors keyed by the exact
 *     question text (only the memory-recall case registers one; every other
 *     question falls through to the DB text search — empty for a memory-less
 *     answerer — on both sides).
 *   - `runBrahmaQuery` (brahma-console/one-shot): canned to the corpus result
 *     (mirrors the Rust `RunBrahmaConsole` seam).
 *   - `ensureProcessorRunning`: no-op (the enqueued PENDING `background_jobs` rows
 *     are state under test).
 *
 * The wall clock is frozen to the corpus `nowMs` (the memory-recall block's
 * effective-weight + relative-age read `Date.now()`, which must equal the Rust
 * injected `now_ms` so the recall system-prompt block matches → the canned stream
 * key matches). Minted DB timestamps are normalized to `<ts>` in the harness.
 *
 * The RECORDED `onPosted` message (wrapped as `{ carinaAnswer: message }`) is
 * emitted per case as `kind:"event"`; the `CarinaResult` as `kind:"result"`;
 * `chat_messages` / `chats` / `background_jobs` are dumped once at the end.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=${V5W:-$HOME/source/quilltap-v5}   # the v5 checkout (or your worktree)
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-carina-query-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-carina-query-mount.db \
 *     $N/npx tsx $W/harness/oracle/fixtures/build-carina-query-fixture.ts
 *   QT_FIXTURE_CARINA_MAIN=/tmp/qt-carina-query-main.db QT_FIXTURE_CARINA_MOUNT=/tmp/qt-carina-query-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-carina-query.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 --roots "$PWD" --roots "$W/harness/oracle/cases" -- carina-query-tier3
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

interface ChunkSpec {
  content?: string;
  done?: boolean;
  rawResponse?: unknown;
}
interface CaseSpec {
  name: string;
  characterName: string;
  question: string;
  whisper: boolean;
  askerParticipantId: string | null;
  operatorInitiated: boolean;
  streams: ChunkSpec[][];
  expectPost: boolean;
  brahma: boolean;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  nowMs: number;
  userId: string;
  chat: { id: string };
  cannedEmbeddings: Record<string, number[]>;
  detection: Record<string, Array<{ name: string; arguments: Record<string, unknown>; callId?: string }>>;
  cannedTools: Array<{
    name: string;
    arguments: Record<string, unknown>;
    callId?: string;
    success: boolean;
    result: unknown;
    error?: string;
    message?: string;
  }>;
  brahma: { ok: boolean; answer: string; detail: string | null };
  cases: CaseSpec[];
}

function toolKey(tc: { name: string; arguments: Record<string, unknown>; callId?: string }): string {
  return `${tc.name}|${JSON.stringify(tc.arguments)}|${tc.callId ?? '-'}`;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'carina-query-tier3.json'), 'utf8')
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_CARINA_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_CARINA_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_CARINA_MAIN / QT_FIXTURE_CARINA_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-carina-query-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'carina-main.db');
  const workMount = join(scratch, 'carina-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // Global canned tool-result map + the per-case mutable stream state the mocks
  // read lazily.
  const cannedToolMap = new Map<string, Spec['cannedTools'][number]>();
  for (const t of spec.cannedTools) cannedToolMap.set(toolKey(t), t);

  let currentCase: CaseSpec = spec.cases[0];
  let streamCallIndex = 0;
  const cannedRows: Array<{
    provider: string;
    model: string;
    temperature: number | null;
    messages: Array<{ role: string; content: string }>;
    sequences: ChunkSpec[][];
  }> = [];

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store')
  );

  jest.doMock('@/lib/embedding/embedding-service', () => {
    const actual = jest.requireActual('@/lib/embedding/embedding-service');
    const { EmbeddingError } = actual;
    return {
      __esModule: true,
      ...actual,
      generateEmbeddingForUser: async (text: string) => {
        const vec = spec.cannedEmbeddings[text];
        if (!vec) {
          throw new EmbeddingError(`no canned embedding registered for input (${text.length} chars)`);
        }
        return {
          embedding: new Float32Array(vec),
          model: 'canned',
          dimensions: vec.length,
          provider: 'canned',
        };
      },
    };
  });

  // streamMessage: scripted per-case sequences popped in call order; buildTools
  // stubbed to an empty slate. Everything else in streaming.service stays REAL.
  jest.doMock('@/lib/services/chat-message/streaming.service', () => {
    const actual = jest.requireActual('@/lib/services/chat-message/streaming.service');
    return {
      __esModule: true,
      ...actual,
      buildTools: async () => ({ tools: [], useNativeWebSearch: false }),
      streamMessage: async function* (opts: {
        messages: Array<{ role: string; content: string }>;
        connectionProfile: { provider: string; modelName: string };
        modelParams?: { temperature?: number };
      }) {
        const seq = currentCase.streams[streamCallIndex];
        streamCallIndex += 1;
        if (!seq) {
          throw new Error(`no scripted stream #${streamCallIndex - 1} for case ${currentCase.name}`);
        }
        cannedRows.push({
          provider: opts.connectionProfile.provider,
          model: opts.connectionProfile.modelName,
          temperature: opts.modelParams?.temperature ?? null,
          messages: opts.messages.map((m) => ({ role: m.role, content: m.content })),
          sequences: [seq],
        });
        for (const chunk of seq) {
          if (chunk.done) {
            yield { done: true, rawResponse: chunk.rawResponse };
          } else {
            yield { content: chunk.content };
          }
        }
      },
    };
  });

  // detectToolCallsInResponse: canned by the raw response marker (processToolCalls
  // stays REAL). createToolContext stays REAL.
  jest.doMock('@/lib/services/chat-message/tool-execution.service', () => {
    const actual = jest.requireActual('@/lib/services/chat-message/tool-execution.service');
    return {
      __esModule: true,
      ...actual,
      detectToolCallsInResponse: (raw: unknown) => {
        const marker = (raw as { marker?: string } | null)?.marker;
        return (marker && spec.detection[marker]) || [];
      },
    };
  });

  // executeToolCallWithContext: canned per-call (mirrors the Rust CannedToolRunner).
  jest.doMock('@/lib/chat/tool-executor', () => {
    const actual = jest.requireActual('@/lib/chat/tool-executor');
    return {
      __esModule: true,
      ...actual,
      executeToolCallWithContext: async (toolCall: {
        name: string;
        arguments: Record<string, unknown>;
        callId?: string;
      }) => {
        const hit = cannedToolMap.get(toolKey(toolCall));
        if (!hit) throw new Error(`no canned tool result for ${toolKey(toolCall)}`);
        return {
          toolName: hit.name,
          success: hit.success,
          result: hit.result,
          error: hit.error,
          message: hit.message,
        };
      },
    };
  });

  // runBrahmaQuery: canned to the corpus result (mirrors the Rust RunBrahmaConsole).
  jest.doMock('@/lib/services/brahma-console/one-shot.service', () => ({
    __esModule: true,
    runBrahmaQuery: async () => ({ ...spec.brahma, detail: spec.brahma.detail ?? undefined }),
  }));

  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { runCarinaQuery } = await import('@/lib/services/carina/carina.service');

  await initializeDatabase();

  // Freeze the wall clock so the memory-recall block's effective-weight +
  // relative-age (which read Date.now()) match the Rust injected now_ms.
  const RealDate = Date;
  const FakeDate = class extends RealDate {
    constructor(...args: unknown[]) {
      if (args.length === 0) super(spec.nowMs);
      // @ts-expect-error variadic forwarding
      else super(...args);
    }
    static now(): number {
      return spec.nowMs;
    }
  } as DateConstructor;
  (global as { Date: DateConstructor }).Date = FakeDate;

  const lines: string[] = [];

  for (const call of spec.cases) {
    currentCase = call;
    streamCallIndex = 0;

    let postedEvent: unknown = null;
    const result = await runCarinaQuery({
      userId: spec.userId,
      chatId: spec.chat.id,
      characterName: call.characterName,
      question: call.question,
      whisper: call.whisper,
      askerParticipantId: call.askerParticipantId,
      operatorInitiated: call.operatorInitiated,
      onPosted: (msg: unknown) => {
        postedEvent = { carinaAnswer: msg };
      },
    } as never);

    lines.push(JSON.stringify({ kind: 'result', call: call.name, result }));
    lines.push(JSON.stringify({ kind: 'event', call: call.name, event: postedEvent }));

    // Let any fire-and-forget settle before the next case.
    await new Promise((resolve) => setTimeout(resolve, 20));
  }

  (global as { Date: DateConstructor }).Date = RealDate;

  for (const row of cannedRows) lines.push(JSON.stringify({ kind: 'cannedStream', ...row }));

  const dumpTable = async (table: string, orderBy: string) => {
    const columns = ((await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>).map(
      (c) => c.name
    );
    const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };

  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chat_messages', 'content')) }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chats', 'id')) }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('background_jobs', 'id')) }));

  closeMountIndexSQLiteClient();
  await closeDatabase();

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`carina-query oracle wrote ${outPath}\n`);
}

test('carina-query tier-3 oracle', async () => {
  await main();
});
