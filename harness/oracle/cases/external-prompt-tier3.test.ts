/**
 * @jest-environment node
 *
 * P4.9K1 EXTERNAL-PROMPT ORACLE (tier 3): drives v4's REAL
 * `characters/[id]/handlers/post.ts` `generate-external-prompt` action —
 * `generateExternalPromptSchema.parse`, then `generateExternalPrompt`
 * (`lib/services/external-prompt-generator.service.ts`: the profile / key /
 * character / system-prompt / scenario resolution, `buildUserMessage`, the
 * `getSafeInputLimit` budget refusal, the ONE model call, the result bag) —
 * over a FRESH copy of the committed characters fixture per case, with the
 * model boundary mocked to a canned `sendMessage` that RECORDS the exact
 * request the Rust port must reproduce.
 *
 * Mocked ONLY where the Rust port injects a seam:
 *   - `@/lib/llm` `createLLMProvider` → `{ sendMessage }` answering the case's
 *     canned reply (or throwing its message) and recording
 *     `provider|baseUrl|model|temperature|maxTokens|cacheKey|profileParameters|
 *     messages|apiKey` — the assembled USER MESSAGE is the unit under test, so
 *     it is recorded whole.
 *   - `@/lib/startup` `isPluginSystemInitialized` → true (the provider
 *     registry is REAL: `initializeProviderRegistry` over the dist plugins, so
 *     `getSafeInputLimit`'s default context window is the registry's own, the
 *     same number v5's baked manifests carry).
 *   - `ensureProcessorRunning` no-op'd (no jobs on this path anyway).
 *
 * `logLLMCall` stays REAL and writes to the scratch data dir's llm-logs DB —
 * not dumped (the committed pair has no llm-logs partition; recorded).
 *
 * Per case: the response (status + body) and the recorded canned calls.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-external-prompt-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/external-prompt-tier3.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/characters.json" "$TMPO/fixtures/"
 *   cp "$V5W/harness/oracle/fixtures/external-prompt-tier3.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARACTERS_MAIN=$V5W/crates/quilltap-web/tests/fixtures/characters-main.db \
 *   QT_FIXTURE_CHARACTERS_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/characters-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-external-prompt.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- external-prompt-tier3
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

interface CharacterPatchSeed {
  kind: 'characterPatch';
  characterId: string;
  patch: Record<string, unknown>;
}
interface ProfilePatchSeed {
  kind: 'profilePatch';
  profileId: string;
  patch: Record<string, unknown>;
}
type Seed = CharacterPatchSeed | ProfilePatchSeed;

interface Reply {
  content?: string;
  usage?: { promptTokens: number; completionTokens: number; totalTokens: number };
  throws?: string;
}

interface CaseSpec {
  name: string;
  characterId: string;
  body: unknown;
  seeds?: Array<Seed | string>;
  reply: Reply;
}

interface Corpus {
  seedTimestamp: string;
  richAria: CharacterPatchSeed;
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
  apiKey: string;
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

  const calls: CannedCall[] = [];
  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async (provider: string, baseUrl?: string) => ({
        sendMessage: async (
          params: {
            model: string;
            messages: Array<{ role: string; content: string }>;
            temperature?: number;
            maxTokens?: number;
            cacheKey?: string;
            profileParameters?: unknown;
          },
          apiKey: string,
        ) => {
          calls.push({
            provider,
            baseUrl: baseUrl ?? null,
            model: params.model,
            temperature: params.temperature ?? null,
            maxTokens: params.maxTokens ?? null,
            cacheKey: params.cacheKey ?? null,
            profileParameters: params.profileParameters ?? null,
            messages: params.messages.map((m) => ({ role: m.role, content: m.content })),
            apiKey,
          });
          if (c.reply.throws !== undefined) throw new Error(c.reply.throws);
          return { content: c.reply.content, usage: c.reply.usage };
        },
      }),
    };
  });

  const work = mkdtempSync(join(scratch, 'ep-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  // The REAL provider registry — `getSafeInputLimit` reads its default window.
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
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();
  const repos = getRepositories();

  try {
    for (const raw of c.seeds ?? []) {
      const seed: Seed = typeof raw === 'string' ? corpus.richAria : raw;
      if (seed.kind === 'characterPatch') {
        await repos.characters.update(seed.characterId, seed.patch as never);
      } else if (seed.kind === 'profilePatch') {
        await repos.connections.update(seed.profileId, seed.patch as never);
      }
    }

    const url = `http://localhost/api/v1/characters/${c.characterId}?action=generate-external-prompt`;
    const { POST } = (await import('@/app/api/v1/characters/[id]/route')) as {
      POST: (...a: unknown[]) => Promise<unknown>;
    };
    const response = (await POST(mockRequest(url, c.body), {
      params: Promise.resolve({ id: c.characterId }),
    })) as { status: number; json: () => Promise<unknown> };
    const status = response.status;
    const body = await response.json();
    return { name: c.name, status, body, calls };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'characters.json'), 'utf8'),
  ) as Spec;
  const corpus = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'external-prompt-tier3.json'), 'utf8'),
  ) as Corpus;

  const fixtures = {
    main: process.env.QT_FIXTURE_CHARACTERS_MAIN ?? '',
    mount: process.env.QT_FIXTURE_CHARACTERS_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-external-prompt-'));
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
  process.stderr.write(`external-prompt oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('external-prompt tier-3 oracle', async () => {
  await main();
});
