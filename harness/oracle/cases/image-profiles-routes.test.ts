/**
 * @jest-environment node
 *
 * P4.6p IMAGE-PROFILES route-surface ORACLE: drives v4's REAL image-profile route
 * handlers over a FRESH copy of the committed groups-projects fixture per case,
 * emitting each response `{status, body(, tables)}` so `api::image_profiles::*` can
 * be diffed byte-for-byte. The 9 built-in provider plugins are registered per case
 * (the create/update provider probe + the empty-registry-otherwise gotcha).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-image-profiles-routes-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp $V5W/harness/oracle/cases/image-profiles-routes.test.ts "$TMPO/cases/"
 *   cp $V5W/harness/oracle/fixtures/groups-projects.json       "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_GP_MAIN=$V5W/crates/quilltap-web/tests/fixtures/groups-projects-main.db \
 *   QT_FIXTURE_GP_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/groups-projects-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-image-profiles-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- image-profiles-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { createRequire } from 'node:module';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

const APIKEY = 'a0000001-0000-4000-8000-000000000001';
const DIANA = 'a1000000-0000-4000-8000-000000000004';
const IP_1 = 'a6000000-0000-4000-8000-000000000001'; // Primary Imagery (default)
const IP_2 = 'a6000000-0000-4000-8000-000000000002'; // Scenic
const IP_3 = 'a6000000-0000-4000-8000-000000000003'; // Bare
const BOGUS = 'a6000000-0000-4000-8000-0000000000ff';

// NanoGPT is APPENDED, not slotted alphabetically — the same convention as
// `provider-registry.ts` / `providers-listing.ts` (P4.D101): the hardcoded list
// predates v4's alphabetical `fs.readdir` order, and appending keeps every
// pre-existing row byte-identical on both sides. (The unify gate caught this
// list missing the append: P4.D100 authored the case before the NANOGPT
// manifest existed, P4.D101 appended the OTHER two cases' lists, and only the
// union could red — `list_providers` answered five providers against v5's six.)
const PLUGIN_DIRS = [
  'anthropic', 'openai', 'google', 'grok', 'deepseek', 'z-ai', 'openrouter', 'ollama', 'openai-compatible',
  'nanogpt',
];

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'GET',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

function applyMocks(spec: Spec): void {
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
  // `jest.setup.ts` globally mocks the plugin factory with a bare `jest.fn()`,
  // so `createImageProvider` answers `undefined` for EVERY provider — the
  // registry probe becomes a no-op and `?action=list-models` 500s on the
  // `undefined.supportedModels` read. Un-mock it (the same class as the
  // empty-provider-registry trap) so v4's real factory, filter, labelling and
  // caching code all run.
  jest.doMock('@/lib/llm/plugin-factory', () => jest.requireActual('@/lib/llm/plugin-factory'));
  jest.doMock('@/lib/plugins/provider-registry', () =>
    jest.requireActual('@/lib/plugins/provider-registry'),
  );
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
}

async function registerPlugins(): Promise<void> {
  const { registerProvider } = (await import('@/lib/plugins/provider-registry')) as {
    registerProvider: (p: unknown) => void;
  };
  const nodeRequire = createRequire(join(process.cwd(), 'noop.js'));
  for (const dir of PLUGIN_DIRS) {
    const m = nodeRequire(join(process.cwd(), 'plugins', 'dist', `qtap-plugin-${dir}`, 'index.js'));
    const plugin = m.plugin || m.default?.plugin || m.default;
    try {
      registerProvider(plugin);
    } catch {
      /* already registered */
    }
  }
}

async function dumpProfiles(): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const main = getRawDatabase() as unknown as { prepare: (s: string) => { all: () => unknown } };
  return { profiles: main.prepare('SELECT name, isDefault FROM image_profiles ORDER BY name').all() };
}

/**
 * The `provider_models` cache side-effect of `?action=list-models`. Only
 * genuinely live-fetched lists may be cached — a built-in list would masquerade
 * as provider-confirmed on later reads — so this dump is what makes the
 * cache-only-live rule a measured comparand rather than a claim.
 */
async function dumpProviderModels(): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const main = getRawDatabase() as unknown as { prepare: (s: string) => { all: () => unknown } };
  // The committed fixture predates `provider_models`; v4 creates the collection
  // lazily on first write. A missing table is therefore "nothing cached", which
  // is exactly what the built-in arms must show.
  const exists = main
    .prepare("SELECT name, sql FROM sqlite_master WHERE type='table' AND name='provider_models'")
    .all() as { sql?: string }[];
  if (exists.length === 0) return { providerModels: [], providerModelsDDL: null };
  return {
    // v4's own `CREATE TABLE` text, emitted so the Rust side can start each
    // list-models case from the SAME table shape rather than from a fixture
    // that predates the collection. Without it, "nothing cached" would be true
    // on the v5 side for the WRONG reason (a failed write on a missing table),
    // and the cache-only-live rule would be vacuously green.
    providerModelsDDL: exists[0].sql ?? null,
    providerModels: main
      .prepare(
        'SELECT provider, modelId, modelType, displayName, baseUrl FROM provider_models ORDER BY provider, modelType, modelId',
      )
      .all(),
  };
}

