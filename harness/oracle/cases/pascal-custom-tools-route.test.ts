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
 * `whispered` live only in char A's vault — since v4 `cfa0cba29` a vault's tools
 * are that character's ALONE (there is no participant tier), so B / C are dealt
 * neither and the rows carry A alone. All rolls are `min === max` (deterministic).
 *
 * P4.d24 (v4 `e8a49597`): the five perspective rooms the fixture gained drive
 * `operatorCharacterIds`/`preferOperator`. The original CHAT cannot see that
 * change at all — its user-controlled participant plays CHAR_A, who is also
 * first in stored order, so the new preference and the old `sightings[0]` agree
 * on every row.
 *
 * P4.D379 (v4 `fd165fce0` at the `624d0d25c` pin): the `presets-*` GET rows drive
 * `?action=presets` over IN-CASE plants (ruling P-1 — the committed pair is
 * shared by four families, so it is never rebuilt for this). Each row RECORDS
 * its plants, and the Rust side replays them byte-for-byte:
 *   - `sql` — a raw statement on a keyed connection to the case's copy, BEFORE
 *     `initializeDatabase()` (memory note `jest-oracle-in-case-plants`);
 *   - `diskStore` — a filesystem store: a scratch directory `<BASE>` holding
 *     `Tools/<files>`, its `doc_mount_points` row and its owner link, planted
 *     raw before init; `toolsMode: 0` chmods `Tools/` to 000 (restored in a
 *     `finally`). `<BASE>` is masked back into every body and log line;
 *   - `doc` — a database-store document through v4's REAL
 *     `writeDatabaseDocument`, after init.
 * Every plant SELF-CHECKS (memory note `tier-r-vacuous-equal-rows`).
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
// [P4.D362] the two 4.12 columns, through v4's REAL migrations.
import { plant412Columns } from '../lib/v4-migrations';

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
/** P4.D305's trigger room — its Trigger Shelf carries P4.D319's `announced_dresser`. */
const CHAT_TRIGGERS = 'c1000000-0000-4000-8000-000000000007';
const CHAR_A = 'a1000000-0000-4000-8000-00000000000a';
const CHAR_B = 'a1000000-0000-4000-8000-00000000000b';
/** P4.D35: the group tier the store dump reads back. */
const GROUP = 'a2000000-0000-4000-8000-0000000000aa';
const CHAR_C = 'a1000000-0000-4000-8000-00000000000c';
// === P4.D379 ===
/** Spode — the fixture's BROKEN vault (`properties.json` link deleted). */
const CHAR_D = 'a1000000-0000-4000-8000-00000000000d';
/** A character id that is in no chat. */
const STRANGER = 'a1000000-0000-4000-8000-0000000000ff';
const UNKNOWN_CHAT = 'c1000000-0000-4000-8000-0000000000ff';
/** The fixture's stores, MEASURED from the committed pair (P4.D379 lane record). */
const VAULT_A = 'f22f5571-e962-41c3-94e8-051185b33016';
const VAULT_B = '55db253f-ca9e-4426-ad33-94b3ee5299a0';
const GROUP_STORE = 'f6b09c65-bd2b-4f7f-8d80-bf984e303c5c';
const PROJECT_STORE = '35b077a1-b95f-405b-adb0-cdc881444d93';
const GENERAL_STORE = '93000000-0000-4000-8000-0000000000aa';
const PROJECT = 'a3000000-0000-4000-8000-0000000000aa';
const DISK_STORE = '9d000000-0000-4000-8000-0000000000d1';

type Plant =
  | { kind: 'sql'; db: 'main' | 'mount'; sql: string; params: unknown[] }
  | {
      kind: 'diskStore';
      mountPointId: string;
      name: string;
      /** `project_doc_mount_links` owner. */
      projectId: string;
      files: string[];
      /** `0` → `chmod 000 Tools/`. */
      toolsMode?: number;
    }
  | { kind: 'doc'; mountPointId: string; relativePath: string; content: string };

