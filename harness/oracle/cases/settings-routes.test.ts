/**
 * @jest-environment node
 *
 * P4.6d settings-server ORACLE: drives v4's REAL route handlers for the Settings
 * DB families — chat-settings (GET default-injection + PUT), connection profiles
 * (list / create / update / delete / reorder / reset-sort), API keys (list /
 * create / update / delete), and the cached models GET — over a FRESH copy of the
 * committed settings fixture per case, emitting the response body (+ the affected
 * MAIN-db table dump for mutation cases) so the Rust `api::settings::*` ports can
 * be diffed. The Rust harness normalizes minted ids/timestamps.
 *
 * Only the seams the Rust harness also neutralizes are mocked: the auth session
 * (per-case userId) and the startup gate; the DB stack is the REAL cipher binding
 * past jest.setup, so the routes run against a fresh copy of the baked fixture.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<worktree>
 *   TMPO=/tmp/qt-settings-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/settings-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/settings.json"        "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SETTINGS_MAIN=/tmp/qt-settings-fixture.db \
 *   QT_ORACLE_OUT=/tmp/oracle-settings-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- settings-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, copyFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userA: string;
  userB: string;
  settingsIdA: string;
  roleplayTemplateId: string;
  apiKeys: { openai: string; anthropic: string };
  profiles: { gpt: string; claude: string };
  providerModels: { a: string; b: string };
}

const CP_URL = '/api/v1/connection-profiles';
const AK_URL = '/api/v1/api-keys';

type Method = 'GET' | 'POST' | 'PUT' | 'DELETE';
interface CaseSpec {
  name: string;
  family: string;
  user: 'A' | 'B';
  route:
    | 'settingsChat'
    | 'connProfiles'
    | 'connProfileItem'
    | 'apiKeys'
    | 'apiKeyItem'
    | 'models';
  method: Method;
  url: string;
  paramId?: string;
  body?: Record<string, unknown>;
  /** Re-fetch the family list after the op (observe the persisted effect via the
   *  already-verified read marshaling), or none. */
  after?: 'connProfiles' | 'apiKeys';
}

function mockRequest(url: string, method: Method, body?: unknown): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

