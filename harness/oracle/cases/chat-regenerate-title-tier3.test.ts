/**
 * @jest-environment node
 *
 * P4.9E3A TIER-3 (mocked-LLM) ORACLE for the MANUAL `?action=regenerate-title`
 * action: drives v4's REAL `handleRegenerateTitle` through the REAL chat route
 * over a FRESH copy of the committed chat-admin fixture per case, with the cheap
 * LLM CANNED, and emits the response body plus the resulting chat row so the
 * Rust port diffs the write tier-2 style.
 *
 * The canned reply is keyed by the SYSTEM prompt, which also proves the right
 * generator reached the provider: the literary `CHAT_TITLE_PROMPT` vs the
 * practical `HELP_CHAT_TITLE_PROMPT`. Note these are NOT the `TITLE_UPDATE` job's
 * *evaluator* prompts — v4 has two distinct title paths and this is the manual
 * one (see `chat_admin.rs`'s note on the order's tier-2 item 7).
 *
 * The system prompt is ALSO emitted verbatim, so the transcript weighting
 * (`titleChat`'s last-100 / last-10-in-full-to-500 / earlier-to-150) and the
 * never-appended "Current title" rider are pinned rather than inferred.
 *
 * P4.D215 (v4 `00c290c9a`, bugs 163/164) sends the title through
 * `applyAutoTitle` with `clearManualRename: true`, and grows six arms that
 * only the chokepoint can tell apart — the overruled hand rename, the
 * unchanged title, and the story background a CHANGED title now queues (bug
 * 163) but an unchanged one or a help chat never does. Their state is PLANTED
 * through v4's real repositories (see `CaseSpec.plant`), and every row now
 * also carries the `background_jobs` rows.
 *
 * Seams mocked, and why:
 *   - `createLLMProvider` — the tier-3 model boundary (the Rust side injects the
 *     same canned reply).
 *   - `getApiKeyForCheapLLMSelection` — the key step runs BEFORE the provider
 *     call and throws with no `api_keys` row resolvable, which
 *     `executeCheapLLMTask` catches into `{success:false}`, sending every case
 *     down the failure arm and making the oracle vacuous. The Rust port's
 *     boundary starts at the provider call, so it has no equivalent step.
 *   - the background-jobs processor — off, so nothing races the dump.
 *   - `Date` — frozen, so the stamped `updatedAt` is deterministic on both sides.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-rt-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/chat-regenerate-title-tier3.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/chat-admin-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CA_MAIN=$V5W/crates/quilltap-web/tests/fixtures/chat-admin-main.db \
 *   QT_FIXTURE_CA_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/chat-admin-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-chat-regenerate-title.ndjson TZ=UTC \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- chat-regenerate-title-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Canned {
  content: string;
  promptTokens: number;
  completionTokens: number;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  frozenNowMs: number;
  cannedTitles: Record<string, Canned>;
}

const CHAT = 'c1000000-0000-4000-8000-000000000001';
const EMPTY_CHAT = 'c1000000-0000-4000-8000-000000000003';
const HELP_CHAT = 'c1000000-0000-4000-8000-000000000004';
const MISSING_ID = '99999999-9999-4999-8999-999999999999';

const RealDate = Date;

/** Which canned key a system prompt selects (also the prompt-identity proof). */
function keyForSystemPrompt(system: string): 'help' | 'literary' {
  return system.startsWith('Generate a short, practical title') ? 'help' : 'literary';
}

interface CaseSpec {
  name: string;
  chat: string;
  /** Override the canned reply for this case. */
  reply?: Canned;
  /** Make the provider throw instead of replying. */
  providerThrows?: boolean;
  /**
   * P4.D215 (v4 `00c290c9a`, bugs 163/164): state planted on the case's COPY
   * through v4's REAL repositories before the route runs (after the clock
   * freeze, so every stamp is the frozen one). The planted copy is saved beside
   * the oracle NDJSON (`<out>.<case>.{main,mount}.db`) and the Rust side opens
   * THAT — so both sides start from byte-identical planted state without a
   * second hand-written copy of v4's row shapes.
   *   - `manualRename`: the chat is `isManuallyRenamed: true` (a hand rename).
   *   - `backgrounds`: an image profile the user owns, WITH an `apiKeyId`, named
   *     as `storyBackgroundsSettings.defaultImageProfileId` with `enabled:
   *     true` — so a changed title can queue the Lantern.
   */
  plant?: { manualRename?: boolean; backgrounds?: boolean };
}

