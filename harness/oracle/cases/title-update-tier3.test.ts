/**
 * @jest-environment node
 *
 * P4.6ao unit-3 TIER-3 (mocked-LLM) ORACLE for the `TITLE_UPDATE` job handler:
 * drives v4's REAL `handleTitleUpdate` over a FRESH copy of the committed
 * cost-background fixture per case, with the cheap-LLM provider CANNED, and
 * dumps the resulting DB state (the chat row's title + checkpoint cursor, the
 * TITLE_GENERATION system events, and the background_jobs rows) so the Rust port
 * diffs the writes tier-2 style.
 *
 * The canned provider is keyed by the SYSTEM prompt, which also proves the right
 * evaluator prompt reached it: the literary `CHAT_TITLE_CONSIDERATION_PROMPT` vs
 * the practical `HELP_CHAT_TITLE_CONSIDERATION_PROMPT`. `spec.cannedTitles` names
 * the reply per key.
 *
 * Cases: the manually-renamed early return (cursor STILL advances, no LLM call);
 * the help arm; the normal arm producing a title (chat row + the TITLE_GENERATION
 * event + the enqueued STORY_BACKGROUND_GENERATION row); the no-rename-needed
 * verdict (cursor only); the unparseable reply (same); the provider throw (cursor
 * only, no event); the two background-gate skips (autonomous chat / disabled
 * settings — renamed, NO job); and the missing-chat throw.
 *
 * P4.D110 (v4 `3c041e46`, bug 96) adds seven title-verdict cases driving the
 * tolerant parser through the REAL handler, so key recovery is measured as a
 * WRITE — the renamed chat row and the story-background job's scene context —
 * rather than as a parser return value: the exact live `suggestTitle` payload,
 * a `suggested_title` fold hit, canonical-beats-near-miss, a rename with
 * nothing readable (cursor burns, title unchanged), a quoted+padded title (the
 * second trim), an overlong title recovered from a near-miss key, and an
 * explicit-null canonical key falling through to a later one.
 *
 * P4.D215 (v4 `00c290c9a`, bugs 163/164) routes the rename through
 * `applyAutoTitle` and adds two arms only the chokepoint can produce: a verdict
 * suggesting the title the chat already has (`unchanged_title` — cursor only,
 * NO background), and a hand rename made while the LLM call is in flight
 * (`renamed_mid_flight` — planted by the canned provider through v4's real
 * `chats.update`, then kept by the chokepoint's post-call re-read).
 *
 * Seams mocked, and why:
 *   - `createLLMProvider` — the tier-3 model boundary (the Rust side injects the
 *     same canned reply).
 *   - `estimateMessageCost` → `{cost: null}` — host-resolved, the established
 *     context-summary-tier3 arrangement (`:328`); the Rust seam is NoMessageCost.
 *   - the background-jobs processor — off, so it can't claim the row this
 *     handler enqueues and race the dump (the P4.6y lesson).
 *   - `Date` — frozen, so `updatedAt` is deterministic on both sides.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-tu-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/title-update-tier3.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/cost-background-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CB_MAIN=$V5W/crates/quilltap-web/tests/fixtures/cost-background-main.db \
 *   QT_FIXTURE_CB_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/cost-background-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-title-update.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- title-update-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  frozenNowMs: number;
  userEnabledId: string;
  userDisabledId: string;
  connectionProfileId: string;
  chatTitleId: string;
  chatHelpId: string;
  chatAutonomousId: string;
  chatRenamedId: string;
  chatMixedStatusId: string;
  chatAllLeftId: string;
  chatRegenId: string;
  missingId: string;
  cannedTitles: Record<string, { content: string; promptTokens: number; completionTokens: number }>;
}

const RealDate = Date;

/** Which canned key a system prompt selects (also the prompt-identity proof). */
function keyForSystemPrompt(system: string): 'help' | 'literary' {
  return system.includes('help chat title evaluator') ? 'help' : 'literary';
}

