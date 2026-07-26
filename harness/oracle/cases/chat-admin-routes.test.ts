/**
 * @jest-environment node
 *
 * P4.9E3A CHAT-ADMIN + TOOLS route-surface ORACLE: drives v4's REAL chat action
 * handlers (`actions/tags.ts`, `actions/tools.ts`, `actions/agent-mode.ts`,
 * `actions/danger-classification.ts`, `actions/render-conversation.ts`,
 * `actions/bulk.ts`, `actions/merge.ts`, `actions/rng.ts`, `actions/run-tool.ts`)
 * over a FRESH copy of the committed chat-admin fixture per case, and emits each
 * response body plus post-mutation table dumps so the Rust port
 * (`services::chat_admin::*` and friends) diffs byte-for-byte.
 *
 * `regenerate-title` is model-dependent and is pinned by the separate tier-3
 * `chat-regenerate-title-tier3` oracle, not here.
 *
 * The clock is frozen to NOW_MS for every case, so the minted `createdAt` on the
 * rng / run-tool TOOL messages lands on the same bytes as the Rust side's
 * injected constant. Job-row ids and timestamps are blanked by the differential
 * (both sides mint fresh UUIDs), but the DEDUPE cases prove identity in-band by
 * emitting whether the two calls returned the same job id.
 *
 * **Must run under `TZ=UTC`** — the house convention for every clock-touching
 * oracle. The guard below fails loudly rather than emitting a locale-shifted
 * fixture.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-ca-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/chat-admin-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/chat-admin-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CA_MAIN=$V5W/crates/quilltap-web/tests/fixtures/chat-admin-main.db \
 *   QT_FIXTURE_CA_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/chat-admin-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-chat-admin.ndjson TZ=UTC \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- chat-admin-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  frozenNowMs: number;
  /** The committed `crypto.randomBytes` stream every rng case replays. */
  rngByteStream: number[];
}

// Pinned entity ids (shared verbatim with the builder + the Rust differential).
const TAG_A = 'b1000000-0000-4000-8000-000000000001';
const TAG_B = 'b1000000-0000-4000-8000-000000000002';
const CHAT = 'c1000000-0000-4000-8000-000000000001';
const EMPTY_CHAT = 'c1000000-0000-4000-8000-000000000003';
const MISSING_ID = '99999999-9999-4999-8999-999999999999';
const P_EVE = 'e1000000-0000-4000-8000-000000000001';
const P_ARIA = 'e1000000-0000-4000-8000-000000000002';
const P_CLIO = 'e1000000-0000-4000-8000-000000000003';
// A participant id belonging to the SOURCE chat, so "not found in THIS chat" is
// reachable with a well-formed uuid.
const P_STRANGER = 'e1000000-0000-4000-8000-000000000011';
const CLIO = 'a1000000-0000-4000-8000-000000000003';
const BEA = 'a1000000-0000-4000-8000-000000000002';
const DORIAN = 'a1000000-0000-4000-8000-000000000004';
const SOURCE_CHAT = 'c1000000-0000-4000-8000-000000000002';

const RealDate = Date;

// The per-case injected byte stream for `crypto.randomBytes` (mirrors the Rust
// `FixedBytes`). The rng cases set it; every other case leaves it empty and
// never draws. Exhaustion throws, so an unexpected consumer is loud.
let currentBytes: number[] = [];
let byteCursor = 0;

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'POST',
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
  jest.doMock('@/lib/mount-index/character-vault', () =>
    jest.requireActual('@/lib/mount-index/character-vault'),
  );
  // The chat route pulls the markdown renderer (unified/ESM) transitively — mock
  // it away; no case under test renders HTML.
  jest.doMock('@/lib/services/markdown-renderer.service', () => ({
    __esModule: true,
    renderMarkdownToHtml: async () => null,
    canPreRenderMessage: () => false,
  }));
  // Keep the background-jobs processor OFF so it can't claim a row this suite
  // enqueues and race the dump (the P4.6y lesson).
  jest.doMock('@/lib/background-jobs/processor', () => ({
    __esModule: true,
    ensureProcessorRunning: () => {},
    startProcessor: () => {},
    stopProcessor: () => {},
  }));
  // Pin ONLY `crypto.randomBytes` (the rng handler's `import { randomBytes }
  // from 'crypto'`). `randomUUID` + `createHash` stay REAL — the TOOL message's
  // id is minted and the vault overlay hashes. No `__esModule: true` and no
  // explicit `default`, so esModuleInterop still synthesizes a working default
  // (`jest-crypto-randombytes-mock`).
  jest.doMock('crypto', () => {
    const actual = jest.requireActual('crypto');
    return {
      ...actual,
      randomBytes: (n: number) => {
        const end = byteCursor + n;
        if (end > currentBytes.length) {
          throw new Error(
            `randomBytes mock exhausted: needed ${n} at cursor ${byteCursor} of ${currentBytes.length}`,
          );
        }
        const slice = currentBytes.slice(byteCursor, end);
        byteCursor = end;
        return Buffer.from(slice);
      },
    };
  });
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

