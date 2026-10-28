/**
 * @jest-environment node
 *
 * P4.6ay unit 4 (the handler half) ORACLE: drives v4's REAL
 * `executeRunCustomTool` (lib/tools/handlers/run-custom-handler.ts) over a FRESH
 * copy of the committed `pascal-run-custom-{main,mount}.db` fixture per case,
 * emitting the returned model output + the posted MAIN-db `chat_messages` rows
 * (the Pascal outcome / Prospero error). The Rust port
 * (`tools::run_custom::execute_run_custom_tool`) is diffed against it.
 *
 * Every roll is `min === max` (no draw), so the outcome is deterministic without
 * a shared byte source. Posted rows carry a minted id + createdAt — the Rust
 * diff normalizes both positionally.
 *
 * Only the DB stack is un-mocked past jest.setup (the real cipher binding); the
 * handler's seams (roster/writers/repos) are all REAL — the whole point is the
 * end-to-end DB behavior.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   QT_FIXTURE_PASCAL_MAIN=<v5>/crates/quilltap-web/tests/fixtures/pascal-run-custom-main.db \
 *   QT_FIXTURE_PASCAL_MOUNT=<v5>/crates/quilltap-web/tests/fixtures/pascal-run-custom-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-pascal-run-custom-handler.ndjson npx jest -- pascal-run-custom-handler
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

interface Meta {
  vaultA: string;
  vaultB: string;
  vaultC: string;
  vaultD: string;
}

const CHAT = 'c1000000-0000-4000-8000-000000000001';
const CHAR_A = 'a1000000-0000-4000-8000-00000000000a';
const CHAR_B = 'a1000000-0000-4000-8000-00000000000b';
const CHAR_C = 'a1000000-0000-4000-8000-00000000000c';
/** P4.D35: the group tier the store dump reads back. */
const GROUP = 'a2000000-0000-4000-8000-0000000000aa';
/** P4.d19: the character whose vault is broken (no `properties.json`). */
const CHAR_D = 'a1000000-0000-4000-8000-00000000000d';
const P_A = 'e1000000-0000-4000-8000-00000000000a';
/** P4.D319: P4.D305's trigger room — its Trigger Shelf (the room's project
 *  tier) carries `announced_dresser`; CHAR_A sits in it as `P7_A`. */
const CHAT_TRIGGERS = 'c1000000-0000-4000-8000-000000000007';
const PROJECT_TRIGGERS = 'a3000000-0000-4000-8000-0000000000bb';
const P7_A = 'e7000000-0000-4000-8000-00000000000a';
/** P4.D329: CHAR_B's (user-driven) seat in the trigger room. */
const P7_B = 'e7000000-0000-4000-8000-00000000000b';
const TRIGGER_ROOM_A = { chatId: CHAT_TRIGGERS, projectId: PROJECT_TRIGGERS, callerParticipantId: P7_A };
const TRIGGER_ROOM_B = { chatId: CHAT_TRIGGERS, projectId: PROJECT_TRIGGERS, callerParticipantId: P7_B };

