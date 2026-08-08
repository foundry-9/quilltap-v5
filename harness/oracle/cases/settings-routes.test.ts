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
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-settings-fixture.ts
 *   QT_FIXTURE_SETTINGS_MAIN=/tmp/qt-settings-fixture.db \
 *   QT_ORACLE_OUT=/tmp/oracle-settings-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "settings-routes\.test\.ts$"
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
    | 'models'
    | 'dataRetention'
    | 'taboo'
    | 'brahmaConsole';
  method: Method;
  url: string;
  paramId?: string;
  body?: Record<string, unknown>;
  /** Re-fetch the family list after the op (observe the persisted effect via the
   *  already-verified read marshaling), or none. */
  after?: 'connProfiles' | 'apiKeys' | 'taboo' | 'brahmaConsole';
  /** P4.D50: seed `instance_settings['taboo']` through v4's REAL setter before
   *  the case runs. Each case gets a pristine fixture copy, so this is the only
   *  way to reach the arms that depend on a list already being stored (the
   *  merge-over-current PUT above all). The Rust harness seeds identically. */
  seedTaboo?: string[];
  /** P4.D57: seed `instance_settings['brahmaConsole'].maxAgentTurns` through v4's
   *  REAL setter before the case runs — the merge-over-current arms need it.
   *  The Rust harness seeds identically. */
  seedBrahmaConsole?: number;
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
    if (c.seedTaboo) {
      const { setTabooSettings } = await import('@/lib/instance-settings');
      await setTabooSettings({ phrases: c.seedTaboo });
    }
    if (c.seedBrahmaConsole !== undefined) {
      const { setBrahmaConsoleSettings } = await import('@/lib/instance-settings');
      await setBrahmaConsoleSettings({ maxAgentTurns: c.seedBrahmaConsole });
    }
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
    } else if (c.route === 'dataRetention') {
      const mod = await import('@/app/api/v1/settings/data-retention/route');
      response = (await (c.method === 'GET' ? mod.GET : mod.PUT)(req as never)) as never;
    } else if (c.route === 'taboo') {
      const mod = await import('@/app/api/v1/settings/taboo/route');
      response = (await (c.method === 'GET' ? mod.GET : mod.PUT)(req as never)) as never;
    } else if (c.route === 'brahmaConsole') {
      const mod = await import('@/app/api/v1/settings/brahma-console/route');
      response = (await (c.method === 'GET' ? mod.GET : mod.PUT)(req as never)) as never;
    } else {
      const mod = await import('@/app/api/v1/models/route');
      response = (await mod.GET(req as never)) as never;
    }

    const status = response.status;
    const body = await response.json();
    // The Rust port surfaces validation failures as the `{error}` envelope; the
    // Zod issue array is v4-implementation-specific, so drop `details` here.
    if (
      (c.route === 'dataRetention' || c.route === 'taboo' || c.route === 'brahmaConsole') &&
      body &&
      typeof body === 'object' &&
      'details' in (body as Record<string, unknown>)
    ) {
      delete (body as Record<string, unknown>).details;
    }
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
        seedTaboo: c.seedTaboo ?? null,
        seedBrahmaConsole: c.seedBrahmaConsole ?? null,
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
    } else if (c.after === 'taboo') {
      // P4.D50: the PUT echoes what the SETTER stored; the refetch proves the
      // echo and the stored row agree (v4's whole reason for returning the
      // normalized list rather than the submission).
      const mod = await import('@/app/api/v1/settings/taboo/route');
      const r = (await mod.GET(mockRequest('http://x/api/v1/settings/taboo', 'GET') as never)) as never as {
        json: () => Promise<unknown>;
      };
      out.after = await r.json();
    } else if (c.after === 'brahmaConsole') {
      // P4.D57: the PUT echoes what the SETTER stored; the refetch proves the
      // echo and the stored row agree, and that a rejected PUT left the seeded
      // value untouched.
      const mod = await import('@/app/api/v1/settings/brahma-console/route');
      const r = (await mod.GET(
        mockRequest('http://x/api/v1/settings/brahma-console', 'GET') as never,
      )) as never as {
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
      // P4.6an — the Dangerous Content card's exact "Auto-detect" payload: the
      // client spread-merges the whole bag and sends the three
      // `.nullable().optional()` fields as EXPLICIT null. Zod keeps a present
      // null, so the stored bytes carry the keys.
      name: 's_put_danger_nulls',
      family: 'settings_chat',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: {
        dangerousContentSettings: {
          mode: 'AUTO_ROUTE',
          threshold: 1,
          scanTextChat: true,
          scanImagePrompts: true,
          scanImageGeneration: false,
          uncensoredTextProfileId: null,
          uncensoredImageProfileId: null,
          displayMode: 'BLUR',
          showWarningBadges: false,
          customClassificationPrompt: null,
        },
      },
    },
    {
      // P4.6an — a PARTIAL dangerousContentSettings bag. This is a ROUTE-level
      // `DangerousContentSettingsSchema.parse` (not the repo's merge-then-
      // validate), so every absent key takes its Zod default and the three
      // nullable-optionals stay ABSENT from the stored bytes.
      name: 's_put_danger_partial',
      family: 'settings_chat',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { dangerousContentSettings: { mode: 'DETECT_ONLY' } },
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
    // data-retention (P4.d3) — GET default, PUT valid / empty-merge / invalid arms.
    { name: 'dr_get_default', family: 'data_retention', user: 'A', route: 'dataRetention', method: 'GET', url: 'http://x/api/v1/settings/data-retention' },
    { name: 'dr_put_valid', family: 'data_retention', user: 'A', route: 'dataRetention', method: 'PUT', url: 'http://x/api/v1/settings/data-retention', body: { staleChatDays: 90 } },
    { name: 'dr_put_boundary_max', family: 'data_retention', user: 'A', route: 'dataRetention', method: 'PUT', url: 'http://x/api/v1/settings/data-retention', body: { staleChatDays: 3650 } },
    { name: 'dr_put_boundary_min', family: 'data_retention', user: 'A', route: 'dataRetention', method: 'PUT', url: 'http://x/api/v1/settings/data-retention', body: { staleChatDays: 1 } },
    { name: 'dr_put_empty_merge', family: 'data_retention', user: 'A', route: 'dataRetention', method: 'PUT', url: 'http://x/api/v1/settings/data-retention', body: {} },
    { name: 'dr_put_too_big', family: 'data_retention', user: 'A', route: 'dataRetention', method: 'PUT', url: 'http://x/api/v1/settings/data-retention', body: { staleChatDays: 5000 } },
    { name: 'dr_put_too_small', family: 'data_retention', user: 'A', route: 'dataRetention', method: 'PUT', url: 'http://x/api/v1/settings/data-retention', body: { staleChatDays: 0 } },
    { name: 'dr_put_non_integer', family: 'data_retention', user: 'A', route: 'dataRetention', method: 'PUT', url: 'http://x/api/v1/settings/data-retention', body: { staleChatDays: 12.5 } },
    { name: 'dr_put_wrong_type', family: 'data_retention', user: 'A', route: 'dataRetention', method: 'PUT', url: 'http://x/api/v1/settings/data-retention', body: { staleChatDays: 'abc' } },
    // taboo (P4.D50, v4 `7df7de8e`) — the instance-wide forbidden-phrase list.
    // Every case runs over its OWN fresh fixture copy, so a PUT case that also
    // wants to observe storage carries `after: 'taboo'` (the GET refetch).
    { name: 'taboo_get_default', family: 'taboo', user: 'A', route: 'taboo', method: 'GET', url: 'http://x/api/v1/settings/taboo' },
    // Normalization: trims, drops blanks, drops case-insensitive duplicates
    // keeping the FIRST occurrence, and never sorts. The echo IS the storage.
    {
      name: 'taboo_put_normalizes',
      family: 'taboo', user: 'A', route: 'taboo', method: 'PUT',
      url: 'http://x/api/v1/settings/taboo',
      body: { phrases: ['  zeta  ', 'Weight-Bearing', 'weight-bearing', "that's not nothing", 'WEIGHT-BEARING'] },
      after: 'taboo',
    },
    { name: 'taboo_get_seeded', family: 'taboo', user: 'A', route: 'taboo', method: 'GET', url: 'http://x/api/v1/settings/taboo', seedTaboo: ['weight-bearing', "that's not nothing"] },
    // The merge: a partial body — `{}` in particular — must leave the SEEDED
    // list untouched, both in the echo and in storage.
    { name: 'taboo_put_empty_merge', family: 'taboo', user: 'A', route: 'taboo', method: 'PUT', url: 'http://x/api/v1/settings/taboo', body: {}, seedTaboo: ['weight-bearing', 'tapestry'], after: 'taboo' },
    // …while an explicit empty array is the clear gesture and DOES wipe it.
    { name: 'taboo_put_explicit_empty', family: 'taboo', user: 'A', route: 'taboo', method: 'PUT', url: 'http://x/api/v1/settings/taboo', body: { phrases: [] }, seedTaboo: ['weight-bearing'], after: 'taboo' },
    // A rejected PUT leaves the seeded list exactly as it was.
    { name: 'taboo_put_replaces_seeded', family: 'taboo', user: 'A', route: 'taboo', method: 'PUT', url: 'http://x/api/v1/settings/taboo', body: { phrases: ['tapestry'] }, seedTaboo: ['weight-bearing'], after: 'taboo' },
    // The schema trims BEFORE measuring: 204 raw units that trim to 200 pass,
    // and the STORED value is the trimmed one.
    { name: 'taboo_put_trim_then_length', family: 'taboo', user: 'A', route: 'taboo', method: 'PUT', url: 'http://x/api/v1/settings/taboo', body: { phrases: [`  ${'x'.repeat(200)}  `] }, after: 'taboo' },
    { name: 'taboo_put_boundary_max_length', family: 'taboo', user: 'A', route: 'taboo', method: 'PUT', url: 'http://x/api/v1/settings/taboo', body: { phrases: ['x'.repeat(200)] } },
    { name: 'taboo_put_boundary_max_count', family: 'taboo', user: 'A', route: 'taboo', method: 'PUT', url: 'http://x/api/v1/settings/taboo', body: { phrases: Array.from({ length: 500 }, (_, i) => `phrase ${i}`) } },
    // Rejections (400 `Validation error`, nothing written).
    { name: 'taboo_put_too_long', family: 'taboo', user: 'A', route: 'taboo', method: 'PUT', url: 'http://x/api/v1/settings/taboo', body: { phrases: ['x'.repeat(201)] }, seedTaboo: ['weight-bearing'], after: 'taboo' },
    { name: 'taboo_put_too_many', family: 'taboo', user: 'A', route: 'taboo', method: 'PUT', url: 'http://x/api/v1/settings/taboo', body: { phrases: Array.from({ length: 501 }, (_, i) => `phrase ${i}`) } },
    { name: 'taboo_put_not_an_array', family: 'taboo', user: 'A', route: 'taboo', method: 'PUT', url: 'http://x/api/v1/settings/taboo', body: { phrases: 'weight-bearing' } },
    { name: 'taboo_put_non_string_entry', family: 'taboo', user: 'A', route: 'taboo', method: 'PUT', url: 'http://x/api/v1/settings/taboo', body: { phrases: [42] } },
    // Whitespace-only is REJECTED (it trims to length 0, failing `.min(1)`),
    // not silently dropped — the check order made visible.
    { name: 'taboo_put_whitespace_only_entry', family: 'taboo', user: 'A', route: 'taboo', method: 'PUT', url: 'http://x/api/v1/settings/taboo', body: { phrases: ['   '] } },
    // An explicit `null` is a PRESENT value, so Zod's `.default([])` does not
    // fire and the parse fails — distinct from omitting the key entirely.
    { name: 'taboo_put_null_phrases', family: 'taboo', user: 'A', route: 'taboo', method: 'PUT', url: 'http://x/api/v1/settings/taboo', body: { phrases: null } },
    // A non-object body: `{...current, ...body}` spreads a string into indexed
    // keys, contributing no `phrases`, so the stored list survives.
    { name: 'taboo_put_string_body', family: 'taboo', user: 'A', route: 'taboo', method: 'PUT', url: 'http://x/api/v1/settings/taboo', body: 'weight-bearing' as never, seedTaboo: ['tapestry'], after: 'taboo' },
    // Unicode: the length bound is UTF-16 code units (JS `String.length`), so
    // 101 astral characters is 202 units and fails while 100 passes.
    { name: 'taboo_put_astral_within_bound', family: 'taboo', user: 'A', route: 'taboo', method: 'PUT', url: 'http://x/api/v1/settings/taboo', body: { phrases: ['\u{1F3A9}'.repeat(100)] }, after: 'taboo' },
    { name: 'taboo_put_astral_over_bound', family: 'taboo', user: 'A', route: 'taboo', method: 'PUT', url: 'http://x/api/v1/settings/taboo', body: { phrases: ['\u{1F3A9}'.repeat(101)] } },

    // P4.D57: the instance-wide Brahma Console turn budget (v4 `6452e2c3`).
    // GET returns the schema default (50) when never written.
    { name: 'bc_get_default', family: 'brahma_console', user: 'A', route: 'brahmaConsole', method: 'GET', url: 'http://x/api/v1/settings/brahma-console' },
    { name: 'bc_get_seeded', family: 'brahma_console', user: 'A', route: 'brahmaConsole', method: 'GET', url: 'http://x/api/v1/settings/brahma-console', seedBrahmaConsole: 120 },
    // A valid PUT persists and echoes the stored value; the refetch proves it stuck.
    { name: 'bc_put_valid', family: 'brahma_console', user: 'A', route: 'brahmaConsole', method: 'PUT', url: 'http://x/api/v1/settings/brahma-console', body: { maxAgentTurns: 80 }, after: 'brahmaConsole' },
    { name: 'bc_put_boundary_min', family: 'brahma_console', user: 'A', route: 'brahmaConsole', method: 'PUT', url: 'http://x/api/v1/settings/brahma-console', body: { maxAgentTurns: 5 } },
    { name: 'bc_put_boundary_max', family: 'brahma_console', user: 'A', route: 'brahmaConsole', method: 'PUT', url: 'http://x/api/v1/settings/brahma-console', body: { maxAgentTurns: 200 } },
    // An empty body must not wipe the stored value back to the schema default.
    { name: 'bc_put_empty_merge', family: 'brahma_console', user: 'A', route: 'brahmaConsole', method: 'PUT', url: 'http://x/api/v1/settings/brahma-console', body: {}, seedBrahmaConsole: 120, after: 'brahmaConsole' },
    // Rejections (400 `Validation error`, nothing written) — the seeded value survives.
    { name: 'bc_put_below_min', family: 'brahma_console', user: 'A', route: 'brahmaConsole', method: 'PUT', url: 'http://x/api/v1/settings/brahma-console', body: { maxAgentTurns: 4 }, seedBrahmaConsole: 120, after: 'brahmaConsole' },
    { name: 'bc_put_above_max', family: 'brahma_console', user: 'A', route: 'brahmaConsole', method: 'PUT', url: 'http://x/api/v1/settings/brahma-console', body: { maxAgentTurns: 201 } },
    { name: 'bc_put_non_integer', family: 'brahma_console', user: 'A', route: 'brahmaConsole', method: 'PUT', url: 'http://x/api/v1/settings/brahma-console', body: { maxAgentTurns: 12.5 } },
    { name: 'bc_put_wrong_type', family: 'brahma_console', user: 'A', route: 'brahmaConsole', method: 'PUT', url: 'http://x/api/v1/settings/brahma-console', body: { maxAgentTurns: 'fifty' } },
    // An explicit `null` is a PRESENT value, so Zod's `.default(50)` does not fire
    // and the parse fails — distinct from omitting the key entirely.
    { name: 'bc_put_null', family: 'brahma_console', user: 'A', route: 'brahmaConsole', method: 'PUT', url: 'http://x/api/v1/settings/brahma-console', body: { maxAgentTurns: null }, seedBrahmaConsole: 120, after: 'brahmaConsole' },
    // A non-object body: `{...current, ...body}` spreads a string into indexed
    // keys, contributing no `maxAgentTurns`, so the stored value survives.
    { name: 'bc_put_string_body', family: 'brahma_console', user: 'A', route: 'brahmaConsole', method: 'PUT', url: 'http://x/api/v1/settings/brahma-console', body: 'fifty' as never, seedBrahmaConsole: 120, after: 'brahmaConsole' },
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
