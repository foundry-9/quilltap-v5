/**
 * @jest-environment node
 *
 * P4.6a mutation-surface ORACLE: drives v4's REAL route handlers for the Salon
 * mutations — the three impersonation verbs, the turn action (query/nudge), and
 * the message edit / delete / swipe-switch — over a FRESH copy of the committed
 * salon fixture per case, emitting the response body + the affected MAIN-db table
 * dumps (`chats` / `chat_messages`) so the Rust ports (`api::salon::*`) can be
 * diffed byte-for-byte. Every case is zero-mint (skipUserTurn, which posts a Host
 * announcement, is deliberately excluded), so no id/timestamp remap is needed.
 *
 * Only the seams the Rust harness also neutralizes are mocked: the auth session,
 * the startup gate, and `Math.random` (pinned per case for the turn next-speaker
 * selection); the DB stack is the REAL cipher binding past jest.setup. Also mocks
 * the memory vector-store as REAL so the delete cascade's store load is genuine.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-salon-mutations-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp $V5W/harness/oracle/cases/salon-mutations.test.ts "$TMPO/cases/"
 *   cp $V5W/harness/oracle/fixtures/salon.json           "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SALON_MAIN=$V5W/crates/quilltap-web/tests/fixtures/salon-main.db \
 *   QT_FIXTURE_SALON_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/salon-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-salon-mutations.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- salon-mutations
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
  method: 'chatPost' | 'chatPut' | 'chatDelete' | 'messagePut' | 'messageDelete' | 'messagePost';
  url: string;
  paramId: string;
  body?: Record<string, unknown>;
  random01?: number;
}

const TABLES = [
  { key: 'chats', table: 'chats' },
  { key: 'chatMessages', table: 'chat_messages' },
];

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'POST',
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
  // The chat POST route transitively imports the npm-native markdown renderer
  // (via handlers/index → get.ts); stub it so jest can compile.
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

  const work = mkdtempSync(join(scratch, 'salon-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const origRandom = Math.random;
  if (c.random01 !== undefined) Math.random = () => c.random01!;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite');

  await initializeDatabase();

  try {
    const params = { params: Promise.resolve({ id: c.paramId }) };
    let response: { status: number; json: () => Promise<unknown> };
    if (c.method === 'chatPost') {
      const { POST } = await import('@/app/api/v1/chats/[id]/route');
      response = (await POST(mockRequest(c.url, c.body) as never, params as never)) as never;
    } else if (c.method === 'chatPut') {
      const { PUT } = await import('@/app/api/v1/chats/[id]/route');
      response = (await PUT(mockRequest(c.url, c.body) as never, params as never)) as never;
    } else if (c.method === 'chatDelete') {
      // Bug 25 (`bd419ae9`): stop-impersonate moved POST -> DELETE.
      const { DELETE } = await import('@/app/api/v1/chats/[id]/route');
      response = (await DELETE(mockRequest(c.url, c.body) as never, params as never)) as never;
    } else if (c.method === 'messagePut') {
      const { PUT } = await import('@/app/api/v1/messages/[id]/route');
      response = (await PUT(mockRequest(c.url, c.body) as never, params as never)) as never;
    } else if (c.method === 'messageDelete') {
      const { DELETE } = await import('@/app/api/v1/messages/[id]/route');
      response = (await DELETE(mockRequest(c.url, c.body) as never, params as never)) as never;
    } else {
      const { POST } = await import('@/app/api/v1/messages/[id]/route');
      response = (await POST(mockRequest(c.url, c.body) as never, params as never)) as never;
    }
    const status = response.status;
    const body = await response.json();

    const mdb = getRawDatabase();
    if (!mdb) throw new Error('main DB handle unavailable');
    const tables: Record<string, unknown> = {};
    for (const t of TABLES) {
      const columns = (
        mdb.prepare(`PRAGMA table_info(${t.table})`).all() as Array<{ name: string }>
      ).map((col) => col.name);
      const rawRows = mdb.prepare(`SELECT * FROM ${t.table}`).all() as Array<
        Record<string, unknown>
      >;
      const rows = rawRows.map((r) => {
        const out: Record<string, unknown> = {};
        for (const col of columns) out[col] = canonValue(r[col]);
        return out;
      });
      tables[t.key] = { table: t.table, columns, rows };
    }
    return { name: c.name, status, body, tables };
  } finally {
    Math.random = origRandom;
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'salon.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_SALON_MAIN ?? '',
    mount: process.env.QT_FIXTURE_SALON_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-salon-mut-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const GROUP = 'c1000000-0000-4000-8000-000000000002';
  const USER_P = 'b2000000-0000-4000-8000-000000000003'; // Cleo (controlledBy user)
  const LLM_P = 'b2000000-0000-4000-8000-000000000001'; // Aria (llm)
  const EDIT_MSG = 'd2000000-0000-4000-8000-000000000002'; // ASSISTANT (2 memories)
  const SWIPE_MSG = 'd2000000-0000-4000-8000-000000000005'; // swipeIndex 0 in a group

  const cbase = `http://localhost/api/v1/chats/${GROUP}`;
  const mbase = (id: string) => `http://localhost/api/v1/messages/${id}`;
  const cases: CaseSpec[] = [
    { name: 'impersonate', method: 'chatPost', url: `${cbase}?action=impersonate`, paramId: GROUP, body: { participantId: USER_P } },
    { name: 'stop_impersonate', method: 'chatDelete', url: `${cbase}?action=stop-impersonate`, paramId: GROUP, body: { participantId: USER_P } },
    { name: 'set_active_speaker', method: 'chatPost', url: `${cbase}?action=set-active-speaker`, paramId: GROUP, body: { participantId: USER_P } },
    { name: 'turn_query', method: 'chatPost', url: `${cbase}?action=turn`, paramId: GROUP, body: { action: 'query' }, random01: 0.42 },
    { name: 'turn_nudge', method: 'chatPost', url: `${cbase}?action=turn`, paramId: GROUP, body: { action: 'nudge', participantId: LLM_P }, random01: 0.42 },
    { name: 'message_edit', method: 'messagePut', url: mbase(EDIT_MSG), paramId: EDIT_MSG, body: { content: 'I stride toward the ridge, resolute.' } },
    { name: 'message_delete_confirm', method: 'messageDelete', url: `${mbase(EDIT_MSG)}`, paramId: EDIT_MSG },
    { name: 'message_delete_swipe', method: 'messageDelete', url: `${mbase(SWIPE_MSG)}`, paramId: SWIPE_MSG },
    { name: 'message_swipe_switch', method: 'messagePost', url: `${mbase(SWIPE_MSG)}?action=swipe`, paramId: SWIPE_MSG, body: { swipeIndex: 1 } },
    { name: 'chat_update', method: 'chatPut', url: cbase, paramId: GROUP, body: { chat: { isPaused: true, title: 'Paused Expedition' } } },
    // P4.6c: the full `chat` bag field set (every updateChatSchema column the bag
    // can carry, minus the gated roleplayTemplateId/projectId) — one raw update,
    // no updatedAt mint, so still zero-mint.
    {
      name: 'chat_update_broad',
      method: 'chatPut',
      url: cbase,
      paramId: GROUP,
      body: {
        chat: {
          title: 'Broadly Reconfigured',
          contextSummary: 'A terse recap.',
          isManuallyRenamed: true,
          documentEditingMode: true,
          allowCrossCharacterVaultReads: true,
          coreWhisperEnabled: false,
          coreWhisperInterval: 5,
          turnSkippingEnabled: false,
          showThinking: true,
          answerConfirmationOverride: 'OFF',
          documentMode: 'split',
          dividerPosition: 60,
          terminalMode: 'focus',
          activeTerminalSessionId: null,
          rightPaneVerticalSplit: 40,
          alertCharactersOfLanternImages: true,
          imageProfileId: null,
        },
      },
    },
    // P4.6c: the existence gates (404) — a non-null roleplayTemplateId / projectId
    // that doesn't resolve.
    { name: 'chat_update_roleplay_404', method: 'chatPut', url: cbase, paramId: GROUP, body: { chat: { roleplayTemplateId: '99999999-9999-4999-8999-999999999999' } } },
    { name: 'chat_update_project_404', method: 'chatPut', url: cbase, paramId: GROUP, body: { chat: { projectId: '99999999-9999-4999-8999-999999999999' } } },
    // P4.d13 (episodic spine, tier 2): the timelineMode PUT accept arm —
    // z.enum(['realtime','narrative']).nullish(). Set, explicit-null clear,
    // and the invalid-enum parse failure (whatever the route yields for a
    // thrown Zod parse — pinned by capture, mirrored by v5).
    { name: 'chat_update_timeline_set', method: 'chatPut', url: cbase, paramId: GROUP, body: { chat: { timelineMode: 'narrative' } } },
    { name: 'chat_update_timeline_null', method: 'chatPut', url: cbase, paramId: GROUP, body: { chat: { timelineMode: null } } },
    { name: 'chat_update_timeline_invalid', method: 'chatPut', url: cbase, paramId: GROUP, body: { chat: { timelineMode: 'dreamtime' } } },
    // P4.D141 (v4 `60e3c4a0a`): the `conciergeState` arm of the chat PUT — a
    // SIBLING of `chat`, routed through `applyConciergeFlip`. Each accepted value
    // writes the stored pair AND posts a Concierge bubble; the no-op writes
    // nothing; the two refusals are `chatUpdateRequestSchema.parse` throwing
    // before `processChatUpdates` runs, so nothing is written at all.
    { name: 'chat_update_concierge_flagged', method: 'chatPut', url: cbase, paramId: GROUP, body: { conciergeState: 'flagged' } },
    { name: 'chat_update_concierge_vouched', method: 'chatPut', url: cbase, paramId: GROUP, body: { conciergeState: 'vouched' } },
    { name: 'chat_update_concierge_uncensored', method: 'chatPut', url: cbase, paramId: GROUP, body: { conciergeState: 'uncensored' } },
    // The seeded chat is already Monitored, so this is the no-op arm.
    { name: 'chat_update_concierge_noop', method: 'chatPut', url: cbase, paramId: GROUP, body: { conciergeState: 'monitored' } },
    // `'off'` is the RETIRED tri-state spelling — the most valuable invalid value
    // there is, because a port that forgot to widen the enum would accept it.
    { name: 'chat_update_concierge_invalid', method: 'chatPut', url: cbase, paramId: GROUP, body: { conciergeState: 'off' } },
    // `.optional()` is not `.nullish()`: an explicit null is a ZodError too.
    { name: 'chat_update_concierge_null', method: 'chatPut', url: cbase, paramId: GROUP, body: { conciergeState: null } },
    // A wrong TYPE must reach the same Zod refusal, not a transport-level decode
    // error (the P4.60 wrong-type-collapse convention).
    { name: 'chat_update_concierge_wrong_type', method: 'chatPut', url: cbase, paramId: GROUP, body: { conciergeState: 42 } },
    // Both families in one request: the `chat` bag is applied first, then the flip.
    { name: 'chat_update_concierge_with_bag', method: 'chatPut', url: cbase, paramId: GROUP, body: { chat: { title: 'Vouched And Renamed' }, conciergeState: 'vouched' } },
    // GUARD ORDER — the arm that matters most. v4 parses the WHOLE body before
    // `processChatUpdates` runs, so an invalid `conciergeState` refuses the
    // request with the `chat` bag UNWRITTEN. A port that validated the state at
    // the flip (after the bag write) would rename the chat and then 400.
    { name: 'chat_update_concierge_invalid_with_bag', method: 'chatPut', url: cbase, paramId: GROUP, body: { chat: { title: 'Should Not Land' }, conciergeState: 'off' } },
    { name: 'message_delete_cascade', method: 'messageDelete', url: `${mbase(EDIT_MSG)}?memoryAction=DELETE_MEMORIES`, paramId: EDIT_MSG },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`salon-mutations oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('salon-mutations oracle', async () => {
  await main();
});
