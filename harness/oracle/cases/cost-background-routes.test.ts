/**
 * @jest-environment node
 *
 * P4.6ao TOKEN-COST + REGENERATE-BACKGROUND route-surface ORACLE: drives v4's
 * REAL route handlers over a FRESH copy of the committed cost-background fixture
 * per case, and emits each response {status, body} — plus, for the mutating
 * regenerate cases, the resulting `background_jobs` rows — so the Rust ports
 * (`api::chat_media::chat_get_cost` / `chat_regenerate_background`) diff
 * byte-for-byte.
 *
 * Unit 1 (`?action=cost`) cases: the three aggregate arms (populated / legacy
 * cost-without-priceSource / untouched), the chat-missing 404, the detailed arm
 * over the itemized corpus (a diverging tokenCount + a costed system event + an
 * uncosted one — the `!== null` source quirk), the detailed no-messages arm, and
 * the exact-string `detailed=1` arm (v4 compares `=== 'true'`).
 *
 * Unit 2 (`?action=regenerate-background`) cases: the three badRequest arms
 * (disabled settings / no image profile / no characters), the success arm, the
 * dedupe arm (a second call returns the SAME jobId with the already-in-progress
 * message), and the chat-missing 404. Each mutating case also emits `jobs`: the
 * background_jobs rows the call left behind (ids/timestamps blanked by the Rust
 * harness; the payload + priority + type are the diff).
 *
 * The settings arm is selected by the SESSION USER (v4 reads chat settings by
 * user, and `handleRegenerateBackground` never cross-checks chat ownership) —
 * so `user` on a case names which of the fixture's three users signs in.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-cb-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/cost-background-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/cost-background-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CB_MAIN=$V5W/crates/quilltap-web/tests/fixtures/cost-background-main.db \
 *   QT_FIXTURE_CB_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/cost-background-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-cost-background.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- cost-background-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userEnabledId: string;
  userDisabledId: string;
  userNoProfileId: string;
  chatFullId: string;
  chatLegacyId: string;
  chatEmptyId: string;
  chatDetailedId: string;
  chatRegenId: string;
  chatNoCharsId: string;
  missingId: string;
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

function applyMocks(userId: string): void {
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
  // The chat GET/POST handlers pull the markdown renderer (unified/ESM) — mock it
  // away; neither the cost body nor the regenerate body is rendered.
  jest.doMock('@/lib/services/markdown-renderer.service', () => ({
    __esModule: true,
    renderMarkdownToHtml: async () => null,
    canPreRenderMessage: () => false,
  }));
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: userId } }),
  }));
  // Processor-off (the P4.6y lesson). `enqueueJob` kicks the in-process job
  // processor, which CLAIMS the freshly-queued STORY_BACKGROUND_GENERATION row
  // and flips it PENDING → PROCESSING mid-case — an artifact of v4's runtime,
  // not of handleRegenerateBackground, and a race against the row dump. The
  // port's differential runs no processor, so leaving this on would diff v4's
  // scheduler against nothing.
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
  });
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

async function loadRoute(
  path: string,
): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
  return (await import(path)) as never;
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  if (resp.status === 204) return { status: 204, body: null };
  return { status: resp.status, body: await resp.json() };
}

const CHAT_ROUTE = '@/app/api/v1/chats/[id]/route';
const B = 'http://localhost/api/v1';

const getChat = async (url: string, id: string) =>
  respond(
    await (await loadRoute(CHAT_ROUTE)).GET(mockRequest(url), {
      params: Promise.resolve({ id }),
    }),
  );
const postChat = async (url: string, id: string) =>
  respond(
    await (await loadRoute(CHAT_ROUTE)).POST(mockRequest(url), {
      params: Promise.resolve({ id }),
    }),
  );

interface CaseSpec {
  name: string;
  /** Which fixture user signs in (defaults to userEnabledId). */
  user?: (s: Spec) => string;
  blank?: string[];
  /** Emit the background_jobs rows after the run (the unit-2 DB effect). */
  emitJobs?: boolean;
  run: (s: Spec) => Promise<{ status: number; body: unknown; extra?: unknown }>;
}

