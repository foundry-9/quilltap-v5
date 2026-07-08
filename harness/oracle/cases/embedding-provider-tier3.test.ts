/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for v4's `generateEmbeddingForUser` (P4.1a deliverable 4 — the
 * API-path EmbeddingProvider). Drives v4's REAL embedding service over a baked
 * fixture (embedding profiles + api keys + a fitted BUILTIN vocabulary), with
 * ONLY the HTTP boundary mocked: `global.fetch` is scripted per case (which also
 * intercepts the bundled OpenRouter SDK — its responses must be real `Response`
 * objects), recording every request (method / url / raw body string) and the
 * exact response text served. The Rust side registers a CannedWireTransport from
 * the recorded wire, so a request-building divergence is a canned miss.
 *
 * jest-real-db mocks (see [[jest-real-db-oracle]]): better-sqlite3 → the cipher
 * driver, manager / repositories / factory → requireActual, AND
 * `@/lib/embedding/embedding-service` itself → requireActual (jest.setup mocks
 * it globally). The provider registry is initialized the runtime way with the
 * openai / ollama / openrouter / builtin-embeddings dist bundles.
 *
 * NDJSON per case: { case, result: {embedding, model, dimensions, provider} |
 * error: {message, provider}, wire: [{method,url,body,status,
 * statusText,responseBody}] }. NOTE (verified against source + empirically):
 * v4 `generateEmbedding`'s outer catch is DEAD CODE — the body does
 * `return generateApiEmbedding(...)` WITHOUT await inside the try, so an async
 * rejection bypasses the catch and PLAIN errors escape UNwrapped (no
 * "Embedding failed for provider …" wrap ever fires). The oracle therefore
 * records plain errors too, with `isEmbeddingError: false` and provider null.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_EMBED_MAIN=/tmp/qt-embed-main.db \
 *   QT_FIXTURE_EMBED_MOUNT=/tmp/qt-embed-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-embedding-provider-fixture.ts
 *   QT_FIXTURE_EMBED_MAIN=/tmp/qt-embed-main.db \
 *   QT_FIXTURE_EMBED_MOUNT=/tmp/qt-embed-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-embedding-provider.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$V5/harness/oracle/cases" -- embedding-provider-tier3
 */

import * as fs from 'fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface WireEntry {
  urlSuffix: string;
  status: number;
  statusText: string;
  body: unknown;
}

interface Case {
  id: string;
  user: string;
  profileId: string | null;
  text: string;
  priority?: 'interactive' | 'background';
  wire: WireEntry[];
}

interface Spec {
  testPepperBase64: string;
  users: Record<string, string>;
  cases: Case[];
}

