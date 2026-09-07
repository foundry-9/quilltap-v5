/**
 * @jest-environment node
 *
 * P4.80 CHAT-DELETE route-surface ORACLE (dogfood finding #117): drives v4's
 * REAL `handleDelete` through v4's own route module
 * (`app/api/v1/chats/[id]/route.ts` → `handlers/delete.ts`), so the whole
 * dispatch is v4's — the `?action=` classification, the pre-body 404, the Zod
 * parse, the unknown-action refusal, and underneath them the REAL
 * `ChatsRepository.delete` cascade + `removeConversationSummariesFromVaults`.
 *
 * Each case runs against a FRESH copy of the committed
 * `chat-delete-{main,mount,llmlogs}.db` family, then dumps a whole-DB TABLE
 * CENSUS across all three partitions. **The census is the discriminator**, not
 * the body: `{success: true}` says nothing about what the cascade reached, and
 * the entire question of this family is what v4 deletes versus what it
 * deliberately leaves standing (memories, `llm_logs`, `background_jobs`,
 * `conversation_chunks`, `chat_documents`, `files`, `characters.avatarOverrides`,
 * the Scriptorium render's mount-index rows).
 *
 * The census SQL lives in `chat-delete-web.json` and is executed VERBATIM on
 * both sides, so neither side can quietly select a different projection.
 *
 * The clock is TICKING-frozen from NOW_MS (each argless `new Date()` /
 * `Date.now()` advances 1 ms), and the whole file **must run under `TZ=UTC`**
 * — the house convention for every clock-touching oracle.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-cd-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/chat-delete-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/chat-delete-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/chat-delete-main.db \
 *   QT_FIXTURE_CD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/chat-delete-mount.db \
 *   QT_FIXTURE_CD_LLMLOGS=$V5W/crates/quilltap-web/tests/fixtures/chat-delete-llmlogs.db \
 *   QT_ORACLE_OUT=/tmp/oracle-chat-delete.ndjson TZ=UTC \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "chat-delete-routes\.test\.ts$"
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
  censusMain: Record<string, string>;
  censusMount: Record<string, string>;
  censusLlmLogs: Record<string, string>;
  /** Character ids whose `Conversation Summaries/` folder is listed per case. */
  vaultCharacters: string[];
}

// Pinned ids (shared verbatim with the builder + the Rust differential).
const CHAT_FULL = 'c2000000-0000-4000-8000-000000000001';
const CHAT_SHARED = 'c2000000-0000-4000-8000-000000000002';
const CHAT_BROKEN = 'c2000000-0000-4000-8000-000000000003';
const CHAT_STATE = 'c2000000-0000-4000-8000-000000000004';
const CHAT_IMP = 'c2000000-0000-4000-8000-000000000005';
const MISSING_ID = '99999999-9999-4999-8999-999999999999';

const P_IMP_CLIO = 'e2000000-0000-4000-8000-000000000042';
const P_UNKNOWN = 'e2000000-0000-4000-8000-0000000000de';
const CONN_PROFILE = '93000000-0000-4000-8000-000000000001';

const RealDate = Date;