/** The planted image profile + its (never-resolved) API key id. */
const PLANT_IMAGE_PROFILE = 'c9000000-0000-4000-8000-000000000001';
const PLANT_API_KEY = 'c9000000-0000-4000-8000-000000000002';
/** `CHAT`'s committed title — a canned reply of exactly this is "unchanged". */
const CHAT_TITLE = 'The Evening Post';

function buildCases(spec: Spec): CaseSpec[] {
  return [
    // The literary arm over the fixture's crossed transcript.
    { name: 'regen_title_normal', chat: CHAT },
    // The help arm — a DIFFERENT system prompt, which the canned key proves.
    { name: 'regen_title_help', chat: HELP_CHAT },
    // The 50-character clamp (the manual literary arm's; the evaluator's is 60).
    { name: 'regen_title_clamped', chat: CHAT, reply: spec.cannedTitles.long },
    // Quote stripping: one leading and one trailing quote come off.
    {
      name: 'regen_title_quoted',
      chat: CHAT,
      reply: { content: '  \'The Ledger and the Lamp\'  ', promptTokens: 10, completionTokens: 6 },
    },
    // Padding INSIDE the quotes: since `dcab791c2` collapsed the inline
    // cleaners onto `cleanTitle`, a second trim runs after the quote strip,
    // so the stored title carries no padding. (Before the sweep this case
    // persisted ' A Padded Title ' verbatim.)
    {
      name: 'regen_title_quoted_padded_inside',
      chat: CHAT,
      reply: { content: '" A Padded Title "', promptTokens: 10, completionTokens: 6 },
    },
    // An empty reply is falsy in JS → the serverError arm, and NO write.
    {
      name: 'regen_title_empty_reply',
      chat: CHAT,
      reply: { content: '   ', promptTokens: 10, completionTokens: 1 },
    },
    // A provider throw → the serverError arm carrying the task's error.
    { name: 'regen_title_provider_throws', chat: CHAT, providerThrows: true },
    // No visible conversation → the 400, before the provider is ever called.
    { name: 'regen_title_no_messages', chat: EMPTY_CHAT },
    { name: 'regen_title_chat_missing', chat: MISSING_ID },
    // ── P4.D215 (v4 `00c290c9a`): the verb through `applyAutoTitle` with
    // `clearManualRename: true`. The body is `{success, title}` for EVERY
    // outcome.
    // An explicit regenerate OVERRULES a hand rename and hands the title back
    // to the automatic titler (the flag clears).
    { name: 'regen_title_overrules_manual', chat: CHAT, plant: { manualRename: true } },
    // The canned title equals the current one: the chokepoint's unchanged arm
    // writes only its extra patch (`isManuallyRenamed: false` + `updatedAt`).
    {
      name: 'regen_title_unchanged',
      chat: CHAT,
      reply: { content: CHAT_TITLE, promptTokens: 10, completionTokens: 4 },
    },
    // Bug 163: a CHANGED title cues the Lantern — one STORY_BACKGROUND_GENERATION
    // row, where before the commit this verb never queued one.
    { name: 'regen_title_queues_background', chat: CHAT, plant: { backgrounds: true } },
    // …and a hand-renamed chat that is overruled queues it too.
    {
      name: 'regen_title_overrules_manual_queues_background',
      chat: CHAT,
      plant: { manualRename: true, backgrounds: true },
    },
    // An UNCHANGED title is no scene change: backgrounds on, nothing queued.
    {
      name: 'regen_title_unchanged_no_background',
      chat: CHAT,
      reply: { content: CHAT_TITLE, promptTokens: 10, completionTokens: 4 },
      plant: { backgrounds: true },
    },
    // Help-like chats never get a story background (the chokepoint's gate, on
    // the RE-READ row's type) — the help chat HAS a present character, so only
    // the gate stands between it and a job.
    { name: 'regen_title_help_no_background', chat: HELP_CHAT, plant: { backgrounds: true } },
  ];
}

function mockRequest(url: string): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue({}),
  };
}

/** Every message list the provider actually saw, in order. The USER entry is
 *  where `titleChat`'s transcript weighting lands, so it must be diffed too —
 *  a system-prompt-only comparison would leave the 100/10/500/150 rendering
 *  entirely unchecked. */
let seenMessages: Array<Array<{ role: string; content: string }>> = [];