interface CaseSpec {
  name: string;
  chat: (s: Spec) => string;
  user?: (s: Spec) => string;
  currentInterchange?: number;
  /** Override the canned reply for this case (else spec.cannedTitles). */
  reply?: (key: 'help' | 'literary') => { content: string; promptTokens: number; completionTokens: number };
  /** Make the provider throw instead of replying. */
  providerThrows?: boolean;
  /**
   * The message it throws with. Defaults to `canned provider failure`; a
   * timeout-shaped message is what drives `isTimeoutFailure` → the same-route
   * retry → `timedOut` → `throwIfLostToTimeout` (bug 107).
   */
  providerThrowMessage?: string;
  /** The handler is expected to throw; record the message instead of state. */
  expectThrow?: boolean;
  /**
   * P4.D215 (v4 `00c290c9a`, bug 164): the user renames the chat by hand WHILE
   * the cheap-LLM call is in flight. The canned provider writes the rename
   * through v4's real `chats.update` before it answers, so the chokepoint's
   * post-call re-read is the only thing that can see it.
   */
  midFlightRename?: boolean;
  /**
   * P4.112: the chat row is DELETED while the cheap-LLM call is in flight
   * (raw `DELETE FROM chats` — the same statement both sides, so the arm
   * isolates the chokepoint rather than any delete cascade). The chokepoint's
   * post-call re-read finds nothing: `applyAutoTitle`'s `missing` outcome,
   * which writes NOTHING — not even the `extraPatch` cursor.
   */
  deleteInFlight?: boolean;
}

/** The hand rename `midFlightRename` plants (both sides write these bytes). */
const MID_FLIGHT_TITLE = 'A Title Chosen Mid-Flight';