/** A body `req.json()` cannot parse (empty / not JSON) — the SyntaxError arm. */
const UNPARSEABLE = Symbol('unparseable');

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'DELETE',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    // `undefined` (no body given) → `{}` as before; an EXPLICIT `null` must
    // survive, so the non-object rows measure Zod's root-level refusal.
    json:
      body === UNPARSEABLE
        ? jest.fn().mockRejectedValue(new SyntaxError('Unexpected end of JSON input'))
        : jest.fn().mockResolvedValue(body === undefined ? {} : body),
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
  jest.doMock('@/lib/file-storage/conversation-summary-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/conversation-summary-vault-bridge'),
  );
  // ⚠ `jest.setup.ts` mocks the CHARACTER-vault bridge globally, and its stub
  // answers a fake `mock-vault-mount` for every character — which makes
  // `findExistingSummaryPaths` list an EMPTY folder and the summary sweep a
  // silent no-op. Without this line the whole vault half of this family would
  // measure nothing and pass (the `jest-oracle-*` mock-leak class; caught here
  // by a probe when the mount census did not move on `delete_full`).
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
  );
  jest.doMock('@/lib/mount-index/character-vault', () =>
    jest.requireActual('@/lib/mount-index/character-vault'),
  );
  jest.doMock('@/lib/mount-index/database-store', () =>
    jest.requireActual('@/lib/mount-index/database-store'),
  );
  // The chat route pulls the markdown renderer (unified/ESM) transitively — mock
  // it away; no case under test renders HTML.
  jest.doMock('@/lib/services/markdown-renderer.service', () => ({
    __esModule: true,
    renderMarkdownToHtml: async () => null,
    canPreRenderMessage: () => false,
  }));
  // Keep the background-jobs processor OFF so it can't claim the seeded row and
  // race the census (the P4.6y lesson).
  jest.doMock('@/lib/background-jobs/processor', () => ({
    __esModule: true,
    ensureProcessorRunning: () => {},
    startProcessor: () => {},
    stopProcessor: () => {},
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

interface CaseSpec {
  name: string;
  action?: string;
  chatId: string;
  body?: unknown;
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

const CHAT_ROUTE = '@/app/api/v1/chats/[id]/route';

/**
 * The whole-DB census, executed from the SAME SQL both sides read. Run over the
 * raw encrypted handles rather than the repositories, so a row a repository
 * would hide (a schema-invalid one, say) still shows up.
 */
async function census(spec: Spec): Promise<Record<string, unknown>> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawLLMLogsDatabase } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const run = (
    db: { prepare: (s: string) => { all: () => unknown[] } } | null,
    sqls: Record<string, string>,
  ): Record<string, unknown> => {
    const out: Record<string, unknown> = {};
    for (const [table, sql] of Object.entries(sqls)) {
      if (!db) throw new Error(`no handle for ${table}`);
      out[table] = db.prepare(sql).all();
    }
    return out;
  };
  // The vault listing goes through the STORE READER on both sides
  // (`listDatabaseFiles` here, `list_database_files` in the port), not through
  // the raw tables above — a reader that stopped resolving a folder would still
  // leave the rows in place, so this is the arm the table census cannot make.
  const { listDatabaseFiles } = await import('@/lib/mount-index/database-store');
  const vaults: Record<string, unknown> = {};
  // The vault POINTER comes off the raw `characters` row, never through
  // `repos.characters.findById` — the hydrated read throws
  // `CharacterVaultUnavailableError` for the broken-vault character, which is
  // the one this section most needs to look at. v5's own
  // `resolve_vault_mount_id` reads it raw for exactly that reason.
  const rawMain = getRawDatabase() as unknown as {
    prepare: (s: string) => { get: (...a: unknown[]) => { p?: string | null } | undefined };
  } | null;
  if (!rawMain) throw new Error('no main handle for the vault listing');
  for (const characterId of spec.vaultCharacters) {
    const row = rawMain
      .prepare('SELECT "characterDocumentMountPointId" AS p FROM "characters" WHERE "id" = ?')
      .get(characterId);
    const mountPointId = row?.p ?? null;
    if (!mountPointId) {
      vaults[characterId] = null;
      continue;
    }
    let entries: Array<{ relativePath: string; kind: string }>;
    try {
      entries = (await listDatabaseFiles(mountPointId, {
        folder: 'Conversation Summaries',
      } as never)) as never;
    } catch {
      // A vault whose mount point has no row: the reader refuses, and BOTH
      // sides must refuse the same way (MOTE, after the fixture repoints her).
      vaults[characterId] = 'unreadable';
      continue;
    }
    vaults[characterId] = entries
      .filter((e) => e.kind !== 'folder')
      .map((e) => e.relativePath)
      .sort();
  }

  return {
    main: run(getRawDatabase() as never, spec.censusMain),
    mount: run(getRawMountIndexDatabase() as never, spec.censusMount),
    llmLogs: run(getRawLLMLogsDatabase() as never, spec.censusLlmLogs),
    vaults,
  };
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string; llmLogs: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'cd-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  const llmWork = join(work, 'llmlogs.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  copyFileSync(fixtures.llmLogs, llmWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.SQLITE_LLM_LOGS_PATH = llmWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  await initializeDatabase();

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
    const route = (await import(CHAT_ROUTE)) as unknown as {
      DELETE: (...a: unknown[]) => Promise<unknown>;
    };
    const qs = c.action === undefined ? '' : `?action=${c.action}`;
    const url = `http://localhost/api/v1/chats/${c.chatId}${qs}`;
    const { status, body } = await respond(
      await route.DELETE(mockRequest(url, c.body), {
        params: Promise.resolve({ id: c.chatId }),
      }),
    );
    return { name: c.name, status, body, tables: await census(spec) };
  } finally {
    global.Date = RealDate;
    await closeDatabase();
    closeMountIndexSQLiteClient();
    closeLLMLogsSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(`chat-delete-routes oracle must run under TZ=UTC (getTimezoneOffset=${offset})`);
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'chat-delete-web.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_CD_MAIN ?? '',
    mount: process.env.QT_FIXTURE_CD_MOUNT ?? '',
    llmLogs: process.env.QT_FIXTURE_CD_LLMLOGS ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cd-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cases: CaseSpec[] = [
    // ── the delete itself ────────────────────────────────────────────────────
    // The happy path over the chat that carries a row in every chat-keyed
    // table. The census says which of them the cascade reached.
    { name: 'delete_full', chatId: CHAT_FULL },
    // Missing chat: 404 and NOTHING written (the census is the proof).
    { name: 'delete_missing', chatId: MISSING_ID },
    // MOTE's vault pointer is dangling, so the summary sweep cannot find her
    // file. v4 catches per character and the delete still succeeds — the
    // orphaned document rows survive on both sides.
    { name: 'delete_broken_vault', chatId: CHAT_BROKEN },
    // ARIA sits in BOTH chats, so her vault holds two summary files. Deleting
    // one must leave the other's alone — the frontmatter `conversationId` match,
    // not a folder wipe.
    { name: 'delete_shared_summary', chatId: CHAT_SHARED },

    // ── ?action=reset-state ──────────────────────────────────────────────────
    { name: 'reset_state', action: 'reset-state', chatId: CHAT_STATE },
    { name: 'reset_state_missing', action: 'reset-state', chatId: MISSING_ID },

    // ── ?action=stop-impersonate ─────────────────────────────────────────────
    {
      name: 'stop_impersonate',
      action: 'stop-impersonate',
      chatId: CHAT_IMP,
      body: { participantId: P_IMP_CLIO },
    },
    {
      name: 'stop_impersonate_with_profile',
      action: 'stop-impersonate',
      chatId: CHAT_IMP,
      body: { participantId: P_IMP_CLIO, newConnectionProfileId: CONN_PROFILE },
    },
    {
      name: 'stop_impersonate_unknown_profile',
      action: 'stop-impersonate',
      chatId: CHAT_IMP,
      body: { participantId: P_IMP_CLIO, newConnectionProfileId: MISSING_ID },
    },
    {
      name: 'stop_impersonate_unknown_participant',
      action: 'stop-impersonate',
      chatId: CHAT_IMP,
      body: { participantId: P_UNKNOWN },
    },
    {
      name: 'stop_impersonate_missing_chat',
      action: 'stop-impersonate',
      chatId: MISSING_ID,
      body: { participantId: P_IMP_CLIO },
    },
    // THE GUARD ORDER: v4 fetches the chat BEFORE `handleStopImpersonate` reads
    // the body, so an invalid body against a missing chat is a 404, not a 400.
    {
      name: 'stop_impersonate_missing_chat_bad_body',
      action: 'stop-impersonate',
      chatId: MISSING_ID,
      body: {},
    },
    // The Zod refusals, on a chat that exists — v4's `validationError` envelope.
    {
      name: 'stop_impersonate_no_participant',
      action: 'stop-impersonate',
      chatId: CHAT_IMP,
      body: {},
    },
    {
      name: 'stop_impersonate_bad_uuid',
      action: 'stop-impersonate',
      chatId: CHAT_IMP,
      body: { participantId: 'not-a-uuid' },
    },
    // BOTH fields wrong: `z.object` collects every field's first failing check,
    // so this answers TWO issues in declaration order.
    {
      name: 'stop_impersonate_both_fields_bad',
      action: 'stop-impersonate',
      chatId: CHAT_IMP,
      body: { participantId: 7, newConnectionProfileId: null },
    },

    // ── the unknown-action refusal, and the empty-action fall-through ────────
    // v4 refuses rather than deleting: "prevent accidental chat deletion".
    // Zod 4.5.4's `z.object` refuses a NON-object body before any field walk,
    // with ONE root-path issue (the §3 unification review's catch).
    { name: 'stop_impersonate_null_body', action: 'stop-impersonate', chatId: CHAT_IMP, body: null },
    { name: 'stop_impersonate_array_body', action: 'stop-impersonate', chatId: CHAT_IMP, body: [] },
    // An UNREADABLE body: 500 on the leg that reads it, AFTER its chat gate.
    { name: 'stop_impersonate_empty_body', action: 'stop-impersonate', chatId: CHAT_IMP, body: UNPARSEABLE },
    { name: 'stop_impersonate_missing_chat_empty_body', action: 'stop-impersonate', chatId: MISSING_ID, body: UNPARSEABLE },
    { name: 'action_bogus', action: 'zzz', chatId: CHAT_FULL },
    // `?action=` is present but EMPTY, which is JS-falsy — so it takes the
    // no-action leg and the chat IS DELETED. The census is what proves it.
    { name: 'action_empty', action: '', chatId: CHAT_FULL },
  ];

  const lines: string[] = [];
  for (const c of cases) {
    lines.push(JSON.stringify(await runCase(spec, c, scratch, fixtures)));
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(`wrote ${lines.length} chat-delete oracle rows to ${outPath}\n`);
}

test('chat-delete-routes oracle', async () => {
  await main();
});
