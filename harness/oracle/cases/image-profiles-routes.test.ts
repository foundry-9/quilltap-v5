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

const PLUGIN_DIRS = [
  'anthropic', 'openai', 'google', 'grok', 'deepseek', 'z-ai', 'openrouter', 'ollama', 'openai-compatible',
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
