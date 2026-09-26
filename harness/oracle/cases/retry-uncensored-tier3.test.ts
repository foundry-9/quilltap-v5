/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for "Try uncensored" (P4.D228; v4 `ce2f1dabf`, #77).
 *
 * Drives v4's REAL code over the purpose-built fixture
 * `build-retry-uncensored-fixture.ts` builds from `retry-uncensored.json`:
 *
 *   - `service` cases call `lib/services/dangerous-content/retry-uncensored.ts`
 *     directly — `resolveTextRetryUnderstudy`, `resolveImageRetryUnderstudy`,
 *     `composeRetryRouteTrail`, `mayRetryUncensored` — and
 *     `resolveConfiguredConciergeDesk`, over the stored chats, messages,
 *     profiles and settings;
 *   - `chat` cases POST the REAL `app/api/v1/chats/[id]/route.ts` with
 *     `?action=retry-image-uncensored` (the union decode, the 409 gate, the
 *     picture arm's trail / +1 ms filing / save / announcement, the background
 *     arm's REAL `handleRegenerateBackground` + enqueue);
 *   - `message` cases POST the REAL `app/api/v1/chats/[id]/messages/
 *     [messageId]/route.ts` with `?action=retry-uncensored[&stream=1]`.
 *
 * Two collaborators are RECORDING boundaries, and only two — each has its own
 * differential, and each is mocked as the ARGUMENT it is handed, never as a
 * decision (`a-v4-mock-is-not-v4-in-a-route-family`):
 *   - `executeImageGenerationTool` (the picture arm's generator — proven by
 *     `image_generation_tier3`, incl. its `primaryVia: 'concierge'` arm):
 *     records `(arguments, context)` and answers the case's canned result;
 *   - `regenerateMessageAsSwipe` / `streamSwipeRegeneration` (the message arm's
 *     generation — proven by `regenerate_swipe_tier3`, incl. its override
 *     arms): records the override profile, the trail, the target and the
 *     Speaking-As id, and answers a canned swipe (or throws the case's error).
 *
 * v4's own unit tests for #77 mock the GATE itself, the repositories and the
 * announcer — none of them is an oracle. Here the gate, the repositories, the
 * Concierge writer and the queue are real.
 *
 * API keys come from a canned seam on the connections repository (the
 * danger-routing arrangement). The cases run IN ORDER on ONE database (a
 * case's writes are visible to the next, on both sides); a case's settings
 * patch is applied by raw SQL, recorded verbatim for the Rust side to replay,
 * and reverted after the case.
 *
 * Per case: status + body (or the service result), the recorded boundary
 * calls, every `ConciergeRetryUncensored` / `[DangerousContent]` /
 * `[Chats v1]` / `[ConciergeNotification]` line, and the rows the case wrote
 * to `chat_messages` and `background_jobs`.
 *
 * Regen recipe: the header of
 * `crates/quilltap-harness/tests/retry_uncensored_tier3_equivalence.rs`.
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  return v;
}

interface Case {
  kind: 'service' | 'chat' | 'message';
  name: string;
  fn?: 'text' | 'image' | 'compose' | 'may' | 'desk';
  userId?: string;
  chatId?: string;
  messageId?: string;
  exclude?: Array<string | null> | 'chat';
  answeredFromTool?: boolean;
  profileKind?: 'connection' | 'image';
  answering?: string;
  chatIds?: string[];
  settingsList?: Array<Record<string, unknown> | null>;
  settings?: Record<string, unknown>;
  storyBackgroundsEnabled?: boolean;
  body?: unknown;
  malformed?: boolean;
  imageResult?: Record<string, unknown>;
  plantJob?: boolean;
  stream?: boolean;
  serviceThrows?: string;
}
interface Spec {
  testPepperBase64: string;
  apiKeys: Record<string, string>;
  cases: Case[];
}

const SERVICES = new Set(['ConciergeRetryUncensored']);
const PREFIXES = ['[DangerousContent]', '[Chats v1]', '[ConciergeNotification]'];

function mockRequest(url: string, body: unknown, malformed: boolean): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: malformed
      ? jest.fn().mockRejectedValue(new SyntaxError('Unexpected token'))
      : jest.fn().mockResolvedValue(body),
  };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'retry-uncensored.json'), 'utf8'),
  ) as Spec;
  const fixtureMain = process.env.QT_FIXTURE_RETRY_UNCENSORED_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_RETRY_UNCENSORED_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_RETRY_UNCENSORED_MAIN / _MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-retry-uncensored-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'retry-main.db');
  const workMount = join(scratch, 'retry-mount.db');
  copyFileSync(fixtureMain, work);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  delete process.env.QUILLTAP_JOB_CHILD;
  process.env.LOG_LEVEL = 'error';

  let sessionUserId = '';
  const logs: Array<{ service: string | null; level: string; message: string; bag: unknown }> = [];
  const calls: unknown[] = [];
  let imageResult: Record<string, unknown> | null = null;
  let serviceThrows: string | null = null;

  jest.resetModules();
  jest.doMock('@/lib/logger', () => {
    const recorder = (service: string | null) => {
      const record = (level: string) => (message: string, bag?: Record<string, unknown>) =>
        logs.push({ service, level, message, bag: JSON.parse(JSON.stringify(bag ?? {})) });
      const self: Record<string, unknown> = {
        debug: record('debug'),
        info: record('info'),
        warn: record('warn'),
        error: record('error'),
        trace: record('trace'),
      };
      self.child = (ctx: Record<string, unknown>) =>
        recorder(typeof ctx?.service === 'string' ? ctx.service : service);
      return self;
    };
    return {
      __esModule: true,
      LogLevel: { ERROR: 'error', WARN: 'warn', INFO: 'info', DEBUG: 'debug', TRACE: 'trace' },
      logger: recorder(null),
    };
  });
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/services/concierge-notifications/writer', () =>
    jest.requireActual('@/lib/services/concierge-notifications/writer'),
  );
  jest.doMock('@/lib/services/markdown-renderer.service', () => ({
    __esModule: true,
    renderMarkdownToHtml: async () => null,
    canPreRenderMessage: () => false,
  }));
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: sessionUserId } }),
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
  // v4's enqueue kicks the in-process job processor, which CLAIMS a
  // freshly-queued row (PENDING → PROCESSING) and would RUN the story job
  // mid-case — an artifact of v4's runtime. The port's differential runs no
  // processor (the `cost-background-routes` arrangement).
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => {} };
  });
  // Boundary 1: the picture arm's generator.
  jest.doMock('@/lib/tools/handlers/image-generation-handler', () => {
    const actual = jest.requireActual('@/lib/tools/handlers/image-generation-handler');
    return {
      __esModule: true,
      ...actual,
      executeImageGenerationTool: async (input: unknown, context: unknown) => {
        calls.push({ boundary: 'executeImageGenerationTool', input, context });
        if (!imageResult) throw new Error('no canned image result for this case');
        return JSON.parse(JSON.stringify(imageResult));
      },
    };
  });
  // Boundary 2: the message arm's generation.
  const recordSwipe = (options: Record<string, any>, logContext?: string) => {
    const override = options.profileOverride as { profile: { id: string; name: string }; apiKey: string } | undefined;
    calls.push({
      boundary: logContext === undefined ? 'regenerateMessageAsSwipe' : 'streamSwipeRegeneration',
      ...(logContext === undefined ? {} : { logContext }),
      targetMessageId: options.targetMessage?.id,
      chatId: options.chat?.id,
      allMessageCount: options.allMessages?.length,
      activeUserParticipantId: options.activeUserParticipantId,
      overrideProfileId: override?.profile.id ?? null,
      overrideProfileName: override?.profile.name ?? null,
      overrideApiKey: override?.apiKey ?? null,
      routeTrail: options.routeTrail ?? null,
    });
  };
  jest.doMock('@/lib/services/chat-message', () => {
    const actual = jest.requireActual('@/lib/services/chat-message');
    return {
      __esModule: true,
      ...actual,
      regenerateMessageAsSwipe: async (options: Record<string, any>) => {
        recordSwipe(options);
        if (serviceThrows) throw new Error(serviceThrows);
        return { id: 'canned-swipe', type: 'message', role: 'ASSISTANT', content: 'canned' };
      },
      streamSwipeRegeneration: (options: Record<string, any>, logContext: string) => {
        recordSwipe(options, logContext);
        return new Response('data: {"canned":true}\n\n', {
          status: 200,
          headers: { 'Content-Type': 'text/event-stream' },
        });
      },
    };
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { ConciergeSettingsSchema } = await import('@/lib/schemas/settings.types');
  const retry = await import('@/lib/services/dangerous-content/retry-uncensored');
  const { resolveConfiguredConciergeDesk } = await import(
    '@/lib/services/dangerous-content/resolver.service'
  );

  await initializeDatabase();
  const repos = getRepositories();
  (repos.connections as any).findApiKeyByIdAndUserId = async (id: string, userId: string) => {
    const kv = spec.apiKeys[id];
    if (!kv) return null;
    return {
      id, userId, label: 'canned', provider: 'OPENAI', key_value: kv, isActive: true,
      createdAt: '2020-01-01T00:00:00.000Z', updatedAt: '2020-01-01T00:00:00.000Z',
    };
  };

  const { POST: chatPost } = await import('@/app/api/v1/chats/[id]/route');
  const { POST: messagePost } = await import('@/app/api/v1/chats/[id]/messages/[messageId]/route');

  const rowIds = async (table: string): Promise<Set<string>> =>
    new Set(((await rawQuery(`SELECT id FROM ${table}`)) as Array<{ id: string }>).map((r) => r.id));
  const rowsNotIn = async (table: string, before: Set<string>) => {
    const columns = ((await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>).map(
      (c) => c.name,
    );
    const raw = (await rawQuery(`SELECT * FROM ${table} ORDER BY rowid`)) as Array<Record<string, unknown>>;
    return raw
      .filter((r) => !before.has(r.id as string))
      .map((r) => {
        const out: Record<string, unknown> = {};
        for (const col of columns) out[col] = canonValue(r[col]);
        return out;
      });
  };

  const lines: string[] = [];
  for (const c of spec.cases) {
    // --- the case's settings patch (raw SQL, recorded for replay) ---
    const patches: Array<{ sql: string; params: unknown[] }> = [];
    const reverts: Array<{ sql: string; params: unknown[] }> = [];
    if (c.userId && (c.settings || c.storyBackgroundsEnabled !== undefined)) {
      const [row] = (await rawQuery(
        'SELECT conciergeSettings, storyBackgroundsSettings FROM chat_settings WHERE userId = ?',
        [c.userId],
      )) as Array<{ conciergeSettings: string; storyBackgroundsSettings: string }>;
      if (c.settings) {
        const text = JSON.stringify(ConciergeSettingsSchema.parse(c.settings));
        patches.push({ sql: 'UPDATE chat_settings SET conciergeSettings = ? WHERE userId = ?', params: [text, c.userId] });
        reverts.push({ sql: 'UPDATE chat_settings SET conciergeSettings = ? WHERE userId = ?', params: [row.conciergeSettings, c.userId] });
      }
      if (c.storyBackgroundsEnabled !== undefined) {
        const text = JSON.stringify({ ...JSON.parse(row.storyBackgroundsSettings), enabled: c.storyBackgroundsEnabled });
        patches.push({ sql: 'UPDATE chat_settings SET storyBackgroundsSettings = ? WHERE userId = ?', params: [text, c.userId] });
        reverts.push({ sql: 'UPDATE chat_settings SET storyBackgroundsSettings = ? WHERE userId = ?', params: [row.storyBackgroundsSettings, c.userId] });
      }
    }
    if (c.plantJob) {
      patches.push({
        sql:
          'INSERT INTO background_jobs (id, userId, type, status, payload, priority, attempts, maxAttempts, scheduledAt, createdAt, updatedAt) ' +
          "VALUES (?, ?, 'STORY_BACKGROUND_GENERATION', 'PENDING', ?, 0, 0, 3, ?, ?, ?)",
        params: [
          'b9000001-0000-4000-8000-000000000001',
          c.userId,
          JSON.stringify({ chatId: c.chatId, imageProfileId: '17000001-0000-4000-8000-000000000001', characterIds: [], sceneContext: 'planted' }),
          '2026-01-02T03:04:05.000Z',
          '2026-01-02T03:04:05.000Z',
          '2026-01-02T03:04:05.000Z',
        ],
      });
    }
    for (const p of patches) await rawQuery(p.sql, p.params as never);

    logs.length = 0;
    calls.length = 0;
    imageResult = c.imageResult ?? null;
    serviceThrows = c.serviceThrows ?? null;
    sessionUserId = c.userId ?? '';
    const msgsBefore = await rowIds('chat_messages');
    const jobsBefore = await rowIds('background_jobs');
    let result: Record<string, unknown>;

    if (c.kind === 'service') {
      const chat = c.chatId ? await repos.chats.findById(c.chatId) : null;
      const chatSettings = c.userId ? await repos.chatSettings.findByUserId(c.userId) : null;
      const message = c.messageId
        ? (await repos.chats.getMessages(c.chatId!)).find((m: any) => m.id === c.messageId)
        : undefined;
      const summarize = (g: any) =>
        g.ok
          ? { ok: true, understudyProfileId: g.understudy.profile.id, apiKey: g.understudy.apiKey }
          : { ok: false, reason: g.reason };
      if (c.fn === 'text') {
        result = summarize(
          await retry.resolveTextRetryUnderstudy({
            repos, userId: c.userId!, chat: chat as never, chatSettings, targetMessage: message as never,
          }),
        );
      } else if (c.fn === 'image') {
        let exclude: Array<string | null | undefined>;
        if (c.exclude === 'chat') exclude = [(chat as any).imageProfileId];
        else exclude = (c.exclude ?? []) as Array<string | null>;
        let answeredBy: { provider?: string; modelName?: string } | undefined;
        if (c.answeredFromTool) {
          const content = JSON.parse((message as any).content);
          answeredBy = { provider: content.provider, modelName: content.model };
        }
        result = summarize(
          await retry.resolveImageRetryUnderstudy({
            userId: c.userId!,
            chat: chat as never,
            chatSettings,
            excludeProfileIds: exclude,
            ...(c.answeredFromTool ? { trail: (message as any).routeTrail, answeredBy } : {}),
          }),
        );
      } else if (c.fn === 'compose') {
        const answering =
          c.profileKind === 'image'
            ? await repos.imageProfiles.findById(c.answering!)
            : await repos.connections.findById(c.answering!);
        const trail = retry.composeRetryRouteTrail(
          (message as any).routeTrail,
          answering as never,
          c.profileKind!,
        );
        result = { trailJson: JSON.stringify(trail) };
      } else if (c.fn === 'may') {
        const out: Record<string, boolean> = {};
        for (const id of c.chatIds!) out[id] = retry.mayRetryUncensored((await repos.chats.findById(id)) as never);
        result = { may: out };
      } else {
        result = {
          desks: c.settingsList!.map((s) =>
            resolveConfiguredConciergeDesk(s ? { conciergeSettings: ConciergeSettingsSchema.parse(s) } : null),
          ),
        };
      }
    } else if (c.kind === 'chat') {
      const url = `http://localhost/api/v1/chats/${c.chatId}?action=retry-image-uncensored`;
      const response = (await chatPost(
        mockRequest(url, c.body, c.malformed === true) as never,
        { params: Promise.resolve({ id: c.chatId }) } as never,
      )) as { status: number; json: () => Promise<unknown> };
      result = { status: response.status, body: await response.json() };
    } else {
      const url =
        `http://localhost/api/v1/chats/${c.chatId}/messages/${c.messageId}?action=retry-uncensored` +
        (c.stream ? '&stream=1' : '');
      const response = (await messagePost(
        mockRequest(url, {}, false) as never,
        { params: Promise.resolve({ id: c.chatId, messageId: c.messageId }) } as never,
      )) as { status: number; headers: Headers; json?: () => Promise<unknown>; text?: () => Promise<string> };
      const contentType = response.headers?.get?.('content-type') ?? '';
      result = contentType.startsWith('text/event-stream')
        ? { status: response.status, stream: true }
        : { status: response.status, body: await (response as any).json() };
    }

    const newMessages = await rowsNotIn('chat_messages', msgsBefore);
    const newJobs = await rowsNotIn('background_jobs', jobsBefore);
    for (const r of [...reverts].reverse()) await rawQuery(r.sql, r.params as never);
    if (c.plantJob) {
      await rawQuery('DELETE FROM background_jobs WHERE id = ?', ['b9000001-0000-4000-8000-000000000001']);
    }

    lines.push(
      JSON.stringify({
        kind: 'case',
        name: c.name,
        patches,
        result,
        calls: JSON.parse(JSON.stringify(calls)),
        logs: logs
          .filter(
            (l) =>
              (l.service !== null && SERVICES.has(l.service)) ||
              (l.service === null && PREFIXES.some((p) => l.message.startsWith(p))),
          )
          .map(({ service, level, message, bag }) => ({ service, level, message, bag })),
        newMessages,
        newJobs,
      }),
    );
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`retry-uncensored oracle wrote ${outPath} (${spec.cases.length} cases)\n`);
}

test('retry-uncensored tier-3 oracle', async () => {
  await main();
});