function buildCases(): CaseSpec[] {
  return [
    // The user's own rename is never overridden — but the cursor still moves.
    { name: 'manually_renamed', chat: (s) => s.chatRenamedId },
    // The help arm: the practical evaluator prompt, a practical title.
    { name: 'help_chat', chat: (s) => s.chatHelpId },
    // The normal arm: renamed + TITLE_GENERATION event + a story-background job.
    { name: 'normal_renamed', chat: (s) => s.chatTitleId },
    // A verdict of "no rename needed" — cursor only, but the spend still logs.
    {
      name: 'no_rename_needed',
      chat: (s) => s.chatTitleId,
      reply: () => ({
        content: '{"needsNewTitle": false, "reason": "the title still fits", "suggestedTitle": null}',
        promptTokens: 40,
        completionTokens: 8,
      }),
    },
    // An unparseable reply parses to the no-op verdict (NOT a task failure).
    {
      name: 'unparseable_reply',
      chat: (s) => s.chatTitleId,
      reply: () => ({ content: 'I am afraid I cannot do that.', promptTokens: 40, completionTokens: 6 }),
    },
    // A provider throw IS a task failure: cursor advances, no event, no job.
    { name: 'provider_throws', chat: (s) => s.chatTitleId, providerThrows: true },
    // …unless it TIMED OUT (bug 107). Then the check never ran, the cursor must
    // NOT be burned over it, and the handler throws so the job is retried. The
    // throw sits before the cursor write for exactly that reason.
    {
      name: 'provider_times_out',
      chat: (s) => s.chatTitleId,
      providerThrows: true,
      providerThrowMessage: 'Request timed out.',
      expectThrow: true,
    },
    // The two background-gate skips: renamed + the event, but NO job enqueued.
    { name: 'autonomous_no_background', chat: (s) => s.chatAutonomousId },
    {
      // Driven under the user whose storyBackgroundsSettings are DISABLED. NB:
      // this must use a chat WITH messages — pointed at an empty one it takes
      // the empty-conversation arm below and tests nothing.
      name: 'disabled_no_background',
      chat: (s) => s.chatTitleId,
      user: (s) => s.userDisabledId,
    },
    // The ONE arm that returns WITHOUT advancing the cursor (v4
    // title-update.ts:125-127): no visible conversation to evaluate.
    { name: 'empty_conversation', chat: (s) => s.chatRegenId },
    // ── Bug 96 (v4 `3c041e46`): the tolerant title-verdict parser, driven
    // through the REAL handler so the recovery is measured as a WRITE (the
    // renamed chat row + the story-background job's scene context), not as a
    // parser return value. Each canned reply is a shape the pre-fix parser
    // read as "no rename wanted".
    {
      // The exact live payload from Friday chat 745e8a5e: `suggestTitle`,
      // two letters short of the key the prompt asked for.
      name: 'key_typo_suggest_title',
      chat: (s) => s.chatTitleId,
      reply: () => ({
        content: JSON.stringify({
          needsNewTitle: true,
          reason: "The current title is generic and doesn't reflect the content.",
          suggestTitle: "The Beast's Hundred Gigajoules",
        }),
        promptTokens: 44,
        completionTokens: 12,
      }),
    },
    {
      // The fold pass: `suggested_title` matches no literal TITLE_KEYS entry
      // and is only reachable through `foldKey`.
      name: 'fold_key_snake_case',
      chat: (s) => s.chatTitleId,
      reply: () => ({
        content: JSON.stringify({
          needsNewTitle: true,
          reason: 'the title is generic',
          suggested_title: 'Amber Lines Above the Table',
        }),
        promptTokens: 41,
        completionTokens: 9,
      }),
    },
    {
      // Precedence: TITLE_KEYS is walked canonical-first, so the canonical key
      // wins even when a near-miss appears earlier in the object.
      name: 'canonical_beats_near_miss',
      chat: (s) => s.chatTitleId,
      reply: () => ({
        content: JSON.stringify({
          needsNewTitle: true,
          reason: 'the title is generic',
          title: 'The Wrong One',
          suggestedTitle: 'The Right One',
        }),
        promptTokens: 42,
        completionTokens: 11,
      }),
    },
    {
      // The residue the fix makes LOUD but does not rescue: a rename asked for
      // under a key nothing can read. The cursor still burns (v4's comment at
      // title-update.ts:190-194) — that is the durable effect the warn narrates.
      name: 'rename_with_no_usable_title',
      chat: (s) => s.chatTitleId,
      reply: () => ({
        content: JSON.stringify({
          needsNewTitle: true,
          reason: 'the current title says nothing',
          headline: 'Under An Unknown Key',
        }),
        promptTokens: 40,
        completionTokens: 10,
      }),
    },
    {
      // `normalizeTitle` trims, strips ONE quote at each end, then trims AGAIN
      // — the second trim the pre-fix inline parsers lacked, so the padding
      // inside the quotes used to survive into the stored title.
      name: 'quoted_padded_title',
      chat: (s) => s.chatTitleId,
      reply: () => ({
        content: JSON.stringify({
          needsNewTitle: true,
          reason: 'the title is generic',
          suggestedTitle: '  "  A Padded Quoted Title  "  ',
        }),
        promptTokens: 43,
        completionTokens: 10,
      }),
    },
    {
      // The 60-unit cap still applies to a title recovered from a near-miss key.
      name: 'overlong_near_miss_title',
      chat: (s) => s.chatTitleId,
      reply: () => ({
        content: JSON.stringify({
          needsNewTitle: true,
          reason: 'the title is generic',
          newTitle:
            'The Ballonet, the Beast, and the Long Interminable Descent Over Chicago',
        }),
        promptTokens: 45,
        completionTokens: 18,
      }),
    },
    {
      // Pass 1 skips an explicit null on the canonical key rather than stopping
      // there, so a title parked on a later key is still found.
      name: 'null_canonical_falls_through',
      chat: (s) => s.chatTitleId,
      reply: () => ({
        content: JSON.stringify({
          needsNewTitle: true,
          reason: 'the title is generic',
          suggestedTitle: null,
          newTitle: 'Recovered From A Null',
        }),
        promptTokens: 42,
        completionTokens: 9,
      }),
    },
    // ── [P4.D146 / v4 70505745a] The story-background enqueue's presence gate.
    // `queueStoryBackgroundIfEnabled` collects `chat.participants.filter(p =>
    // isParticipantPresent(p.status) && p.characterId)`, so the enqueued job's
    // payload `characterIds` is the comparand: absent and soft-removed
    // participants must not appear, and with nobody present no job is enqueued
    // at all (the rename + its TITLE_GENERATION event still happen).
    { name: 'present_participants_only', chat: (s) => s.chatMixedStatusId },
    { name: 'all_participants_left', chat: (s) => s.chatAllLeftId },
    // The throwing read.
    { name: 'chat_missing', chat: (s) => s.missingId, expectThrow: true },
    // ── P4.D215 (v4 `00c290c9a`, bugs 163/164): the job through `applyAutoTitle`.
    // The verdict suggests the title the chat already has: the chokepoint's
    // unchanged arm writes ONLY the cursor (its `extraPatch`) and queues NO
    // background — before the commit the handler wrote the title and queued.
    {
      name: 'unchanged_title',
      chat: (s) => s.chatTitleId,
      reply: () => ({
        content: JSON.stringify({
          needsNewTitle: true,
          reason: 'the title should say what it already says',
          suggestedTitle: 'New Chat',
        }),
        promptTokens: 40,
        completionTokens: 9,
      }),
    },
    // A hand rename during the LLM call wins: the chokepoint re-reads the chat
    // after the call, keeps the user's title, writes only the cursor, queues
    // nothing. (The up-front `isManuallyRenamed` gate read the chat BEFORE the
    // rename, so only the re-read can catch it.)
    { name: 'renamed_mid_flight', chat: (s) => s.chatTitleId, midFlightRename: true },
    // P4.112 — the chat vanishes during the LLM call: the chokepoint's
    // `missing` arm (the job discards the outcome, as v4's handler does).
    { name: 'deleted_mid_flight', chat: (s) => s.chatTitleId, deleteInFlight: true },
  ];
}

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

  // The tier-3 model boundary.
  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async () => ({
        sendMessage: async (params: { messages: Array<{ role: string; content: string }> }) => {
          if (c.providerThrows) throw new Error(c.providerThrowMessage ?? 'canned provider failure');
          if (c.midFlightRename) {
            // The same registry generation the handler imported (resetModules ran
            // before it), so this is v4's real repository on the case's DB copy.
            const { getRepositories } = await import('@/lib/repositories/factory');
            await getRepositories().chats.update(c.chat(spec), {
              title: MID_FLIGHT_TITLE,
              isManuallyRenamed: true,
            } as never);
          }
          if (c.deleteInFlight) {
            const { rawQuery } = await import('@/lib/database/manager');
            await rawQuery(`DELETE FROM chats WHERE id = '${c.chat(spec)}'`);
          }
          const system = params.messages.find((m) => m.role === 'system')?.content ?? '';
          const key = keyForSystemPrompt(system);
          const canned = c.reply ? c.reply(key) : spec.cannedTitles[key];
          if (!canned) throw new Error(`no canned reply for key ${key}`);
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

  // Host-resolved (the context-summary-tier3 arrangement); the Rust seam is
  // NoMessageCost, so both sides write estimatedCostUSD: null.
  jest.doMock('@/lib/services/cost-estimation.service', () => {
    const actual = jest.requireActual('@/lib/services/cost-estimation.service');
    return { __esModule: true, ...actual, estimateMessageCost: async () => ({ cost: null }) };
  });

  // Processor-off: it would claim the STORY_BACKGROUND_GENERATION row this
  // handler enqueues and race the dump.
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
  });

  // The API-key step runs BEFORE the (mocked) provider call and throws when no
  // key resolves — which `executeCheapLLMTask` catches into {success:false},
  // sending every case down the advance-cursor-and-return arm and quietly making
  // this whole oracle vacuous. The fixture seeds no `api_keys` row (the factory
  // exposes no repo for it), so hand back a canned key. The Rust port's boundary
  // starts at the provider call, so it has no equivalent step.
  jest.doMock('@/lib/services/api-key.service', () => {
    const actual = jest.requireActual('@/lib/services/api-key.service');
    return {
      __esModule: true,
      ...actual,
      getApiKeyForCheapLLMSelection: async () => 'canned-test-key',
    };
  });
}

