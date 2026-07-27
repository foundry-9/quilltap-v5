/**
 * @jest-environment node
 *
 * P4.9E4A attach-mount-file ORACLE: drives v4's REAL
 * `POST /api/v1/chats/[id]/files?action=attach-mount-file` handler
 * (`handleAttachMountFile`, `app/api/v1/chats/[id]/files/route.ts:250`) over a
 * FRESH copy of the committed `attach-file-{main,mount,llmlogs}.db` fixture per
 * case, and emits each response body plus post-mutation table dumps so the Rust
 * port (`api::chat_media::chat_attach_mount_file`) diffs byte-for-byte.
 *
 * Cases, one per rung of v4's description ladder plus every error arm:
 *   cached / kept_image / vision (mocked) / refusal_retry (mocked, two calls) /
 *   reasoning (mocked, the max_tokens bump) / non_image / missing_file /
 *   missing_blob / no_chat / missing_mount_point_id / missing_relative_path /
 *   twice (two announcements, one `files` entry on the read-back).
 *
 * ## Seams (mirrored exactly on the Rust side)
 *   - `createLLMProvider(...)` → a canned `sendMessage` resolving each vision
 *     call by (provider, model, attachment filename) from `spec.vision`, and
 *     RECORDING the exact call it answered as a `canned` entry the Rust
 *     `CannedCompletionProvider` replays ([[tier3-completion-oracle]]).
 *   - `logLLMCall` runs REAL: the `IMAGE_DESCRIPTION` rows are diffed evidence,
 *     which is why the fixture ships an EMPTY llm-logs partition.
 *   - No image-codec mock: every corpus image is tiny, so
 *     `resizeImageForProvider` early-returns before touching the codec — which
 *     is what makes the Rust side's `NotConfiguredTranscoder` equivalent.
 *   - `getPricingCache` → empty (the logging cost lookup is host IO).
 *
 * The clock is NOT frozen: every minted value on this path (the announcement's
 * id + createdAt, the blob description's `descriptionUpdatedAt`) is either
 * blanked by the harness's shared normalization or deliberately left out of the
 * dumps. The `reasoning` case runs as a SECOND user, whose `chat_settings` point
 * at a `gpt-5-*` model — that is how v4's reasoning-model max-tokens bump is
 * reached without mutating the fixture mid-case.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-attach-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/attach-mount-file.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/attach-file.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   TZ=UTC \
 *   QT_FIXTURE_ATTACH_MAIN=$V5W/crates/quilltap-web/tests/fixtures/attach-file-main.db \
 *   QT_FIXTURE_ATTACH_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/attach-file-mount.db \
 *   QT_FIXTURE_ATTACH_LLMLOGS=$V5W/crates/quilltap-web/tests/fixtures/attach-file-llmlogs.db \
 *   QT_FIXTURE_ATTACH_META=$V5W/crates/quilltap-web/tests/fixtures/attach-file-main.db.meta.json \
 *   QT_ORACLE_OUT=/tmp/oracle-attach-mount-file.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- attach-mount-file
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface VisionSpec {
  provider: string;
  modelName: string;
  filename: string;
  content: string;
  finishReason: string | null;
  usage: { promptTokens: number; completionTokens: number; totalTokens: number } | null;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  reasoningUserId: string;
  seedTimestamp: string;
  vision: VisionSpec[];
}
interface Meta {
  mountPointId: string;
  linkIds: Record<string, string>;
  blobIds: Record<string, string>;
  ghostLink: string;
}

const CHAT = 'c1000000-0000-4000-8000-000000000001';
const MISSING_CHAT = '99999999-9999-4999-8999-999999999999';
const B = 'http://localhost:3000/api/v1';

/** Canned vision calls recorded during the run, keyed as the Rust lookup keys. */
const cannedRecorded = new Map<
  string,
  {
    provider: string;
    model: string;
    temperature: number | null;
    filename: string;
    mimeType: string;
    content: string;
    finishReason: string | null;
    usage: { promptTokens: number; completionTokens: number; totalTokens: number } | null;
  }
>();

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