interface CaseSpec {
  name: string;
  characterId: string | null;
  vaultKey: 'vaultA' | 'vaultB' | 'vaultC' | 'vaultD' | null;
  input: unknown;
  /**
   * P4.6bd: insert ONE cheap connection profile before the run, so the consult
   * RESOLVES through the (mocked, recording) provider instead of stopping at
   * `no connection profiles are configured`. The committed fixture itself stays
   * profile-free — the no-profiles cases depend on that.
   */
  profile?: boolean;
  /**
   * P4.D169: freeze the handler's ONE clock reading for this case, to the same
   * instant the Rust side passes as `PASCAL_NOW_MS`. Required for any case that
   * gates on, renders or WRITES a progression — a derived field is a function
   * of the clock, and two implementations reading two wall clocks read two
   * sheets. Omitted on every clock-independent case, so their recorded bytes
   * stay exactly where they were.
   */
  nowMs?: number;
  /**
   * P4.D305: SQL planted on the case's MAIN copy BEFORE the database opens
   * (the same statements the Rust side runs on its copy). Each plant must fail
   * BOTH sides' reads (memory note `throwing-plant-must-fail-both-reads`); a
   * planted case dumps no stores (the dump reads the chat).
   */
  plant?: string[];
  /**
   * P4.D319: run in ANOTHER room — the chat, its project and the caller's seat
   * there. Recorded on the row only when set, so every earlier row keeps its
   * bytes; the Rust side builds its context from the recorded values.
   */
  room?: { chatId: string; projectId: string | null; callerParticipantId: string };
  /**
   * P4.D333: a `.tool.json` written into the case's vault ON THE PER-RUN COPY
   * (never baked into the committed pair), through v4's REAL `storeMountFile`
   * after the database opens — the Rust side writes the same bytes through
   * its own `store_mount_file`. The text travels verbatim, so both sides store
   * identical bytes.
   */
  plantTool?: { relativePath: string; text: string };
}

/**
 * P4.D333 (v4 `322a5488e`, bug 257): a consult-bearing tool whose table does
 * NOT test the consult — so the table is dealt FIRST, the winner's brief is
 * rendered, and the prompt is told the verdict. `roll` is degenerate, so the
 * failure row wins without a draw.
 */
const NARRATOR_TOOL_TEXT = JSON.stringify({
  name: 'narrator',
  description: 'Narrate a scan.',
  parameters: { layers: { type: 'number', default: 95 } },
  roll: { min: 1, max: 1 },
  llm: {
    prompt: 'Write a report. State: {{outcome.state}}, row {{outcome.index}}. Conclusion to reach: {{outcome.brief}}',
    errorMessage: 'The link falters.',
  },
  outcomes: [
    { when: { roll: { gte: { $param: 'layers' } } }, message: '{{llm}}', state: 'success', brief: 'Confirmed at {{params.layers}} layers.' },
    { when: true, message: '{{llm}} [{{outcome.state}}]', state: 'failure', brief: 'Not found.' },
  ],
});

/**
 * P4.D305: the log lines this family pins — every line v4 emits at the pin on
 * the run-custom path that v5 ports. Anything else v4 logs on the way (the
 * Pascal / Prospero writers' own lines, the roster's) is not compared here.
 */
/**
 * P4.D329: the inventory lines a mirror row pins, kept OUT of `logs` (so every
 * earlier row keeps its bytes) and emitted as `inventoryLines` only when a case
 * logged one: the roster's three bag DEBUGs, the run service's WARN — whole —
 * and P4.D328's `[InventoryContext] Loaded` by TAG only (its fields are that
 * lane's).
 */
function inventoryLine(message: string): 'whole' | 'tag' | null {
  if (message.startsWith('Custom-tool gates:') && message.includes('inventory')) return 'whole';
  if (message === "Custom tool run: the rolling character's inventory could not be read; inventory tests decline") return 'whole';
  if (message === '[InventoryContext] Loaded') return 'tag';
  return null;
}

const PINNED_MESSAGES = new Set([
  'Custom tool run: state cascade unavailable, defaulting to {}',
  // P4.169 (F-4): the handler's two WARNs.
  'run_custom validation failed',
  'run_custom named an unknown tool',
  'Custom tool run refused',
  'Custom tool dealt',
  'Custom tool run failed',
  'Custom tool run completed',
  // P4.D333 (v4 `322a5488e`): the table-first order announces itself.
  'Custom tool table dealt ahead of the consult',
]);

/** chats.title rebuilt as a generated column that THROWS on read — v4's
 *  `SELECT *` and v5's named-column read both evaluate it (memory note
 *  `throwing-plant-must-fail-both-reads`), so `chats.findById` fails on both. */
