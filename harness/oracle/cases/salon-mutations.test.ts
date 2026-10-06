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
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures" "$TMPO/lib"
 *   cp $V5W/harness/oracle/cases/salon-mutations.test.ts "$TMPO/cases/"
 *   cp $V5W/harness/oracle/lib/p4d171-columns.ts "$TMPO/lib/"
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
import { ensureP4D171Columns } from '../lib/p4d171-columns';

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
  /** P4.106 item 5: plant the three `chat_informs` rows below on the copy
   * (through v4's REAL repository) and dump the table. */
  plantInforms?: boolean;
  /**
   * P4.149 (Ruling R-F): the chat PUT's project gate (`helpers.ts:504-509`,
   * `repos.projects.findById`, store-backed). `dbError` renames the slim
   * `projects.id` column on the copy, so `_findById`'s fallback read fails
   * (ERROR `Error finding entity by ID {collection: projects, id, error}` →
   * `null` → 404 `Project not found`); `storeCorrupt` writes `properties.json`
   * = `{` into the project's store (the `get_store_corrupt` recipe), so
   * `applyOverlayOne` throws → the middleware's 503 (`context.ts:176-185`).
   * Either case records the ERROR/WARN lines v4 logged (`logs`).
   */
  projectPlant?: 'dbError' | 'storeCorrupt' | 'mountDbError' | 'mountUnavailable';
  /** P4.156 (R-F): with `projectPlant: 'mountDbError'`, the MOUNT-INDEX column
   * renamed on the copy (v4's own raw mount-index handle, after
   * `initializeDatabase()`) — the plant the project store's reads fail on. */
  renameMountColumn?: { table: string; from: string; to: string };
}

/** P4.149: the fixture's one project (`salon.json`'s `projects[0]`). */
const SKYHAVEN = '70000002-0000-4000-8000-000000000001';

/**
 * P4.106 item 5 — the participant-removal rows' plant: a PENDING and a
 * CONSUMED inform for the seat being removed (Aria) and a PENDING one for the
 * other LLM seat. Ids and stamps pinned; v5 plants the same cells through its
 * own `ChatInformsRepository::create`.
 *
 * P4.D249 (v4 `52d6e7ecd`) adds a fourth row: a DELIVERED STANDING inform for
 * Aria. It is still in force, so the remove-participant action's
 * `deletePendingForParticipant` now takes it too (no code hunk in v4 — the
 * method reads through the in-force predicate), while the consumed one-shot
 * (`aaa2`) still survives.
 */