/** The hydrated chat row — every column the admin verbs can move. */
async function readChat(chatId: string): Promise<unknown> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  return ((await getRepositories().chats.findById(chatId)) ?? null) as unknown;
}

/** Every chat event, in order — the bulk rewrite's evidence. */
async function readMessages(chatId: string): Promise<unknown> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  return (await getRepositories().chats.getMessages(chatId)) as unknown;
}

/** Every memory row, id-sorted — bulk re-attribution deletes some of them. */
async function readMemories(): Promise<unknown> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const rows = (await getRepositories().memories.findAll()) as Array<Record<string, unknown>>;
  return rows
    .map((m) => ({ id: m.id, characterId: m.characterId, sourceMessageId: m.sourceMessageId ?? null }))
    .sort((a, b) => String(a.id).localeCompare(String(b.id)));
}

/** The user's background jobs, oldest first (the two enqueueing verbs). */
async function readJobs(userId: string): Promise<unknown> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const repos = getRepositories();
  const rows = [
    ...(await repos.backgroundJobs.findByUserId(userId, 'PENDING')),
    ...(await repos.backgroundJobs.findByUserId(userId, 'PROCESSING')),
  ] as Array<Record<string, unknown>>;
  return rows
    .map((j) => ({
      type: j.type,
      status: j.status,
      payload: j.payload,
      priority: j.priority,
      attempts: j.attempts,
      maxAttempts: j.maxAttempts,
      lastError: j.lastError ?? null,
    }))
    .sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
}

interface CaseSpec {
  name: string;
  /** Replay the committed byte stream for this case (the rng cases). */
  rng?: boolean;
  run: () => Promise<{ status: number; body: unknown; tables?: unknown }>;
}

async function loadRoute(path: string): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
  return (await import(path)) as never;
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

