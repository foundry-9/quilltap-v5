/**
 * @jest-environment node
 *
 * P4.6ay unit 7 ORACLE: drives v4's REAL custom-tools route handlers
 * (`app/api/v1/chats/[id]/custom-tools/route.ts` GET + POST ?action=run) over a
 * FRESH copy of the committed `pascal-run-custom-{main,mount}.db` fixture per
 * case. GET emits the response body (the merged-per-perspective roster); POST
 * emits the body + the posted `chat_messages` system rows. Diffed against the
 * Rust dispatch (`api::custom_tools::{chat_custom_tools_list,
 * chat_custom_tool_run}`).
 *
 * The fixture's three characters each carry their OWN `ansible.tool.json`
 * (distinct files → distinct variants → the labelled branch), while `coin` and
 * `whispered` live only in char A's vault (seen by B/C via the participant tier
 * → one unlabelled row). All rolls are `min === max` (deterministic).
 *
 * Run (Node 24, from the v4 checkout — mirror the case file to /tmp; jest
 * ignores `.claude/`):
 *   QT_FIXTURE_PASCAL_MAIN=<v5>/crates/quilltap-web/tests/fixtures/pascal-run-custom-main.db \
 *   QT_FIXTURE_PASCAL_MOUNT=<v5>/crates/quilltap-web/tests/fixtures/pascal-run-custom-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-pascal-custom-tools-route.ndjson \
 *     npx jest --silent --roots "$PWD" --roots "$M/cases" -- pascal-custom-tools-route
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

const CHAT = 'c1000000-0000-4000-8000-000000000001';
const CHAR_A = 'a1000000-0000-4000-8000-00000000000a';
const CHAR_B = 'a1000000-0000-4000-8000-00000000000b';

interface CaseSpec {
  name: string;
  method: 'GET' | 'POST';
  body?: Record<string, unknown>;
  /**
   * P4.6bd: insert ONE cheap connection profile before the run, so the consult
   * RESOLVES through the (mocked, recording) provider. The committed fixture
   * itself stays profile-free — the no-profiles case depends on that.
   */
  profile?: boolean;
}

/** The same profile the handler oracle inserts (see its CONSULT_PROFILE). */
const CONSULT_PROFILE = {
  name: 'Consult Canned',
  provider: 'Anthropic',
  transport: 'api',
  courierDeltaMode: false,
  apiKeyId: null,
  baseUrl: null,
  modelName: 'consult-canned-model',
  parameters: {},
  isDefault: true,
  isCheap: true,
  allowWebSearch: false,
  useNativeWebSearch: false,
  allowToolUse: false,
  pseudoToolMode: 'auto',
  modelClass: null,
  maxContext: null,
  maxTokens: null,
  isDangerousCompatible: false,
  supportsImageUpload: false,
  tags: [],
  sortIndex: 0,
  totalTokens: 0,
  totalPromptTokens: 0,
  totalCompletionTokens: 0,
  messageCount: 0,
};
const CONSULT_PROFILE_ID = 'cc000000-0000-4000-8000-000000000001';
const CONSULT_PROFILE_TS = '2026-03-01T00:00:00.000Z';