const INFORM_PLANT = [
  { id: '11110000-0000-4000-8000-00000000aaa1', participantId: 'b2000000-0000-4000-8000-000000000001', consumedAt: null, consumedByMessageId: null },
  { id: '11110000-0000-4000-8000-00000000aaa2', participantId: 'b2000000-0000-4000-8000-000000000001', consumedAt: '2026-02-02T00:00:00.000Z', consumedByMessageId: 'd2000000-0000-4000-8000-000000000002' },
  { id: '11110000-0000-4000-8000-00000000aaa3', participantId: 'b2000000-0000-4000-8000-000000000002', consumedAt: null, consumedByMessageId: null },
  { id: '11110000-0000-4000-8000-00000000aaa4', participantId: 'b2000000-0000-4000-8000-000000000001', consumedAt: '2026-02-02T00:00:00.000Z', consumedByMessageId: 'd2000000-0000-4000-8000-000000000002', permanent: true },
];

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

  // P4.D172: heal the fixture copy — see the helper's own note.
  ensureP4D171Columns(getRawDatabase() as never);

  if (c.plantInforms) {
    const { ensureCollection } = await import('@/lib/database/manager');
    const { ChatInformsRepository } = await import(
      '@/lib/database/repositories/chat-informs.repository'
    );
    const { ChatInformSchema } = await import('@/lib/schemas/chat-inform.types');
    await ensureCollection('chat_informs', ChatInformSchema);
    const informs = new ChatInformsRepository();
    for (const row of INFORM_PLANT) {
      await informs.create(
        {
          chatId: 'c1000000-0000-4000-8000-000000000002',
          batchId: 'bbbbbbbb-0000-4000-8000-000000000001',
          participantId: row.participantId,
          contentMarkdown: 'The clock in the hall has stopped.',
          recordMessageId: null,
          ...('permanent' in row ? { permanent: row.permanent } : {}),
          consumedAt: row.consumedAt,
          consumedByMessageId: row.consumedByMessageId,
        } as never,
        { id: row.id, createdAt: '2026-02-01T00:00:00.000Z', updatedAt: '2026-02-01T00:00:00.000Z' },
      );
    }
  }

  if (c.projectPlant === 'dbError') {
    getRawDatabase()!.prepare('ALTER TABLE projects RENAME COLUMN id TO id_x').run();
  }
  if (c.projectPlant === 'mountDbError') {
    const { getRawMountIndexDatabase } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    const midb = getRawMountIndexDatabase();
    if (!midb) throw new Error('raw mount-index handle unavailable');
    const { table, from, to } = c.renameMountColumn!;
    midb.exec(`ALTER TABLE "${table}" RENAME COLUMN "${from}" TO "${to}"`);
  }
  // P4.156 (R-F): the mount-index database cannot be ACQUIRED — v4's
  // `withRawDb` then answers every overlay read its fallback QUIETLY (a DEBUG,
  // `dedicated-db.repository.ts:242-251`). Closing the client is v4's own way
  // to that state; v5's twin is a read-pool checkout that fails.
  if (c.projectPlant === 'mountUnavailable') {
    closeMountIndexSQLiteClient();
  }
  if (c.projectPlant === 'storeCorrupt') {
    const rows = getRawDatabase()!
      .prepare('SELECT officialMountPointId AS mp FROM projects WHERE id = ?')
      .all(SKYHAVEN) as Array<{ mp: string | null }>;
    const mp = rows[0]?.mp;
    if (!mp) throw new Error('Skyhaven has no officialMountPointId');
    const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
    await writeDatabaseDocument(mp, 'properties.json', '{');
  }
  // P4.149: the lines a plant case logs, off the `Logger` prototype (root and
  // children alike), restored in `finally`.
  const logs: Array<{ level: string; message: string; context: unknown }> = [];
  const restoreLogger: Array<() => void> = [];
  if (c.projectPlant) {
    const { Logger } = await import('@/lib/logger');
    // P4.156 (R-F): the unavailable-mount case's lines are DEBUGs (v4's quiet
    // `withRawDb` arm), so that case records `debug` too.
    const levels =
      c.projectPlant === 'mountUnavailable'
        ? (['error', 'warn', 'debug'] as const)
        : (['error', 'warn'] as const);
    for (const level of levels) {
      const original = Logger.prototype[level];
      Logger.prototype[level] = function (
        this: unknown,
        message: string,
        context?: Record<string, unknown>,
        ...rest: unknown[]
      ) {
        logs.push({ level, message, context: JSON.parse(JSON.stringify(context ?? {})) });
        return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
      } as never;
      restoreLogger.push(() => {
        Logger.prototype[level] = original;
      });
    }
  }

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
    const tableList = c.plantInforms
      ? [...TABLES, { key: 'chatInforms', table: 'chat_informs' }]
      : TABLES;
    for (const t of tableList) {
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
    return { name: c.name, status, body, tables, ...(c.projectPlant ? { logs } : {}) };
  } finally {
    for (const restore of restoreLogger) restore();
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
  scratchDirs.push(scratch);
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
    // P4.149 (Ruling R-F): the project gate's two failure arms.
    { name: 'chat_update_project_db_error', method: 'chatPut', url: cbase, paramId: GROUP, body: { chat: { projectId: SKYHAVEN } }, projectPlant: 'dbError' },
    { name: 'chat_update_project_store_corrupt', method: 'chatPut', url: cbase, paramId: GROUP, body: { chat: { projectId: SKYHAVEN } }, projectPlant: 'storeCorrupt' },
    // P4.156 (R-F): the gate's MOUNT side under a broken mount index — the
    // store's document read (`doc_mount_file_links.relativePath`) and its mount
    // point row (`doc_mount_points.id`) each renamed on the copy.
    { name: 'chat_update_project_mount_links_error', method: 'chatPut', url: cbase, paramId: GROUP, body: { chat: { projectId: SKYHAVEN } }, projectPlant: 'mountDbError', renameMountColumn: { table: 'doc_mount_file_links', from: 'relativePath', to: 'relativePath_x' } },
    { name: 'chat_update_project_mount_unavailable', method: 'chatPut', url: cbase, paramId: GROUP, body: { chat: { projectId: SKYHAVEN } }, projectPlant: 'mountUnavailable' },
    { name: 'chat_update_project_mount_points_error', method: 'chatPut', url: cbase, paramId: GROUP, body: { chat: { projectId: SKYHAVEN } }, projectPlant: 'mountDbError', renameMountColumn: { table: 'doc_mount_points', from: 'id', to: 'id_x' } },
    // P4.d13 (episodic spine, tier 2): the timelineMode PUT accept arm —
    // z.enum(['realtime','narrative']).nullish(). Set, explicit-null clear,
    // and the invalid-enum parse failure (whatever the route yields for a
    // thrown Zod parse — pinned by capture, mirrored by v5).
    { name: 'chat_update_timeline_set', method: 'chatPut', url: cbase, paramId: GROUP, body: { chat: { timelineMode: 'narrative' } } },
    { name: 'chat_update_timeline_null', method: 'chatPut', url: cbase, paramId: GROUP, body: { chat: { timelineMode: null } } },
    { name: 'chat_update_timeline_invalid', method: 'chatPut', url: cbase, paramId: GROUP, body: { chat: { timelineMode: 'dreamtime' } } },
    // P4.D141 (v4 `60e3c4a0a`), re-keyed by P4.D226 (v4 `4d370a90f`, the three
    // states): the `conciergeState` arm of the chat PUT — a SIBLING of `chat`,
    // routed through `applyConciergeFlip`. Each accepted value writes the three
    // Concierge columns (`setConciergeMode`, no `updatedAt` mint) AND posts a
    // Concierge bubble; the no-op writes nothing; every refusal is
    // `chatUpdateRequestSchema.parse` throwing before `processChatUpdates` runs,
    // so nothing is written at all.
    { name: 'chat_update_concierge_unmoderated', method: 'chatPut', url: cbase, paramId: GROUP, body: { conciergeState: 'unmoderated' } },
    { name: 'chat_update_concierge_locked', method: 'chatPut', url: cbase, paramId: GROUP, body: { conciergeState: 'locked' } },
    // The seeded chat is already Moderated (the migration's default), so this is
    // the no-op arm.
    { name: 'chat_update_concierge_noop', method: 'chatPut', url: cbase, paramId: GROUP, body: { conciergeState: 'moderated' } },
    // The four RETIRED four-state values are refused with 400 (v4 `schemas.ts`:
    // "rejected with 400").
    { name: 'chat_update_concierge_retired_monitored', method: 'chatPut', url: cbase, paramId: GROUP, body: { conciergeState: 'monitored' } },
    { name: 'chat_update_concierge_retired_flagged', method: 'chatPut', url: cbase, paramId: GROUP, body: { conciergeState: 'flagged' } },
    { name: 'chat_update_concierge_retired_vouched', method: 'chatPut', url: cbase, paramId: GROUP, body: { conciergeState: 'vouched' } },
    { name: 'chat_update_concierge_retired_uncensored', method: 'chatPut', url: cbase, paramId: GROUP, body: { conciergeState: 'uncensored' } },
    // `'off'` is the RETIRED tri-state spelling — the most valuable invalid value
    // there is, because a port that forgot to widen the enum would accept it.
    { name: 'chat_update_concierge_invalid', method: 'chatPut', url: cbase, paramId: GROUP, body: { conciergeState: 'off' } },
    // `.optional()` is not `.nullish()`: an explicit null is a ZodError too.
    { name: 'chat_update_concierge_null', method: 'chatPut', url: cbase, paramId: GROUP, body: { conciergeState: null } },
    // A wrong TYPE must reach the same Zod refusal, not a transport-level decode
    // error (the P4.60 wrong-type-collapse convention).
    { name: 'chat_update_concierge_wrong_type', method: 'chatPut', url: cbase, paramId: GROUP, body: { conciergeState: 42 } },
    // Both families in one request: the `chat` bag is applied first, then the flip.
    { name: 'chat_update_concierge_with_bag', method: 'chatPut', url: cbase, paramId: GROUP, body: { chat: { title: 'Locked And Renamed' }, conciergeState: 'locked' } },
    // GUARD ORDER — the arm that matters most. v4 parses the WHOLE body before
    // `processChatUpdates` runs, so an invalid `conciergeState` refuses the
    // request with the `chat` bag UNWRITTEN. A port that validated the state at
    // the flip (after the bag write) would rename the chat and then 400.
    { name: 'chat_update_concierge_invalid_with_bag', method: 'chatPut', url: cbase, paramId: GROUP, body: { chat: { title: 'Should Not Land' }, conciergeState: 'off' } },
    { name: 'message_delete_cascade', method: 'messageDelete', url: `${mbase(EDIT_MSG)}?memoryAction=DELETE_MEMORIES`, paramId: EDIT_MSG },
    // P4.106 item 5 (the P4.D205 gap): the two participant-removal entrances,
    // over PLANTED informs. v4 drops the departing seat's PENDING rows in the
    // `?action=remove-participant` ROUTE only (`participants.ts:566-582`) —
    // the consumed row stays (a later swipe must still re-apply it) and the
    // other seat's row is untouched. The chat-PUT bag's `removeParticipantId`
    // reaches `helpers.ts::handleRemoveParticipant`, which carries NO drop.
    { name: 'remove_participant_action_drops_informs', method: 'chatPost', url: `${cbase}?action=remove-participant`, paramId: GROUP, body: { participantId: LLM_P }, plantInforms: true },
    { name: 'remove_participant_bag_keeps_informs', method: 'chatPut', url: cbase, paramId: GROUP, body: { removeParticipantId: LLM_P }, plantInforms: true },
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

// Remove the OS-temp scratch dir(s) once the oracle has written its NDJSON.
const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