function buildCases(): CaseSpec[] {
  return [
    // ── Unit 1: ?action=cost ──
    {
      name: 'cost_aggregates',
      run: (s) => getChat(`${B}/chats/${s.chatFullId}?action=cost`, s.chatFullId),
    },
    {
      // A stored cost with NO priceSource → the legacy inference → 'registry'.
      name: 'cost_legacy_no_price_source',
      run: (s) => getChat(`${B}/chats/${s.chatLegacyId}?action=cost`, s.chatLegacyId),
    },
    {
      name: 'cost_no_aggregates',
      run: (s) => getChat(`${B}/chats/${s.chatEmptyId}?action=cost`, s.chatEmptyId),
    },
    {
      name: 'cost_chat_missing',
      run: (s) => getChat(`${B}/chats/${s.missingId}?action=cost`, s.missingId),
    },
    {
      name: 'cost_detailed',
      run: (s) =>
        getChat(`${B}/chats/${s.chatDetailedId}?action=cost&detailed=true`, s.chatDetailedId),
    },
    {
      // detailed=true over a chat with NO messages → the all-zeros object (and
      // NO messageBreakdown/systemEventBreakdown keys at all).
      name: 'cost_detailed_no_messages',
      run: (s) => getChat(`${B}/chats/${s.chatEmptyId}?action=cost&detailed=true`, s.chatEmptyId),
    },
    {
      // v4 compares `searchParams.get('detailed') === 'true'` — `1` is NOT true,
      // so this must answer the AGGREGATE shape over the itemized chat.
      name: 'cost_detailed_not_the_exact_string',
      run: (s) =>
        getChat(`${B}/chats/${s.chatDetailedId}?action=cost&detailed=1`, s.chatDetailedId),
    },

    // ── Unit 2: ?action=regenerate-background ──
    {
      // Arm 1: the settings user has story backgrounds DISABLED.
      name: 'regen_disabled',
      user: (s) => s.userDisabledId,
      emitJobs: true,
      run: (s) => regen(s.chatRegenId),
    },
    {
      // Arm 2: enabled, but NO image profile resolves at any tier.
      name: 'regen_no_image_profile',
      user: (s) => s.userNoProfileId,
      emitJobs: true,
      run: (s) => regen(s.chatRegenId),
    },
    {
      // Arm 3: enabled + a profile, but the chat has no character participants.
      name: 'regen_no_characters',
      emitJobs: true,
      run: (s) => regen(s.chatNoCharsId),
    },
    {
      // The success arm: queued, isNew → the "queued" message. The job row is the
      // real assertion (type + payload + priority −1).
      name: 'regen_queued',
      emitJobs: true,
      blank: ['id', 'jobId'],
      run: (s) => regen(s.chatRegenId),
    },
    {
      // The dedupe arm: a SECOND call reuses the pending job — the
      // already-in-progress message, still exactly ONE job row, and the SAME
      // jobId as the first call. The jobId is minted (blanked in the body diff),
      // so `extra.sameJobId` carries the identity claim explicitly.
      name: 'regen_dedupe',
      emitJobs: true,
      blank: ['id', 'jobId'],
      run: async (s) => {
        const first = await regen(s.chatRegenId);
        const second = await regen(s.chatRegenId);
        // NB: successResponse puts the data at the TOP level here — `body.jobId`,
        // not `body.data.jobId`. Reading `.data.jobId` would compare
        // undefined === undefined and pass vacuously.
        const jobId = (r: { body: unknown }) => (r.body as Record<string, unknown>).jobId;
        if (jobId(first) === undefined) throw new Error('jobId missing from the success body');
        return { ...second, extra: { sameJobId: jobId(first) === jobId(second) } };
      },
    },
    {
      name: 'regen_chat_missing',
      run: (s) => regen(s.missingId),
    },
  ];
}

const regen = (id: string) =>
  postChat(`${B}/chats/${id}?action=regenerate-background`, id);

/** Every background_jobs row the case left behind, id-ordered (the enqueue
 *  proof). Driven through v4's REAL repository — the repo factory exposes no
 *  background-jobs accessor, and a raw `getRawDatabase()` read does not see the
 *  manager's connection here. */
async function readJobs(): Promise<unknown[]> {
  const { BackgroundJobsRepository } = await import(
    '@/lib/database/repositories/background-jobs.repository'
  );
  const rows = (await new BackgroundJobsRepository().findAll()) as Array<
    Record<string, unknown>
  >;
  return rows
    .map((j) => ({
      id: j.id,
      userId: j.userId,
      type: j.type,
      status: j.status,
      priority: j.priority,
      maxAttempts: j.maxAttempts,
      payload: j.payload,
    }))
    .sort((a, b) => String(a.id).localeCompare(String(b.id)));
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(c.user ? c.user(spec) : spec.userEnabledId);

  const work = mkdtempSync(join(scratch, 'cb-'));
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

  try {
    const out = await c.run(spec);
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      ...(out.extra !== undefined ? { extra: out.extra } : {}),
      ...(c.emitJobs ? { jobs: await readJobs() } : {}),
      ...(c.blank ? { blank: c.blank } : {}),
    };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'cost-background-web.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_CB_MAIN ?? '',
    mount: process.env.QT_FIXTURE_CB_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cb-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const outLines: string[] = [];
  for (const c of buildCases()) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`cost-background-routes oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('cost-background-routes oracle', async () => {
  await main();
});
