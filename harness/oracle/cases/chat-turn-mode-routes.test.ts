/**
 * @jest-environment node
 *
 * P4.D343 STORYTELLER-MODE route-surface ORACLE (tier 2): drives v4's REAL chat
 * routes at the pin — the chat PUT's `turnMode` sibling + the
 * `storytellerInviteFilter` bag key (`handlers/put.ts` → `schemas.ts`'s
 * `chatUpdateRequestSchema` → `helpers.ts::processChatUpdates`), the chat GET
 * wire (`handlers/get.ts`'s two keys after `isPaused`), and the four
 * Storyteller actions (`actions/storyteller.ts`: `GET ?action=storyteller-round`,
 * `POST ?action=storyteller-run | -ask | -lie`) — over a FRESH copy of the
 * committed salon fixture per case, emitting the response status + body, the
 * `chats` and `storyteller_rounds` dumps, and the Storyteller / turn-mode log
 * lines (captured by splicing `logger.transports`), so the Rust port
 * (`api::salon::chat_update` / `assemble_chat_get`, `api::chat_storyteller::*`)
 * diffs byte-for-byte.
 *
 * NO case reaches a model: the run / ask rows are the REFUSAL arms only (409 /
 * 400 / 404 and the ZodError `Invalid request` arm), every one of which v4
 * answers before `openTurnStream` / `bidForSeat`.
 *
 * Plants, identical cells both sides: `chats.turnMode` / `storytellerInviteFilter`
 * by a raw UPDATE on the copy; rounds through v4's REAL
 * `StorytellerRoundsRepository.create` with a pinned `{id, createdAt,
 * updatedAt}` (v5: `StorytellerRoundsRepository::create(…, id, now)`), a closed
 * round then through the REAL `markClosed` (its minted stamps are placeholdered
 * on both sides).
 *
 * Mocked seams (the salon-mutations set): the auth session, the startup gate,
 * the npm-native markdown renderer. The DB stack is the REAL cipher binding.
 *
 * Run (Node 24; jest ignores `.claude/`, so mirror the case; the fixture pair
 * is copied first so the committed pair is never written):
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   M=/tmp/qt-chat-turn-mode-mirror; rm -rf $M; mkdir -p $M/cases $M/fixtures $M/lib $M/db
 *   cp $V5W/harness/oracle/cases/chat-turn-mode-routes.test.ts $M/cases/
 *   cp $V5W/harness/oracle/lib/p4d171-columns.ts $M/lib/
 *   cp $V5W/harness/oracle/fixtures/salon.json $M/fixtures/
 *   cp $V5W/crates/quilltap-web/tests/fixtures/salon-main.db $M/db/
 *   cp $V5W/crates/quilltap-web/tests/fixtures/salon-mount.db $M/db/
 *   QT_FIXTURE_SALON_MAIN=$M/db/salon-main.db \
 *   QT_FIXTURE_SALON_MOUNT=$M/db/salon-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-chat-turn-mode-routes.ndjson \
 *     npx jest --silent --watchman=false --roots "$PWD" --roots "$M/cases" \
 *       --testTimeout=180000 --runTestsByPath "$M/cases/chat-turn-mode-routes.test.ts"
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

type Method = 'chatGet' | 'chatPut' | 'chatPost';

interface RoundPlant {
  id: string;
  chatId: string;
  createdAt: string;
  /** The operator post that opened it (production passes the send's message
   * id; `null` is the measured unreadable arm — see `round_null_trigger`). */
  triggerMessageId: string | null;
  bids: Array<Record<string, unknown>>;
  /** Close it through the REAL `markClosed` after the create. */
  closeAs?: 'let-lie' | 'ran' | 'superseded' | 'abandoned';
}

interface CaseSpec {
  name: string;
  method: Method;
  chatId: string;
  action?: string;
  body?: unknown;
  /** Raw cells on the case chat before the call. */
  turnMode?: string | null;
  inviteFilter?: number;
  rounds?: RoundPlant[];
}