const presetDoc = (mountPointId: string, relativePath: string): Plant => ({
  kind: 'doc',
  mountPointId,
  relativePath,
  content: JSON.stringify({ preset: relativePath }, null, 2),
});
const participantsSql = (chatId: string, participants: Array<[string, string, string]>): Plant => ({
  kind: 'sql',
  db: 'main',
  sql: 'UPDATE "chats" SET "participants" = ? WHERE "id" = ?',
  params: [
    JSON.stringify(
      participants.map(([id, characterId, controlledBy]) => ({
        id,
        type: 'CHARACTER',
        characterId,
        controlledBy,
        connectionProfileId: null,
        displayOrder: 0,
        isActive: true,
        status: 'active',
        hasHistoryAccess: false,
        createdAt: '2026-04-01T00:00:00.000Z',
        updatedAt: '2026-04-01T00:00:00.000Z',
      })),
    ),
    chatId,
  ],
});
// === end P4.D379 ===

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
  /**
   * P4.D169: freeze the manual run's ONE clock reading for this case, to the
   * same instant the Rust side takes from the recorded row. Required for any
   * case that gates on, renders or writes a progression — a derived field is a
   * function of the clock. Omitted, the case runs at `PASCAL_NOW_MS` — the same default the Rust
   * side takes (no case reads the wall clock).
   */
  nowMs?: number;
  // === P4.D379 ===
  /** Appended to the GET URL (`?action=presets&…`). */
  query?: string;
  /** In-case plants, recorded in the row (see the header). */
  plants?: Plant[];
  // === end P4.D379 ===
}

/**
 * P4.D305: the log lines this family pins — the run path's lines v5 ports (the
 * run service's and the route's own). The GET's roster lines are not compared.
 */
const PINNED_MESSAGES = new Set([
  'Custom tool run: state cascade unavailable, defaulting to {}',
  'Custom tool run refused',
  'Custom tool dealt',
  'Manual custom-tool run named an unknown tool',
  'Manual custom-tool run refused',
  '[Pascal] Manual custom-tool run completed',
  // === P4.D379 ===
  'Preset listing failed for store',
  'Tool presets resolved',
  'Custom-tool presets listed for the operator',
  'Custom-tool perspective dropped — character unreadable',
  // === end P4.D379 ===
]);

