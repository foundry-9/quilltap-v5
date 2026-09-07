/**
 * @jest-environment node
 *
 * P4.9K2 CHARACTER-WIZARD ORACLE (tier 3): drives v4's REAL
 * `characters/handlers/post.ts` `ai-wizard` and `ai-wizard-stream` actions —
 * `wizardRequestSchema.parse`, then `runCharacterWizard` /
 * `runCharacterWizardStreaming` (`lib/services/character-wizard.service.ts`:
 * the profile + api key, the image source through `profileSupportsMimeType`
 * and the vision call, the document source through `extractFileContent`, the
 * name-first rule, the per-field loop with its three dedicated generators) —
 * over a FRESH copy of the committed `character-generators-{main,mount}.db`
 * pair per case, and emits the status + body (the non-streaming twin's
 * `WizardResult`, or the refusal), the decoded SSE frame trace (the streaming
 * twin), the recorded model calls (attachments included), and the
 * `[CharacterWizard]` log lines.
 *
 * Mocked ONLY where the Rust port injects a seam:
 *   - `@/lib/llm` `createLLMProvider` → `{ sendMessage }` answering the case's
 *     scripted `calls` BY CALL INDEX (a string = an answer, `{throws}` = a
 *     thrown error) and recording `provider|baseUrl|model|temperature|maxTokens|
 *     profileParameters|messages` (with each message's attachments as
 *     `{id, filename, mimeType, data}`) — the prompts and the base64 bytes are
 *     the port. The vision call rides the same seam.
 *   - `@/lib/startup` `isPluginSystemInitialized` → true; the provider registry
 *     is REAL (`initializeProviderRegistry` over the dist plugins).
 *   - `ensureProcessorRunning` no-op'd.
 * `fileStorageManager.downloadFile`, `extractFileContent`,
 * `profileSupportsMimeType`, `trackActivity` and `logLLMCall` stay REAL (the
 * fixture's files are mount-blob-stored so the bytes come from the mount DB).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-character-wizard-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/character-wizard-tier3.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/character-generators.json" "$TMPO/fixtures/"
 *   cp "$V5W/harness/oracle/fixtures/character-wizard-tier3.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CG_MAIN=$V5W/crates/quilltap-web/tests/fixtures/character-generators-main.db \
 *   QT_FIXTURE_CG_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/character-generators-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-character-wizard.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- character-wizard-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

type CallSpec = string | { content?: string; throws?: string };

interface CaseSpec {
  name: string;
  action: 'ai-wizard' | 'ai-wizard-stream';
  body: unknown;
  calls: CallSpec[];
}

interface Corpus {
  cases: CaseSpec[];
}

interface RecordedAttachment {
  id: string;
  filename: string;
  mimeType: string;
  data: string;
}

interface CannedCall {
  provider: string;
  baseUrl: string | null;
  model: string;
  temperature: number | null;
  maxTokens: number | null;
  profileParameters: unknown;
  messages: Array<{ role: string; content: string; attachments?: RecordedAttachment[] }>;
}

function resolveCall(c: CallSpec): { content?: string; throws?: string } {
  if (typeof c === 'string') return { content: c };
  return c;
}

function mockRequest(url: string, body: unknown): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body),
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
  jest.doMock('@/lib/mount-index/database-store', () =>
    jest.requireActual('@/lib/mount-index/database-store'),
  );
  // jest.setup stubs the storage manager to return the literal string "mock
  // file content"; the document source and the vision call read the fixture's
  // REAL mount-blob bytes through the real manager.
  jest.doMock('@/lib/file-storage/manager', () =>
    jest.requireActual('@/lib/file-storage/manager'),
  );
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
  jest.doMock('@/lib/startup', () => {
    const actual = jest.requireActual('@/lib/startup');
    return { __esModule: true, ...actual, isPluginSystemInitialized: () => true };
  });
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
  });

  // The model seam: scripted by call index, recording every request.
  const calls: CannedCall[] = [];
  let callIndex = 0;
  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async (provider: string, baseUrl?: string) => ({
        sendMessage: async (params: {
          model: string;
          messages: Array<{
            role: string;
            content: string;
            attachments?: Array<{ id: string; filename: string; mimeType: string; data: string }>;
          }>;
          temperature?: number;
          maxTokens?: number;
          profileParameters?: unknown;
        }) => {
          calls.push({
            provider,
            baseUrl: baseUrl ?? null,
            model: params.model,
            temperature: params.temperature ?? null,
            maxTokens: params.maxTokens ?? null,
            profileParameters: params.profileParameters ?? null,
            messages: params.messages.map((m) => {
              const out: { role: string; content: string; attachments?: RecordedAttachment[] } = {
                role: m.role,
                content: m.content,
              };
              if (m.attachments && m.attachments.length > 0) {
                out.attachments = m.attachments.map((a) => ({
                  id: a.id,
                  filename: a.filename,
                  mimeType: a.mimeType,
                  data: a.data,
                }));
              }
              return out;
            }),
          });
          const scripted = c.calls[callIndex];
          callIndex += 1;
          if (scripted === undefined) throw new Error(`no scripted call #${callIndex - 1} for case ${c.name}`);
          const r = resolveCall(scripted);
          if (r.throws !== undefined) throw new Error(r.throws);
          return { content: r.content, usage: { promptTokens: 1, completionTokens: 1, totalTokens: 2 } };
        },
      }),
    };
  });

  const logLines: Array<{ level: string; message: string; context: unknown }> = [];

  const work = mkdtempSync(join(scratch, 'wiz-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  {
    const nodeRequire = createRequire(join(process.cwd(), 'noop.js'));
    const PLUGIN_DIRS = ['anthropic', 'openai', 'google', 'grok', 'deepseek', 'z-ai', 'openrouter', 'ollama', 'openai-compatible', 'nanogpt'];
    const { initializeProviderRegistry } = await import('@/lib/plugins/provider-registry');
    const providers = PLUGIN_DIRS.map((d) => {
      const m = nodeRequire(join(process.cwd(), 'plugins', 'dist', `qtap-plugin-${d}`, 'index.js'));
      return m.plugin || m.default?.plugin || m.default;
    });
    await initializeProviderRegistry(providers);
  }

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();

  const loggerModule = (await import('@/lib/logger')) as { logger: Record<string, (...a: unknown[]) => unknown> };
  const spies: Array<{ mockRestore: () => void }> = [];
  for (const level of ['info', 'warn', 'error', 'debug']) {
    const original = loggerModule.logger[level];
    const spy = jest.spyOn(loggerModule.logger as never, level as never).mockImplementation(
      ((message: string, context?: unknown, err?: unknown) => {
        if (typeof message === 'string' && message.startsWith('[CharacterWizard]')) {
          logLines.push({ level, message, context: context ?? null });
        }
        return original.call(loggerModule.logger, message, context, err);
      }) as never,
    );
    spies.push(spy);
  }

  try {
    const url = `http://localhost/api/v1/characters?action=${c.action}`;
    const { POST } = (await import('@/app/api/v1/characters/route')) as {
      POST: (...a: unknown[]) => Promise<unknown>;
    };
    const response = (await POST(mockRequest(url, c.body))) as {
      status: number;
      headers: Headers;
      json: () => Promise<unknown>;
      body: ReadableStream<Uint8Array> | null;
    };
    const status = response.status;
    const contentType = response.headers.get('content-type') ?? '';
    let body: unknown = null;
    const events: unknown[] = [];
    if (contentType.startsWith('text/event-stream') && response.body) {
      const decoder = new TextDecoder();
      const reader = response.body.getReader();
      let buffered = '';
      // eslint-disable-next-line no-constant-condition
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        if (value) buffered += decoder.decode(value, { stream: true });
      }
      buffered += decoder.decode();
      for (const line of buffered.split('\n')) {
        const t = line.trim();
        if (t.startsWith('data:')) {
          const payload = t.slice('data:'.length).trim();
          if (payload) events.push(JSON.parse(payload));
        }
      }
    } else {
      body = await response.json();
    }
    return { name: c.name, action: c.action, status, body, events, calls, logLines };
  } finally {
    for (const spy of spies) spy.mockRestore();
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'character-generators.json'), 'utf8'),
  ) as Spec;
  const corpus = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'character-wizard-tier3.json'), 'utf8'),
  ) as Corpus;

  const fixtures = {
    main: process.env.QT_FIXTURE_CG_MAIN ?? '',
    mount: process.env.QT_FIXTURE_CG_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-character-wizard-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const outLines: string[] = [];
  for (const c of corpus.cases) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`character-wizard oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('character-wizard tier-3 oracle', async () => {
  await main();
});