const PLANT_CHAT_UNREADABLE = [
  'ALTER TABLE "chats" DROP COLUMN "title"',
  'ALTER TABLE "chats" ADD COLUMN "title" TEXT AS (json("id")) VIRTUAL',
];
/** Every chat_messages INSERT aborts, so the Pascal post (and Prospero's)
 *  fails after the deal while the chat itself stays readable. */
const PLANT_POST_FAILS = [
  'CREATE TRIGGER "p4d305_no_posts" BEFORE INSERT ON "chat_messages" BEGIN SELECT RAISE(ABORT, \'planted: no posts\'); END',
];

/** See `CaseSpec.nowMs`. 2026-08-29T12:00:00Z, the fixture's spans' anchor. */
const PASCAL_NOW_MS = 1_788_004_800_000;

/**
 * The one profile a `profile: true` case inserts (both sides insert the SAME
 * row through their real repos — the Rust differential mirrors these fields).
 */
const CONSULT_PROFILE = {
  userId: '', // filled from the spec at insert time
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

async function runCase(
  spec: Spec,
  meta: Meta,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();

  // P4.6bd: recorded canned-completion entries, deduped by the exact call key.
  // The key format MUST match `quilltap_core::model::completion::canned_completion_key`.
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
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories'),
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  // P4.6bd: the consult's provider seam — a recording canned provider (the
  // memory-processor recipe). Only a resolving consult ever reaches it.
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
  // The v5 side has no API-key step in the cheap path — pin v4's to a constant.
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

  const work = mkdtempSync(join(scratch, 'pascal-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  if (c.plant) {
    // Before the database opens: no cross-case singleton can hold the copy.
    const Cipher = jest.requireActual(cipherDriverPath);
    const conn = new Cipher(mainWork);
    conn.pragma(`key = "x'${Buffer.from(spec.testPepperBase64, 'base64').toString('hex')}'"`);
    for (const sql of c.plant) conn.exec(sql);
    conn.close();
  }

  // P4.D305: v4's log lines, through the singleton logger's shared transports
  // (memory note `v4-oracle-job-host-and-log-capture` §2) — `LOG_LEVEL` set
  // before the (reset) logger module loads; `service` / `environment` dropped.
  process.env.LOG_LEVEL = 'debug';
  const { logger } = await import('@/lib/logger');
  const logs: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  const inventoryLines: Array<{ level: string; message: string; fields?: Record<string, unknown> }> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown>; error?: { message: string } };
      const inv = inventoryLine(data.message);
      if (inv) {
        if (inv === 'tag') {
          inventoryLines.push({ level: data.level, message: data.message });
        } else {
          const { service: _s, environment: _e, ...fields } = (data.context ?? {}) as Record<string, unknown>;
          if (data.error) fields.error = data.error.message;
          inventoryLines.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
        }
        return;
      }
      if (!PINNED_MESSAGES.has(data.message)) return;
      const { service: _s, environment: _e, ...fields } = (data.context ?? {}) as Record<string, unknown>;
      if (data.error) fields.error = data.error.message;
      logs.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
    },
  });
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
  const { executeRunCustomTool } = await import('@/lib/tools/handlers/run-custom-handler');

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

  if (c.plantTool) {
    const { storeMountFile } = await import('@/lib/mount-index/store-file');
    if (!c.vaultKey) throw new Error(`case '${c.name}': a tool plant needs a vault`);
    await storeMountFile({
      mountPointId: meta[c.vaultKey],
      relativePath: c.plantTool.relativePath,
      data: Buffer.from(c.plantTool.text, 'utf8'),
      originalMimeType: 'application/json',
      transcodeImages: false,
      extractText: false,
      enqueueEmbedding: false,
    } as never);
  }

  try {
    const context = {
      userId: spec.userId,
      chatId: c.room?.chatId ?? CHAT,
      characterId: c.characterId,
      characterMountPointId: c.vaultKey ? meta[c.vaultKey] : null,
      projectId: c.room ? c.room.projectId : null,
      callerParticipantId: c.room?.callerParticipantId ?? P_A,
    };
    const realNow = Date.now;
    if (c.nowMs !== undefined) Date.now = () => c.nowMs as number;
    let output: unknown;
    try {
      output = await executeRunCustomTool(c.input, context as never);
    } finally {
      Date.now = realNow;
    }

    // Let the fire-and-forget logLLMCall settle before dumping (the
    // oracle-fire-and-forget-tail rule: stray promises poison the NEXT case).
    await new Promise((resolve) => setTimeout(resolve, 200));

    const mdb = getRawDatabase();
    if (!mdb) throw new Error('main DB handle unavailable');
    const columns = (
      mdb.prepare(`PRAGMA table_info(chat_messages)`).all() as Array<{ name: string }>
    ).map((col) => col.name);
    const rawRows = mdb.prepare(`SELECT * FROM chat_messages ORDER BY createdAt`).all() as Array<
      Record<string, unknown>
    >;
    const rows = rawRows.map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
      return out;
    });

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
      // P4.D169: the instant this case ran at, recorded so the Rust side takes
      // its clock FROM the row rather than from a constant the two sides would
      // have to keep in step by hand. Absent on a case that pins none.
      ...(c.nowMs === undefined ? {} : { nowMs: c.nowMs }),
      ...(c.room ? { room: c.room } : {}),
      output,
      columns,
      rows,
      logs,
      ...(inventoryLines.length > 0 ? { inventoryLines } : {}),
      stores: c.plant ? null : await dumpStores(c.room?.chatId ?? CHAT),
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
  const meta = JSON.parse(fs.readFileSync(fixtures.main + '.meta.json', 'utf8')) as Meta;
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-pascal-handler-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cases: CaseSpec[] = [
    { name: 'gated-hit', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'ansible' } },
    { name: 'gated-miss', characterId: CHAR_B, vaultKey: 'vaultB', input: { tool: 'ansible' } },
    { name: 'gated-empty', characterId: CHAR_C, vaultKey: 'vaultC', input: { tool: 'ansible' } },
    { name: 'public-coin', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'coin' } },
    { name: 'whispered-default', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'whispered' } },
    { name: 'private-override', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'coin', private: true } },
    { name: 'public-override', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'whispered', private: false } },
    { name: 'unknown-tool', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'nonexistent' } },
    { name: 'validation-fail', characterId: CHAR_A, vaultKey: 'vaultA', input: { notTool: 1 } },
    // P4.169 (F-4): the validation WARN's `input` over a string and a non-string `tool`.
    { name: 'validation-fail-string-input', characterId: CHAR_A, vaultKey: 'vaultA', input: 'coin' },
    { name: 'validation-fail-tool-number', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 5 } },
    { name: 'run-error', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'coin', parameters: { bad: 1 } } },
    // The 616930db consult, end to end. The fixture carries no connection
    // profiles, so BOTH sides take the invoker's `no connection profiles are
    // configured` arm: the run does NOT fail, `pascalMeta.llm` records the
    // rendered prompt + the technical reason + the author's errorMessage as the
    // output, and the table branches to the `ok: false` row. Deterministic
    // without mocking a provider.
    { name: 'llm-consult-no-profiles', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'oracle' } },
    { name: 'llm-consult-private', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'oracle', private: true } },
    // P4.6bd: the consult RESOLVES — a profile is inserted, the (recording)
    // provider answers 'YES', the `eq: 'YES'` outcome row fires, and the
    // CUSTOM_TOOL_CONSULT llm-log row lands. The tier-1 proof of the wire.
    { name: 'llm-consult-resolved', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'oracle' }, profile: true },
    // P4.d10 `$state`: the entrance resolves the merged cascade scoped to the
    // ROLLING character's own groups. CHAR_A (in Table Stakes): roll = chat
    // difficulty 6 (over general's 9), 6 >= gscore 4 → success, with
    // {{state.*}} rendered from all three tiers. CHAR_B (no group): gscore
    // falls back to 99 → failure — the scoping made visible in the outcome.
    { name: 'state-cascade-hit', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'stateful' } },
    { name: 'state-no-group-falls-back', characterId: CHAR_B, vaultKey: 'vaultB', input: { tool: 'stateful' } },

    // ---- P4.d19: the availability gate, end-to-end through the handler ----
    // The two gated definitions live in the GENERAL store, so the only thing
    // that differs between these rows is the invoker's own fact sheet. A
    // withheld tool is not merely un-run: it is ABSENT from the roster, so the
    // handler answers with the same "No custom tool named …" it gives an
    // invented name — which is exactly what the gate is for.
    { name: 'gate-available-runs', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'secure_line' } },
    { name: 'gate-available-missing-key-unrunnable', characterId: CHAR_B, vaultKey: 'vaultB', input: { tool: 'secure_line' } },
    { name: 'gate-available-empty-sheet-unrunnable', characterId: CHAR_C, vaultKey: 'vaultC', input: { tool: 'secure_line' } },
    { name: 'gate-withheld-when-holds-unrunnable', characterId: CHAR_B, vaultKey: 'vaultB', input: { tool: 'novice_aid' } },
    // The same empty sheet that fails `availableWhen` above satisfies no
    // `withheldWhen`, so CHAR_C IS dealt this one. The asymmetry, at the door.
    { name: 'gate-withheld-when-empty-sheet-runs', characterId: CHAR_C, vaultKey: 'vaultC', input: { tool: 'novice_aid' } },

    // ---- P4.d19: the fact-sheet read MOVED ahead of the roster -----------
    // A broken vault now fails before a definition is in hand, so the whisper
    // decision can no longer consult `defaultVisibility`. `private: true` is the
    // only thing that whispers one of these.
    { name: 'vault-broken-private-true', characterId: CHAR_D, vaultKey: 'vaultD', input: { tool: 'coin', private: true } },
    { name: 'vault-broken-private-false', characterId: CHAR_D, vaultKey: 'vaultD', input: { tool: 'whispered', private: false } },
    // THE arm whose behavior CHANGED: `whispered` declares
    // defaultVisibility:'whisper', and before 6864bf0e that made this failure a
    // whisper. It is public now.
    { name: 'vault-broken-private-absent-whisper-default', characterId: CHAR_D, vaultKey: 'vaultD', input: { tool: 'whispered' } },
    // A name no roster carries, through a broken vault: the vault failure wins,
    // because it is answered first.
    { name: 'vault-broken-unknown-tool', characterId: CHAR_D, vaultKey: 'vaultD', input: { tool: 'nonexistent' } },

    // ---- P4.D35: side effects, end-to-end through the LLM entrance -------
    // CHAR_A touches all five stores in one run: chat (banner + the
    // nowhere-default fresh_key + the outcome-conditioned win), project
    // (proj_tier + the nested encounter.count), group (gscore), general
    // (gen_tier), and the fact sheet (lastEntry, folded into a whole-object
    // RMW that must keep hasAnsibleAccess and clearanceLevel).
    { name: 'effects-all-tiers', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'ledger', parameters: { entry: 'brass' } } },
    // CHAR_B is in no group, so `gscore` is not in their cascade at all: the
    // expression reading it fails to evaluate and that effect is SKIPPED —
    // NOT shadowed at the chat tier. The store dump is what proves the
    // difference.
    { name: 'effects-no-group-skips-the-group-write', characterId: CHAR_B, vaultKey: 'vaultB', input: { tool: 'ledger' } },
    // `difficulty` lives at BOTH the chat and general tiers: chat is searched
    // first and wins, and the expression reads the merged view (6, not 9).
    // Also the `revealOdds: false` arm — the write happens, the odds do not.
    { name: 'effects-precedence-under-sealed-odds', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'sealed_tally' } },

    // ---- P4.D169: progressions, end-to-end through the LLM entrance ------
    //
    // Every row here pins the clock, because a derived field is a function of
    // it. CHAR_A's sheet carries two spans anchored to that same instant: a
    // `cannon` that finished ten minutes ago and a `gestation` with two months
    // to run.
    //
    // The gate, at the door. `recharged` is dealt because its span finished;
    // `gestating` is NOT in the roster at all, so the handler answers the same
    // "No custom tool named …" it gives an invented name — which is the whole
    // point of answering a gate before the deal.
    { name: 'progress-gate-finished-span-runs', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'recharged' }, nowMs: PASCAL_NOW_MS },
    { name: 'progress-gate-running-span-unrunnable', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'gestating' }, nowMs: PASCAL_NOW_MS },
    // The READ and the WRITE in one run. `recharge` chooses its outcome on
    // `when.progress`, renders `{{progress.cannon.state}}`,
    // `{{progress.cannon.percent}}` and `{{now}}` into the message, and writes
    // three `progress.cannon.*` fields — which must land in ONE character write
    // that keeps `hasAnsibleAccess`, `clearanceLevel` and the untouched
    // `gestation` beside them. The dumped vault is what proves that last part.
    // (`pascalMeta.effects` here is the `{target, previous, next}` projection;
    // the `applied` list's store label is the applier family's comparand, not
    // this one's.)
    { name: 'progress-effect-writes-the-span', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'recharge' }, nowMs: PASCAL_NOW_MS },
    // Create-on-write: `kettle` is a progression nobody authored, so the
    // applier mints it with the format's defaults (a one-hour span from the run
    // clock, `timeIncrement` inferred, `percentageReport`, `reportFrequency`,
    // `onComplete`) before the two authored fields land on top.
    { name: 'progress-effect-creates-a-progression', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'kindle' }, nowMs: PASCAL_NOW_MS },
    // The same writer an hour later. Every stamped field moves with the clock,
    // and the cannon's `complete` is still true — so this row differs from the
    // one above in exactly the values the run clock touches, which is what says
    // the write takes the caller's instant rather than one of its own.
    { name: 'progress-effect-writes-under-a-different-clock', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'recharge' }, nowMs: PASCAL_NOW_MS + 3_600_000 },

    // ---- P4.D305 (v4 `362426c8f`): the run service's fail-soft arms --------
    // The chat cannot be read. v4's `chats.findById` is a `safeQuery` fallback
    // (a failed read answers `null`), so the service's cascade simply has no
    // chat — SILENTLY: its `state cascade unavailable` DEBUG is NOT reached
    // this way (pinned NEGATIVE; the catch is reachable only through a throw
    // inside `resolveStateCascade` itself). The deal lands against `{}`, the
    // Pascal post's own chat check answers null, and the model hears the
    // unposted failure.
    { name: 'run-llm-chat-unreadable', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'coin' }, plant: PLANT_CHAT_UNREADABLE },
    // The chat reads fine but no message can be inserted: the cascade holds,
    // the deal lands, the post fails — the handler's unposted arm.
    { name: 'run-llm-unposted', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'coin' }, plant: PLANT_POST_FAILS },

    // ---- P4.D319 (v4 `a05c6f755`): `{{character}}` + `announceAsCharacter` --
    // `announced_dresser` says `{{character}} rolls {{value}}` and announces as
    // the rolling character. The handler passes NO name, so the run service
    // reads the RAW row: Bertie, and the row carries `customAnnouncer` LAST.
    // The padded RAW name is a per-case plant (never baked): both trims.
    { name: 'announced-dresser-raw-name', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'announced_dresser' }, room: { chatId: CHAT_TRIGGERS, projectId: PROJECT_TRIGGERS, callerParticipantId: P7_A } },
    { name: 'announced-dresser-padded-raw-name', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'announced_dresser' }, room: { chatId: CHAT_TRIGGERS, projectId: PROJECT_TRIGGERS, callerParticipantId: P7_A }, plant: [`UPDATE "characters" SET "name" = '  Bertie  ' WHERE "id" = '${CHAR_A}'`] },
    // A tool WITHOUT the key, in the same room, by the same character: Pascal's.
    { name: 'announced-dresser-regression-spell', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'spell' }, room: { chatId: CHAT_TRIGGERS, projectId: PROJECT_TRIGGERS, callerParticipantId: P7_A } },
    // === P4.D329: v4 `cad017599`'s seven `it`s, MIRRORED over the widened
    // pair (`pascal-run-custom-inventory-pascal.ts`, R-9) — the trigger room
    // carries CHAR_A's titled tape measure, both resonators and a walkie-talkie,
    // and CHAR_B's untitled tape measure. ===
    { name: 'measure-twice-carrier-runs', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'measure_twice' }, room: TRIGGER_ROOM_A },
    { name: 'measure-twice-other-instance-withheld', characterId: CHAR_B, vaultKey: 'vaultB', input: { tool: 'measure_twice' }, room: TRIGGER_ROOM_B },
    { name: 'measure-twice-nobody-withheld', characterId: null, vaultKey: null, input: { tool: 'measure_twice' }, room: TRIGGER_ROOM_A },
    { name: 'measure-twice-probe-kind-name', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'probe_resonator' }, room: TRIGGER_ROOM_A },
    { name: 'measure-twice-probe-title-twin', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'probe_resonator_title' }, room: TRIGGER_ROOM_A },
    { name: 'measure-twice-probe-walkie', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'probe_walkie' }, room: TRIGGER_ROOM_A },
    { name: 'measure-twice-probe-spyglass-withheld', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'probe_spyglass' }, room: TRIGGER_ROOM_A },
    // The run service's bag read (R-15): one door each; a tool naming none
    // reads nothing; nobody rolled reads nothing.
    { name: 'inventory-read-message-door', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'reads_in_message' }, room: TRIGGER_ROOM_A },
    { name: 'inventory-read-default-door', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'reads_in_default' }, room: TRIGGER_ROOM_A },
    { name: 'inventory-read-prompt-door', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'reads_in_prompt' }, room: TRIGGER_ROOM_A },
    { name: 'inventory-read-none-named', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'spell' }, room: TRIGGER_ROOM_A },
    { name: 'inventory-read-nobody-rolled', characterId: null, vaultKey: null, input: { tool: 'reads_in_message' }, room: TRIGGER_ROOM_A },
    // === end P4.D329 ===

    // ---- P4.D333 (v4 `322a5488e`, bug 257): the table dealt FIRST ----------
    // The consult resolves (a profile, the recording provider): the posted
    // row's `pascalMeta.llm.prompt` and the CUSTOM_TOOL_CONSULT llm-log row
    // carry the TOLD verdict (`State: failure, row 1. … Not found.`), the
    // message quotes `{{outcome.state}}`, and the run logs `Custom tool table
    // dealt ahead of the consult` before the consult.
    { name: 'llm-consult-table-first', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'narrator' }, profile: true, plantTool: { relativePath: 'Tools/narrator.tool.json', text: NARRATOR_TOOL_TEXT } },
    // No profile: the table still dealt first (the line fires), the consult
    // fails soft, and the failure row's message renders the author's error.
    { name: 'llm-consult-table-first-no-profiles', characterId: CHAR_A, vaultKey: 'vaultA', input: { tool: 'narrator' }, plantTool: { relativePath: 'Tools/narrator.tool.json', text: NARRATOR_TOOL_TEXT } },
  ];

  const out = fs.createWriteStream(outPath);
  for (const c of cases) {
    const row = await runCase(spec, meta, c, scratch, fixtures);
    out.write(JSON.stringify(row) + '\n');
  }
  out.end();
  await new Promise((r) => out.on('finish', r));
  rmSync(scratch, { recursive: true, force: true });
}

it('emits the pascal run_custom handler oracle', async () => {
  await main();
}, 120000);