const SOLO = 'c1000000-0000-4000-8000-000000000001';
const GROUP = 'c1000000-0000-4000-8000-000000000002';
const MISSING_CHAT = 'c1000000-0000-4000-8000-0000000000ff';
const ARIA_P = 'b2000000-0000-4000-8000-000000000001';
const BRAM_P = 'b2000000-0000-4000-8000-000000000002';
const QUIET_P = 'b2000000-0000-4000-8000-000000000009';

const OPEN_ROUND = '5e000000-0000-4000-8000-000000000001';
const CLOSED_ROUND = '5e000000-0000-4000-8000-000000000002';
const SOLO_ROUND = '5e000000-0000-4000-8000-000000000003';

/** Three cards: a Destroy bid, a Simple bid, a kept-quiet seat. Every key
 * present (the production `pendingCard` / `cardFromOutcome` shape). */
const BIDS: Array<Record<string, unknown>> = [
  {
    participantId: ARIA_P,
    characterId: 'a1000000-0000-4000-8000-000000000001',
    characterName: 'Aria',
    status: 'bid',
    statedTier: 'act',
    tier: 'destroy',
    tierRaised: true,
    tools: ['delete_file'],
    plan: 'Burn the map before Bram can read it.',
    reason: null,
    talkativeness: 0.7,
    silent: false,
    askedAt: '2026-02-03T00:00:00.000Z',
    answeredAt: '2026-02-03T00:00:01.000Z',
  },
  {
    participantId: BRAM_P,
    characterId: 'a1000000-0000-4000-8000-000000000002',
    characterName: 'Bram',
    status: 'bid',
    statedTier: 'simple',
    tier: 'simple',
    tierRaised: false,
    tools: [],
    plan: 'Grumble about the weather.',
    reason: null,
    talkativeness: 0.5,
    silent: false,
    askedAt: '2026-02-03T00:00:00.000Z',
    answeredAt: '2026-02-03T00:00:02.000Z',
  },
  {
    participantId: QUIET_P,
    characterId: 'a1000000-0000-4000-8000-000000000004',
    characterName: 'Dax',
    status: 'kept-quiet',
    statedTier: null,
    tier: null,
    tierRaised: false,
    tools: [],
    plan: null,
    reason: 'Kept quiet this round (talkativeness)',
    talkativeness: 0.1,
    silent: true,
    askedAt: null,
    answeredAt: null,
  },
];

const TRIGGER = 'd2000000-0000-4000-8000-000000000004';
const openRound = (chatId = GROUP, id = OPEN_ROUND, createdAt = '2026-02-03T00:00:00.000Z'): RoundPlant => ({
  id,
  chatId,
  createdAt,
  triggerMessageId: TRIGGER,
  bids: BIDS,
});
const closedRound = (closeAs: RoundPlant['closeAs'] = 'let-lie'): RoundPlant => ({
  id: CLOSED_ROUND,
  chatId: GROUP,
  createdAt: '2026-02-02T00:00:00.000Z',
  triggerMessageId: TRIGGER,
  bids: BIDS,
  closeAs,
});

/** The lines this family owns (v5 logs the same set). */
const LINE_PREFIXES = [
  '[Chats v1] Turn mode set',
  'Storyteller round closed unrun',
  'Storyteller round let lie',
  '[Storyteller] ',
];
let capturedLines: string[] = [];

function renderField(k: string, v: unknown): string {
  if (v === null || v === undefined) return ` ${k}=null`;
  if (Array.isArray(v) || typeof v === 'object') return ` ${k}Json=${JSON.stringify(v)}`;
  return ` ${k}=${String(v)}`;
}

async function installCapture(): Promise<void> {
  capturedLines = [];
  const { logger } = await import('@/lib/logger');
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as {
        level: string;
        message: string;
        context?: Record<string, unknown>;
        error?: { message?: string };
      };
      if (!LINE_PREFIXES.some((p) => data.message.startsWith(p))) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      let line = `${data.level.toUpperCase()} ${data.message}`;
      for (const [k, v] of Object.entries(fields)) line += renderField(k, v);
      if (data.error) line += ` error=${data.error.message}`;
      capturedLines.push(line);
    },
  });
}

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