/** The canned consult answer: `YES` hits the oracle tool's `eq: 'YES'` row. */
const CONSULT_ANSWER = 'YES';
const CONSULT_USAGE = { promptTokens: 42, completionTokens: 3, totalTokens: 45 };

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: body ? 'POST' : 'GET',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();

  // P4.6bd: recorded canned-completion entries (the key format MUST match
  // `quilltap_core::model::completion::canned_completion_key`).
  const cannedRecorded = new Map<
    string,
    {
      provider: string;
      model: string;
      temperature: number | null;
      messages: Array<{ role: string; content: string }>;
      response: string;
      usage: { promptTokens: number; completionTokens: number; totalTokens: number };
    }
  >();

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  // P4.6bd: the consult's provider seam — a recording canned provider. Only a
  // resolving consult ever reaches it.
  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async (provider: string, _baseUrl?: string) => ({
        sendMessage: async (
          params: {
            messages: Array<{ role: string; content: string }>;
            model: string;
            temperature?: number;
          },
          _apiKey: string,
        ) => {
          const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
          const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
          if (!cannedRecorded.has(key)) {
            cannedRecorded.set(key, {
              provider,
              model: params.model,
              temperature: params.temperature ?? null,
              messages,
              response: CONSULT_ANSWER,
              usage: CONSULT_USAGE,
            });
          }
          return { content: CONSULT_ANSWER, finishReason: 'stop', usage: CONSULT_USAGE };
        },
      }),
    };
  });
  jest.doMock('@/lib/services/api-key.service', () => {
    const actual = jest.requireActual('@/lib/services/api-key.service');
    return {
      __esModule: true,
      ...actual,
      getApiKeyForCheapLLMSelection: async () => 'test-key',
    };
  });
  // Run the REAL logLLMCall so the CUSTOM_TOOL_CONSULT row lands and is diffed.
  jest.doMock('@/lib/services/llm-logging.service', () =>
    jest.requireActual('@/lib/services/llm-logging.service'),
  );
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories'),
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
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

  const work = mkdtempSync(join(scratch, 'route-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  // A fresh llm-logs DB per case for the un-mocked logLLMCall (P4.6bd).
  process.env.SQLITE_LLM_LOGS_PATH = join(work, 'llm-logs.db');

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient, getRawLLMLogsDatabase } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite');

  await initializeDatabase();

  if (c.profile) {
    const { ConnectionProfilesRepository } = await import(
      '@/lib/database/repositories/connection-profiles.repository'
    );
    const repo = new ConnectionProfilesRepository();
    await repo.create({ ...CONSULT_PROFILE, userId: spec.userId } as never, {
      id: CONSULT_PROFILE_ID,
      createdAt: CONSULT_PROFILE_TS,
      updatedAt: CONSULT_PROFILE_TS,
    });
  }

  try {
    const params = { params: Promise.resolve({ id: CHAT }) };
    const base = `http://localhost/api/v1/chats/${CHAT}/custom-tools`;
    let response: { status: number; json: () => Promise<unknown> };
    if (c.method === 'GET') {
      const { GET } = await import('@/app/api/v1/chats/[id]/custom-tools/route');
      response = (await GET(mockRequest(base) as never, params as never)) as never;
    } else {
      const { POST } = await import('@/app/api/v1/chats/[id]/custom-tools/route');
      response = (await POST(
        mockRequest(`${base}?action=run`, c.body) as never,
        params as never,
      )) as never;
    }
    const status = response.status;
    const body = await response.json();

    // Let the fire-and-forget logLLMCall settle before dumping (the
    // oracle-fire-and-forget-tail rule: stray promises poison the NEXT case).
    await new Promise((resolve) => setTimeout(resolve, 200));

    let systemRows: unknown[] = [];
    if (c.method === 'POST') {
      const mdb = getRawDatabase();
      if (!mdb) throw new Error('main DB handle unavailable');
      const hasTable = mdb
        .prepare(`SELECT name FROM sqlite_master WHERE type='table' AND name='chat_messages'`)
        .get();
      if (hasTable) {
        const columns = (
          mdb.prepare(`PRAGMA table_info(chat_messages)`).all() as Array<{ name: string }>
        ).map((col) => col.name);
        const rawRows = mdb
          .prepare(`SELECT * FROM chat_messages WHERE systemSender IS NOT NULL ORDER BY createdAt`)
          .all() as Array<Record<string, unknown>>;
        systemRows = rawRows.map((r) => {
          const out: Record<string, unknown> = {};
          for (const col of columns) out[col] = canonValue(r[col]);
          return out;
        });
      }
    }
    // P4.6bd: the llm_logs rows this case wrote (id/timestamps placeholdered,
    // sorted by canonical JSON — the memory-processor dump shape).
    let llmLogs: { columns: string[]; rows: Array<Record<string, unknown>> } = {
      columns: [],
      rows: [],
    };
    const lldb = getRawLLMLogsDatabase();
    if (lldb) {
      try {
        const llColumns = (lldb.pragma('table_info(llm_logs)') as Array<{ name: string }>).map(
          (col) => col.name,
        );
        const llRaw = lldb.prepare('SELECT * FROM llm_logs').all() as Array<
          Record<string, unknown>
        >;
        const llRows = llRaw
          .map((r) => {
            const out: Record<string, unknown> = {};
            for (const col of llColumns) out[col] = canonValue(r[col]);
            out.id = '<id>';
            out.createdAt = '<ts>';
            out.updatedAt = '<ts>';
            return out;
          })
          .sort((a, b) => {
            const sa = JSON.stringify(a);
            const sb = JSON.stringify(b);
            return sa < sb ? -1 : sa > sb ? 1 : 0;
          });
        llmLogs = { columns: llColumns, rows: llRows };
      } catch {
        // No llm_logs table (the case never logged) — keep the empty dump.
      }
    }

    return {
      name: c.name,
      status,
      body,
      systemRows,
      canned: Array.from(cannedRecorded.values()),
      llmLogs,
    };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    closeLLMLogsSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'pascal-run-custom.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_PASCAL_MAIN ?? '',
    mount: process.env.QT_FIXTURE_PASCAL_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-pascal-route-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cases: CaseSpec[] = [
    { name: 'list', method: 'GET' },
    { name: 'run-coin-as-a', method: 'POST', body: { tool: 'coin', asCharacterId: CHAR_A } },
    { name: 'run-ansible-hit', method: 'POST', body: { tool: 'ansible', asCharacterId: CHAR_A } },
    { name: 'run-ansible-miss', method: 'POST', body: { tool: 'ansible', asCharacterId: CHAR_B } },
    { name: 'run-no-character', method: 'POST', body: { tool: 'coin' } },
    { name: 'run-private', method: 'POST', body: { tool: 'coin', asCharacterId: CHAR_A, private: true } },
    { name: 'run-unknown-tool', method: 'POST', body: { tool: 'nope', asCharacterId: CHAR_A } },
    { name: 'run-unknown-character', method: 'POST', body: { tool: 'coin', asCharacterId: 'a1000000-0000-4000-8000-0000000000ff' } },
    { name: 'run-error', method: 'POST', body: { tool: 'coin', asCharacterId: CHAR_A, parameters: { bad: 1 } } },
    // The 616930db consult through the CHAT entrance (v4 `handleRun` builds the
    // invoker with `chatId: id`). The fixture has no connection profiles, so
    // both sides take the `no connection profiles are configured` arm and the
    // posted row carries `pascalMeta.llm` — this is what covers the chat-run
    // writer, the third of the three.
    { name: 'run-oracle-consult', method: 'POST', body: { tool: 'oracle', asCharacterId: CHAR_A } },
    // P4.6bd: the consult RESOLVES through the CHAT entrance — a profile is
    // inserted, the (recording) provider answers 'YES', the `eq: 'YES'` outcome
    // row fires, and the CUSTOM_TOOL_CONSULT llm-log row lands.
    { name: 'run-oracle-consult-resolved', method: 'POST', body: { tool: 'oracle', asCharacterId: CHAR_A }, profile: true },
    // P4.d10 `$state`: the manual-run entrance resolves the cascade scoped to
    // `asCharacterId`'s own groups (hit for CHAR_A; group-less fallback for
    // CHAR_B) — the same asymmetry as metadata.
    { name: 'run-stateful-as-a', method: 'POST', body: { tool: 'stateful', asCharacterId: CHAR_A } },
    { name: 'run-stateful-as-b', method: 'POST', body: { tool: 'stateful', asCharacterId: CHAR_B } },
  ];

  const out = fs.createWriteStream(outPath);
  for (const c of cases) {
    const row = await runCase(spec, c, scratch, fixtures);
    out.write(JSON.stringify(row) + '\n');
  }
  out.end();
  await new Promise((r) => out.on('finish', r));
  rmSync(scratch, { recursive: true, force: true });
}

it('emits the pascal custom-tools route oracle', async () => {
  await main();
}, 120000);
