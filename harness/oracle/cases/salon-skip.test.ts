/**
 * @jest-environment node
 *
 * P4.6c skipUserTurn ORACLE (the minted-values sibling of `salon-mutations`):
 * drives v4's REAL `handleTurnAction` `skipUserTurn` over a FRESH copy of the
 * committed salon GROUP fixture per case, emitting the response body + the MAIN-db
 * `chats` / `chat_messages` dumps so the Rust port (`api::salon::turn_action`) can
 * be diffed. Unlike `salon-mutations` (zero-mint), the successful skip posts a Host
 * turn-pass announcement (minted id + timestamps), so the Rust harness normalizes:
 * the minted turn-pass row's id → `<HOSTPASS>`, its createdAt + the chats
 * lastMessageAt/updatedAt bumps → `<ts>`.
 *
 * Two branches:
 *   - `skip_success`: a user-controlled participant (Cleo) skips in the group scene
 *     (`qualifiesForTurnSkipping` = 3 active chars) with no prior passes → the skip
 *     applies: a Host turn-pass row lands, `spokenThisCycle` folds, the turn queue +
 *     `lastTurnParticipantId` persist.
 *   - `skip_refusal`: two Host turn-pass rows for the LLM participants (Aria + Bram)
 *     are seeded after the last substantive message, so `all-others-skipped` fires
 *     and v4 refuses the human skip with the exact copy. The refusal writes nothing.
 *
 * Same seams as `salon-mutations` (auth, startup gate, Math.random, the real cipher
 * binding + vector store past jest.setup).
 *
 * P4.D335 (v4 `2b5e8ee24`): the two interrupt actions ride the same REAL route
 * `POST` (so v4's chat-404-first order and the ZodError → `validationError` arm
 * are the real ones) — `?action=stop-turn` (no stream open in the process, so
 * `{stopped: 0, settled: true, paused}` and the pause write) and
 * `?action=raise-hand` (the `raised` matrix). Per-case `plantSql` sets the chat
 * state first; the `[Interrupt]` / `[TurnControl]` lines are captured by
 * splicing the logger's transports.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-salon-skip-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures" "$TMPO/lib"
 *   cp $V5W/harness/oracle/cases/salon-skip.test.ts "$TMPO/cases/"
 *   cp $V5W/harness/oracle/lib/p4d171-columns.ts "$TMPO/lib/"
 *   cp $V5W/harness/oracle/fixtures/salon.json      "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SALON_MAIN=$V5W/crates/quilltap-web/tests/fixtures/salon-main.db \
 *   QT_FIXTURE_SALON_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/salon-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-salon-skip.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- salon-skip
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { ensureP4D171Columns } from '../lib/p4d171-columns';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

interface SeedTurnPass {
  id: string;
  participantId: string; // the passing (LLM) participant, into hostEvent
  createdAt: string;
}

interface CaseSpec {
  name: string;
  paramId: string;
  body: Record<string, unknown> | unknown;
  random01: number;
  seed?: SeedTurnPass[];
  /** P4.D335: the `?action=` (default `turn`). */
  action?: string;
  /** P4.D335: raw SQL run on the fixture copy before the call. */
  plantSql?: string[];
  /** P4.D335: `req.json()` rejects (an unparsable body). */
  jsonThrows?: boolean;
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