function mockRequest(method: string, url: string, body?: unknown): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body === undefined ? {} : body),
  };
}

function dump(mdb: { prepare: (s: string) => { all: () => unknown[] } }, table: string) {
  const columns = (mdb.prepare(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>).map(
    (col) => col.name,
  );
  const rawRows = mdb.prepare(`SELECT * FROM ${table} ORDER BY id`).all() as Array<Record<string, unknown>>;
  const rows = rawRows.map((r) => {
    const out: Record<string, unknown> = {};
    for (const col of columns) out[col] = canonValue(r[col]);
    return out;
  });
  return { table, columns, rows };
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
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () => jest.requireActual('@/lib/embedding/vector-store'));
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

  const work = mkdtempSync(join(scratch, 'turn-mode-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite');

  await initializeDatabase();
  ensureP4D171Columns(getRawDatabase() as never);
  // A re-initialised manager has forgotten the collection's schema (its JSON
  // columns), so every read would fail the round schema and drop silently;
  // production's boot registers it — do the same here.
  {
    const { ensureCollection } = await import('@/lib/database/manager');
    const { StorytellerRoundSchema } = await import('@/lib/schemas/storyteller.types');
    await ensureCollection('storyteller_rounds', StorytellerRoundSchema);
  }
  const mdb = getRawDatabase() as unknown as {
    prepare: (s: string) => { run: (...a: unknown[]) => unknown; all: (...a: unknown[]) => unknown[] };
  };

  if (c.turnMode !== undefined) {
    mdb.prepare('UPDATE chats SET "turnMode" = ? WHERE id = ?').run(c.turnMode, c.chatId);
  }
  if (c.inviteFilter !== undefined) {
    mdb.prepare('UPDATE chats SET "storytellerInviteFilter" = ? WHERE id = ?').run(c.inviteFilter, c.chatId);
  }
  if (c.rounds) {
    const { StorytellerRoundsRepository } = await import(
      '@/lib/database/repositories/storyteller-rounds.repository'
    );
    const repo = new StorytellerRoundsRepository();
    for (const r of c.rounds) {
      await repo.create(
        { chatId: r.chatId, triggerMessageId: r.triggerMessageId, status: 'open', bids: r.bids } as never,
        { id: r.id, createdAt: r.createdAt, updatedAt: r.createdAt },
      );
      if (r.closeAs) await repo.markClosed(r.id, r.closeAs);
    }
  }

  await installCapture();

  try {
    const params = { params: Promise.resolve({ id: c.chatId }) };
    const base = `http://localhost/api/v1/chats/${c.chatId}`;
    const url = c.action ? `${base}?action=${c.action}` : base;
    const route = await import('@/app/api/v1/chats/[id]/route');
    let response: { status: number; json: () => Promise<unknown> };
    if (c.method === 'chatGet') {
      response = (await route.GET(mockRequest('GET', url) as never, params as never)) as never;
    } else if (c.method === 'chatPut') {
      response = (await route.PUT(mockRequest('PUT', url, c.body) as never, params as never)) as never;
    } else {
      response = (await route.POST(mockRequest('POST', url, c.body) as never, params as never)) as never;
    }
    const status = response.status;
    const body = await response.json();
    return {
      name: c.name,
      status,
      body,
      tables: { chats: dump(mdb, 'chats'), storytellerRounds: dump(mdb, 'storyteller_rounds') },
      logs: capturedLines,
    };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

const put = (name: string, body: unknown, extra: Partial<CaseSpec> = {}): CaseSpec => ({
  name,
  method: 'chatPut',
  chatId: GROUP,
  body,
  ...extra,
});
const post = (name: string, action: string, body: unknown, extra: Partial<CaseSpec> = {}): CaseSpec => ({
  name,
  method: 'chatPost',
  chatId: GROUP,
  action,
  body,
  ...extra,
});

export const CASES: CaseSpec[] = [
  // ── the PUT's `turnMode` sibling ───────────────────────────────────────────
  put('put_storyteller', { chat: {}, turnMode: 'storyteller' }),
  put('put_rotation_from_null', { chat: {}, turnMode: 'rotation' }),
  put('put_rotation_from_storyteller', { chat: {}, turnMode: 'rotation' }, { turnMode: 'storyteller' }),
  put('put_storyteller_again_keeps_round', { chat: {}, turnMode: 'storyteller' }, { turnMode: 'storyteller', rounds: [openRound()] }),
  put('put_leave_with_open_round', { chat: {}, turnMode: 'rotation' }, { turnMode: 'storyteller', rounds: [closedRound(), openRound()] }),
  put('put_leave_with_closed_round_only', { chat: {}, turnMode: 'rotation' }, { turnMode: 'storyteller', rounds: [closedRound()] }),
  put('put_with_bag_and_mode', { chat: { title: 'Told By The Fire' }, turnMode: 'storyteller' }),
  put('put_null_refused', { chat: {}, turnMode: null }),
  put('put_unknown_refused', { chat: {}, turnMode: 'x' }),
  put('put_number_refused', { chat: {}, turnMode: 5 }),
  put('put_null_with_bag_writes_nothing', { chat: { title: 'Should Not Land' }, turnMode: null }, { turnMode: 'storyteller', rounds: [openRound()] }),
  put('put_absent_no_change', { chat: {} }, { turnMode: 'storyteller' }),
  // ── the bag key ────────────────────────────────────────────────────────────
  put('bag_filter_true', { chat: { storytellerInviteFilter: true } }),
  put('bag_filter_false', { chat: { storytellerInviteFilter: false } }, { inviteFilter: 1 }),
  put('bag_filter_string_refused', { chat: { storytellerInviteFilter: 'yes' } }),
  put('bag_filter_null_refused', { chat: { storytellerInviteFilter: null } }),
  put('bag_filter_string_with_title_refused', { chat: { title: 'Should Not Land', storytellerInviteFilter: 'yes' } }),
  // ── the chat GET wire ──────────────────────────────────────────────────────
  { name: 'get_chat_null_mode', method: 'chatGet', chatId: GROUP },
  { name: 'get_chat_storyteller_filter_on', method: 'chatGet', chatId: GROUP, turnMode: 'storyteller', inviteFilter: 1 },
  // ── GET ?action=storyteller-round ──────────────────────────────────────────
  { name: 'round_none', method: 'chatGet', chatId: GROUP, action: 'storyteller-round' },
  { name: 'round_open', method: 'chatGet', chatId: GROUP, action: 'storyteller-round', rounds: [openRound()] },
  { name: 'round_latest_closed', method: 'chatGet', chatId: GROUP, action: 'storyteller-round', rounds: [closedRound()] },
  { name: 'round_open_beats_later_closed', method: 'chatGet', chatId: GROUP, action: 'storyteller-round', rounds: [openRound(GROUP, OPEN_ROUND, '2026-02-01T00:00:00.000Z'), closedRound()] },
  { name: 'round_other_chats_round_unseen', method: 'chatGet', chatId: GROUP, action: 'storyteller-round', rounds: [openRound(SOLO, SOLO_ROUND)] },
  // v4's reader hands Zod a NULL cell as `undefined`, and `triggerMessageId`
  // is `.nullable()` WITHOUT `.optional()` — so a round opened with no trigger
  // is unreadable (`Data validation failed`, dropped). Measured, not hidden.
  { name: 'round_null_trigger', method: 'chatGet', chatId: GROUP, action: 'storyteller-round', rounds: [{ ...openRound(), triggerMessageId: null }] },
  { name: 'round_missing_chat', method: 'chatGet', chatId: MISSING_CHAT, action: 'storyteller-round' },
  // ── POST ?action=storyteller-lie ───────────────────────────────────────────
  post('lie_success', 'storyteller-lie', { roundId: OPEN_ROUND }, { rounds: [openRound()] }),
  post('lie_closed_round', 'storyteller-lie', { roundId: CLOSED_ROUND }, { rounds: [closedRound()] }),
  post('lie_missing_round', 'storyteller-lie', { roundId: OPEN_ROUND }),
  post('lie_other_chats_round', 'storyteller-lie', { roundId: SOLO_ROUND }, { rounds: [openRound(SOLO, SOLO_ROUND)] }),
  post('lie_no_round_id', 'storyteller-lie', {}, { rounds: [openRound()] }),
  post('lie_round_id_number', 'storyteller-lie', { roundId: 7 }, { rounds: [openRound()] }),
  post('lie_missing_chat', 'storyteller-lie', { roundId: OPEN_ROUND }, { chatId: MISSING_CHAT }),
  // ── POST ?action=storyteller-run — refusal arms only ───────────────────────
  post('run_closed_round', 'storyteller-run', { roundId: CLOSED_ROUND, participantIds: [BRAM_P] }, { rounds: [closedRound()] }),
  post('run_empty_selection', 'storyteller-run', { roundId: OPEN_ROUND, participantIds: [] }, { rounds: [openRound()] }),
  post('run_not_a_bid', 'storyteller-run', { roundId: OPEN_ROUND, participantIds: [QUIET_P] }, { rounds: [openRound()] }),
  post('run_unknown_seat', 'storyteller-run', { roundId: OPEN_ROUND, participantIds: ['nobody'] }, { rounds: [openRound()] }),
  post('run_destroy_with_others', 'storyteller-run', { roundId: OPEN_ROUND, participantIds: [BRAM_P, ARIA_P], order: 'simultaneous', keepOrder: true }, { rounds: [openRound()] }),
  post('run_bad_order', 'storyteller-run', { roundId: OPEN_ROUND, participantIds: [BRAM_P], order: 'random' }, { rounds: [openRound()] }),
  post('run_missing_participant_ids', 'storyteller-run', { roundId: OPEN_ROUND }, { rounds: [openRound()] }),
  post('run_keep_order_string', 'storyteller-run', { roundId: OPEN_ROUND, participantIds: [BRAM_P], keepOrder: 'yes' }, { rounds: [openRound()] }),
  post('run_participant_id_number', 'storyteller-run', { roundId: OPEN_ROUND, participantIds: [1] }, { rounds: [openRound()] }),
  post('run_missing_chat', 'storyteller-run', { roundId: OPEN_ROUND, participantIds: [BRAM_P] }, { chatId: MISSING_CHAT }),
  // ── POST ?action=storyteller-ask — refusal arms only ───────────────────────
  post('ask_closed_round', 'storyteller-ask', { roundId: CLOSED_ROUND, participantId: QUIET_P }, { rounds: [closedRound()] }),
  post('ask_seat_not_on_round', 'storyteller-ask', { roundId: OPEN_ROUND, participantId: 'nobody' }, { rounds: [openRound()] }),
  post('ask_missing_participant_id', 'storyteller-ask', { roundId: OPEN_ROUND }, { rounds: [openRound()] }),
  post('ask_missing_chat', 'storyteller-ask', { roundId: OPEN_ROUND, participantId: QUIET_P }, { chatId: MISSING_CHAT }),
];

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'salon.json'), 'utf8')) as Spec;
  const fixtures = {
    main: process.env.QT_FIXTURE_SALON_MAIN ?? '',
    mount: process.env.QT_FIXTURE_SALON_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-turn-mode-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'debug';

  const outLines: string[] = [];
  for (const c of CASES) outLines.push(JSON.stringify(await runCase(spec, c, scratch, fixtures)));
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`chat-turn-mode-routes oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('chat-turn-mode-routes oracle', async () => {
  await main();
});

const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
