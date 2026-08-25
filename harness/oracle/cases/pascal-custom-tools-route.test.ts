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
 * P4.d24 (v4 `e8a49597`): the five perspective rooms the fixture gained drive
 * `operatorCharacterIds`/`preferOperator`. The original CHAT cannot see that
 * change at all — its user-controlled participant plays CHAR_A, who is also
 * first in stored order, so the new preference and the old `sightings[0]` agree
 * on every row.
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
/** P4.d24 — the five operator-perspective rooms (see the fixture builder). */
const CHAT_LLM_LED = 'c1000000-0000-4000-8000-000000000002';
const CHAT_TWO_OWN = 'c1000000-0000-4000-8000-000000000003';
const CHAT_ALL_LLM = 'c1000000-0000-4000-8000-000000000004';
const CHAT_SOLO = 'c1000000-0000-4000-8000-000000000005';
const CHAT_REMOVED = 'c1000000-0000-4000-8000-000000000006';
const CHAR_A = 'a1000000-0000-4000-8000-00000000000a';
const CHAR_B = 'a1000000-0000-4000-8000-00000000000b';
/** P4.D35: the group tier the store dump reads back. */
const GROUP = 'a2000000-0000-4000-8000-0000000000aa';
const CHAR_C = 'a1000000-0000-4000-8000-00000000000c';