function mockRequest(url: string, body?: unknown, jsonThrows = false): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jsonThrows
      ? jest.fn().mockRejectedValue(new SyntaxError('Unexpected token'))
      : jest.fn().mockResolvedValue(body ?? {}),
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
  Math.random = () => c.random01;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite');
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();

  // P4.D172: heal the fixture copy — see the helper's own note.
  ensureP4D171Columns(getRawDatabase() as never);

  try {
    // Seed the prior Host turn-pass records (refusal case) via the REAL repo, with
    // explicit ids + createdAt AFTER the last substantive message so the walk-back
    // stall guard sees them.
    if (c.seed && c.seed.length > 0) {
      const repos = getRepositories();
      await repos.chats.addMessages(
        c.paramId,
        c.seed.map((s) => ({
          id: s.id,
          type: 'message',
          role: 'ASSISTANT',
          content: 'A pass, quietly noted.',
          participantId: undefined,
          createdAt: s.createdAt,
          attachments: [],
          systemSender: 'host',
          systemKind: 'turn-pass',
          hostEvent: { participantId: s.participantId },
        })) as never,
      );
    }

    for (const sql of c.plantSql ?? []) {
      (getRawDatabase() as unknown as { exec: (s: string) => void }).exec(sql);
    }

    // P4.D335: capture the interrupt lines (the transports are spliced so
    // nothing reaches stdout).
    const logs: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
    const { logger } = await import('@/lib/logger');
    const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
    internal.transports.splice(0, internal.transports.length, {
      write: (d: unknown) => {
        const data = d as { level: string; message: string; context?: Record<string, unknown> };
        // [P4.170 F.7] + the actions' own catch ERRORs (the malformed-body rows).
        if (
          !data.message.startsWith('[Interrupt]') &&
          !data.message.startsWith('[TurnControl]') &&
          !data.message.startsWith('[Chats v1] Error')
        )
          return;
        const { service: _s, module: _m, environment: _e, ...fields } = (data.context ?? {}) as Record<
          string,
          unknown
        >;
        logs.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
      },
    });

    const url = `http://localhost/api/v1/chats/${c.paramId}?action=${c.action ?? 'turn'}`;
    const params = { params: Promise.resolve({ id: c.paramId }) };
    const { POST } = await import('@/app/api/v1/chats/[id]/route');
    const response = (await POST(
      mockRequest(url, c.body, c.jsonThrows === true) as never,
      params as never,
    )) as {
      status: number;
      json: () => Promise<unknown>;
    };
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
    return { name: c.name, status, body, tables, logs };
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-salon-skip-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  // P4.D335: DEBUG so the `[TurnControl]` lines reach the spliced transport.
  process.env.LOG_LEVEL = 'debug';

  const GROUP = 'c1000000-0000-4000-8000-000000000002';
  const USER_P = 'b2000000-0000-4000-8000-000000000003'; // Cleo (controlledBy user)
  const ARIA_P = 'b2000000-0000-4000-8000-000000000001'; // Aria (llm)
  const BRAM_P = 'b2000000-0000-4000-8000-000000000002'; // Bram (llm)
  // P4.D335: the interrupt actions' chats.
  const SOLO = 'c1000000-0000-4000-8000-000000000001';
  const UNKNOWN = 'c1000000-0000-4000-8000-0000000000ff';
  const stop = (name: string, paramId: string, plantSql?: string[]): CaseSpec => ({
    name,
    paramId,
    body: {},
    random01: 0.42,
    action: 'stop-turn',
    plantSql,
  });
  // [P4.170] any POST action on the GROUP chat (or `paramId`).
  const act = (name: string, action: string, extra: Partial<CaseSpec> = {}): CaseSpec => ({
    name,
    paramId: GROUP,
    body: {},
    random01: 0.42,
    action,
    ...extra,
  });
  const raise = (name: string, body: unknown, extra: Partial<CaseSpec> = {}): CaseSpec => ({
    name,
    paramId: GROUP,
    body,
    random01: 0.42,
    action: 'raise-hand',
    ...extra,
  });

  const cases: CaseSpec[] = [
    {
      name: 'skip_success',
      paramId: GROUP,
      body: { action: 'skipUserTurn', participantId: USER_P },
      random01: 0.42,
    },
    {
      name: 'skip_refusal',
      paramId: GROUP,
      body: { action: 'skipUserTurn', participantId: USER_P },
      random01: 0.42,
      seed: [
        { id: 'aa000000-0000-4000-8000-000000000001', participantId: ARIA_P, createdAt: '2026-02-02T00:00:00.000Z' },
        { id: 'aa000000-0000-4000-8000-000000000002', participantId: BRAM_P, createdAt: '2026-02-03T00:00:00.000Z' },
      ],
    },
    // --- P4.D335: the interrupt actions (no stream open in this process) ---
    stop('stop_no_stream_group', GROUP),
    stop('stop_no_stream_already_paused', GROUP, [
      `UPDATE chats SET "isPaused" = 1 WHERE id = '${GROUP}'`,
    ]),
    // A room with no active LLM seat and one user seat is NOT multi-character.
    stop('stop_no_stream_single', SOLO, [
      `UPDATE chats SET participants = json_set(participants, '$[0].status', 'removed') WHERE id = '${SOLO}'`,
    ]),
    // The plain two-seat Solo room (one LLM, one user) IS multi-character to v4.
    stop('stop_no_stream_solo_pair', SOLO),
    stop('stop_no_stream_autonomous', GROUP, [
      `UPDATE chats SET "chatType" = 'autonomous' WHERE id = '${GROUP}'`,
    ]),
    stop('stop_unknown_chat', UNKNOWN),
    raise('raise_absent_body', {}),
    raise('raise_true', { raised: true }),
    raise('raise_false', { raised: false }),
    raise('raise_null', { raised: null }),
    raise('raise_string', { raised: 'true' }),
    raise('raise_number', { raised: 1 }),
    raise('raise_object', { raised: {} }),
    raise('raise_extra_keys', { raised: true, extra: 1 }),
    raise('raise_unparsable_body', null, { jsonThrows: true }),
    // REST-only: a body that PARSES to a non-object reaches the Zod parse.
    raise('raise_array_body', []),
    raise('raise_string_body', 'yes'),
    raise('raise_unknown_chat', { raised: null }, { paramId: UNKNOWN }),
    // [P4.170 P-12] REST-only: `handlePost` finds the chat BEFORE any action
    // reads its body, so a missing chat is the 404 whatever the body holds —
    // the "body before load" class (raise-hand I-1, the rollback F.8, and the
    // other POST actions whose REST edge refuses a body itself).
    raise('raise_array_body_unknown_chat', [], { paramId: UNKNOWN }),
    act('equip_unparsable_body', 'equip', { jsonThrows: true }),
    act('equip_unparsable_unknown_chat', 'equip', { jsonThrows: true, paramId: UNKNOWN }),
    act('rollback_unparsable_body', 'wardrobe-rollback', { jsonThrows: true }),
    act('rollback_unparsable_unknown_chat', 'wardrobe-rollback', { jsonThrows: true, paramId: UNKNOWN }),
    act('inventory_equip_unparsable_unknown_chat', 'inventory-equip', { jsonThrows: true, paramId: UNKNOWN }),
    act('regenerate_avatar_unparsable_unknown_chat', 'regenerate-avatar', { jsonThrows: true, paramId: UNKNOWN }),
    act('inform_array_body_unknown_chat', 'inform', { body: [], paramId: UNKNOWN }),
    act('cancel_inform_array_body_unknown_chat', 'cancel-inform', { body: [], paramId: UNKNOWN }),
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`salon-skip oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('salon-skip oracle', async () => {
  await main();
});

// Remove the OS-temp scratch dir(s) once the oracle has written its NDJSON.
const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