/** The tier-2 diff surface: what the handler wrote. */
async function dumpState(chatId: string): Promise<unknown> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { BackgroundJobsRepository } = await import(
    '@/lib/database/repositories/background-jobs.repository'
  );
  const repos = getRepositories();

  const chat = (await repos.chats.findById(chatId)) as Record<string, unknown> | null;
  // P4.112: read unconditionally — `getMessages` queries by `chatId` and never
  // consults the chat row, so a chat deleted mid-job still shows any event
  // the job wrote for it (the `deleted_mid_flight` comparand).
  const messages = (await repos.chats.getMessages(chatId)) as Array<Record<string, unknown>>;
  const jobs = (await new BackgroundJobsRepository().findAll()) as Array<Record<string, unknown>>;

  return {
    chat: chat
      ? {
          title: chat.title,
          lastRenameCheckInterchange: chat.lastRenameCheckInterchange,
          updatedAt: chat.updatedAt,
        }
      : null,
    // Only the events this handler can create.
    systemEvents: messages
      .filter((m) => m.type === 'system' && m.systemEventType === 'TITLE_GENERATION')
      .map((m) => ({
        systemEventType: m.systemEventType,
        description: m.description,
        promptTokens: m.promptTokens,
        completionTokens: m.completionTokens,
        totalTokens: m.totalTokens,
        provider: m.provider,
        modelName: m.modelName,
        estimatedCostUSD: m.estimatedCostUSD ?? null,
      })),
    jobs: jobs
      .map((j) => ({ type: j.type, status: j.status, priority: j.priority, payload: j.payload }))
      .sort((a, b) => String(a.type).localeCompare(String(b.type))),
  };
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec, c);

  const work = mkdtempSync(join(scratch, 'tu-'));
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

  const frozen = spec.frozenNowMs;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  global.Date = class extends RealDate {
    constructor(...a: unknown[]) {
      if (a.length === 0) super(frozen);
      // @ts-expect-error forward variadic args
      else super(...a);
    }
    static now(): number {
      return frozen;
    }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
  } as any;

  try {
    const chatId = c.chat(spec);
    const userId = c.user ? c.user(spec) : spec.userEnabledId;
    const currentInterchange = c.currentInterchange ?? 5;
    const { handleTitleUpdate } = await import('@/lib/background-jobs/handlers/title-update');

    const job = {
      id: 'a0000000-0000-4000-8000-00000000job1',
      userId,
      type: 'TITLE_UPDATE',
      status: 'PROCESSING',
      payload: {
        chatId,
        connectionProfileId: spec.connectionProfileId,
        currentInterchange,
      },
      priority: 0,
      attempts: 1,
      maxAttempts: 3,
      scheduledAt: spec.seedTimestamp,
      createdAt: spec.seedTimestamp,
      updatedAt: spec.seedTimestamp,
    };

    let threw: string | null = null;
    try {
      await handleTitleUpdate(job as never);
    } catch (e) {
      threw = e instanceof Error ? e.message : String(e);
    }

    return {
      name: c.name,
      threw,
      ...(c.expectThrow ? {} : { state: await dumpState(chatId) }),
    };
  } finally {
    global.Date = RealDate;
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-tu-oracle-'));
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
  process.stderr.write(`title-update-tier3 oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('title-update-tier3 oracle', async () => {
  await main();
});
