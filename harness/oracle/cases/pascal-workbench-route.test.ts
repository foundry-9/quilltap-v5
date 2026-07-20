/**
 * @jest-environment node
 *
 * P4.6ay unit 12 ORACLE (part B): drives v4's REAL `/api/v1/custom-tools` route
 * handlers (GET library / `?action=destinations`, POST `?action=preview` /
 * `?action=audit`) over a FRESH copy of the committed
 * `workbench-{main,mount}.db` fixture per case. Emits `{status, body}` rows,
 * diffed against the Rust dispatch (`api::custom_tools::{custom_tools_library,
 * custom_tools_destinations, custom_tool_preview, custom_tool_audit}`).
 *
 * The case corpus lives in `harness/oracle/fixtures/workbench-route-cases.json`
 * and is read by BOTH sides, so the two can never drift apart. Every roll in it
 * is `min === max`, which is what makes preview and audit exactly comparable
 * across languages: v4's `simulateOutcomes` draws through the crypto with no
 * seam, so a draw-for-draw diff is impossible — a deterministic corpus never
 * consumes a byte and pins exact `runs`/`hits`/`share`/min/max/mean instead.
 * Stochastic spread is covered by the v5-side statistical tests mirroring v4's
 * own `custom-tools-simulate.test.ts`.
 *
 * Run (Node 24, from the v4 checkout — mirror the case file to /tmp; jest
 * ignores `.claude/`):
 *   QT_FIXTURE_WORKBENCH_MAIN=<v5>/crates/quilltap-web/tests/fixtures/workbench-main.db \
 *   QT_FIXTURE_WORKBENCH_MOUNT=<v5>/crates/quilltap-web/tests/fixtures/workbench-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-pascal-workbench-route.ndjson \
 *     npx jest --silent --roots "$PWD" --roots "$M/cases" -- pascal-workbench-route
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

interface CaseSpec {
  name: string;
  method: 'GET' | 'POST';
  action?: string;
  definition?: string;
  params?: Record<string, unknown>;
  private?: boolean;
  metadata?: Record<string, unknown>;
  metadataCharacter?: string;
  /** §B: the bench oracle, sent verbatim (an explicit null is a distinct arm). */
  llm?: unknown;
  /** P4.d10 §B: the mock merged state, sent verbatim (null is a distinct arm). */
  state?: unknown;
  /**
   * P4.6bd: insert ONE cheap connection profile before the run, so a
   * `{live:true}` consult RESOLVES through the (mocked, recording) provider.
   * The committed fixture itself stays profile-free.
   */
  profile?: boolean;
}

/** The same profile the pascal handler/route oracles insert (P4.6bd). */
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

/** The canned consult answer: `YES` hits the oracle definition's `eq: 'YES'` row. */
const CONSULT_ANSWER = 'YES';
const CONSULT_USAGE = { promptTokens: 42, completionTokens: 3, totalTokens: 45 };

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

interface Corpus {
  characters: Record<string, string>;
  definitions: Record<string, unknown>;
  cases: CaseSpec[];
}

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: body === undefined ? 'GET' : 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

/** Build the POST body a case sends, resolving its definition + metadata refs. */
function bodyFor(c: CaseSpec, corpus: Corpus): Record<string, unknown> {
  const body: Record<string, unknown> = { definition: corpus.definitions[c.definition ?? ''] };
  if (c.params !== undefined) body.params = c.params;
  if (c.private !== undefined) body.private = c.private;
  if (c.metadata !== undefined) body.metadata = c.metadata;
  if (c.metadataCharacter !== undefined) {
    body.metadata = { characterId: corpus.characters[c.metadataCharacter] };
  }
  // §B: the bench oracle. `hasOwnProperty` rather than `!== undefined`, so a
  // case carrying an explicit `null` still SENDS the null — that is a distinct
  // arm (`.nullish()`) from omitting the field.
  if (Object.prototype.hasOwnProperty.call(c, 'llm')) body.llm = (c as {llm: unknown}).llm;
  // P4.d10 §B: the mock merged state rides the same hasOwnProperty rule.
  if (Object.prototype.hasOwnProperty.call(c, 'state')) body.state = (c as {state: unknown}).state;
  return body;
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  corpus: Corpus,
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
  // resolving `{live:true}` consult ever reaches it.
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

  const work = mkdtempSync(join(scratch, 'wbr-'));
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
    const base = 'http://localhost/api/v1/custom-tools';
    const url = c.action ? `${base}?action=${c.action}` : base;
    let response: { status: number; json: () => Promise<unknown> };
    if (c.method === 'GET') {
      const { GET } = await import('@/app/api/v1/custom-tools/route');
      response = (await GET(mockRequest(url) as never, {} as never)) as never;
    } else {
      const { POST } = await import('@/app/api/v1/custom-tools/route');
      response = (await POST(mockRequest(url, bodyFor(c, corpus)) as never, {} as never)) as never;
    }
    const status = response.status;
    const body = await response.json();

    // Let the fire-and-forget logLLMCall settle before dumping (the
    // oracle-fire-and-forget-tail rule: stray promises poison the NEXT case).
    await new Promise((resolve) => setTimeout(resolve, 200));

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
  const fixturesDir = join(here, '..', 'fixtures');
  const spec = JSON.parse(fs.readFileSync(join(fixturesDir, 'workbench.json'), 'utf8')) as Spec;
  const corpus = JSON.parse(
    fs.readFileSync(join(fixturesDir, 'workbench-route-cases.json'), 'utf8'),
  ) as Corpus;

  const fixtures = {
    main: process.env.QT_FIXTURE_WORKBENCH_MAIN ?? '',
    mount: process.env.QT_FIXTURE_WORKBENCH_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-workbench-route-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const out = fs.createWriteStream(outPath);
  for (const c of corpus.cases) {
    const row = await runCase(spec, c, corpus, scratch, fixtures);
    out.write(JSON.stringify(row) + '\n');
  }
  out.end();
  await new Promise((r) => out.on('finish', r));
  rmSync(scratch, { recursive: true, force: true });
}

it('emits the pascal workbench route oracle', async () => {
  await main();
}, 300000);