const CHAT_ROUTE = '@/app/api/v1/chats/[id]/route';

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  currentBytes = c.rng ? spec.rngByteStream : [];
  byteCursor = 0;
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'ca-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();

  // A TICKING frozen clock: each argless `new Date()` / `Date.now()` advances
  // 1 ms from `frozenNowMs`. A hard freeze would give every message written in
  // one case the SAME `createdAt`, and v4's ordered read then returns them in an
  // order that is an artifact of its tie-break rather than of insertion — while
  // v5's real clock separates them. Ticking makes both sides order by insertion
  // (`chain-depth-frozen-clock-artifact`). The Rust side asserts v4's stamps are
  // at-or-after the base rather than equal to it.
  let tick = 0;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  global.Date = class extends RealDate {
    constructor(...a: unknown[]) {
      if (a.length === 0) super(spec.frozenNowMs + tick++);
      // @ts-expect-error forward variadic args
      else super(...a);
    }
    static now(): number {
      return spec.frozenNowMs + tick++;
    }
  } as unknown as DateConstructor;

  try {
    const out = await c.run();
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      ...(out.tables !== undefined ? { tables: out.tables } : {}),
    };
  } finally {
    global.Date = RealDate;
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(`chat-admin-routes oracle must run under TZ=UTC (getTimezoneOffset=${offset})`);
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'chat-admin-web.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_CA_MAIN ?? '',
    mount: process.env.QT_FIXTURE_CA_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ca-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const B = 'http://localhost/api/v1';
  const post = async (chatId: string, action: string, body: unknown) =>
    (await loadRoute(CHAT_ROUTE)).POST(
      mockRequest(`${B}/chats/${chatId}?action=${action}`, body),
      { params: Promise.resolve({ id: chatId }) },
    );

  const chatTables = async (chatId = CHAT) => ({ chat: await readChat(chatId) });
  const bulkTables = async () => ({
    chat: await readChat(CHAT),
    messages: await readMessages(CHAT),
    memories: await readMemories(),
  });
  const rngTables = async () => ({ messages: await readMessages(CHAT) });
  // The merge touches BOTH chats plus the target's participants, tags,
  // identity-stack cache and equipped outfits.
  const mergeTables = async () => ({
    chat: await readChat(CHAT),
    messages: await readMessages(CHAT),
    sourceMessages: await readMessages(SOURCE_CHAT),
  });
  const jobTables = async (chatId = CHAT) => ({
    chat: await readChat(chatId),
    jobs: await readJobs(spec.userId),
  });

  const cases: CaseSpec[] = [
    // ── ?action=add-tag ─────────────────────────────────────────────────────
    {
      name: 'add_tag_new',
      run: async () => {
        const { status, body } = await respond(await post(CHAT, 'add-tag', { tagId: TAG_B }));
        return { status, body, tables: await chatTables() };
      },
    },
    {
      // Already present: v4's addTag pushes only when absent, so NO update runs
      // and `updatedAt` is untouched. The chat dump is the proof.
      name: 'add_tag_already_present',
      run: async () => {
        const { status, body } = await respond(await post(CHAT, 'add-tag', { tagId: TAG_A }));
        return { status, body, tables: await chatTables() };
      },
    },
    {
      name: 'add_tag_unknown_tag',
      run: async () => respond(await post(CHAT, 'add-tag', { tagId: MISSING_ID })),
    },
    {
      name: 'add_tag_chat_missing',
      run: async () => respond(await post(MISSING_ID, 'add-tag', { tagId: TAG_B })),
    },
    // ── ?action=remove-tag ──────────────────────────────────────────────────
    {
      name: 'remove_tag_present',
      run: async () => {
        const { status, body } = await respond(await post(CHAT, 'remove-tag', { tagId: TAG_A }));
        return { status, body, tables: await chatTables() };
      },
    },
    {
      // Absent: the filter doesn't shorten the array, so no update runs.
      name: 'remove_tag_absent',
      run: async () => {
        const { status, body } = await respond(await post(CHAT, 'remove-tag', { tagId: TAG_B }));
        return { status, body, tables: await chatTables() };
      },
    },
    {
      // v4 never verifies the tag row on the REMOVE path — an unknown id is a
      // successful no-op, not a 404. (The ADD path does verify.)
      name: 'remove_tag_unknown_id',
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'remove-tag', { tagId: MISSING_ID }),
        );
        return { status, body, tables: await chatTables() };
      },
    },
    // ── ?action=update-tool-settings ────────────────────────────────────────
    {
      name: 'update_tool_settings',
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'update-tool-settings', {
            disabledTools: ['generate_image', 'web_search'],
            disabledToolGroups: ['scriptorium'],
          }),
        );
        return { status, body, tables: await chatTables() };
      },
    },
    {
      // Zod defaults both arrays to `[]` when the body omits them, and
      // `forceToolsOnNextMessage` is still set — the half the response hides.
      name: 'update_tool_settings_empty',
      run: async () => {
        const { status, body } = await respond(await post(CHAT, 'update-tool-settings', {}));
        return { status, body, tables: await chatTables() };
      },
    },
    // ── ?action=toggle-agent-mode ───────────────────────────────────────────
    {
      // The §1 wire's only reachable arm: no `enabled` key. v4 spreads
      // `undefined` into the patch, Zod drops it, the column is untouched — and
      // the message still reads "disabled" (the ternary's falsy branch).
      name: 'toggle_agent_mode_absent',
      run: async () => {
        const { status, body } = await respond(await post(CHAT, 'toggle-agent-mode', {}));
        return { status, body, tables: await chatTables() };
      },
    },
    {
      name: 'toggle_agent_mode_true',
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'toggle-agent-mode', { enabled: true }),
        );
        return { status, body, tables: await chatTables() };
      },
    },
    {
      name: 'toggle_agent_mode_false',
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'toggle-agent-mode', { enabled: false }),
        );
        return { status, body, tables: await chatTables() };
      },
    },
    {
      name: 'toggle_agent_mode_null',
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'toggle-agent-mode', { enabled: null }),
        );
        return { status, body, tables: await chatTables() };
      },
    },
    {
      name: 'toggle_agent_mode_chat_missing',
      run: async () => respond(await post(MISSING_ID, 'toggle-agent-mode', {})),
    },
    // ── ?action=reclassify-danger ───────────────────────────────────────────
    {
      // ARIA is an llm participant carrying CONN, so the reset is followed by a
      // fresh CHAT_DANGER_CLASSIFICATION job.
      name: 'reclassify_danger',
      run: async () => {
        const { status, body } = await respond(await post(CHAT, 'reclassify-danger', {}));
        return { status, body, tables: await jobTables() };
      },
    },
    {
      // The enqueue dedupes on the chat, so the second call returns the FIRST
      // job's id and no second row lands.
      name: 'reclassify_danger_dedupe',
      run: async () => {
        const first = await respond(await post(CHAT, 'reclassify-danger', {}));
        const second = await respond(await post(CHAT, 'reclassify-danger', {}));
        const a = first.body as { jobId?: string };
        const b = second.body as { jobId?: string };
        return {
          status: second.status,
          body: { ...second.body, sameJobId: a.jobId === b.jobId },
          tables: await jobTables(),
        };
      },
    },
    {
      // No participants at all → the reset stands, no job, and the response says
      // so (with no `jobId` key).
      name: 'reclassify_danger_no_profile',
      run: async () => {
        const { status, body } = await respond(await post(EMPTY_CHAT, 'reclassify-danger', {}));
        return { status, body, tables: await jobTables(EMPTY_CHAT) };
      },
    },
    // ── ?action=render-conversation ─────────────────────────────────────────
    {
      name: 'render_conversation',
      run: async () => {
        const { status, body } = await respond(await post(CHAT, 'render-conversation', {}));
        return { status, body, tables: await jobTables() };
      },
    },
    {
      name: 'render_conversation_dedupe',
      run: async () => {
        const first = await respond(await post(CHAT, 'render-conversation', {}));
        const second = await respond(await post(CHAT, 'render-conversation', {}));
        const a = first.body as { jobId?: string };
        const b = second.body as { jobId?: string };
        return {
          status: second.status,
          body: { ...second.body, sameJobId: a.jobId === b.jobId },
          tables: await jobTables(),
        };
      },
    },
    {
      name: 'render_conversation_chat_missing',
      run: async () => respond(await post(MISSING_ID, 'render-conversation', {})),
    },
    // ── ?action=bulk-reattribute ────────────────────────────────────────────
    // ARIA's rows are crossed by role on purpose: ASSISTANT selects M2+M3
    // (both of which carry a memory), USER selects M4, `both` selects all three.
    {
      name: 'bulk_reattribute_both',
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'bulk-reattribute', {
            sourceParticipantId: P_ARIA,
            targetParticipantId: P_EVE,
          }),
        );
        return { status, body, tables: await bulkTables() };
      },
    },
    {
      name: 'bulk_reattribute_assistant_only',
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'bulk-reattribute', {
            sourceParticipantId: P_ARIA,
            targetParticipantId: P_EVE,
            roleFilter: 'ASSISTANT',
          }),
        );
        return { status, body, tables: await bulkTables() };
      },
    },
    {
      name: 'bulk_reattribute_user_only',
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'bulk-reattribute', {
            sourceParticipantId: P_ARIA,
            targetParticipantId: P_EVE,
            roleFilter: 'USER',
          }),
        );
        return { status, body, tables: await bulkTables() };
      },
    },
    {
      // The nullable source: an explicit null selects the UNATTRIBUTED rows
      // (M6 + M7), which carry no memories.
      name: 'bulk_reattribute_null_source',
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'bulk-reattribute', {
            sourceParticipantId: null,
            targetParticipantId: P_CLIO,
          }),
        );
        return { status, body, tables: await bulkTables() };
      },
    },
    {
      // Nothing matches (CLIO has no USER-role rows) → the early return, with no
      // clear-and-replay at all, so `updatedAt` and `messageCount` stand.
      name: 'bulk_reattribute_no_matches',
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'bulk-reattribute', {
            sourceParticipantId: P_CLIO,
            targetParticipantId: P_EVE,
            roleFilter: 'USER',
          }),
        );
        return { status, body, tables: await bulkTables() };
      },
    },
    {
      name: 'bulk_reattribute_same_participant',
      run: async () =>
        respond(
          await post(CHAT, 'bulk-reattribute', {
            sourceParticipantId: P_ARIA,
            targetParticipantId: P_ARIA,
          }),
        ),
    },
    {
      name: 'bulk_reattribute_source_not_in_chat',
      run: async () =>
        respond(
          await post(CHAT, 'bulk-reattribute', {
            sourceParticipantId: P_STRANGER,
            targetParticipantId: P_EVE,
          }),
        ),
    },
    {
      name: 'bulk_reattribute_target_not_in_chat',
      run: async () =>
        respond(
          await post(CHAT, 'bulk-reattribute', {
            sourceParticipantId: P_ARIA,
            targetParticipantId: P_STRANGER,
          }),
        ),
    },
    {
      name: 'bulk_reattribute_bad_role_filter',
      run: async () =>
        respond(
          await post(CHAT, 'bulk-reattribute', {
            sourceParticipantId: P_ARIA,
            targetParticipantId: P_EVE,
            roleFilter: 'TOOL',
          }),
        ),
    },
    // ── ?action=rng ─────────────────────────────────────────────────────────
    // Every rng case replays the SAME committed byte stream from cursor 0, so
    // each is independently reproducible; the dice algorithm's byte consumption
    // is already pinned by `rng_executor_equivalence` and is not re-proven here.
    {
      name: 'rng_d20_single',
      rng: true,
      run: async () => {
        const { status, body } = await respond(await post(CHAT, 'rng', { type: 20 }));
        return { status, body, tables: await rngTables() };
      },
    },
    {
      name: 'rng_d6_three',
      rng: true,
      run: async () => {
        const { status, body } = await respond(await post(CHAT, 'rng', { type: 6, rolls: 3 }));
        return { status, body, tables: await rngTables() };
      },
    },
    {
      name: 'rng_flip_coin',
      rng: true,
      run: async () => {
        const { status, body } = await respond(await post(CHAT, 'rng', { type: 'flip_coin' }));
        return { status, body, tables: await rngTables() };
      },
    },
    {
      name: 'rng_flip_coin_multi',
      rng: true,
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'rng', { type: 'flip_coin', rolls: 3 }),
        );
        return { status, body, tables: await rngTables() };
      },
    },
    {
      name: 'rng_spin_the_bottle',
      rng: true,
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'rng', { type: 'spin_the_bottle' }),
        );
        return { status, body, tables: await rngTables() };
      },
    },
    {
      // Preview writes nothing — the message dump is the proof — and it is the
      // ONLY body carrying `summary` / `requestPrompt` / `arguments`.
      name: 'rng_preview_d20',
      rng: true,
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'rng', { type: 20, preview: true }),
        );
        return { status, body, tables: await rngTables() };
      },
    },
    {
      // A coin carries no `sum`, and `NextResponse.json` drops `undefined`, so
      // the key must be ABSENT rather than null.
      name: 'rng_preview_coin_multi',
      rng: true,
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'rng', { type: 'flip_coin', rolls: 4, preview: true }),
        );
        return { status, body, tables: await rngTables() };
      },
    },
    {
      name: 'rng_bad_kind',
      run: async () => respond(await post(CHAT, 'rng', { type: 1 })),
    },
    {
      name: 'rng_bad_rolls',
      run: async () => respond(await post(CHAT, 'rng', { type: 20, rolls: 0 })),
    },
    {
      name: 'rng_chat_missing',
      run: async () => respond(await post(MISSING_ID, 'rng', { type: 20 })),
    },
    // ── ?action=run-tool ────────────────────────────────────────────────────
    // The tools driven here are DB-only and deterministic on purpose:
    // `read_conversation` reads the fixture's own transcript, and an unknown
    // name lands on v4's `Unknown tool: …` failure result. Neither needs a
    // model, a network call, or the host environment.
    {
      name: 'run_tool_denied_submit_final_response',
      run: async () =>
        respond(await post(CHAT, 'run-tool', { toolName: 'submit_final_response' })),
    },
    {
      name: 'run_tool_denied_request_full_context',
      run: async () =>
        respond(await post(CHAT, 'run-tool', { toolName: 'request_full_context' })),
    },
    {
      name: 'run_tool_unknown_tool',
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'run-tool', { toolName: 'no_such_tool', arguments: { a: 1 } }),
        );
        return { status, body, tables: await rngTables() };
      },
    },
    {
      name: 'run_tool_read_conversation',
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'run-tool', {
            toolName: 'read_conversation',
            arguments: { interchanges: 2 },
          }),
        );
        return { status, body, tables: await rngTables() };
      },
    },
    {
      // `private: true` targets the OPERATOR's userId — a uuid that is not a
      // participant id, which is exactly how the whisper filters exclude it.
      name: 'run_tool_private',
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'run-tool', {
            toolName: 'read_conversation',
            arguments: { interchanges: 1 },
            private: true,
          }),
        );
        return { status, body, tables: await rngTables() };
      },
    },
    {
      // A named character selects THAT participant's context. `wardrobe_list` is
      // the discriminator: it lists the CALLING character's own wardrobe, and
      // ARIA and CLIO carry distinctly-titled items, so a participant-picking
      // bug is visible in the bytes.
      name: 'run_tool_named_character',
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'run-tool', {
            toolName: 'wardrobe_list',
            arguments: {},
            characterId: CLIO,
          }),
        );
        return { status, body, tables: await rngTables() };
      },
    },
    {
      // A characterId matching nobody yields NO participant — v4 still runs the
      // tool, with an empty context.
      name: 'run_tool_unmatched_character',
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'run-tool', {
            toolName: 'wardrobe_list',
            arguments: {},
            characterId: MISSING_ID,
          }),
        );
        return { status, body, tables: await rngTables() };
      },
    },
    {
      // No `characterId` → the FIRST active CHARACTER participant (ARIA), whose
      // wardrobe differs from CLIO's.
      name: 'run_tool_default_character',
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'run-tool', { toolName: 'wardrobe_list', arguments: {} }),
        );
        return { status, body, tables: await rngTables() };
      },
    },
    {
      name: 'run_tool_empty_name',
      run: async () => respond(await post(CHAT, 'run-tool', { toolName: '' })),
    },
    {
      name: 'run_tool_chat_missing',
      run: async () =>
        respond(await post(MISSING_ID, 'run-tool', { toolName: 'read_conversation' })),
    },
    // ── ?action=merge-conversation ──────────────────────────────────────────
    {
      // No allowlist: BEA + DORIAN travel (CLIO is already here → skipped;
      // FLINT is `removed` in the source → does not travel). Each join posts a
      // Host welcome, compiles that participant's identity stack, and applies
      // the default `previous_chat` outfit — BEA has one in the source, DORIAN
      // does not and falls back to their default item.
      name: 'merge_all',
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'merge-conversation', { sourceChatId: SOURCE_CHAT }),
        );
        return { status, body, tables: await mergeTables() };
      },
    },
    {
      // The operator's gate: only DORIAN comes across.
      name: 'merge_allowlist',
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'merge-conversation', {
            sourceChatId: SOURCE_CHAT,
            characterIds: [DORIAN],
          }),
        );
        return { status, body, tables: await mergeTables() };
      },
    },
    {
      // Explicit outfit selections: BEA gets `default`, DORIAN gets `none`.
      name: 'merge_outfit_selections',
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'merge-conversation', {
            sourceChatId: SOURCE_CHAT,
            outfitSelections: [
              { characterId: BEA, mode: 'default' },
              { characterId: DORIAN, mode: 'none' },
            ],
          }),
        );
        return { status, body, tables: await mergeTables() };
      },
    },
    {
      // Everyone chosen is already present → the no-op refusal, and NO bubbles.
      name: 'merge_all_already_present',
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'merge-conversation', {
            sourceChatId: SOURCE_CHAT,
            characterIds: [CLIO],
          }),
        );
        return { status, body, tables: await mergeTables() };
      },
    },
    {
      name: 'merge_into_itself',
      run: async () =>
        respond(await post(CHAT, 'merge-conversation', { sourceChatId: CHAT })),
    },
    {
      name: 'merge_empty_allowlist',
      run: async () =>
        respond(
          await post(CHAT, 'merge-conversation', {
            sourceChatId: SOURCE_CHAT,
            characterIds: [],
          }),
        ),
    },
    {
      name: 'merge_source_missing',
      run: async () =>
        respond(await post(CHAT, 'merge-conversation', { sourceChatId: MISSING_ID })),
    },
    {
      name: 'merge_chat_missing',
      run: async () =>
        respond(await post(MISSING_ID, 'merge-conversation', { sourceChatId: SOURCE_CHAT })),
    },
  ];

  const lines: string[] = [];
  for (const c of cases) {
    lines.push(JSON.stringify(await runCase(spec, c, scratch, fixtures)));
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(`wrote ${lines.length} chat-admin oracle rows to ${outPath}\n`);
}

test('chat-admin-routes oracle', async () => {
  await main();
});