function applyMocks(spec: Spec, sessionUserId: string): void {
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
  // jest.setup stubs the character-vault bridge to a single "mock-vault-mount";
  // un-mock it so the real vaults resolve ([[p4.6i-characters-remainder-server]]).
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
  );

  // The vision boundary: canned by (provider, model, attachment filename), and
  // RECORDED keyed exactly as the Rust CannedCompletionProvider looks up.
  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async (provider: string) => ({
        sendMessage: async (params: {
          model: string;
          temperature?: number;
          messages: Array<{
            role: string;
            content: string;
            attachments?: Array<{ filename: string; mimeType: string }>;
          }>;
        }) => {
          const att = params.messages?.[0]?.attachments?.[0];
          const filename = att?.filename ?? '';
          const mimeType = att?.mimeType ?? '';
          const entry = spec.vision.find(
            (v) => v.provider === provider && v.modelName === params.model && v.filename === filename,
          );
          if (!entry) {
            throw new Error(`no canned vision response for ${provider} ${params.model} ${filename}`);
          }
          const temperature = (params.temperature as number | undefined) ?? null;
          const key = `${provider}|${params.model}|${temperature ?? '-'}|${filename}|${mimeType}`;
          if (!cannedRecorded.has(key)) {
            cannedRecorded.set(key, {
              provider,
              model: params.model,
              temperature,
              filename,
              mimeType,
              content: entry.content,
              finishReason: entry.finishReason,
              usage: entry.usage,
            });
          }
          return { content: entry.content, finishReason: entry.finishReason, usage: entry.usage };
        },
      }),
    };
  });

  // API-key seams (host-side on the Rust port).
  jest.doMock('@/lib/plugins/provider-validation', () => {
    const actual = jest.requireActual('@/lib/plugins/provider-validation');
    return { __esModule: true, ...actual, requiresApiKey: () => false };
  });
  // logLLMCall runs REAL — the IMAGE_DESCRIPTION rows are diffed evidence.
  jest.doMock('@/lib/services/llm-logging.service', () =>
    jest.requireActual('@/lib/services/llm-logging.service'),
  );
  jest.doMock('@/lib/llm/pricing-fetcher', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/llm/pricing-fetcher'),
    getPricingCache: async () => ({ providers: {} }),
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
}

/** The chat's message events, minus the volatile per-row stamps. */
async function readMessages(chatId: string): Promise<unknown> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const repos = getRepositories();
  const events = (await repos.chats.getMessages(chatId)) as Array<Record<string, unknown>>;
  return events.map((e) => ({
    type: e.type,
    role: e.role ?? null,
    content: e.content ?? null,
    opaqueContent: e.opaqueContent ?? null,
    attachments: e.attachments ?? [],
    participantId: e.participantId ?? null,
    senderName: e.senderName ?? null,
    systemSender: e.systemSender ?? null,
  }));
}

/** The link-side description a describe caches (`updateDescription` writes it). */
async function readDescription(mountPointId: string, relativePath: string): Promise<unknown> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const repos = getRepositories();
  const blob = (await repos.docMountBlobs.findByMountPointAndPath(mountPointId, relativePath)) as
    | { description?: string }
    | null;
  return blob ? { description: blob.description ?? '' } : null;
}

/** llm_logs over stable columns (id / timestamps / durationMs excluded). */
async function readLlmLogsStable(): Promise<unknown> {
  const { getRawLLMLogsDatabase } = await import('@/lib/database/backends/sqlite/llm-logs-client');
  const db = getRawLLMLogsDatabase() as unknown as {
    prepare: (s: string) => { all: (...a: unknown[]) => Array<Record<string, unknown>> };
  } | null;
  if (!db) return [];
  const parse = (v: unknown): unknown => (typeof v === 'string' ? JSON.parse(v) : (v ?? null));
  const rows = db
    .prepare(
      'SELECT type, userId, chatId, messageId, characterId, provider, modelName, ' +
        'request, response, usage FROM llm_logs',
    )
    .all()
    .map((r) => ({
      ...r,
      request: parse(r.request),
      response: parse(r.response),
      usage: parse(r.usage),
    }));
  rows.sort((a, b) => (JSON.stringify(a) < JSON.stringify(b) ? -1 : 1));
  return rows;
}

interface CaseSpec {
  name: string;
  /** The session user the route runs as (defaults to the main user). */
  user?: 'main' | 'reasoning';
  run: (meta: Meta) => Promise<{ status: number; body: unknown; tables?: unknown }>;
}

async function loadRoute(path: string): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
  return (await import(path)) as never;
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

const FILES_ROUTE = '@/app/api/v1/chats/[id]/files/route';

/** POST ?action=attach-mount-file against a chat. */
async function attach(
  chatId: string,
  body: unknown,
): Promise<{ status: number; body: unknown }> {
  const r = await (await loadRoute(FILES_ROUTE)).POST(
    mockRequest(`${B}/chats/${chatId}/files?action=attach-mount-file`, body),
    { params: Promise.resolve({ id: chatId }) },
  );
  return respond(r);
}

