/**
 * @jest-environment node
 *
 * P4.D212 REBUILD-SUMMARY ORACLE: drives v4's REAL `handleRebuildSummary`
 * (`app/api/v1/chats/[id]/actions/rebuild-summary.ts`, `e7821606f`) THROUGH the
 * real chat route (`POST /api/v1/chats/[id]?action=rebuild-summary`) over a
 * FRESH per-case COPY of the committed `chat-admin-{main,mount}.db` pair —
 * never the committed pair itself (§R.12). Each case may PLANT extra state on
 * its copy with raw SQL; the SQL is emitted in the NDJSON row so the Rust side
 * (`chat_rebuild_summary_equivalence.rs`) applies EXACTLY the same statements,
 * at the same point (after the database is opened, before the request).
 *
 * Per case it emits: the status + body, the hydrated `chats` row, the RAW
 * `background_jobs` rows (the `payload` column's BYTES — key order is part of
 * the comparand), and the distinct realtime topics published.
 *
 * ⚠ jest.setup.ts measured at the pin: it does NOT mock
 * `@/lib/background-jobs/queue-service` nor `@/lib/realtime/bus`, so the REAL
 * `enqueueContextSummary` → `enqueueJob` writes the job row. It DOES mock the
 * repositories factory + the database manager, which this case `requireActual`s
 * (the chat-admin-routes precedent). The processor is mocked OFF so it cannot
 * claim the row before the dump. `publishRealtime` is wrapped (not replaced) to
 * record each publish's `topic[:id]`.
 *
 * **Must run under `TZ=UTC`.**
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-rs-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/chat-rebuild-summary.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/chat-admin-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CA_MAIN=$V5W/crates/quilltap-web/tests/fixtures/chat-admin-main.db \
 *   QT_FIXTURE_CA_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/chat-admin-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-chat-rebuild-summary.ndjson TZ=UTC \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- chat-rebuild-summary
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
}

// Pinned entity ids (the committed chat-admin fixture's; shared verbatim with
// the Rust differential).
const CHAT = 'c1000000-0000-4000-8000-000000000001';
const HELP_CHAT = 'c1000000-0000-4000-8000-000000000004';
const MISSING_ID = '99999999-9999-4999-8999-999999999999';
const CONN = 'c0000001-0000-4000-8000-000000000001';
const CONN_B = 'c0000002-0000-4000-8000-000000000002';

const RealDate = Date;

/** Every `publishRealtime(topic, id?)` this case made, as `topic[:id]`. */
let published: string[] = [];

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

function cipherDriverPath(): string {
  return join(process.cwd(), 'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers');
}

