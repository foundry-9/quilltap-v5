/**
 * @jest-environment node
 *
 * P4.166 (dogfood #156 / #157, C.11) — the DEGRADED MOUNT INDEX at the route
 * surface: drives v4's REAL route handlers over a FRESH copy of the committed
 * groups-projects pair whose mount-index file is GARBAGE, so v4's client boots
 * it degraded (`mount-index-client.ts` — `__quilltapMountIndexDegraded`) the
 * way P4.159's rig does at the open. Each row records the status, the body and
 * every logger call (`Logger.prototype` spy — DEBUG included, so the overlay's
 * quiet `withRawDb` arm is compared too).
 *
 * Row classes (the survey's §A item 156 table):
 *   - C1 entity-first (project / group / character, vaulted + vault-less + a
 *     missing id), the tier factories (project wardrobe list, General
 *     wardrobe list/create, project inventory create, project scenarios list);
 *   - C3 creates (project, character);
 *   - C.11 the transfers GET (its catch arm);
 *   - `guard` — the guard sentence `requireMountIndexDb()` throws (the C4 job
 *     text) and #157's `Failed to log LLM call` on a GARBAGE llm-logs file.
 *
 * Run (Node 24, from the v4 checkout or the pin — cp to a /tmp mirror; jest
 * ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-degraded-mount-routes-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/degraded-mount-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/groups-projects.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_GP_MAIN=$V5W/crates/quilltap-web/tests/fixtures/groups-projects-main.db \
 *   QT_ORACLE_OUT=/tmp/oracle-degraded-mount-routes.ndjson \
 *     PATH=$N:$PATH npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- degraded-mount-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

const KAPPA = 'a3000000-0000-4000-8000-000000000003';
const MISSING_PROJECT = 'a3000000-0000-4000-8000-0000000000ff';
const GAMMA = 'a2000000-0000-4000-8000-000000000001';
const ARIA = 'a1000000-0000-4000-8000-000000000001';
const CLEO = 'a1000000-0000-4000-8000-000000000003';
const MISSING_CHARACTER = 'a1000000-0000-4000-8000-0000000000ff';
const CHAT_A = 'c1000000-0000-4000-8000-000000000001';
const CHAT_B = 'c1000000-0000-4000-8000-000000000002';

/** The garbage the mount-index (and, for #157, the llm-logs) file is
 *  overwritten with — not a SQLite header, so every open fails. */
const GARBAGE = 'this is not a database';

function mockRequest(method: string, url: string, body?: unknown): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

function applyMocks(spec: Spec): void {
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
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
  );
  // The chat route pulls the markdown renderer (unified/ESM) transitively.
  jest.doMock('@/lib/services/markdown-renderer.service', () => ({
    __esModule: true,
    renderMarkdownToHtml: async () => null,
    canPreRenderMessage: () => false,
  }));
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
}

interface Line {
  level: string;
  message: string;
  fields: Array<[string, string]>;
  error: string | null;
}

/** Every logger call the row makes, DEBUG included, as `{level, message,
 *  fields, error}` — `module` omitted, a string `error` context lifted into
 *  `error` (v4's context-string shape), an `Error` third argument's message
 *  likewise. Lines logged by the case's own setup are excluded (the spy is
 *  armed around the call only). */
async function withLogs<T>(call: () => Promise<T>): Promise<{ out: T; logs: Line[] }> {
  const { Logger } = await import('@/lib/logger');
  const logs: Line[] = [];
  const levels = ['debug', 'info', 'warn', 'error'] as const;
  const originals = Object.fromEntries(levels.map((l) => [l, Logger.prototype[l]])) as Record<
    (typeof levels)[number],
    (...a: unknown[]) => void
  >;
  for (const level of levels) {
    const original = originals[level];
    Logger.prototype[level] = function (
      this: unknown,
      message: string,
      context?: Record<string, unknown>,
      ...rest: unknown[]
    ) {
      const fields: Array<[string, string]> = [];
      for (const [k, v] of Object.entries(context ?? {})) {
        if (k === 'module' || k === 'error') continue;
        fields.push([k, typeof v === 'string' ? v : JSON.stringify(v)]);
      }
      const ctxError = context?.error;
      const third = rest[0];
      const error =
        typeof ctxError === 'string'
          ? ctxError
          : third instanceof Error
            ? third.message
            : null;
      logs.push({ level, message, fields, error });
      return original.call(this, message, context, ...rest);
    } as never;
  }
  try {
    return { out: await call(), logs };
  } finally {
    for (const level of levels) Logger.prototype[level] = originals[level] as never;
  }
}