function applyMocks(spec: Spec, c: CaseSpec): void {
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

  // The tier-3 model boundary.
  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async () => ({
        sendMessage: async (params: { messages: Array<{ role: string; content: string }> }) => {
          const system = params.messages.find((m) => m.role === 'system')?.content ?? '';
          seenMessages.push(params.messages.map((m) => ({ role: m.role, content: m.content })));
          if (c.providerThrows) throw new Error('canned provider failure');
          const canned = c.reply ?? spec.cannedTitles[keyForSystemPrompt(system)];
          if (!canned) throw new Error('no canned reply');
          return {
            content: canned.content,
            usage: {
              promptTokens: canned.promptTokens,
              completionTokens: canned.completionTokens,
              totalTokens: canned.promptTokens + canned.completionTokens,
            },
          };
        },
      }),
    };
  });

  // See the header: without this every case takes the failure arm.
  jest.doMock('@/lib/services/api-key.service', () => {
    const actual = jest.requireActual('@/lib/services/api-key.service');
    return {
      __esModule: true,
      ...actual,
      getApiKeyForCheapLLMSelection: async () => 'canned-test-key',
    };
  });
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

async function readChat(chatId: string): Promise<unknown> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const chat = (await getRepositories().chats.findById(chatId)) as Record<string, unknown> | null;
  if (!chat) return null;
  return {
    id: chat.id,
    title: chat.title,
    isManuallyRenamed: chat.isManuallyRenamed,
    updatedAt: chat.updatedAt,
  };
}

/** Every background job, in the title-update family's comparand shape. */
async function readJobs(): Promise<unknown> {
  const { BackgroundJobsRepository } = await import(
    '@/lib/database/repositories/background-jobs.repository'
  );
  const jobs = (await new BackgroundJobsRepository().findAll()) as Array<Record<string, unknown>>;
  return jobs
    .map((j) => ({ type: j.type, status: j.status, priority: j.priority, payload: j.payload }))
    .sort((a, b) => String(a.type).localeCompare(String(b.type)));
}

/** Apply `c.plant` through v4's real repositories (see `CaseSpec.plant`). */
async function applyPlant(spec: Spec, c: CaseSpec): Promise<void> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const repos = getRepositories();
  if (c.plant?.backgrounds) {
    await repos.imageProfiles.create(
      {
        userId: spec.userId,
        name: 'Planted Lantern Profile',
        provider: 'OPENAI',
        modelName: 'dall-e-3',
        apiKeyId: PLANT_API_KEY,
        baseUrl: null,
        parameters: {},
        isDefault: false,
        isDangerousCompatible: false,
        tags: [],
      } as never,
      { id: PLANT_IMAGE_PROFILE } as never,
    );
    await repos.chatSettings.updateForUser(spec.userId, {
      storyBackgroundsSettings: { enabled: true, defaultImageProfileId: PLANT_IMAGE_PROFILE },
    } as never);
  }
  if (c.plant?.manualRename) {
    await repos.chats.update(c.chat, { isManuallyRenamed: true } as never);
  }
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  seenMessages = [];
  applyMocks(spec, c);

  const work = mkdtempSync(join(scratch, 'rt-'));
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

  const iso = new RealDate(spec.frozenNowMs).toISOString();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  global.Date = class extends RealDate {
    constructor(...a: unknown[]) {
      if (a.length === 0) super(iso);
      // @ts-expect-error forward variadic args
      else super(...a);
    }
    static now(): number {
      return spec.frozenNowMs;
    }
  } as unknown as DateConstructor;

  try {
    if (c.plant) {
      await applyPlant(spec, c);
      const out = process.env.QT_ORACLE_OUT!;
      copyFileSync(mainWork, `${out}.${c.name}.main.db`);
      copyFileSync(mountWork, `${out}.${c.name}.mount.db`);
    }
    const route = (await import('@/app/api/v1/chats/[id]/route')) as never as Record<
      string,
      (...a: unknown[]) => Promise<{ status: number; json: () => Promise<unknown> }>
    >;
    const resp = await route.POST(
      mockRequest(`http://localhost/api/v1/chats/${c.chat}?action=regenerate-title`),
      { params: Promise.resolve({ id: c.chat }) },
    );
    return {
      name: c.name,
      status: resp.status,
      body: await resp.json(),
      llmMessages: seenMessages,
      chat: await readChat(c.chat),
      jobs: await readJobs(),
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
    throw new Error(
      `chat-regenerate-title-tier3 oracle must run under TZ=UTC (getTimezoneOffset=${offset})`,
    );
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-rt-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const lines: string[] = [];
  for (const c of buildCases(spec)) {
    lines.push(JSON.stringify(await runCase(spec, c, scratch, fixtures)));
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(`wrote ${lines.length} regenerate-title oracle rows to ${outPath}\n`);
}

test('chat-regenerate-title-tier3 oracle', async () => {
  await main();
});