function applyMocks(spec: Spec): void {
  const driver = cipherDriverPath();
  jest.doMock('better-sqlite3', () => jest.requireActual(driver));
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
  jest.doMock('@/lib/services/markdown-renderer.service', () => ({
    __esModule: true,
    renderMarkdownToHtml: async () => null,
    canPreRenderMessage: () => false,
  }));
  // Keep the background-jobs processor OFF so it can't claim the row this
  // action enqueues and race the dump.
  jest.doMock('@/lib/background-jobs/processor', () => ({
    __esModule: true,
    ensureProcessorRunning: () => {},
    startProcessor: () => {},
    stopProcessor: () => {},
  }));
  // Wrap — not replace — the realtime bus, so every publish is recorded and
  // still runs v4's real coalescer.
  jest.doMock('@/lib/realtime/bus', () => {
    const actual = jest.requireActual('@/lib/realtime/bus');
    return {
      __esModule: true,
      ...actual,
      publishRealtime: (topic: string, id?: string) => {
        published.push(id ? `${topic}:${id}` : topic);
        return actual.publishRealtime(topic, id);
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

/** A raw cipher-keyed connection on the case's copy (plant + job dump). */
function rawOpen(spec: Spec, path: string): {
  exec: (sql: string) => void;
  all: (sql: string) => Array<Record<string, unknown>>;
  close: () => void;
} {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const Driver = jest.requireActual(cipherDriverPath());
  const db = new Driver(path);
  const hex = Buffer.from(spec.testPepperBase64, 'base64').toString('hex');
  db.pragma(`key = "x'${hex}'"`);
  return {
    exec: (sql) => db.exec(sql),
    all: (sql) => db.prepare(sql).all(),
    close: () => db.close(),
  };
}

// ── The per-copy WIDEN ────────────────────────────────────────────────────────
//
// ⚠ The committed `chat-admin-*` pair predates v4's `cycleOrderParticipantIds`
// column (bug 147, `add-cycle-order-column-v1.ts`), and v4's `chats.update`
// writes every schema key — so on the UNWIDENED pair every v4 chat write fails
// with `no such column: cycleOrderParticipantIds` and this action answers 500.
// The committed pair is not this family's to widen (§R.12), so each per-case
// COPY is widened here through v4's OWN migration statement
// (`addColumnIfMissing('chats', 'cycleOrderParticipantIds', "TEXT DEFAULT '[]'")`),
// guarded on `pragma_table_info` exactly as that helper is. Emitted in the
// NDJSON row, so the Rust side applies the identical widen.
const WIDEN: Array<[string, string, string]> = [
  ['chats', 'cycleOrderParticipantIds', "TEXT DEFAULT '[]'"],
];

// ── Plants (raw SQL, emitted verbatim for the Rust side) ──────────────────────

/** A running summary worth clearing: non-null text, a non-empty anchor set, a
 *  non-zero fold cursor — and a SEEDED NON-ZERO `lastFullRebuildTurn` the
 *  action must leave where it is. */
const SEED_SUMMARY =
  `UPDATE "chats" SET "contextSummary" = 'Aria and the lamplighter Mr Quint argued over the ledger.', ` +
  `"summaryAnchorMessageIds" = '["d1000000-0000-4000-8000-000000000001","d1000000-0000-4000-8000-000000000002"]', ` +
  `"lastSummaryTurn" = 42, "lastFullRebuildTurn" = 60 WHERE "id" = '${CHAT}'`;
const autonomous = (runState: string | null) =>
  `UPDATE "chats" SET "chatType" = 'autonomous', "runState" = ${
    runState === null ? 'NULL' : `'${runState}'`
  } WHERE "id" = '${CHAT}'`;
const NO_PROFILES = `DELETE FROM "connection_profiles"`;
/** `participants[i].connectionProfileId` on CHAT (json_set rewrites the column). */
const seatProfile = (i: number, profileId: string | null) =>
  `UPDATE "chats" SET "participants" = json_set("participants", '$[${i}].connectionProfileId', ${
    profileId === null ? 'NULL' : `'${profileId}'`
  }) WHERE "id" = '${CHAT}'`;
/** Move CONN behind CONN_B in rowid order, so the user's FIRST profile — the
 *  fallback — is CONN_B (neither side sorts the profile read). */
const CONN_B_FIRST =
  `CREATE TEMP TABLE "qt_cp" AS SELECT * FROM "connection_profiles" WHERE "id" = '${CONN}'; ` +
  `DELETE FROM "connection_profiles" WHERE "id" = '${CONN}'; ` +
  `INSERT INTO "connection_profiles" SELECT * FROM "qt_cp"; DROP TABLE "qt_cp"`;
/** The update throws (v4's catch arm): no job, no `chats` publish. */
const POISON_CHATS_UPDATE =
  `CREATE TRIGGER "qt_poison_chats_update" BEFORE UPDATE ON "chats" ` +
  `BEGIN SELECT RAISE(ABORT, 'poisoned chats update'); END`;

interface CaseSpec {
  name: string;
  chatId: string;
  plant: string[];
  /** Call the action this many times (the no-dedupe arm). Default 1. */
  calls?: number;
}

const CASES: CaseSpec[] = [
  // The happy path: CHAT's first CHARACTER seat (EVE) carries no profile, so the
  // user's first profile (CONN) is chosen.
  { name: 'rebuild_happy_path', chatId: CHAT, plant: [SEED_SUMMARY] },
  { name: 'rebuild_chat_missing', chatId: MISSING_ID, plant: [SEED_SUMMARY] },
  // ── the 409 conjunction, each conjunct held open ──
  { name: 'rebuild_running_room_409', chatId: CHAT, plant: [SEED_SUMMARY, autonomous('running')] },
  { name: 'rebuild_paused_room_200', chatId: CHAT, plant: [SEED_SUMMARY, autonomous('paused')] },
  { name: 'rebuild_autonomous_no_run_state_200', chatId: CHAT, plant: [SEED_SUMMARY, autonomous(null)] },
  {
    // `runState` running on a NON-autonomous chat: the chatType conjunct fails.
    name: 'rebuild_salon_running_state_200',
    chatId: CHAT,
    plant: [SEED_SUMMARY, `UPDATE "chats" SET "runState" = 'running' WHERE "id" = '${CHAT}'`],
  },
  // ── the 400 and its ORDER against the 409 ──
  { name: 'rebuild_no_profiles_400', chatId: CHAT, plant: [SEED_SUMMARY, NO_PROFILES] },
  {
    name: 'rebuild_running_room_no_profiles_409',
    chatId: CHAT,
    plant: [SEED_SUMMARY, autonomous('running'), NO_PROFILES],
  },
  // ── NO cheap-LLM-settings gate (regenerate-title has one; this does not) ──
  {
    name: 'rebuild_null_cheap_llm_settings_200',
    chatId: CHAT,
    plant: [SEED_SUMMARY, `UPDATE "chat_settings" SET "cheapLLMSettings" = NULL`],
  },
  {
    name: 'rebuild_no_chat_settings_row_200',
    chatId: CHAT,
    plant: [SEED_SUMMARY, `DELETE FROM "chat_settings"`],
  },
  // ── profile precedence ──
  {
    // The first CHARACTER seat names a profile that exists → that one.
    name: 'rebuild_cast_profile_named',
    chatId: CHAT,
    plant: [SEED_SUMMARY, seatProfile(0, CONN_B)],
  },
  {
    // The first CHARACTER seat names NO profile row → the user's first profile.
    name: 'rebuild_cast_profile_dangling',
    chatId: CHAT,
    plant: [SEED_SUMMARY, CONN_B_FIRST, seatProfile(0, MISSING_ID)],
  },
  {
    // Only the FIRST CHARACTER seat is consulted: the second seat's CONN_B is
    // ignored and the fallback (rowid-first CONN) wins.
    name: 'rebuild_only_first_seat_consulted',
    chatId: CHAT,
    plant: [SEED_SUMMARY, seatProfile(1, CONN_B)],
  },
  {
    // No seat profile and CONN_B first by rowid → CONN_B, proving the fallback
    // is the READ's first row, not the user default (`isDefault` is CONN's).
    name: 'rebuild_fallback_first_profile_by_read_order',
    chatId: CHAT,
    plant: [SEED_SUMMARY, CONN_B_FIRST],
  },
  // A help chat is not refused, and its seat's own profile is used.
  { name: 'rebuild_help_chat', chatId: HELP_CHAT, plant: [] },
  // NO dedupe: two calls → two job rows.
  { name: 'rebuild_twice_no_dedupe', chatId: CHAT, plant: [SEED_SUMMARY], calls: 2 },
  // The clearing update throws → 500; no job row, no `chats` publish.
  { name: 'rebuild_update_fails_500', chatId: CHAT, plant: [SEED_SUMMARY, POISON_CHATS_UPDATE] },
];

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  published = [];
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'rs-'));
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

  // Plant AFTER the open (both sides): a boot-time data migration can then
  // never trip a planted trigger, and each side plants onto the same schema.
  {
    const raw = rawOpen(spec, mainWork);
    for (const [table, column, decl] of WIDEN) {
      const have = raw.all(`SELECT 1 FROM pragma_table_info('${table}') WHERE name = '${column}'`);
      if (have.length === 0) raw.exec(`ALTER TABLE "${table}" ADD COLUMN "${column}" ${decl}`);
    }
    for (const sql of c.plant) raw.exec(sql);
    raw.close();
  }
  published = [];

  // A TICKING frozen clock (the chat-admin-routes convention).
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
    const route = (await import('@/app/api/v1/chats/[id]/route')) as unknown as {
      POST: (...a: unknown[]) => Promise<{ status: number; json: () => Promise<unknown> }>;
    };
    const calls = c.calls ?? 1;
    const responses: Array<{ status: number; body: unknown }> = [];
    for (let i = 0; i < calls; i++) {
      const resp = await route.POST(
        mockRequest(`http://localhost/api/v1/chats/${c.chatId}?action=rebuild-summary`, {}),
        { params: Promise.resolve({ id: c.chatId }) },
      );
      responses.push({ status: resp.status, body: await resp.json() });
    }
    const last = responses[responses.length - 1];
    const jobIds = responses.map((r) => (r.body as { jobId?: string }).jobId ?? null);

    const { getRepositories } = await import('@/lib/repositories/factory');
    const chat = (await getRepositories().chats.findById(CHAT)) ?? null;
    const helpChat = (await getRepositories().chats.findById(HELP_CHAT)) ?? null;
    global.Date = RealDate;
    await closeDatabase();
    closeMountIndexSQLiteClient();

    const raw = rawOpen(spec, mainWork);
    const jobs = raw.all(
      `SELECT "userId", "type", "status", "payload", "priority", "attempts", "maxAttempts", ` +
        `"lastError", "startedAt", "completedAt" FROM "background_jobs" ORDER BY rowid`,
    );
    raw.close();

    return {
      name: c.name,
      chatId: c.chatId,
      widen: WIDEN,
      plant: c.plant,
      calls,
      status: last.status,
      body: last.body,
      // In-band identity for the repeat case (both sides mint UUIDs).
      distinctJobIds: new Set(jobIds.filter((j) => j !== null)).size,
      tables: { chat, helpChat, jobs },
      published: [...new Set(published)].sort(),
    };
  } finally {
    global.Date = RealDate;
    try {
      await closeDatabase();
    } catch {
      /* already closed */
    }
    try {
      closeMountIndexSQLiteClient();
    } catch {
      /* already closed */
    }
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(`chat-rebuild-summary oracle must run under TZ=UTC (getTimezoneOffset=${offset})`);
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-rs-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const lines: string[] = [];
  for (const c of CASES) {
    lines.push(JSON.stringify(await runCase(spec, c, scratch, fixtures)));
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(`wrote ${lines.length} chat-rebuild-summary oracle rows to ${outPath}\n`);
}

test('chat-rebuild-summary oracle', async () => {
  await main();
});