async function loadRoute(path: string): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
  return (await import(path)) as never;
}
async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

interface RouteRow {
  name: string;
  /** SQL run on the main copy before the client opens. */
  plant?: string;
  call: () => Promise<unknown>;
}

async function runRow(
  spec: Spec,
  row: RouteRow,
  scratch: string,
  fixtureMain: string,
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec);
  const work = mkdtempSync(join(scratch, 'dm-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtureMain, mainWork);
  if (row.plant) {
    const D = require(join(process.cwd(), 'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'));
    const db = new D(mainWork);
    db.pragma(`key = "x'${Buffer.from(spec.testPepperBase64, 'base64').toString('hex')}'"`);
    db.exec(row.plant);
    db.close();
  }
  writeFileSync(mountWork, GARBAGE);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient, isMountIndexDegraded } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();
  try {
    if (!isMountIndexDegraded()) throw new Error(`${row.name}: the mount index did not boot degraded`);
    const { out, logs } = await withLogs(async () => respond(await row.call()));
    return {
      kind: 'route',
      name: row.name,
      ...(row.plant ? { plant: row.plant } : {}),
      status: out.status,
      body: out.body,
      logs,
    };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

/** The C4 job text — `requireMountIndexDb()`'s throw under a degraded
 *  partition — and #157's line: `logLLMCall` over a GARBAGE llm-logs file
 *  (the main copy sound, the mount index sound-absent). */
async function guardRows(spec: Spec, scratch: string, fixtureMain: string): Promise<Array<Record<string, unknown>>> {
  jest.resetModules();
  applyMocks(spec);
  const work = mkdtempSync(join(scratch, 'dm-guard-'));
  const mainWork = join(work, 'main.db');
  copyFileSync(fixtureMain, mainWork);
  writeFileSync(join(work, 'mount.db'), GARBAGE);
  writeFileSync(join(work, 'llm.db'), GARBAGE);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = join(work, 'mount.db');
  process.env.SQLITE_LLM_LOGS_PATH = join(work, 'llm.db');
  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
  await initializeDatabase();
  const rows: Array<Record<string, unknown>> = [];
  try {
    const { requireMountIndexDb } = await import('@/lib/database/backends/sqlite/mount-index-guard');
    let mountError: string | null = null;
    try {
      requireMountIndexDb();
    } catch (e) {
      mountError = (e as Error).message;
    }
    rows.push({ kind: 'guard', name: 'mount_index_guard', error: mountError });

    const { isLLMLogsDegraded } = await import('@/lib/database/backends/sqlite/llm-logs-client');
    if (!isLLMLogsDegraded()) throw new Error('the llm-logs partition did not boot degraded');
    // jest.setup mocks the service globally (`jest.setup.ts:381`) — the REAL one.
    const { logLLMCall } = jest.requireActual('@/lib/services/llm-logging.service') as {
      logLLMCall: (p: unknown) => Promise<unknown>;
    };
    const { out, logs } = await withLogs(() =>
      logLLMCall({
        userId: spec.userId,
        type: 'CHAT_MESSAGE',
        provider: 'OPENAI',
        modelName: 'gpt-4o-mini',
        request: { messages: [{ role: 'user', content: 'Hello' }] },
        response: { content: 'Hi' },
      } as never),
    );
    rows.push({ kind: 'guard', name: 'llm_log_degraded', returned: out ?? null, logs });
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
    delete process.env.SQLITE_LLM_LOGS_PATH;
  }
  return rows;
}

async function main(): Promise<void> {
  // v4's `enqueueJob` lazily spawns the job host (the rollback's announcement
  // job); hold it off so no child process claims jobs mid-run
  // (`processor-host.ts:300-301`, memory note `v4-oracle-job-host`).
  (globalThis as Record<string, unknown>).__quilltapJobHost = {
    child: null,
    spawning: false,
    shuttingDown: true,
    childCrashed: false,
    restartTimestamps: [],
    signalHandlersInstalled: false,
    invalidationListeners: new Set(),
  };
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'groups-projects.json'), 'utf8'),
  ) as Spec;
  const fixtureMain = process.env.QT_FIXTURE_GP_MAIN ?? '';
  if (!fixtureMain || !existsSync(fixtureMain)) throw new Error(`fixture main missing: ${fixtureMain}`);
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-degraded-mount-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const H = 'http://localhost/api/v1';
  const p = (id: string) => ({ params: Promise.resolve({ id }) });
  const VAULTLESS = `UPDATE characters SET characterDocumentMountPointId = NULL WHERE id = '${CLEO}'`;
  // 156.a — one recorded outfit for Aria in Chat A: v4's table DDL
  // (`chat-wardrobe-history-ddl.ts`, byte-for-byte) and one row whose slots
  // name an item the degraded pool cannot hold.
  const HISTORY = [
    `CREATE TABLE IF NOT EXISTS "chat_wardrobe_history" (
    "id" TEXT PRIMARY KEY,
    "chatId" TEXT NOT NULL REFERENCES "chats"("id") ON DELETE CASCADE,
    "characterId" TEXT NOT NULL,
    "entryIndex" INTEGER NOT NULL,
    "slots" TEXT NOT NULL,
    "slotsHash" TEXT NOT NULL,
    "source" TEXT NOT NULL,
    "actorCharacterId" TEXT,
    "isCurrent" INTEGER NOT NULL DEFAULT 0,
    "createdAt" TEXT NOT NULL,
    "actorToolName" TEXT
  );`,
    `INSERT INTO "chat_wardrobe_history" ("id", "chatId", "characterId", "entryIndex", "slots", "slotsHash", "source", "actorCharacterId", "actorToolName", "isCurrent", "createdAt") VALUES ('d1000000-0000-4000-8000-000000000001', '${CHAT_A}', '${ARIA}', 0, '{"top":["ab000000-0000-4000-8000-000000000001"],"bottom":[],"footwear":[],"accessories":[]}', 'h0', 'operator', NULL, NULL, 1, '2026-03-02T00:00:00.000Z');`,
  ].join('\n');
  // C2 rollback — two entries, the current at 1: rolling to 0 resolves the
  // recorded slots against the (empty) degraded pool.
  const HISTORY_TWO = [
    HISTORY,
    `UPDATE "chat_wardrobe_history" SET "isCurrent" = 0;`,
    `INSERT INTO "chat_wardrobe_history" ("id", "chatId", "characterId", "entryIndex", "slots", "slotsHash", "source", "actorCharacterId", "actorToolName", "isCurrent", "createdAt") VALUES ('d1000000-0000-4000-8000-000000000002', '${CHAT_A}', '${ARIA}', 1, '{"top":[],"bottom":[],"footwear":[],"accessories":[]}', 'h1', 'operator', NULL, NULL, 1, '2026-03-03T00:00:00.000Z');`,
  ].join('\n');
  // The workbench family's `gated` definition (`workbench-route-cases.json`).
  const GATED = {
    name: 'gated',
    title: 'The Gated Table',
    description: 'A table that consults the sheet.',
    roll: { min: 0.8, max: 0.8 },
    outcomes: [
      { when: { gt: 0.5, metadata: { hasKey: { eq: true } } }, message: 'The lock turns for {{value}}.', state: 'success' },
      { when: true, message: 'The lock holds.', state: 'failure' },
    ],
  };
  const rows: RouteRow[] = [
    // --- C1 project ---
    {
      name: 'project_add_chat',
      call: async () =>
        (await loadRoute('@/app/api/v1/projects/[id]/route')).POST(
          mockRequest('POST', `${H}/projects/${KAPPA}?action=add-chat`, { chatId: CHAT_A }),
          p(KAPPA),
        ),
    },
    {
      name: 'project_add_chat_missing',
      call: async () =>
        (await loadRoute('@/app/api/v1/projects/[id]/route')).POST(
          mockRequest('POST', `${H}/projects/${MISSING_PROJECT}?action=add-chat`, { chatId: CHAT_A }),
          p(MISSING_PROJECT),
        ),
    },
    // --- C1 group ---
    {
      name: 'group_update',
      call: async () =>
        (await loadRoute('@/app/api/v1/groups/[id]/route')).PUT(
          mockRequest('PUT', `${H}/groups/${GAMMA}`, { name: 'Gamma II' }),
          p(GAMMA),
        ),
    },
    // --- C1 character ---
    {
      name: 'character_favorite',
      call: async () =>
        (await loadRoute('@/app/api/v1/characters/[id]/route')).POST(
          mockRequest('POST', `${H}/characters/${ARIA}?action=favorite`),
          p(ARIA),
        ),
    },
    {
      name: 'character_favorite_vaultless',
      plant: VAULTLESS,
      call: async () =>
        (await loadRoute('@/app/api/v1/characters/[id]/route')).POST(
          mockRequest('POST', `${H}/characters/${CLEO}?action=favorite`),
          p(CLEO),
        ),
    },
    {
      name: 'character_favorite_missing',
      call: async () =>
        (await loadRoute('@/app/api/v1/characters/[id]/route')).POST(
          mockRequest('POST', `${H}/characters/${MISSING_CHARACTER}?action=favorite`),
          p(MISSING_CHARACTER),
        ),
    },
    // The raw-row-first character sites (v4's broken-vault-safe gates).
    {
      name: 'character_update',
      call: async () =>
        (await loadRoute('@/app/api/v1/characters/[id]/route')).PUT(
          mockRequest('PUT', `${H}/characters/${ARIA}`, { title: 'The Bold' }),
          p(ARIA),
        ),
    },
    {
      name: 'character_delete',
      call: async () =>
        (await loadRoute('@/app/api/v1/characters/[id]/route')).DELETE(
          mockRequest('DELETE', `${H}/characters/${ARIA}`),
          p(ARIA),
        ),
    },
    {
      name: 'character_add_tag',
      call: async () =>
        (await loadRoute('@/app/api/v1/characters/[id]/route')).POST(
          mockRequest('POST', `${H}/characters/${ARIA}?action=add-tag`, { tagId: 'aa000000-0000-4000-8000-000000000001' }),
          p(ARIA),
        ),
    },
    {
      name: 'character_prompt_create',
      call: async () =>
        (await loadRoute('@/app/api/v1/characters/[id]/prompts/route')).POST(
          mockRequest('POST', `${H}/characters/${ARIA}/prompts`, { name: 'Terse', content: 'Be brief.' }),
          p(ARIA),
        ),
    },
    {
      name: 'project_delete',
      call: async () =>
        (await loadRoute('@/app/api/v1/projects/[id]/route')).DELETE(
          mockRequest('DELETE', `${H}/projects/${KAPPA}`),
          p(KAPPA),
        ),
    },
    {
      name: 'group_add_member',
      call: async () =>
        (await loadRoute('@/app/api/v1/groups/[id]/route')).POST(
          mockRequest('POST', `${H}/groups/${GAMMA}?action=addMember`, { characterId: ARIA }),
          p(GAMMA),
        ),
    },
    // --- 156.a — the wardrobe-history GET over a degraded pool ---
    {
      name: 'chat_wardrobe_history',
      plant: HISTORY,
      call: async () =>
        (await loadRoute('@/app/api/v1/chats/[id]/route')).GET(
          mockRequest('GET', `${H}/chats/${CHAT_A}?action=wardrobe-history&characterId=${ARIA}`),
          { params: Promise.resolve({ id: CHAT_A }) },
        ),
    },
    // --- C2 — the equip / rollback routes over the degraded pool ---
    {
      name: 'chat_equip_wear',
      call: async () =>
        (await loadRoute('@/app/api/v1/chats/[id]/route')).POST(
          mockRequest('POST', `${H}/chats/${CHAT_A}?action=equip`, {
            characterId: ARIA,
            mode: 'wear',
            itemId: 'ab000000-0000-4000-8000-000000000001',
          }),
          { params: Promise.resolve({ id: CHAT_A }) },
        ),
    },
    {
      name: 'chat_wardrobe_rollback',
      plant: HISTORY_TWO,
      call: async () =>
        (await loadRoute('@/app/api/v1/chats/[id]/route')).POST(
          mockRequest('POST', `${H}/chats/${CHAT_A}?action=wardrobe-rollback`, { characterId: ARIA, index: 0 }),
          { params: Promise.resolve({ id: CHAT_A }) },
        ),
    },
    // --- 156.c — a merge bringing a vault-less character across ---
    {
      name: 'chat_merge_vaultless',
      plant: VAULTLESS,
      call: async () =>
        (await loadRoute('@/app/api/v1/chats/[id]/route')).POST(
          mockRequest('POST', `${H}/chats/${CHAT_B}?action=merge-conversation`, {
            sourceChatId: CHAT_A,
            characterIds: [CLEO],
          }),
          { params: Promise.resolve({ id: CHAT_B }) },
        ),
    },
    // --- 156.b — the Workbench seating a character (the audit: deterministic
    // under a pinned roll) ---
    {
      name: 'workbench_audit_seated',
      call: async () =>
        (await loadRoute('@/app/api/v1/custom-tools/route')).POST(
          mockRequest('POST', `${H}/custom-tools?action=audit`, {
            definition: GATED,
            metadata: { characterId: ARIA },
          }),
        ),
    },
    {
      name: 'workbench_audit_seated_vaultless',
      plant: VAULTLESS,
      call: async () =>
        (await loadRoute('@/app/api/v1/custom-tools/route')).POST(
          mockRequest('POST', `${H}/custom-tools?action=audit`, {
            definition: GATED,
            metadata: { characterId: CLEO },
          }),
        ),
    },
    // --- C1 tier factories ---
    {
      name: 'project_wardrobe_list',
      call: async () =>
        (await loadRoute('@/app/api/v1/projects/[id]/wardrobe/route')).GET(
          mockRequest('GET', `${H}/projects/${KAPPA}/wardrobe`),
          p(KAPPA),
        ),
    },
    {
      name: 'general_wardrobe_list',
      call: async () => (await loadRoute('@/app/api/v1/wardrobe/route')).GET(mockRequest('GET', `${H}/wardrobe`)),
    },
    {
      name: 'general_wardrobe_create',
      call: async () =>
        (await loadRoute('@/app/api/v1/wardrobe/route')).POST(
          mockRequest('POST', `${H}/wardrobe`, { title: 'Rain Boots', types: ['footwear'] }),
        ),
    },
    {
      name: 'project_inventory_create',
      call: async () =>
        (await loadRoute('@/app/api/v1/projects/[id]/inventory/route')).POST(
          mockRequest('POST', `${H}/projects/${KAPPA}/inventory`, { name: 'Brass Lantern' }),
          p(KAPPA),
        ),
    },
    {
      name: 'character_inventory_create',
      call: async () =>
        (await loadRoute('@/app/api/v1/characters/[id]/inventory/route')).POST(
          mockRequest('POST', `${H}/characters/${ARIA}/inventory`, { name: 'Brass Lantern' }),
          p(ARIA),
        ),
    },
    {
      name: 'general_inventory_list',
      call: async () => (await loadRoute('@/app/api/v1/inventory/route')).GET(mockRequest('GET', `${H}/inventory`)),
    },
    {
      name: 'group_wardrobe_create',
      call: async () =>
        (await loadRoute('@/app/api/v1/groups/[id]/wardrobe/route')).POST(
          mockRequest('POST', `${H}/groups/${GAMMA}/wardrobe`, { title: 'Rain Boots', types: ['footwear'] }),
          p(GAMMA),
        ),
    },
    {
      name: 'character_wardrobe_create',
      call: async () =>
        (await loadRoute('@/app/api/v1/characters/[id]/wardrobe/route')).POST(
          mockRequest('POST', `${H}/characters/${ARIA}/wardrobe`, { title: 'Rain Boots', types: ['footwear'] }),
          p(ARIA),
        ),
    },
    {
      name: 'project_scenarios_list',
      call: async () =>
        (await loadRoute('@/app/api/v1/projects/[id]/scenarios/route')).GET(
          mockRequest('GET', `${H}/projects/${KAPPA}/scenarios`),
          p(KAPPA),
        ),
    },
    // --- C3 creates ---
    {
      name: 'project_create',
      call: async () =>
        (await loadRoute('@/app/api/v1/projects/route')).POST(
          mockRequest('POST', `${H}/projects`, { name: 'Ember' }),
        ),
    },
    {
      name: 'character_create',
      call: async () =>
        (await loadRoute('@/app/api/v1/characters/route')).POST(
          mockRequest('POST', `${H}/characters`, { name: 'Fen' }),
        ),
    },
    // --- C.11 ---
    {
      name: 'transfers_destinations',
      call: async () =>
        (await loadRoute('@/app/api/v1/wardrobe/transfers/route')).GET(mockRequest('GET', `${H}/wardrobe/transfers`)),
    },
  ];

  const only = process.env.QT_DEGRADED_ONLY;
  const outLines: string[] = [];
  for (const row of rows) {
    if (only && row.name !== only) continue;
    outLines.push(JSON.stringify(await runRow(spec, row, scratch, fixtureMain)));
  }
  if (!only || only === 'guard') for (const g of await guardRows(spec, scratch, fixtureMain)) outLines.push(JSON.stringify(g));
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`degraded-mount-routes oracle wrote ${outPath} (${outLines.length} rows)\n`);
}

test('degraded-mount-routes oracle', async () => {
  await main();
});

// Remove the OS-temp scratch dir(s) once the oracle has written its NDJSON.
const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