interface CaseSpec {
  name: string;
  method: 'GET' | 'POST';
  /** Defaults to the original CHAT. */
  chat?: string;
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

/**
 * P4.D35: the four state tiers + every character's fact sheet, read back
 * through v4's REAL repositories after the run. This is what makes the
 * side-effect claim a MEASUREMENT — `pascalMeta.effects` says where each write
 * was meant to go, and this says where it actually landed.
 */
async function dumpStores(chatId: string): Promise<Record<string, unknown>> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { readGeneralState } = await import('@/lib/mount-index/general-state');
  const repos = getRepositories();
  const chat = (await repos.chats.findById(chatId)) as { state?: unknown; projectId?: string } | null;
  const project = chat?.projectId
    ? ((await repos.projects.findById(chat.projectId)) as { state?: unknown } | null)
    : null;
  const group = (await repos.groups.findById(GROUP)) as { state?: unknown } | null;
  const metadata: Record<string, unknown> = {};
  for (const [label, id] of [['A', CHAR_A], ['B', CHAR_B], ['C', CHAR_C]] as const) {
    try {
      const character = (await repos.characters.findById(id)) as { metadata?: unknown } | null;
      metadata[label] = character?.metadata ?? null;
    } catch {
      // A broken vault reads as null rather than sinking the dump.
      metadata[label] = null;
    }
  }
  return {
    chat: chat?.state ?? null,
    project: project?.state ?? null,
    group: group?.state ?? null,
    general: await readGeneralState(),
    metadata,
  };
}

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
    const chatId = c.chat ?? CHAT;
    const params = { params: Promise.resolve({ id: chatId }) };
    const base = `http://localhost/api/v1/chats/${chatId}/custom-tools`;
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
      stores: c.method === 'POST' ? await dumpStores(chatId) : null,
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
    // ---------------------------------------------------------------------
    // P4.d24 (v4 `e8a49597`) — the operator-perspective rooms.
    // ---------------------------------------------------------------------
    // The bug's own shape. CHAR_A leads the cast, but the operator plays
    // CHAR_B: every shared row (coin/whispered/oracle) must carry
    // `asCharacterId` = CHAR_B and NO label, while `secure_line` — which only
    // CHAR_A's clearance passes — falls back to CHAR_A and says "Bertie".
    { name: 'list-llm-led', method: 'GET', chat: CHAT_LLM_LED },
    // The run action's `asCharacterId`-less fallback in the same room: the
    // roster resolves through CHAR_B's vault, so `pascalMeta.definitionMountId`
    // names vault B rather than vault A.
    { name: 'run-no-character-llm-led', method: 'POST', chat: CHAT_LLM_LED, body: { tool: 'ansible' } },
    // Two user-controlled characters with `activeTypingParticipantId` naming
    // the SECOND: the active one wins over stored order, and `secure_line`
    // (which CHAR_B fails) still prefers the operator's OTHER character rather
    // than labelling a fallback.
    { name: 'list-two-own', method: 'GET', chat: CHAT_TWO_OWN },
    { name: 'run-no-character-two-own', method: 'POST', chat: CHAT_TWO_OWN, body: { tool: 'ansible' } },
    // Nobody user-controlled: stored order, and every shared row labelled.
    { name: 'list-all-llm', method: 'GET', chat: CHAT_ALL_LLM },
    // One character, LLM-controlled: fall back, but UNLABELLED.
    { name: 'list-solo', method: 'GET', chat: CHAT_SOLO },
    // The operator's first character is `removed` (not a candidate), their
    // second is `silent` (still present) — the silent one is chosen.
    { name: 'list-removed-operator', method: 'GET', chat: CHAT_REMOVED },
    // ---------------------------------------------------------------------
    // P4.D35 — side effects through the MANUAL entrance.
    // ---------------------------------------------------------------------
    // Made AS a character: all five stores, including the fact sheet.
    { name: 'run-ledger-as-a', method: 'POST', body: { tool: 'ledger', asCharacterId: CHAR_A, parameters: { entry: 'brass' } } },
    // No group for CHAR_B: the group write is SKIPPED, not shadowed.
    { name: 'run-ledger-as-b', method: 'POST', body: { tool: 'ledger', asCharacterId: CHAR_B } },
    // THE asymmetry: a run nobody made writes to nobody's sheet. The state
    // effects still land (the cascade is the chat's, not a character's), but
    // `metadata.lastEntry` must be absent from every fact sheet in the dump.
    { name: 'run-ledger-no-character', method: 'POST', body: { tool: 'ledger' } },
    // `revealOdds: false` + cascade precedence (chat's difficulty wins over
    // general's).
    { name: 'run-sealed-tally', method: 'POST', body: { tool: 'sealed_tally', asCharacterId: CHAR_A } },
    // ---------------------------------------------------------------------
    // P4.60 — the wrong-type-collapse arms. `runSchema.parse` is UNCAUGHT, so
    // every refusal below is the middleware's flat 400 `{error: 'Validation
    // error'}`; the schema's own sentences live only in `details`. v5's edge
    // used to read each key with `and_then(Value::as_str)`/`as_bool`/
    // `as_object`, which turned a present-but-wrong-typed value into "the
    // caller said nothing" — these cases are what measures that.
    // ---------------------------------------------------------------------
    { name: 'run-tool-wrong-type', method: 'POST', body: { tool: 123, asCharacterId: CHAR_A } },
    { name: 'run-tool-empty', method: 'POST', body: { tool: '', asCharacterId: CHAR_A } },
    { name: 'run-tool-missing', method: 'POST', body: { asCharacterId: CHAR_A } },
    { name: 'run-parameters-wrong-type', method: 'POST', body: { tool: 'coin', parameters: 'nope', asCharacterId: CHAR_A } },
    { name: 'run-parameters-array', method: 'POST', body: { tool: 'coin', parameters: [1], asCharacterId: CHAR_A } },
    // `.nullish()` — an explicit null PASSES and reads as "no parameters".
    { name: 'run-parameters-null', method: 'POST', body: { tool: 'coin', parameters: null, asCharacterId: CHAR_A } },
    { name: 'run-private-wrong-type', method: 'POST', body: { tool: 'coin', private: 'yes', asCharacterId: CHAR_A } },
    // `.optional()` is NOT `.nullable()` — an explicit null is a ZodError here
    // where the two `nullish()` keys accept one. The absent/null/value poles
    // are what the corpus needs (js-nullish-chain-is-or-else-not-filter).
    { name: 'run-private-null', method: 'POST', body: { tool: 'coin', private: null, asCharacterId: CHAR_A } },
    { name: 'run-as-character-wrong-type', method: 'POST', body: { tool: 'coin', asCharacterId: 42 } },
    { name: 'run-as-character-null', method: 'POST', body: { tool: 'coin', asCharacterId: null } },
    // The EMPTY-STRING arm: `asCharacterId` passes the schema, then every one
    // of the handler's four reads is a truthiness gate (`body.asCharacterId ?
    // … : …`) — so '' means "nobody named". Run through `ledger` so WHERE the
    // effect lands is in the dump: this must match `run-ledger-no-character`.
    { name: 'run-ledger-as-empty-string', method: 'POST', body: { tool: 'ledger', asCharacterId: '' } },
    // z.object is non-strict: an unknown key is STRIPPED, not refused.
    { name: 'run-unknown-key', method: 'POST', body: { tool: 'coin', asCharacterId: CHAR_A, bogus: 1 } },
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