async function runCase(
  spec: Spec,
  meta: Meta,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string; llmlogs: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec, c.user === 'reasoning' ? spec.reasoningUserId : spec.userId);

  const work = mkdtempSync(join(scratch, 'af-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  const llmLogsWork = join(work, 'llm-logs.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  copyFileSync(fixtures.llmlogs, llmLogsWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.SQLITE_LLM_LOGS_PATH = llmLogsWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  await initializeDatabase();

  try {
    const out = await c.run(meta);
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      ...(out.tables !== undefined ? { tables: out.tables } : {}),
    };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    closeLLMLogsSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'attach-file.json'), 'utf8'),
  ) as Spec;
  const meta = JSON.parse(
    fs.readFileSync(process.env.QT_FIXTURE_ATTACH_META ?? '', 'utf8'),
  ) as Meta;
  const fixtures = {
    main: process.env.QT_FIXTURE_ATTACH_MAIN ?? '',
    mount: process.env.QT_FIXTURE_ATTACH_MOUNT ?? '',
    llmlogs: process.env.QT_FIXTURE_ATTACH_LLMLOGS ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  const scratch = mkdtempSync(join(tmpdir(), 'qt-attach-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  /** An attach case that also dumps the messages + the blob description + logs. */
  const attachCase = (
    name: string,
    relativePath: string,
    user?: 'main' | 'reasoning',
  ): CaseSpec => ({
    name,
    user,
    run: async (m) => {
      const { status, body } = await attach(CHAT, {
        mountPointId: m.mountPointId,
        relativePath,
      });
      return {
        status,
        body,
        tables: {
          messages: await readMessages(CHAT),
          blob: await readDescription(m.mountPointId, relativePath),
          llmLogs: await readLlmLogsStable(),
        },
      };
    },
  });

  const cases: CaseSpec[] = [
    // ── The description ladder, rung by rung ──────────────────────────────
    attachCase('attach_cached', 'library/described.png'),
    attachCase('attach_kept_image', 'photos/kept-lantern.webp'),
    attachCase('attach_vision', 'library/undescribed.png'),
    attachCase('attach_refusal_retry', 'library/refuses.png'),
    attachCase('attach_reasoning', 'library/reasoning.png', 'reasoning'),
    attachCase('attach_non_image', 'library/ledger.txt'),
    // ── The error arms ────────────────────────────────────────────────────
    {
      name: 'attach_missing_file',
      run: async (m) =>
        attach(CHAT, { mountPointId: m.mountPointId, relativePath: 'library/nope.png' }),
    },
    {
      name: 'attach_missing_blob',
      run: async (m) =>
        attach(CHAT, { mountPointId: m.mountPointId, relativePath: 'library/ghost.md' }),
    },
    {
      name: 'attach_no_chat',
      run: async (m) =>
        attach(MISSING_CHAT, {
          mountPointId: m.mountPointId,
          relativePath: 'library/described.png',
        }),
    },
    {
      name: 'attach_missing_mount_point_id',
      run: async () => attach(CHAT, { relativePath: 'library/described.png' }),
    },
    {
      name: 'attach_missing_relative_path',
      run: async (m) => attach(CHAT, { mountPointId: m.mountPointId }),
    },
    // ── Double attach: two announcements, ONE `files` entry on the read-back.
    {
      name: 'attach_twice',
      run: async (m) => {
        const first = await attach(CHAT, {
          mountPointId: m.mountPointId,
          relativePath: 'library/described.png',
        });
        const second = await attach(CHAT, {
          mountPointId: m.mountPointId,
          relativePath: 'library/described.png',
        });
        const list = await respond(
          await (await loadRoute(FILES_ROUTE)).GET(mockRequest(`${B}/chats/${CHAT}/files`), {
            params: Promise.resolve({ id: CHAT }),
          }),
        );
        return {
          status: second.status,
          body: { first: first.body, second: second.body },
          tables: {
            messages: await readMessages(CHAT),
            filesList: list.body,
            filesListStatus: list.status,
          },
        };
      },
    },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, meta, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  // One trailing row carrying every canned vision call the run answered, so the
  // Rust CannedCompletionProvider registers exactly what v4 was asked.
  outLines.push(JSON.stringify({ name: 'canned', canned: [...cannedRecorded.values()] }));
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(`attach-mount-file oracle wrote ${outPath} (${outLines.length} rows)\n`);
}

test('attach-mount-file oracle', async () => {
  await main();
});
