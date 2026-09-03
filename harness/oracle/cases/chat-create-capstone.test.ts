/**
 * @jest-environment node
 *
 * CAPSTONE tier-3 ORACLE for the chat-creation spine (P4.4 unit 2, sub-unit 8):
 * drives v4's REAL `handleCreate` (app/api/v1/chats/route.ts POST, no action) and
 * emits, per corpus case, the created chat state so the Rust port
 * (services::chat_create::handle_create) can be diffed byte-for-byte.
 *
 * Only the seams the Rust harness also injects are mocked: the auth session
 * (getServerSession -> SINGLE_USER_ID), the startup gate (forced ready), the model
 * boundaries (createLLMProvider's streamMessage [greeting] + sendMessage [outfit
 * llm_choose] — recorded + canned by the exact call key), logLLMCall (no-op),
 * ensureProcessorRunning (no-op, so the in-process job runner never races the
 * dumps), getApiKeyForCheapLLMSelection (const), and Math.random (pinned per case,
 * so pickWeightedByTalkativeness is deterministic). The whole DB stack is doMocked
 * to the REAL modules (past jest.setup) + the real cipher binding, so the create
 * runs genuinely against a FRESH copy of the baked fixture per case.
 *
 * Emits one NDJSON line per case:
 *   { name, status, ok, body, tables:{chats,chatMessages,projects,backgroundJobs},
 *     frames, messageOrder, recordings:[{kind,provider,model,temperature,messages,...}] }
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-cc-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/chat-create-capstone.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/chat-create-capstone.json"  "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CC_MAIN=/tmp/qt-cc-main.db QT_FIXTURE_CC_MOUNT=/tmp/qt-cc-mount.db \
 *   QT_FIXTURE_CC_LLM=/tmp/qt-cc-llm.db QT_ORACLE_OUT=/tmp/oracle-chat-create.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- chat-create-capstone
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  cases: CaseSpec[];
}
interface CaseSpec {
  name: string;
  progressId: string;
  tz: string;
  nowMs: number;
  random01: number;
  request: Record<string, unknown>;
  /** Canned greeting content the streaming boundary yields (records the prompt). */
  greetingContent?: string;
  greetingUsage?: { promptTokens: number; completionTokens: number; totalTokens: number } | null;
  /**
   * P4.D79: the CUMULATIVE `reasoningContent` values the greeting stream emits.
   * v4 persists the last one onto the stored greeting message
   * (`reasoningContent: firstMessageReasoning || null`), so it lands in the
   * diffed `chat_messages` dump.
   */
  greetingReasoning?: string[];
  /** Canned outfit-choice content the cheap-LLM boundary returns. */
  outfitContent?: string;
}