type RecordedCall = {
  method: string;
  url: string;
  body: string;
  status: number;
  statusText: string;
  responseBody: string;
};

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'embedding-provider-tier3.json'), 'utf8')
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_EMBED_MAIN;
  const mountFixture = process.env.QT_FIXTURE_EMBED_MOUNT;
  if (!mainFixture || !mountFixture || !existsSync(mainFixture) || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_EMBED_MAIN / QT_FIXTURE_EMBED_MOUNT must point at the built fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );

  // One copy for the whole run — the embedding path only READS.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-embed-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'embed-main.db');
  const mountWork = join(scratch, 'embed-mount.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  delete process.env.BASE_URL; // the OpenRouter referer default path
  process.env.LOG_LEVEL = 'error';

  jest.resetModules();
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories')
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  // jest.setup mocks the embedding service itself — wire the REAL one back in.
  jest.doMock('@/lib/embedding/embedding-service', () =>
    jest.requireActual('@/lib/embedding/embedding-service')
  );
  // jest.setup's provider-validation partial lacks fields the registry may read.
  jest.doMock('@/lib/plugins/provider-validation', () =>
    jest.requireActual('@/lib/plugins/provider-validation')
  );

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );

  // Initialize the REAL provider registry with the plugins the embedding path
  // consults (openai / ollama / openrouter wire + the BUILTIN local provider).
  {
    const nodeRequire = createRequire(join(process.cwd(), 'noop.js'));
    const PLUGIN_DIRS = ['openai', 'ollama', 'openrouter', 'builtin-embeddings'];
    const { initializeProviderRegistry } = await import('@/lib/plugins/provider-registry');
    const providers = PLUGIN_DIRS.map((d) => {
      const m = nodeRequire(join(process.cwd(), 'plugins', 'dist', `qtap-plugin-${d}`, 'index.js'));
      return m.plugin || m.default?.plugin || m.default;
    });
    await initializeProviderRegistry(providers);
  }

  const embeddingService = await import('@/lib/embedding/embedding-service');
  const { generateEmbeddingForUser, EmbeddingError } = embeddingService as unknown as {
    generateEmbeddingForUser: (
      text: string,
      userId: string,
      profileId?: string,
      opts?: { priority?: 'interactive' | 'background' }
    ) => Promise<{ embedding: Float32Array; model: string; dimensions: number; provider: string }>;
    EmbeddingError: new (...a: unknown[]) => Error & { provider?: string };
  };

  // The per-case scripted fetch — records every call + the response served.
  // Real `Response` objects (the bundled OpenRouter SDK consumes them).
  let script: WireEntry[] = [];
  let recorded: RecordedCall[] = [];
  const realFetch = global.fetch;
  // @ts-expect-error override
  global.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
    let url: string;
    let method = 'GET';
    let body = '';
    if (input instanceof Request) {
      url = input.url;
      method = input.method;
      body = await input.clone().text();
    } else {
      url = String(input);
      method = init?.method || 'GET';
      body = typeof init?.body === 'string' ? init.body : String(init?.body ?? '');
    }
    const entry = script.find((e) => url.endsWith(e.urlSuffix));
    if (!entry) {
      throw new Error(`no wire script entry for ${method} ${url}`);
    }
    const responseBody = JSON.stringify(entry.body);
    recorded.push({
      method,
      url,
      body,
      status: entry.status,
      statusText: entry.statusText,
      responseBody,
    });
    return new Response(responseBody, {
      status: entry.status,
      statusText: entry.statusText,
      headers: { 'content-type': 'application/json' },
    });
  };

  const lines: string[] = [];
  try {
    await initializeDatabase();

    for (const c of spec.cases) {
      script = c.wire;
      recorded = [];
      const userId = spec.users[c.user];
      if (!userId) throw new Error(`unknown user ref in case ${c.id}`);

      let row: Record<string, unknown>;
      try {
        const r = await generateEmbeddingForUser(
          c.text,
          userId,
          c.profileId ?? undefined,
          c.priority ? { priority: c.priority } : {}
        );
        row = {
          case: c.id,
          result: {
            embedding: Array.from(r.embedding),
            model: r.model,
            dimensions: r.dimensions,
            provider: r.provider,
          },
        };
      } catch (e) {
        // Plain errors DO escape (the dead outer catch — header note); an
        // EmbeddingError carries its provider, a plain Error has none.
        const isEmbeddingError = e instanceof EmbeddingError;
        row = {
          case: c.id,
          error: {
            message: (e as Error).message,
            provider: isEmbeddingError
              ? ((e as Error & { provider?: string }).provider ?? null)
              : null,
          },
        };
      }
      row.wire = recorded;
      lines.push(JSON.stringify(row));
    }
  } finally {
    global.fetch = realFetch;
    await new Promise((resolve) => setTimeout(resolve, 50));
    closeMountIndexSQLiteClient();
    await closeDatabase();
    rmSync(scratch, { recursive: true, force: true });
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`embedding-provider oracle wrote ${outPath} (${lines.length} cases)\n`);
}

test('embedding-provider tier-3 oracle', async () => {
  await main();
});