async function runCase(spec: Spec, c: CaseSpec, scratch: string, fixtureMain: string) {
  jest.resetModules();
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  const userId = c.user === 'A' ? spec.userA : spec.userB;

  const cipherDriverPath = join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories'),
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  // jest.setup mocks provider-validation to a PARTIAL (only validateProviderConfig)
  // — the connection-profile create calls the real `requiresBaseUrl`. Un-mock it.
  // (The provider registry stays uninitialized; the corpus creates only
  // OPENAI/GOOGLE profiles, whose `requiresBaseUrl` is false either way.)
  jest.doMock('@/lib/plugins/provider-validation', () =>
    jest.requireActual('@/lib/plugins/provider-validation'),
  );
  // jest.setup mocks maskApiKey to `jest.fn()` (→ undefined); the api-keys list
  // masking needs the real one.
  jest.doMock('@/lib/encryption', () => jest.requireActual('@/lib/encryption'));
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: userId } }),
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

  const work = mkdtempSync(join(scratch, 'settings-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtureMain, mainWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  await initializeDatabase();

  try {
    const req = mockRequest(c.url, c.method, c.body);
    let response: { status: number; json: () => Promise<unknown> };
    const params = { params: Promise.resolve({ id: c.paramId ?? '' }) };
    if (c.route === 'settingsChat') {
      const mod = await import('@/app/api/v1/settings/chat/route');
      response = (await (c.method === 'GET' ? mod.GET : mod.PUT)(req as never)) as never;
    } else if (c.route === 'connProfiles') {
      const mod = await import('@/app/api/v1/connection-profiles/route');
      response = (await (c.method === 'GET' ? mod.GET : mod.POST)(req as never)) as never;
    } else if (c.route === 'connProfileItem') {
      const mod = await import('@/app/api/v1/connection-profiles/[id]/route');
      const fn = c.method === 'PUT' ? mod.PUT : c.method === 'DELETE' ? mod.DELETE : mod.GET;
      response = (await fn(req as never, params as never)) as never;
    } else if (c.route === 'apiKeys') {
      const mod = await import('@/app/api/v1/api-keys/route');
      response = (await (c.method === 'GET' ? mod.GET : mod.POST)(req as never)) as never;
    } else if (c.route === 'apiKeyItem') {
      const mod = await import('@/app/api/v1/api-keys/[id]/route');
      const fn = c.method === 'PUT' ? mod.PUT : mod.DELETE;
      response = (await fn(req as never, params as never)) as never;
    } else {
      const mod = await import('@/app/api/v1/models/route');
      response = (await mod.GET(req as never)) as never;
    }

    const status = response.status;
    const body = await response.json();
    // Emit the request context so the Rust side dispatches the matching handler
    // generically (route/method/user/body/paramId), plus the response.
    const out: Record<string, unknown> = {
      name: c.name,
      family: c.family,
      req: {
        route: c.route,
        method: c.method,
        user: c.user,
        url: c.url,
        paramId: c.paramId ?? null,
        body: c.body ?? null,
        after: c.after ?? null,
      },
      status,
      body,
    };

    if (c.after === 'connProfiles') {
      const mod = await import('@/app/api/v1/connection-profiles/route');
      const r = (await mod.GET(mockRequest(`http://x${CP_URL}`, 'GET') as never)) as never as {
        json: () => Promise<unknown>;
      };
      out.after = await r.json();
    } else if (c.after === 'apiKeys') {
      const mod = await import('@/app/api/v1/api-keys/route');
      const r = (await mod.GET(mockRequest(`http://x${AK_URL}`, 'GET') as never)) as never as {
        json: () => Promise<unknown>;
      };
      out.after = await r.json();
    }
    return out;
  } finally {
    await closeDatabase();
    rmSync(work, { recursive: true, force: true });
  }
}

describe('settings-routes oracle', () => {
  const specPath = join(dirname(fileURLToPath(import.meta.url)), '../fixtures/settings.json');
  const spec: Spec = JSON.parse(fs.readFileSync(specPath, 'utf8'));
  const fixtureMain = process.env.QT_FIXTURE_SETTINGS_MAIN as string;
  const outPath = process.env.QT_ORACLE_OUT as string;
  const scratch = mkdtempSync(join(tmpdir(), 'qt-settings-oracle-'));

  const CP = '/api/v1/connection-profiles';
  const cbase = (id: string) => `http://x${CP}/${id}`;
  const AK = '/api/v1/api-keys';
  const abase = (id: string) => `http://x${AK}/${id}`;

  const cases: CaseSpec[] = [
    // chat-settings.
    {
      name: 's_default_inject',
      family: 'settings_chat',
      user: 'B',
      route: 'settingsChat',
      method: 'GET',
      url: 'http://x/api/v1/settings/chat',
    },
    {
      name: 's_put_existing',
      family: 'settings_chat',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: {
        avatarDisplayMode: 'GROUP_ONLY',
        sidebarWidth: 300,
        autoDetectRng: false,
        themePreference: { activeThemeId: null, colorMode: 'dark', showNavThemeSelector: true },
      },
    },
    {
      name: 's_put_fresh',
      family: 'settings_chat',
      user: 'B',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { sidebarWidth: 400, composerSpellcheck: false },
    },
    {
      // A PARTIAL nested bag — the SPA wizard's exact save payload. The base
      // repo's merge-then-validate runs the full CheapLLMSettingsSchema, so the
      // Zod defaults materialize (fallbackToLocal true, embeddingProvider
      // 'OPENAI') and the nullable-optional ids stay ABSENT in the stored bytes.
      name: 's_put_cheap_partial',
      family: 'settings_chat',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { cheapLLMSettings: { strategy: 'PROVIDER_CHEAPEST' } },
    },
    {
      // A partial themePreference — the route-level ThemePreferenceSchema.parse
      // defaults activeThemeId null + showNavThemeSelector false.
      name: 's_put_theme_partial',
      family: 'settings_chat',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { themePreference: { colorMode: 'dark' } },
    },
    {
      name: 's_put_reject',
      family: 'settings_chat',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { avatarDisplayMode: 'BOGUS' },
    },
    {
      name: 's_put_template_valid',
      family: 'settings_chat',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { defaultRoleplayTemplateId: spec.roleplayTemplateId },
    },
    {
      name: 's_put_template_invalid',
      family: 'settings_chat',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { defaultRoleplayTemplateId: '00000000-0000-4000-8000-0000000000ff' },
    },
    // connection profiles.
    { name: 'cp_list', family: 'connection_profiles', user: 'A', route: 'connProfiles', method: 'GET', url: `http://x${CP}` },
    {
      name: 'cp_create',
      family: 'connection_profiles',
      user: 'A',
      route: 'connProfiles',
      method: 'POST',
      url: `http://x${CP}`,
      body: { name: 'Grok', provider: 'OPENAI', modelName: 'gpt-4o', apiKeyId: spec.apiKeys.openai },
    },
    {
      name: 'cp_create_dup',
      family: 'connection_profiles',
      user: 'A',
      route: 'connProfiles',
      method: 'POST',
      url: `http://x${CP}`,
      body: { name: '  gpt ', provider: 'OPENAI', modelName: 'gpt-4o' },
    },
    {
      name: 'cp_update',
      family: 'connection_profiles',
      user: 'A',
      route: 'connProfileItem',
      method: 'PUT',
      url: cbase(spec.profiles.claude),
      paramId: spec.profiles.claude,
      body: { name: 'Claude 3.5', isCheap: false },
      after: 'connProfiles',
    },
    {
      name: 'cp_delete',
      family: 'connection_profiles',
      user: 'A',
      route: 'connProfileItem',
      method: 'DELETE',
      url: cbase(spec.profiles.claude),
      paramId: spec.profiles.claude,
      after: 'connProfiles',
    },
    {
      name: 'cp_reorder',
      family: 'connection_profiles',
      user: 'A',
      route: 'connProfiles',
      method: 'POST',
      url: `http://x${CP}?action=reorder`,
      body: {
        order: [
          { id: spec.profiles.claude, sortIndex: 0 },
          { id: spec.profiles.gpt, sortIndex: 1 },
        ],
      },
      after: 'connProfiles',
    },
    {
      name: 'cp_reset_sort',
      family: 'connection_profiles',
      user: 'A',
      route: 'connProfiles',
      method: 'POST',
      url: `http://x${CP}?action=reset-sort`,
      after: 'connProfiles',
    },
    // api keys.
    { name: 'ak_list', family: 'api_keys', user: 'A', route: 'apiKeys', method: 'GET', url: `http://x${AK}` },
    {
      name: 'ak_create',
      family: 'api_keys',
      user: 'A',
      route: 'apiKeys',
      method: 'POST',
      url: `http://x${AK}`,
      body: { provider: 'GOOGLE', label: 'Gemini', apiKey: 'synthetic-google-key-0000' },
    },
    {
      name: 'ak_update',
      family: 'api_keys',
      user: 'A',
      route: 'apiKeyItem',
      method: 'PUT',
      url: abase(spec.apiKeys.openai),
      paramId: spec.apiKeys.openai,
      body: { label: 'OpenAI Prod', isActive: false },
      after: 'apiKeys',
    },
    {
      name: 'ak_delete',
      family: 'api_keys',
      user: 'A',
      route: 'apiKeyItem',
      method: 'DELETE',
      url: abase(spec.apiKeys.anthropic),
      paramId: spec.apiKeys.anthropic,
      after: 'apiKeys',
    },
    // provider models (cached read).
    { name: 'pm_list_all', family: 'provider_models', user: 'A', route: 'models', method: 'GET', url: 'http://x/api/v1/models' },
    {
      name: 'pm_list_openai',
      family: 'provider_models',
      user: 'A',
      route: 'models',
      method: 'GET',
      url: 'http://x/api/v1/models?provider=OPENAI',
    },
  ];

  it('emits all cases', async () => {
    const lines: string[] = [];
    for (const c of cases) {
      const row = await runCase(spec, c, scratch, fixtureMain);
      lines.push(JSON.stringify(row));
    }
    fs.writeFileSync(outPath, lines.join('\n') + '\n');
    rmSync(scratch, { recursive: true, force: true });
    expect(lines.length).toBe(cases.length);
  });
});