/**
 * Run `body` with `global.fetch` answering every request from `responses` in
 * order — the provider HTTP mocked BELOW the plugin, so v4's REAL filter,
 * labelling and caching code all run. An unscripted request throws rather than
 * repeating the last answer (a runaway page loop must fail, not hang).
 */
async function withMockedProviderHttp<T>(
  responses: { status: number; body: string }[],
  body: () => Promise<T>,
): Promise<T> {
  const original = globalThis.fetch;
  let served = 0;
  globalThis.fetch = (async () => {
    const r = responses[served];
    served += 1;
    if (!r) throw new Error('oracle: provider made more requests than scripted');
    return new Response(r.body, { status: r.status, headers: { 'content-type': 'application/json' } });
  }) as typeof fetch;
  try {
    return await body();
  } finally {
    globalThis.fetch = original;
  }
}

/** An OpenAI `/v1/models` page carrying two image families plus noise. */
const OPENAI_MODELS_PAGE = JSON.stringify({
  object: 'list',
  data: [
    { id: 'gpt-4o' },
    { id: 'dall-e-3' },
    { id: 'gpt-image-1' },
    { id: 'text-embedding-3-small' },
  ],
});

/** A 401 that makes the SDK throw, so the route takes its fetchError arm. */
const OPENAI_MODELS_401 = JSON.stringify({
  error: { message: 'Incorrect API key provided: sk-****.', type: 'invalid_request_error' },
});

interface CaseSpec {
  name: string;
  run: () => Promise<{ status: number; body: unknown; tables?: unknown }>;
}

async function loadRoute(
  path: string,
): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
  return (await import(path)) as never;
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'ip-'));
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
  await registerPlugins();
  try {
    const out = await c.run();
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      ...(out.tables !== undefined ? { tables: out.tables } : {}),
    };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