/** See `CaseSpec.nowMs`. 2026-08-29T12:00:00Z, the fixture's spans' anchor. */
const PASCAL_NOW_MS = 1_788_004_800_000;

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
  // === P4.D379 === At the `624d0d25c` pin the profile repository's strip
  // (`544c7ab1e`) calls `acceptsSamplingParameters`, which v4's own
  // `jest.setup.ts:228` mock of provider-validation does not carry, so the
  // `profile: true` case's create threw and killed the whole run (P4.D362's
  // recorded finding). The setup's three mocks plus the REAL function; the
  // consult profile's `parameters` is `{}`, so nothing is stripped either way.
  jest.doMock('@/lib/plugins/provider-validation', () => ({
    __esModule: true,
    validateProviderConfig: jest.fn().mockReturnValue({ valid: true, errors: [] }),
    acceptsApiKey: jest.fn(() => true),
    requiresApiKey: jest.fn(() => true),
    acceptsSamplingParameters: jest.requireActual('@/lib/plugins/provider-validation').acceptsSamplingParameters,
  }));
  // === end P4.D379 ===
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
  // === P4.D362 ===
  // v4 `544c7ab1e`'s repository strip calls `acceptsSamplingParameters`, which
  // jest.setup's provider-validation mock (three functions) lacks — at the pin
  // every profile create threw `acceptsSamplingParameters is not a function`.
  // Keep the setup's three mocks; add the REAL function (the registry is empty
  // here, so it answers `true` — this corpus creates no Anthropic profile with
  // a sampling key).
  jest.doMock('@/lib/plugins/provider-validation', () => ({
    validateProviderConfig: jest.fn().mockReturnValue({ valid: true, errors: [] }),
    acceptsApiKey: jest.fn(() => true),
    requiresApiKey: jest.fn(() => true),
    acceptsSamplingParameters: jest.requireActual('@/lib/plugins/provider-validation')
      .acceptsSamplingParameters,
  }));
  // === end P4.D362 ===
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

  // === P4.D379: the pre-init plants (raw, on a keyed connection) ===
  const diskBase = join(work, 'disk-store');
  const lockedDirs: string[] = [];
  const pre = (c.plants ?? []).filter((pl) => pl.kind !== 'doc');
  // At the pin the profile repository writes `maxImagesPerRequest`, a column
  // v4 adds at boot (`add-profile-max-images-per-request-v1`, `INTEGER DEFAULT
  // 4`) and the committed pair predates. jest's `initializeDatabase` runs no
  // migrations, so the `profile: true` case's create failed and killed the run.
  // v4's own DDL, on that case's copy only (the column is P4.D362's; v5's side
  // of the row is the union's).
  if (c.profile) {
    pre.unshift({
      kind: 'sql',
      db: 'main',
      sql: 'ALTER TABLE "connection_profiles" ADD COLUMN "maxImagesPerRequest" INTEGER DEFAULT 4',
      params: [],
    });
  }
  if (pre.length > 0) {
    const Driver = jest.requireActual(cipherDriverPath);
    const keyHex = Buffer.from(spec.testPepperBase64, 'base64').toString('hex');
    const openKeyed = (file: string) => {
      const conn = new Driver(file);
      conn.pragma(`key = "x'${keyHex}'"`);
      return conn;
    };
    const conns = { main: openKeyed(mainWork), mount: openKeyed(mountWork) };
    try {
      for (const pl of pre) {
        if (pl.kind === 'sql') {
          const changes = conns[pl.db].prepare(pl.sql).run(...pl.params).changes;
          if (!pl.sql.startsWith('ALTER') && changes !== 1) throw new Error(`plant changed ${changes} rows: ${pl.sql}`);
        } else if (pl.kind === 'diskStore') {
          mkdirSync(join(diskBase, 'Tools'), { recursive: true });
          for (const f of pl.files) fs.writeFileSync(join(diskBase, f), JSON.stringify({ preset: f }, null, 2));
          conns.mount
            .prepare(
              'INSERT INTO "doc_mount_points" ("id","name","basePath","mountType","storeType","includePatterns","enabled","createdAt","updatedAt") VALUES (?,?,?,?,?,?,?,?,?)',
            )
            .run(pl.mountPointId, pl.name, diskBase, 'filesystem', 'documents', '[]', 1, '2026-10-01T00:00:00.000Z', '2026-10-01T00:00:00.000Z');
          conns.mount
            .prepare('INSERT INTO "project_doc_mount_links" ("id","projectId","mountPointId","createdAt","updatedAt") VALUES (?,?,?,?,?)')
            .run(`${pl.mountPointId.slice(0, -2)}11`, pl.projectId, pl.mountPointId, '2026-10-01T00:00:00.000Z', '2026-10-01T00:00:00.000Z');
          const back = conns.mount.prepare('SELECT "basePath" FROM "doc_mount_points" WHERE "id" = ?').get(pl.mountPointId) as { basePath: string } | undefined;
          if (back?.basePath !== diskBase) throw new Error('disk store plant did not read back');
          if (pl.toolsMode !== undefined) {
            fs.chmodSync(join(diskBase, 'Tools'), pl.toolsMode);
            lockedDirs.push(join(diskBase, 'Tools'));
          }
        }
      }
    } finally {
      conns.main.close();
      conns.mount.close();
    }
  }
  // === end P4.D379 ===

  // P4.D305: v4's log lines, through the singleton logger's shared transports
  // (memory note `v4-oracle-job-host-and-log-capture` §2).
  process.env.LOG_LEVEL = 'debug';
  const { logger } = await import('@/lib/logger');
  const logs: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown>; error?: { message: string } };
      if (!PINNED_MESSAGES.has(data.message)) return;
      const { service: _s, environment: _e, ...fields } = (data.context ?? {}) as Record<string, unknown>;
      if (data.error) fields.error = data.error.message;
      logs.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
    },
  });

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient, getRawLLMLogsDatabase } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite');

  await initializeDatabase();

  // === P4.D379: database-store documents through v4's REAL writer ===
  {
    const docs = (c.plants ?? []).filter((pl) => pl.kind === 'doc') as Array<Extract<Plant, { kind: 'doc' }>>;
    if (docs.length > 0) {
      const { writeDatabaseDocument, listDatabaseFiles } = await import('@/lib/mount-index/database-store');
      for (const d of docs) {
        await writeDatabaseDocument(d.mountPointId, d.relativePath, d.content);
        const listed = await listDatabaseFiles(d.mountPointId, { folder: 'Tools' });
        if (!listed.some((e: { relativePath: string }) => e.relativePath === d.relativePath)) {
          throw new Error(`doc plant did not list back: ${d.mountPointId}/${d.relativePath}`);
        }
      }
    }
  }
  // === end P4.D379 ===

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
    // P4.D169: see `CaseSpec.nowMs`.
    const realNow = Date.now;
    // Every case runs at a frozen instant — `c.nowMs`, else the shared
    // `PASCAL_NOW_MS` the Rust side defaults to. A "clock-independent" case
    // left on the wall clock was not: the roster's withheld set depends on
    // the fixture's `gestation` span, which ENDS at PASCAL_NOW_MS + 60 days
    // (2026-10-28T12:00Z), so v4 began listing `gestating` as available that
    // day (a calendar time bomb caught at the `322a5488e` follow-ups
    // unification).
    const frozenNow = c.nowMs ?? PASCAL_NOW_MS;
    Date.now = () => frozenNow;
    try {
      if (c.method === 'GET') {
        const { GET } = await import('@/app/api/v1/chats/[id]/custom-tools/route');
        response = (await GET(mockRequest(`${base}${c.query ?? ''}`) as never, params as never)) as never;
      } else {
        const { POST } = await import('@/app/api/v1/chats/[id]/custom-tools/route');
        response = (await POST(
          mockRequest(`${base}?action=run`, c.body) as never,
          params as never,
        )) as never;
      }
    } finally {
      Date.now = realNow;
    }
    const status = response.status;
    // === P4.D379: the disk store's scratch path differs per side — mask it ===
    const mask = <T,>(v: T): T => JSON.parse(JSON.stringify(v).split(diskBase).join('<BASE>'));
    const body = mask(await response.json());
    for (let i = 0; i < logs.length; i += 1) logs[i] = mask(logs[i]);
    // === end P4.D379 ===

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
      // === P4.D379 ===
      ...(c.query === undefined ? {} : { query: c.query }),
      ...(c.plants === undefined ? {} : { plants: c.plants }),
      // === end P4.D379 ===
      // P4.D169: recorded so the Rust side takes its clock FROM the row rather
      // than from a constant the two sides would have to keep in step by hand.
      ...(c.nowMs === undefined ? {} : { nowMs: c.nowMs }),
      status,
      body,
      systemRows,
      logs,
      stores: c.method === 'POST' ? await dumpStores(chatId) : null,
      canned: Array.from(cannedRecorded.values()),
      llmLogs,
    };
  } finally {
    // === P4.D379 === restore a chmod 000 before the scratch is removed.
    for (const dir of lockedDirs) fs.chmodSync(dir, 0o755);
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
  // [P4.D362] The committed `pascal-run-custom-*` pair predates the 4.12
  // columns, and v4's profile insert names `maxImagesPerRequest` (measured at
  // the pin: `no column named maxImagesPerRequest`). Plant them ONCE on a
  // scratch copy every case then copies (a per-case plant cost the FIRST case
  // its captured log lines); the Rust side runs `ensure_4_12_columns_on`
  // before its profile insert.
  {
    const plantDir = mkdtempSync(join(tmpdir(), 'qt-p4d362-plant-'));
    process.on('exit', () => rmSync(plantDir, { recursive: true, force: true }));
    const planted = join(plantDir, 'main.db');
    copyFileSync(fixtures.main, planted);
    // The migration utils open the copy through `better-sqlite3`: the cipher
    // driver, exactly as each case's own mock serves it; the registry is reset
    // after, so no case inherits a module the plant loaded.
    jest.doMock('better-sqlite3', () =>
      jest.requireActual(
        join(process.cwd(), 'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'),
      ),
    );
    await plant412Columns(planted, spec.testPepperBase64);
    jest.resetModules();
    fixtures.main = planted;
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

    // ---- P4.D169: progressions at the MANUAL run entrance ----------------
    //
    // The asymmetry this entrance owns: `progress` is
    // `body.asCharacterId ? perspective.metadata : {}`, so a run NOBODY made
    // rolls against an EMPTY sheet. The same tool, the same clock, the same
    // fixture — and the only difference is whether a character was named.
    //
    // A consequence worth pinning rather than discovering later: the GATE and
    // the run-time sheet come from different places here. The roster resolved
    // through the perspective's vault tier and answered the gate from that
    // character's progressions, so `recharged` IS dealt on a run naming
    // nobody — and then rolls against an empty sheet. Both rows below are
    // 200s that read "The cannon speaks."; what differs is what the run could
    // SEE, which the effect rows further down make visible.
    { name: 'run-progress-gate-as-a', method: 'POST', body: { tool: 'recharged', asCharacterId: CHAR_A }, nowMs: PASCAL_NOW_MS },
    { name: 'run-progress-gate-no-character', method: 'POST', body: { tool: 'recharged' }, nowMs: PASCAL_NOW_MS },
    // The withheld half, from the same seat: `gestating`'s span is still
    // running, so the roster does not carry it and the route answers the same
    // "No custom tool named …" it gives an invented name.
    { name: 'run-progress-gate-running-span-as-a', method: 'POST', body: { tool: 'gestating', asCharacterId: CHAR_A }, nowMs: PASCAL_NOW_MS },
    // The WRITE, through this entrance: the effect lands in the named
    // character's vault and the store dump reads it back.
    { name: 'run-progress-effect-as-a', method: 'POST', body: { tool: 'recharge', asCharacterId: CHAR_A }, nowMs: PASCAL_NOW_MS },
    // ...and the same tool with nobody named: the sheet is empty, so the
    // outcome takes its catch-all and `{{progress.cannon.percent}}` renders as
    // WRITTEN — and there is no rolling character, so the effects skip rather
    // than writing anywhere. The store dump is what proves the second half.
    { name: 'run-progress-effect-no-character', method: 'POST', body: { tool: 'recharge' }, nowMs: PASCAL_NOW_MS },
    // The empty-string seat, which the four truthiness gates read as "nobody
    // named" — this must match `run-progress-effect-no-character` exactly.
    { name: 'run-progress-effect-as-empty-string', method: 'POST', body: { tool: 'recharge', asCharacterId: '' }, nowMs: PASCAL_NOW_MS },

    // ---- P4.D305 (v4 `362426c8f`): the run service's two route changes ----
    // `whispered` declares `defaultVisibility: 'whisper'`. With NO `private`,
    // the run is whispered by the DEFINITION: targeted to the operator's
    // userId and answered `whispered: true` (the baseline read
    // `body.private === true`, so it posted to the room and said `false`).
    { name: 'run-default-whisper-no-private', method: 'POST', body: { tool: 'whispered', asCharacterId: CHAR_A } },
    // ...and its REFUSAL (a parameter nobody declared): the Prospero bubble is
    // whispered too — `body.private ?? defaultVisibility === 'whisper'`.
    { name: 'run-default-whisper-refused-no-private', method: 'POST', body: { tool: 'whispered', asCharacterId: CHAR_A, parameters: { nobody: 1 } } },

    // ---- P4.D319 (v4 `a05c6f755`): `{{character}}` + `announceAsCharacter` --
    // `announced_dresser` (the Trigger Shelf, `pascal-run-custom-announcer.ts`)
    // says `{{character}} rolls {{value}}` and announces as the character. The
    // operator route passes NO name, so the service reads the RAW row: run as
    // CHAR_A, the row names Bertie and carries `customAnnouncer` LAST; run as
    // nobody, Pascal announces it and the placeholder renders as written.
    { name: 'list-trigger-room', method: 'GET', chat: CHAT_TRIGGERS },
    { name: 'run-announced-dresser-as-a', method: 'POST', chat: CHAT_TRIGGERS, body: { tool: 'announced_dresser', asCharacterId: CHAR_A } },
    { name: 'run-announced-dresser-no-character', method: 'POST', chat: CHAT_TRIGGERS, body: { tool: 'announced_dresser' } },

    // ---- P4.D379 (v4 `fd165fce0`): GET ?action=presets ---------------------
    // === P4.D379 ===
    // v4's own shape on REAL stores: vault A, the group store, the project's
    // official store and General, nearest first; two shadows.
    {
      name: 'presets-shadowed',
      method: 'GET',
      query: '?action=presets&tool=coin',
      plants: [
        presetDoc(VAULT_A, 'Tools/coin.aggressive.settings.json'),
        presetDoc(GROUP_STORE, 'Tools/coin.aggressive.settings.json'),
        presetDoc(GROUP_STORE, 'Tools/coin.careful.settings.json'),
        presetDoc(PROJECT_STORE, 'Tools/coin.quiet.settings.json'),
        presetDoc(GENERAL_STORE, 'Tools/coin.careful.settings.json'),
        presetDoc(GENERAL_STORE, 'Tools/coin.house.settings.json'),
      ],
    },
    // Only `fine` parses as one of coin's presets.
    {
      name: 'presets-filtered',
      method: 'GET',
      query: '?action=presets&tool=coin',
      plants: [
        presetDoc(VAULT_A, 'Tools/lockpick.x.settings.json'),
        presetDoc(VAULT_A, 'Tools/old/coin.y.settings.json'),
        presetDoc(VAULT_A, 'Tools/coin.Bad Name.settings.json'),
        presetDoc(VAULT_A, 'Tools/coin.settings.json'),
        presetDoc(VAULT_A, 'Tools/coin.fine.settings.json'),
      ],
    },
    // A filesystem store linked to the project whose `Tools/` will not list
    // (EACCES): the `unread` entry, the WARN, and the store still in `stores`.
    {
      name: 'presets-unread',
      method: 'GET',
      query: '?action=presets&tool=coin',
      plants: [
        { kind: 'diskStore', mountPointId: DISK_STORE, name: 'Locked Drawer', projectId: PROJECT, files: ['Tools/coin.hidden.settings.json'], toolsMode: 0 },
        presetDoc(GENERAL_STORE, 'Tools/coin.house.settings.json'),
      ],
    },
    // The same filesystem store, readable: a disk store's presets list too.
    {
      name: 'presets-disk-store',
      method: 'GET',
      query: '?action=presets&tool=coin',
      plants: [
        { kind: 'diskStore', mountPointId: DISK_STORE, name: 'Open Drawer', projectId: PROJECT, files: ['Tools/coin.drawer.settings.json', 'Tools/coin.tool.json'] },
        presetDoc(GENERAL_STORE, 'Tools/coin.drawer.settings.json'),
      ],
    },
    // A disabled store on the walk (REACH's `enabled: require`): absent from
    // `stores` AND `presets`.
    {
      name: 'presets-disabled-store',
      method: 'GET',
      query: '?action=presets&tool=coin',
      plants: [
        { kind: 'sql', db: 'mount', sql: 'UPDATE "doc_mount_points" SET "enabled" = 0 WHERE "id" = ?', params: [GROUP_STORE] },
        presetDoc(GROUP_STORE, 'Tools/coin.gone.settings.json'),
        presetDoc(GENERAL_STORE, 'Tools/coin.house.settings.json'),
      ],
    },
    // The tool need not exist.
    { name: 'presets-unknown-tool', method: 'GET', query: '?action=presets&tool=nosuchtool' },
    // The first 400, three ways.
    { name: 'presets-bad-tool', method: 'GET', query: '?action=presets&tool=Coin' },
    { name: 'presets-missing-tool', method: 'GET', query: '?action=presets' },
    { name: 'presets-empty-tool', method: 'GET', query: '?action=presets&tool=' },
    // The tool is checked BEFORE the chat: 400, not 404.
    { name: 'presets-bad-tool-missing-chat', method: 'GET', chat: UNKNOWN_CHAT, query: '?action=presets&tool=Coin' },
    { name: 'presets-missing-chat', method: 'GET', chat: UNKNOWN_CHAT, query: '?action=presets&tool=coin' },
    // B's walk: B's vault, no group tier.
    {
      name: 'presets-as-b',
      method: 'GET',
      query: `?action=presets&tool=coin&asCharacterId=${CHAR_B}`,
      plants: [
        presetDoc(VAULT_A, 'Tools/coin.mine.settings.json'),
        presetDoc(VAULT_B, 'Tools/coin.mine.settings.json'),
        presetDoc(GROUP_STORE, 'Tools/coin.table.settings.json'),
      ],
    },
    { name: 'presets-as-stranger', method: 'GET', query: `?action=presets&tool=coin&asCharacterId=${STRANGER}` },
    // An EMPTY asCharacterId is v4's falsy branch: the operator's own character
    // (B, in the LLM-led room), exactly like an absent one.
    {
      name: 'presets-empty-as',
      method: 'GET',
      chat: CHAT_LLM_LED,
      query: '?action=presets&tool=coin&asCharacterId=',
      plants: [presetDoc(VAULT_A, 'Tools/coin.mine.settings.json'), presetDoc(VAULT_B, 'Tools/coin.mine.settings.json')],
    },
    // A chat with no character participant (planted).
    { name: 'presets-no-character', method: 'GET', chat: CHAT_SOLO, query: '?action=presets&tool=coin', plants: [participantsSql(CHAT_SOLO, [])] },
    {
      name: 'presets-no-character-stranger',
      method: 'GET',
      chat: CHAT_SOLO,
      query: `?action=presets&tool=coin&asCharacterId=${STRANGER}`,
      plants: [participantsSql(CHAT_SOLO, [])],
    },
    // `searchParams.get` reads the FIRST of a repeated key.
    { name: 'presets-duplicate-keys', method: 'GET', query: '?action=presets&tool=coin&tool=Bad' },
    // The unknown-action envelope lists v4's two GET actions, in order.
    { name: 'presets-unknown-action', method: 'GET', query: '?action=foo' },
    // P4.D379 Tier 2: CHAR_D's broken vault — the perspective is DROPPED with
    // v4's WARN, and the walk runs as A.
    {
      name: 'presets-broken-vault-perspective',
      method: 'GET',
      chat: CHAT_SOLO,
      query: '?action=presets&tool=coin',
      plants: [participantsSql(CHAT_SOLO, [['e5000000-0000-4000-8000-00000000000d', CHAR_D, 'llm'], ['e5000000-0000-4000-8000-00000000000a', CHAR_A, 'llm']])],
    },
    // === end P4.D379 ===
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
