/**
 * @jest-environment node
 *
 * P4.9K1 CHARACTER-OPTIMIZER ORACLE (tier 3): drives v4's REAL
 * `characters/[id]/handlers/post.ts` `optimize-stream` action —
 * `optimizeStreamSchema.parse`, then `runCharacterOptimizer`
 * (`lib/services/character-optimizer.service.ts`, the POST-bug-119 shape:
 * the memory pipeline, the analysis call, the per-sub-step passes with
 * `coerceSuggestionArray` + the containment, the suggestions-file writer) —
 * over a FRESH copy of the committed `character-generators-{main,mount}.db`
 * pair per case, and emits the decoded SSE frame trace (every `onProgress`
 * event, verbatim), the recorded model calls, the vault's `Suggestions/` files
 * after the run, and the four log lines the port pins.
 *
 * Mocked ONLY where the Rust port injects a seam:
 *   - `@/lib/llm` `createLLMProvider` → `{ sendMessage }` answering the case's
 *     scripted `calls` BY CALL INDEX (a string = an answer, `{throws}` = a
 *     thrown error) and recording `provider|model|temperature|maxTokens|
 *     cacheKey|profileParameters|messages` — the prompts are the port.
 *   - `@/lib/embedding/embedding-service` `generateEmbeddingForUser` → the
 *     case's canned vector by exact query text, or a throw. `isEmbeddingAvailable`
 *     stays REAL (the fixture's default embedding profile makes it true), and
 *     so does `@/lib/embedding/vector-store` over the fixture's `vector_indices`.
 *   - `@/lib/startup` `isPluginSystemInitialized` → true; the provider registry
 *     is REAL (`initializeProviderRegistry` over the dist plugins).
 *   - `Date` frozen at the corpus `frozenNowMs` (the suggestions-file stamp,
 *     the memory-weight decay, `Date.now()` durations).
 *   - `ensureProcessorRunning` no-op'd (no jobs on this path).
 *
 * `logLLMCall` stays REAL and writes to the scratch data dir's llm-logs DB —
 * not dumped (recorded). The four `[CharacterOptimizer]` log lines the port
 * pins are captured by wrapping the logger.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-character-optimizer-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/character-optimizer-tier3.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/character-generators.json" "$TMPO/fixtures/"
 *   cp "$V5W/harness/oracle/fixtures/character-optimizer-tier3.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CG_MAIN=$V5W/crates/quilltap-web/tests/fixtures/character-generators-main.db \
 *   QT_FIXTURE_CG_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/character-generators-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-character-optimizer.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- character-optimizer-tier3
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
  characterId: string;
  body: unknown;
  calls: CallSpec[];
  embeddings?: Record<string, number[]>;
  embeddingThrows?: boolean;
}

interface Corpus {
  frozenNowMs: number;
  analysis: string;
  subSteps: Record<string, string>;
  cases: CaseSpec[];
}

interface CannedCall {
  provider: string;
  baseUrl: string | null;
  model: string;
  temperature: number | null;
  maxTokens: number | null;
  cacheKey: string | null;
  profileParameters: unknown;
  messages: Array<{ role: string; content: string }>;
}

/** Resolve a scripted call: a named corpus block, a literal JSON string, or an object. */
function resolveCall(corpus: Corpus, c: CallSpec): { content?: string; throws?: string } {
  if (typeof c === 'string') {
    if (c === 'analysis') return { content: corpus.analysis };
    if (c in corpus.subSteps) return { content: corpus.subSteps[c] };
    return { content: c };
  }
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

const RealDate = Date;

/** Freeze the wall clock at `nowMs` (constructor + `Date.now`), keeping every other Date use intact. */
function freezeClock(nowMs: number): void {
  class FrozenDate extends RealDate {
    constructor(...args: unknown[]) {
      if (args.length === 0) {
        super(nowMs);
      } else {
        super(...(args as [number]));
      }
    }
    static now(): number {
      return nowMs;
    }
  }
  (globalThis as { Date: unknown }).Date = FrozenDate;
}
function unfreezeClock(): void {
  (globalThis as { Date: unknown }).Date = RealDate;
}

const SUGGESTIONS_SQL =
  'SELECT l.relativePath AS relativePath, d.content AS content FROM doc_mount_file_links l ' +
  'JOIN doc_mount_documents d ON d.fileId = l.fileId ' +
  "WHERE l.mountPointId = ? AND l.relativePath LIKE 'Suggestions/%' ORDER BY l.relativePath";

async function runCase(
  spec: Spec,
  corpus: Corpus,
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
  jest.doMock('@/lib/mount-index/database-store', () =>
    jest.requireActual('@/lib/mount-index/database-store'),
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

  // The embedding seam: canned by exact query text, or a throw.
  jest.doMock('@/lib/embedding/embedding-service', () => {
    const actual = jest.requireActual('@/lib/embedding/embedding-service');
    const { EmbeddingError } = actual;
    return {
      __esModule: true,
      ...actual,
      generateEmbeddingForUser: async (text: string) => {
        if (c.embeddingThrows) throw new EmbeddingError('canned embedding failure');
        const vec = c.embeddings?.[text];
        if (!vec) throw new EmbeddingError(`no canned embedding registered for input (${text.length} chars)`);
        return { embedding: new Float32Array(vec), model: 'canned', dimensions: vec.length, provider: 'canned' };
      },
    };
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
          messages: Array<{ role: string; content: string }>;
          temperature?: number;
          maxTokens?: number;
          cacheKey?: string;
          profileParameters?: unknown;
        }) => {
          calls.push({
            provider,
            baseUrl: baseUrl ?? null,
            model: params.model,
            temperature: params.temperature ?? null,
            maxTokens: params.maxTokens ?? null,
            cacheKey: params.cacheKey ?? null,
            profileParameters: params.profileParameters ?? null,
            messages: params.messages.map((m) => ({ role: m.role, content: m.content })),
          });
          const scripted = c.calls[callIndex];
          callIndex += 1;
          if (scripted === undefined) throw new Error(`no scripted call #${callIndex - 1} for case ${c.name}`);
          const r = resolveCall(corpus, scripted);
          if (r.throws !== undefined) throw new Error(r.throws);
          return { content: r.content, usage: { promptTokens: 1, completionTokens: 1, totalTokens: 2 } };
        },
      }),
    };
  });

  // The four pinned log lines: spy on the REAL logger's methods after import
  // (a doMock of `@/lib/logger` trips its own transports' circular import).
  const logLines: Array<{ level: string; message: string; context: unknown }> = [];

  const work = mkdtempSync(join(scratch, 'opt-'));
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
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();
  const repos = getRepositories();

  const loggerModule = (await import('@/lib/logger')) as { logger: Record<string, (...a: unknown[]) => unknown> };
  const spies: Array<{ mockRestore: () => void }> = [];
  for (const level of ['info', 'warn', 'error', 'debug']) {
    const original = loggerModule.logger[level];
    const spy = jest.spyOn(loggerModule.logger as never, level as never).mockImplementation(
      ((message: string, context?: unknown, err?: unknown) => {
        if (typeof message === 'string' && message.startsWith('[CharacterOptimizer]')) {
          logLines.push({ level, message, context: context ?? null });
        }
        return original.call(loggerModule.logger, message, context, err);
      }) as never,
    );
    spies.push(spy);
  }

  try {
    freezeClock(corpus.frozenNowMs);
    const url = `http://localhost/api/v1/characters/${c.characterId}?action=optimize-stream`;
    const { POST } = (await import('@/app/api/v1/characters/[id]/route')) as {
      POST: (...a: unknown[]) => Promise<unknown>;
    };
    const response = (await POST(mockRequest(url, c.body), {
      params: Promise.resolve({ id: c.characterId }),
    })) as { status: number; headers: Headers; json: () => Promise<unknown>; body: ReadableStream<Uint8Array> | null };
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
    unfreezeClock();

    // The vault's Suggestions/ files after the run (the suggestions-file mode).
    const character = await repos.characters.findByIdRaw(c.characterId);
    const mountId = (character as { characterDocumentMountPointId?: string } | null)?.characterDocumentMountPointId ?? null;
    const midb = getRawMountIndexDatabase();
    const suggestionsFiles = mountId && midb ? midb.prepare(SUGGESTIONS_SQL).all(mountId) : [];

    return { name: c.name, status, body, events, calls, suggestionsFiles, logLines };
  } finally {
    unfreezeClock();
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
    fs.readFileSync(join(here, '..', 'fixtures', 'character-optimizer-tier3.json'), 'utf8'),
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-character-optimizer-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const outLines: string[] = [];
  for (const c of corpus.cases) {
    const payload = await runCase(spec, corpus, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`character-optimizer oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('character-optimizer tier-3 oracle', async () => {
  await main();
});