const B = 'http://localhost/api/v1/image-profiles';
const coll = () => loadRoute('@/app/api/v1/image-profiles/route');
const idRoute = () => loadRoute('@/app/api/v1/image-profiles/[id]/route');
const params = (id: string) => ({ params: Promise.resolve({ id }) });

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'groups-projects.json'), 'utf8'),
  ) as Spec;
  const fixtures = {
    main: process.env.QT_FIXTURE_GP_MAIN ?? '',
    mount: process.env.QT_FIXTURE_GP_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ip-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cases: CaseSpec[] = [
    { name: 'list_plain', run: async () => respond(await (await coll()).GET(mockRequest(B))) },
    {
      name: 'list_by_char',
      run: async () => respond(await (await coll()).GET(mockRequest(`${B}?sortByCharacter=${DIANA}`))),
    },
    {
      name: 'list_providers',
      run: async () => respond(await (await coll()).GET(mockRequest(`${B}?action=list-providers`))),
    },
    // === ca22ec45: the honest list-models action ===
    {
      name: 'list_models_missing_provider',
      run: async () => respond(await (await coll()).GET(mockRequest(`${B}?action=list-models`))),
    },
    {
      name: 'list_models_unknown_provider',
      run: async () =>
        respond(await (await coll()).GET(mockRequest(`${B}?action=list-models&provider=NOPE`))),
    },
    {
      // No key: the plugin's curated list, labelled builtin, and NOTHING cached.
      name: 'list_models_no_key',
      run: async () => {
        const r = await (await coll()).GET(mockRequest(`${B}?action=list-models&provider=OPENAI`));
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpProviderModels() };
      },
    },
    {
      // The legacy alias resolves to the GOOGLE provider, but the response and
      // the cache key echo the RAW provider string.
      name: 'list_models_legacy_alias',
      run: async () => {
        const r = await (await coll()).GET(
          mockRequest(`${B}?action=list-models&provider=GOOGLE_IMAGEN`),
        );
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpProviderModels() };
      },
    },
    {
      // Keyed live success: source `provider`, and the list cached under
      // modelType `image` with displayName === modelId.
      name: 'list_models_live_ok',
      run: async () => {
        const r = await withMockedProviderHttp([{ status: 200, body: OPENAI_MODELS_PAGE }], async () =>
          (await coll()).GET(
            mockRequest(`${B}?action=list-models&provider=OPENAI&apiKeyId=${APIKEY}`),
          ),
        );
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpProviderModels() };
      },
    },
    {
      // Keyed live FAILURE: fetchError present, models fall back to the built-in
      // list, source stays builtin, and NOTHING is cached.
      name: 'list_models_live_failure',
      run: async () => {
        const r = await withMockedProviderHttp([{ status: 401, body: OPENAI_MODELS_401 }], async () =>
          (await coll()).GET(
            mockRequest(`${B}?action=list-models&provider=OPENAI&apiKeyId=${APIKEY}`),
          ),
        );
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpProviderModels() };
      },
    },
    {
      name: 'list_models_dangling_key',
      run: async () =>
        respond(
          await (await coll()).GET(
            mockRequest(
              `${B}?action=list-models&provider=OPENAI&apiKeyId=00000000-0000-4000-8000-0000000000ff`,
            ),
          ),
        ),
    },
    {
      name: 'get',
      run: async () => respond(await (await idRoute()).GET(mockRequest(`${B}/${IP_1}`), params(IP_1))),
    },
    {
      name: 'get_404',
      run: async () =>
        respond(await (await idRoute()).GET(mockRequest(`${B}/${BOGUS}`), params(BOGUS))),
    },
    {
      name: 'create_happy',
      run: async () =>
        respond(
          await (await coll()).POST(
            mockRequest(B, {
              name: 'New Imagery',
              provider: 'OPENAI',
              apiKeyId: APIKEY,
              baseUrl: 'http://127.0.0.1:2/v1',
              modelName: '  gpt-image-1  ',
              parameters: { steps: 25 },
              isDangerousCompatible: true,
            }),
          ),
        ),
    },
    {
      name: 'create_default_unsets',
      run: async () => {
        const r = await (await coll()).POST(
          mockRequest(B, {
            name: 'Defaulted',
            provider: 'OPENAI',
            modelName: 'gpt-image-1',
            isDefault: true,
          }),
        );
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpProfiles() };
      },
    },
    { name: 'create_name_required', run: async () => respond(await (await coll()).POST(mockRequest(B, {}))) },
    {
      name: 'create_provider_required',
      run: async () => respond(await (await coll()).POST(mockRequest(B, { name: 'X' }))),
    },
    {
      name: 'create_model_required',
      run: async () =>
        respond(await (await coll()).POST(mockRequest(B, { name: 'X', provider: 'OPENAI' }))),
    },
    {
      name: 'create_params_not_object',
      run: async () =>
        respond(
          await (await coll()).POST(
            mockRequest(B, { name: 'X', provider: 'OPENAI', modelName: 'm', parameters: [1, 2] }),
          ),
        ),
    },
    {
      name: 'create_apikey_404',
      run: async () =>
        respond(
          await (await coll()).POST(
            mockRequest(B, {
              name: 'X',
              provider: 'OPENAI',
              modelName: 'm',
              apiKeyId: '00000000-0000-4000-8000-0000000000ff',
            }),
          ),
        ),
    },
    {
      name: 'create_dup_409',
      run: async () =>
        respond(
          await (await coll()).POST(
            mockRequest(B, { name: 'Primary Imagery', provider: 'OPENAI', modelName: 'm' }),
          ),
        ),
    },
    {
      name: 'update_apikey_null',
      run: async () =>
        respond(
          await (await idRoute()).PUT(mockRequest(`${B}/${IP_1}`, { apiKeyId: null }), params(IP_1)),
        ),
    },
    {
      name: 'update_baseurl_empty',
      run: async () =>
        respond(
          await (await idRoute()).PUT(mockRequest(`${B}/${IP_1}`, { baseUrl: '' }), params(IP_1)),
        ),
    },
    // P4.55 (the merge-verb silent-keep sweep), the missing-`else` sub-family:
    // v5 read `apiKeyId` as `if null … else if as_str …` with NO else, so a
    // present non-string was silently dropped and the PUT answered 200. v4 has
    // no Zod schema here either — it falls into `findApiKeyById(apiKeyId)`,
    // which answers null for any non-string. These arms MEASURE that.
    {
      name: 'update_apikey_non_string',
      run: async () =>
        respond(
          await (await idRoute()).PUT(mockRequest(`${B}/${IP_1}`, { apiKeyId: 5 }), params(IP_1)),
        ),
    },
    {
      name: 'update_apikey_object',
      run: async () =>
        respond(
          await (await idRoute()).PUT(mockRequest(`${B}/${IP_1}`, { apiKeyId: {} }), params(IP_1)),
        ),
    },
    {
      // The sibling read: `baseUrl || null`. A TRUTHY non-string is assigned
      // verbatim by v4; v5's `as_str()` filter collapsed it to null, silently
      // CLEARING the column. The table dump proves what (if anything) landed.
      name: 'update_baseurl_non_string',
      run: async () => {
        const r = await (await idRoute()).PUT(
          mockRequest(`${B}/${IP_1}`, { baseUrl: 5 }),
          params(IP_1),
        );
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpProfiles() };
      },
    },
    {
      name: 'update_isdefault',
      run: async () => {
        const r = await (await idRoute()).PUT(
          mockRequest(`${B}/${IP_2}`, { isDefault: true }),
          params(IP_2),
        );
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpProfiles() };
      },
    },
    {
      name: 'delete',
      run: async () =>
        respond(await (await idRoute()).DELETE(mockRequest(`${B}/${IP_3}`), params(IP_3))),
    },
    {
      name: 'delete_404',
      run: async () =>
        respond(await (await idRoute()).DELETE(mockRequest(`${B}/${BOGUS}`), params(BOGUS))),
    },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`image-profiles oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('image-profiles-routes oracle', async () => {
  await main();
});