// The MAIN-db tables to dump after each case, with the sort key.
const MAIN_TABLES: Array<{ key: string; table: string; orderBy: string }> = [
  { key: 'chats', table: 'chats', orderBy: 'id' },
  { key: 'chatMessages', table: 'chat_messages', orderBy: 'id' },
  { key: 'projects', table: 'projects', orderBy: 'id' },
  { key: 'backgroundJobs', table: 'background_jobs', orderBy: 'id' },
];

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
  orderBy: string;
}): { table: string; columns: string[]; rows: Array<Record<string, unknown>> } {
  const { table, columns, rawRows, orderBy } = opts;
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

function createMockRequest(body: unknown): unknown {
  return {
    method: 'POST',
    url: 'http://localhost/api/v1/chats',
    nextUrl: new URL('http://localhost/api/v1/chats'),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body),
  };
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string; llm: string },
): Promise<Record<string, unknown>> {
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
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
  );
  jest.doMock('@/lib/embedding/embedding-service', () =>
    jest.requireActual('@/lib/embedding/embedding-service'),
  );
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: spec.userId } }),
  }));
  jest.doMock('@/lib/startup/startup-state', () => {
    const actual = jest.requireActual('@/lib/startup/startup-state');
    return {
      __esModule: true,
      ...actual,
      startupState: {
        ...actual.startupState,
        isReady: () => true,
        waitForReady: async () => true,
        isPepperResolved: () => true,
        getPepperState: () => 'resolved',
        getPhase: () => 'ready',
        isLockedMode: () => false,
      },
    };
  });
  // Model / infra seams.
  jest.doMock('@/lib/services/llm-logging.service', () => {
    const actual = jest.requireActual('@/lib/services/llm-logging.service');
    return { __esModule: true, ...actual, logLLMCall: jest.fn(async () => undefined) };
  });
  jest.doMock('@/lib/background-jobs/host/processor-host', () => {
    const actual = jest.requireActual('@/lib/background-jobs/host/processor-host');
    return { __esModule: true, ...actual, ensureProcessorRunning: jest.fn(() => undefined) };
  });
  jest.doMock('@/lib/services/api-key.service', () => {
    const actual = jest.requireActual('@/lib/services/api-key.service');
    return {
      __esModule: true,
      ...actual,
      getApiKeyForCheapLLMSelection: jest.fn(async () => 'test-key-anthropic'),
    };
  });

  // Records every model call's exact prompt (the Rust side keys its canned
  // providers off these) and yields the case's canned content.
  const recordings: Array<Record<string, unknown>> = [];

  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    /** The knobs a call carried, with the undefined ones omitted (P4.D83). */
    const samplingOf = (p: { temperature?: number; maxTokens?: number; topP?: number }) => ({
      ...(p.temperature !== undefined ? { temperature: p.temperature } : {}),
      ...(p.maxTokens !== undefined ? { maxTokens: p.maxTokens } : {}),
      ...(p.topP !== undefined ? { topP: p.topP } : {}),
    });
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async (provider: string, _baseUrl?: string) => ({
        streamMessage: async function* streamMessage(
          params: {
            messages: Array<{ role: string; content: string }>;
            model: string;
            temperature?: number;
            maxTokens?: number;
            topP?: number;
          },
          _apiKey: string,
        ) {
          const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
          // P4.D83 (v4 `d89babc4`): the greeting path's sampling knobs, resolved
          // by the REAL `autoGenerateFirstMessage` off the profile's bag. The
          // key carries only the temperature, so Max Tokens and Top P went
          // unmeasured — and v5 read them under camelCase names the editor never
          // writes.
          recordings.push({ kind: 'stream', provider, model: params.model, temperature: params.temperature ?? null, sampling: samplingOf(params), messages });
          if (c.greetingContent === undefined) {
            throw new Error(`unexpected streamMessage call in case ${c.name}`);
          }
          for (const r of c.greetingReasoning ?? []) yield { reasoningContent: r };
          if (c.greetingContent) yield { content: c.greetingContent };
          if (c.greetingUsage) yield { usage: c.greetingUsage };
        },
        sendMessage: async (
          params: {
            messages: Array<{ role: string; content: string }>;
            model: string;
            temperature?: number;
            maxTokens?: number;
            topP?: number;
          },
          _apiKey: string,
        ) => {
          const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
          recordings.push({ kind: 'send', provider, model: params.model, temperature: params.temperature ?? null, sampling: samplingOf(params), messages });
          if (c.outfitContent === undefined) {
            throw new Error(`unexpected sendMessage call in case ${c.name}`);
          }
          return { content: c.outfitContent };
        },
      }),
    };
  });

  // Fresh fixture copy → case independence.
  const work = mkdtempSync(join(scratch, 'cc-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  const llmWork = join(work, 'llm.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  copyFileSync(fixtures.llm, llmWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.SQLITE_LLM_LOGS_PATH = llmWork;

  // Pin Math.random for pickWeightedByTalkativeness.
  const origRandom = Math.random;
  Math.random = () => c.random01;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite');
  const progressMod = await import('@/lib/chat/creation-progress');
  const { POST } = await import('@/app/api/v1/chats/route');

  await initializeDatabase();

  try {
    const req = createMockRequest(c.request);
    const response = await POST(req as never);
    const status: number = response.status;
    const body = await response.json();
    const ok = status >= 200 && status < 300;

    // Collect the ordered progress frames (buffered replay), stripping `ts`.
    const sub = progressMod.subscribeCreationProgress(c.progressId, () => {});
    const frames = sub.replay.map((f: Record<string, unknown>) => {
      const { ts: _ts, ...rest } = f;
      return rest;
    });
    sub.unsubscribe();

    // Dump the main-db tables.
    const mdb = getRawDatabase();
    if (!mdb) throw new Error('main DB handle unavailable for dump');
    const tables: Record<string, unknown> = {};
    for (const t of MAIN_TABLES) {
      const columns = (
        mdb.prepare(`PRAGMA table_info(${t.table})`).all() as Array<{ name: string }>
      ).map((col) => col.name);
      const rawRows = mdb.prepare(`SELECT * FROM ${t.table}`).all() as Array<
        Record<string, unknown>
      >;
      tables[t.key] = canonicalizeRows({ table: t.table, columns, rawRows, orderBy: t.orderBy });
    }

    // P4.D148: the INSERTION-ORDERED message trace. `tables.chatMessages` is
    // sorted (by id here, re-sorted by content on the Rust side), so it cannot
    // see WHERE a row landed — and the whole point of the create-time Concierge
    // flip is that its bubble sits between the system prompt and everything
    // else. `rowid` is insertion order on both sides (neither ever deletes a
    // message row), so this projection is the position proof.
    const messageOrder = (
      mdb
        .prepare(
          'SELECT "chatId", "type", "role", "systemSender", "systemKind", "content" FROM chat_messages ORDER BY rowid',
        )
        .all() as Array<Record<string, unknown>>
    ).map((r) => [r.chatId, r.type, r.role, r.systemSender, r.systemKind, r.content].map(canonValue));

    return { name: c.name, status, ok, body, tables, frames, messageOrder, recordings };
  } finally {
    Math.random = origRandom;
    await closeDatabase();
    closeMountIndexSQLiteClient();
    closeLLMLogsSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'chat-create-capstone.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_CC_MAIN ?? '',
    mount: process.env.QT_FIXTURE_CC_MOUNT ?? '',
    llm: process.env.QT_FIXTURE_CC_LLM ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cc-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const outLines: string[] = [];
  for (const c of spec.cases) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`chat-create oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('chat-create capstone oracle', async () => {
  await main();
});
