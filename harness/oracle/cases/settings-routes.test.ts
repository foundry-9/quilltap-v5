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
  /** P4.D85 — the three baked tags plus a DANGLING id no row backs. */
  tags: { adventure: string; mystery: string; unused: string; dangling: string };
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
  after?: 'connProfiles' | 'apiKeys' | 'taboo' | 'brahmaConsole' | 'dataRetention';
  /** P4.56: seed `instance_settings['dataRetention']` through v4's REAL setter
   *  before the case runs — the merge-over-current and writes-nothing arms need
   *  a NON-DEFAULT stored value, or "kept the current value" and "reset to the
   *  schema default 30" are the same observation. The Rust harness seeds
   *  identically. */
  seedDataRetention?: number;
  /** P4.D50: seed `instance_settings['taboo']` through v4's REAL setter before
   *  the case runs. Each case gets a pristine fixture copy, so this is the only
   *  way to reach the arms that depend on a list already being stored (the
   *  merge-over-current PUT above all). The Rust harness seeds identically. */
  seedTaboo?: string[];
  /** P4.D57: seed `instance_settings['brahmaConsole'].maxAgentTurns` through v4's
   *  REAL setter before the case runs — the merge-over-current arms need it.
   *  The Rust harness seeds identically. */
  seedBrahmaConsole?: number;
  /** P4.D85: a v4 arm with NO v5 counterpart by design — v5 carries no
   *  `?action=` surface for connection profiles (the verbs ARE the action
   *  selection and no REST edge exists), so v4's two action-gate 400s and the
   *  no-action GET body are RECORDED and shape-asserted rather than driven.
   *  The `search_replace` middleware-arm precedent. */
  recorded?: boolean;
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

  // P4.D97 (v4 bug 85): the create route now resolves the prefill default
  // through `providerRegistry.profileRunsThinkingTurn`, so the registry must
  // hold the plugins that declare a thinking rule exactly as production
  // startup registers them — an EMPTY registry answers false for every
  // profile and silently records the OLD provider-shaped default (which is
  // how this gap was found: the new DeepSeek arm recorded `true`). Only the
  // two declaring plugins are registered: for every non-declaring provider an
  // absent plugin and a rule-less plugin evaluate identically (rule null, no
  // model facts → false), so the other arms are unaffected.
  const { initializeProviderRegistry } = await import('@/lib/plugins/provider-registry');
  const { createRequire } = await import('node:module');
  const distRequire = createRequire(join(process.cwd(), 'noop.js'));
  const thinkingPlugins = ['qtap-plugin-deepseek', 'qtap-plugin-ollama'].map((dir) => {
    const m = distRequire(join(process.cwd(), 'plugins', 'dist', dir, 'index.js'));
    return m.plugin || m.default?.plugin || m.default;
  });
  await initializeProviderRegistry(thinkingPlugins);

  try {
    if (c.seedDataRetention !== undefined) {
      const { setDataRetentionSettings } = await import('@/lib/instance-settings');
      await setDataRetentionSettings({ staleChatDays: c.seedDataRetention });
    }
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
      const fn =
        c.method === 'PUT'
          ? mod.PUT
          : c.method === 'DELETE'
            ? mod.DELETE
            : c.method === 'POST'
              ? mod.POST
              : mod.GET;
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
    // (P4.D85 adds `connProfileItem`: the add/remove-tag `z.uuid()` failure is
    // thrown past the route into `handleRouteError`'s ZodError arm, whose
    // `validationError` body carries the raw issue array.)
    if (
      (c.route === 'dataRetention' ||
        c.route === 'taboo' ||
        c.route === 'brahmaConsole' ||
        c.route === 'connProfileItem') &&
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
        seedDataRetention: c.seedDataRetention ?? null,
        seedTaboo: c.seedTaboo ?? null,
        seedBrahmaConsole: c.seedBrahmaConsole ?? null,
        recorded: c.recorded ?? false,
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
    } else if (c.after === 'dataRetention') {
      // P4.56: the PUT echoes the PARSED settings; the refetch proves the echo
      // and the stored row agree, and — on the reject arms — that a refused PUT
      // left the seeded value untouched.
      const mod = await import('@/app/api/v1/settings/data-retention/route');
      const r = (await mod.GET(
        mockRequest('http://x/api/v1/settings/data-retention', 'GET') as never,
      )) as never as {
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
  /** A well-formed uuid that names no row (P4.D85's ownership-404 arms). */
  const MISSING_ID = '5e4f0000-0000-4000-8000-0000000000ff';
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
    // ---- P4.D73 (v4 4.8.2): the three composer/typography settings keys ----
    {
      // Both composer typeahead gates flipped off in one payload — the
      // `typeof x !== 'undefined'` + boolean-guard arms.
      name: 's_put_composer_toggles',
      family: 'composer_settings',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { composerEmoji: false, composerUnicode: false },
    },
    {
      // A FULLY-specified smartTypographySettings bag (the SPA's spread-merge
      // save shape).
      name: 's_put_smart_typo_full',
      family: 'composer_settings',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: {
        smartTypographySettings: { displayQuotes: true, dashes: false, ellipsis: false },
      },
    },
    {
      // A PARTIAL bag — a route-level `SmartTypographySettingsSchema.parse`, so
      // every absent key takes its own Zod default (dashes/ellipsis true).
      name: 's_put_smart_typo_partial',
      family: 'composer_settings',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { smartTypographySettings: { displayQuotes: true } },
    },
    {
      // An EMPTY bag — every key defaults; the stored bytes are the schema
      // default verbatim.
      name: 's_put_smart_typo_empty',
      family: 'composer_settings',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { smartTypographySettings: {} },
    },
    {
      // Wrong type on a composer boolean — v4's manual guard throws its own
      // fixed sentence, which `.includes('Invalid')` turns into a 400.
      name: 's_put_composer_emoji_wrong_type',
      family: 'composer_settings',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { composerEmoji: 'yes' },
    },
    {
      name: 's_put_composer_unicode_wrong_type',
      family: 'composer_settings',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { composerUnicode: 1 },
    },
    {
      // An EXPLICIT null bag — `typeof null !== 'undefined'`, so the arm RUNS
      // and the Zod parse rejects it. The v5 dispatch carries the raw settings
      // bag, so the null must reach the handler rather than reading as absent.
      name: 's_put_smart_typo_null',
      family: 'composer_settings',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { smartTypographySettings: null },
    },
    {
      // A non-boolean INSIDE the bag — the nested Zod type check.
      name: 's_put_smart_typo_wrong_type',
      family: 'composer_settings',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { smartTypographySettings: { dashes: 'yes' } },
    },
    {
      // A non-object bag.
      name: 's_put_smart_typo_not_object',
      family: 'composer_settings',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { smartTypographySettings: 'on' },
    },
    {
      // The three keys on the CREATE branch (user B has no settings row) —
      // proves the seeded defaults and the assignment override compose.
      name: 's_put_composer_fresh',
      family: 'composer_settings',
      user: 'B',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: {
        composerEmoji: false,
        smartTypographySettings: { displayQuotes: true, dashes: true, ellipsis: false },
      },
    },
    // ---- P4.47 (A): the three sibling Zod-collapse arms ----
    // The D73 bank. `smartTypographySettings` above proved the machinery: a
    // route-level `Schema.parse` throw escapes to `getErrorMessage`, whose
    // `.includes('Invalid')` test turns the whole `ZodError.message` (=
    // `JSON.stringify(err.issues, null, 2)`) into the 400 body. These three
    // arms are the same class and had no corpus case at all, so v5 was free to
    // collapse them to fixed sentences. Each schema reaches a DIFFERENT set of
    // Zod issue codes, and the codes serialize with different key orders —
    // which is exactly what a corpus, not inspection, has to pin.
    //
    // `answerConfirmationSettings` — `{ enabled: z.boolean().default(false) }`,
    // parsed at the route (L270). Positive arms first so the parse itself (not
    // only its throws) is pinned.
    {
      name: 's_put_answer_conf_full',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { answerConfirmationSettings: { enabled: true } },
    },
    {
      // An EMPTY bag — the Zod default materializes.
      name: 's_put_answer_conf_empty',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { answerConfirmationSettings: {} },
    },
    {
      name: 's_put_answer_conf_wrong_type',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { answerConfirmationSettings: { enabled: 'yes' } },
    },
    {
      name: 's_put_answer_conf_not_object',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { answerConfirmationSettings: 'on' },
    },
    {
      // `typeof null !== 'undefined'`, so the arm RUNS and the parse rejects.
      name: 's_put_answer_conf_null',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { answerConfirmationSettings: null },
    },
    // `dangerousContentSettings` — parsed at the route (L175). The richest of
    // the three: enums (`invalid_value`), a `.min(0).max(1)` number
    // (`too_small`/`too_big`), `.uuid()` (`invalid_format`) and plain
    // `invalid_type`, each with its own issue key order.
    {
      name: 's_put_danger_not_object',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { dangerousContentSettings: 'on' },
    },
    {
      name: 's_put_danger_null',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { dangerousContentSettings: null },
    },
    {
      name: 's_put_danger_bad_enum',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { dangerousContentSettings: { mode: 'BOGUS' } },
    },
    {
      name: 's_put_danger_threshold_too_big',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { dangerousContentSettings: { threshold: 2 } },
    },
    {
      name: 's_put_danger_threshold_too_small',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { dangerousContentSettings: { threshold: -0.5 } },
    },
    {
      name: 's_put_danger_threshold_wrong_type',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { dangerousContentSettings: { threshold: 'high' } },
    },
    {
      name: 's_put_danger_bad_uuid',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { dangerousContentSettings: { uncensoredTextProfileId: 'not-a-uuid' } },
    },
    {
      // A `.nullable().optional()` uuid handed a NON-string: the type check
      // fires before the format check, so this is `invalid_type`, not
      // `invalid_format`.
      name: 's_put_danger_uuid_wrong_type',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { dangerousContentSettings: { uncensoredImageProfileId: 5 } },
    },
    {
      name: 's_put_danger_bad_string',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { dangerousContentSettings: { customClassificationPrompt: 5 } },
    },
    {
      // FOUR issues at once — Zod collects every key's failure and emits them
      // in schema DECLARATION order (not the order the bag lists them), which
      // is the half a single-issue case cannot pin.
      name: 's_put_danger_multi',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: {
        dangerousContentSettings: {
          displayMode: 'X',
          scanTextChat: 1,
          threshold: 3,
          mode: 'BOGUS',
        },
      },
    },
    // `cheapLLMSettings` — the odd one out. Its ROUTE arm (L76) is two manual
    // enum guards with their own fixed sentences; the bag then rides RAW into
    // `updateData`, and the Zod check that governs it is the base repo's
    // merge-then-`validate` over the WHOLE ChatSettings object. So its issue
    // paths are PREFIXED with `cheapLLMSettings`, and — the part only an
    // ordering case can show — its throw happens AFTER every route-level arm.
    {
      name: 's_put_cheap_bad_strategy',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { cheapLLMSettings: { strategy: 'BOGUS' } },
    },
    {
      // A non-string strategy is truthy and not in the list, so the MANUAL
      // guard catches it before Zod ever sees the bag.
      name: 's_put_cheap_strategy_number',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { cheapLLMSettings: { strategy: 5 } },
    },
    {
      // A FALSY non-member. v4's guard is `if (settings.strategy && !valid…)`,
      // so `null` slips past it entirely and the repo's Zod reports the enum
      // miss — a different answer from the truthy case two rows up, and the
      // whole reason the enum arms have to be modelled rather than declared
      // unreachable.
      name: 's_put_cheap_strategy_null',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { cheapLLMSettings: { strategy: null } },
    },
    {
      name: 's_put_cheap_strategy_empty',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { cheapLLMSettings: { strategy: '' } },
    },
    {
      name: 's_put_cheap_embedding_empty',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { cheapLLMSettings: { embeddingProvider: '' } },
    },
    {
      name: 's_put_cheap_bad_embedding',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { cheapLLMSettings: { embeddingProvider: 'BOGUS' } },
    },
    {
      // No manual guard covers `fallbackToLocal`, so this reaches the repo's
      // whole-object validate — path `["cheapLLMSettings","fallbackToLocal"]`.
      name: 's_put_cheap_bad_bool',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { cheapLLMSettings: { fallbackToLocal: 'yes' } },
    },
    {
      // `typeof 'on' !== 'object'`, so the route's guard block is SKIPPED
      // entirely and the string rides into `updateData` — the repo validate
      // then reports one issue at path `["cheapLLMSettings"]`.
      name: 's_put_cheap_not_object',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { cheapLLMSettings: 'on' },
    },
    {
      name: 's_put_cheap_bad_uuid',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { cheapLLMSettings: { userDefinedProfileId: 'nope' } },
    },
    {
      // The CREATE branch (user B has no settings row): `updateForUser` spreads
      // `data` over the defaults and `_create` validates that — same issue
      // bytes, a different validate call site.
      name: 's_put_cheap_bad_bool_fresh',
      family: 'settings_zod',
      user: 'B',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: { cheapLLMSettings: { fallbackToLocal: 'yes' } },
    },
    {
      // THE ORDERING CASE. `cheapLLMSettings` is handled FIRST in the route's
      // arm sequence and `dangerousContentSettings` ~100 lines later — but the
      // cheap-LLM Zod check does not run at the route at all, so the
      // dangerous-content throw wins. A port that validates cheap-LLM in place
      // answers the wrong error here.
      name: 's_put_cheap_after_route_arms',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: {
        cheapLLMSettings: { fallbackToLocal: 'yes' },
        dangerousContentSettings: { mode: 'BOGUS' },
      },
    },
    {
      // The mirror: a MANUAL cheap-LLM guard DOES run at the route, and it
      // sits before the dangerous-content arm — so this one answers the fixed
      // cheap-LLM sentence.
      name: 's_put_cheap_guard_before_route_arms',
      family: 'settings_zod',
      user: 'A',
      route: 'settingsChat',
      method: 'PUT',
      url: 'http://x/api/v1/settings/chat',
      body: {
        cheapLLMSettings: { strategy: 'BOGUS' },
        dangerousContentSettings: { mode: 'BOGUS' },
      },
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
    // P4.D79 (v4 `23af7146`) — the per-profile multi-character prefill.
    {
      // Absent on an ANTHROPIC create: the server RESOLVES the provider default
      // (false) and STORES it. A create never writes the tri-state NULL.
      name: 'cp_create_prefill_default_anthropic',
      family: 'connection_profiles',
      user: 'A',
      route: 'connProfiles',
      method: 'POST',
      url: `http://x${CP}`,
      body: { name: 'Prefill Default Anthropic', provider: 'ANTHROPIC', modelName: 'claude-sonnet' },
      after: 'connProfiles',
    },
    {
      // Explicitly ON for an Anthropic profile — permitted (warned about in the
      // editor, not forbidden), so the stored value must win over the default.
      name: 'cp_create_prefill_true_anthropic',
      family: 'connection_profiles',
      user: 'A',
      route: 'connProfiles',
      method: 'POST',
      url: `http://x${CP}`,
      body: {
        name: 'Prefill On Anthropic',
        provider: 'ANTHROPIC',
        modelName: 'claude-sonnet',
        multiCharacterPrefill: true,
      },
      after: 'connProfiles',
    },
    {
      // Explicitly OFF for a non-Anthropic provider (the bug-68 Ollama case).
      name: 'cp_create_prefill_false_ollama',
      family: 'connection_profiles',
      user: 'A',
      route: 'connProfiles',
      method: 'POST',
      url: `http://x${CP}`,
      body: {
        name: 'Prefill Off Ollama',
        provider: 'OLLAMA',
        modelName: 'qwen3',
        baseUrl: 'http://localhost:11434',
        multiCharacterPrefill: false,
      },
      after: 'connProfiles',
    },
    {
      // P4.D97 (v4 bug 85): field ABSENT on a DeepSeek profile whose model
      // thinks by default — the resolved default is now FALSE (the join:
      // rule unset in `parameters`, `thinksByDefault` true). Before bug 85
      // this stored 1; the refetch pins the 0.
      name: 'cp_create_prefill_absent_deepseek_thinking',
      family: 'connection_profiles',
      user: 'A',
      route: 'connProfiles',
      method: 'POST',
      url: `http://x${CP}`,
      body: {
        name: 'Prefill Absent DeepSeek',
        provider: 'DEEPSEEK',
        modelName: 'deepseek-v4-flash',
      },
      after: 'connProfiles',
    },
    {
      // P4.D97: same model, but the profile explicitly turns thinking OFF in
      // its parameters — the rule wins over the model habit and the prefill
      // default stays TRUE (bug 68's objection preserved: a non-thinking
      // DeepSeek profile keeps the stronger anchor).
      name: 'cp_create_prefill_absent_deepseek_thinking_disabled',
      family: 'connection_profiles',
      user: 'A',
      route: 'connProfiles',
      method: 'POST',
      url: `http://x${CP}`,
      body: {
        name: 'Prefill Absent DeepSeek Off',
        provider: 'DEEPSEEK',
        modelName: 'deepseek-v4-flash',
        parameters: { thinking: 'disabled' },
      },
      after: 'connProfiles',
    },
    {
      // P4.D97: an Ollama profile with Enable Thinking ticked — the rule
      // answers true (Ollama contributes no thinksByDefault models), so the
      // default resolves FALSE.
      name: 'cp_create_prefill_absent_ollama_thinking',
      family: 'connection_profiles',
      user: 'A',
      route: 'connProfiles',
      method: 'POST',
      url: `http://x${CP}`,
      body: {
        name: 'Prefill Absent Ollama Thinking',
        provider: 'OLLAMA',
        modelName: 'qwen3',
        baseUrl: 'http://localhost:11434',
        parameters: { enable_thinking: true },
      },
      after: 'connProfiles',
    },
    {
      // P4.D97: an explicit TRUE on a thinking profile survives — the
      // tri-state exists so the user may overrule us (the editor warns,
      // never vetoes).
      name: 'cp_create_prefill_true_deepseek_thinking',
      family: 'connection_profiles',
      user: 'A',
      route: 'connProfiles',
      method: 'POST',
      url: `http://x${CP}`,
      body: {
        name: 'Prefill On DeepSeek Thinking',
        provider: 'DEEPSEEK',
        modelName: 'deepseek-v4-flash',
        multiCharacterPrefill: true,
      },
      after: 'connProfiles',
    },
    {
      // An explicit `null` on create is NOT "absent" — it is a 400.
      name: 'cp_create_prefill_null',
      family: 'connection_profiles',
      user: 'A',
      route: 'connProfiles',
      method: 'POST',
      url: `http://x${CP}`,
      body: { name: 'Prefill Null', provider: 'OPENAI', modelName: 'gpt-4o', multiCharacterPrefill: null },
    },
    {
      name: 'cp_create_prefill_nonbool',
      family: 'connection_profiles',
      user: 'A',
      route: 'connProfiles',
      method: 'POST',
      url: `http://x${CP}`,
      body: { name: 'Prefill String', provider: 'OPENAI', modelName: 'gpt-4o', multiCharacterPrefill: 'yes' },
    },
    {
      name: 'cp_update_prefill_false',
      family: 'connection_profiles',
      user: 'A',
      route: 'connProfileItem',
      method: 'PUT',
      url: cbase(spec.profiles.gpt),
      paramId: spec.profiles.gpt,
      body: { multiCharacterPrefill: false },
      after: 'connProfiles',
    },
    {
      name: 'cp_update_prefill_null',
      family: 'connection_profiles',
      user: 'A',
      route: 'connProfileItem',
      method: 'PUT',
      url: cbase(spec.profiles.gpt),
      paramId: spec.profiles.gpt,
      body: { multiCharacterPrefill: null },
    },
    {
      name: 'cp_update_prefill_nonbool',
      family: 'connection_profiles',
      user: 'A',
      route: 'connProfileItem',
      method: 'PUT',
      url: cbase(spec.profiles.gpt),
      paramId: spec.profiles.gpt,
      body: { multiCharacterPrefill: 1 },
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
    // P4.D85 (v4 `d123658d`, Bug 74) — the connection-profile tag surface. The
    // GPT profile's baked bag is [mystery, <dangling>, adventure]: not id order,
    // not name order, and the middle id backs no row.
    {
      // The FLAT `EditorTag` shape (`resolveEditorTags`) — order preserved,
      // dangling dropped, `visualStyle` present on mystery / omitted on
      // adventure.
      name: 'cp_get_tags',
      family: 'connection_profile_tags',
      user: 'A',
      route: 'connProfileItem',
      method: 'GET',
      url: `${cbase(spec.profiles.gpt)}?action=get-tags`,
      paramId: spec.profiles.gpt,
    },
    {
      name: 'cp_get_tags_empty',
      family: 'connection_profile_tags',
      user: 'A',
      route: 'connProfileItem',
      method: 'GET',
      url: `${cbase(spec.profiles.claude)}?action=get-tags`,
      paramId: spec.profiles.claude,
    },
    {
      // Ownership 404 runs BEFORE the action gate.
      name: 'cp_get_tags_unknown_profile',
      family: 'connection_profile_tags',
      user: 'A',
      route: 'connProfileItem',
      method: 'GET',
      url: `${cbase(MISSING_ID)}?action=get-tags`,
      paramId: MISSING_ID,
    },
    {
      // RECORDED-ONLY: v4's GET action gate. v5 has no `?action=` surface here.
      name: 'cp_get_unknown_action',
      family: 'connection_profile_tags',
      user: 'A',
      route: 'connProfileItem',
      method: 'GET',
      url: `${cbase(spec.profiles.gpt)}?action=bogus`,
      paramId: spec.profiles.gpt,
      recorded: true,
    },
    {
      // RECORDED-ONLY: the no-action GET body, which the new gate must NOT have
      // disturbed. v5 has no single-profile GET verb (the SPA reads the list).
      name: 'cp_get_no_action',
      family: 'connection_profile_tags',
      user: 'A',
      route: 'connProfileItem',
      method: 'GET',
      url: cbase(spec.profiles.gpt),
      paramId: spec.profiles.gpt,
      recorded: true,
    },
    {
      name: 'cp_add_tag',
      family: 'connection_profile_tags',
      user: 'A',
      route: 'connProfileItem',
      method: 'POST',
      url: `${cbase(spec.profiles.gpt)}?action=add-tag`,
      paramId: spec.profiles.gpt,
      body: { tagId: spec.tags.unused },
      after: 'connProfiles',
    },
    {
      // Already held: `TaggableBaseRepository.addTag` skips the write entirely,
      // so the bag must NOT gain a duplicate — and the answer is the same
      // `{success, tag}`.
      name: 'cp_add_tag_already_held',
      family: 'connection_profile_tags',
      user: 'A',
      route: 'connProfileItem',
      method: 'POST',
      url: `${cbase(spec.profiles.gpt)}?action=add-tag`,
      paramId: spec.profiles.gpt,
      body: { tagId: spec.tags.mystery },
      after: 'connProfiles',
    },
    {
      // A well-formed uuid no tag row backs → `notFound('Tag')`.
      name: 'cp_add_tag_unknown_tag',
      family: 'connection_profile_tags',
      user: 'A',
      route: 'connProfileItem',
      method: 'POST',
      url: `${cbase(spec.profiles.gpt)}?action=add-tag`,
      paramId: spec.profiles.gpt,
      body: { tagId: spec.tags.dangling },
    },
    {
      // `z.uuid()` failure — measure what v4 ACTUALLY answers rather than
      // assuming (it throws past the route into `handleRouteError`).
      name: 'cp_add_tag_malformed',
      family: 'connection_profile_tags',
      user: 'A',
      route: 'connProfileItem',
      method: 'POST',
      url: `${cbase(spec.profiles.gpt)}?action=add-tag`,
      paramId: spec.profiles.gpt,
      body: { tagId: 'not-a-uuid' },
    },
    {
      name: 'cp_add_tag_missing_field',
      family: 'connection_profile_tags',
      user: 'A',
      route: 'connProfileItem',
      method: 'POST',
      url: `${cbase(spec.profiles.gpt)}?action=add-tag`,
      paramId: spec.profiles.gpt,
      body: {},
    },
    {
      name: 'cp_add_tag_unknown_profile',
      family: 'connection_profile_tags',
      user: 'A',
      route: 'connProfileItem',
      method: 'POST',
      url: `${cbase(MISSING_ID)}?action=add-tag`,
      paramId: MISSING_ID,
      body: { tagId: spec.tags.unused },
    },
    {
      // The removed id is the FIRST of three, so the survivors' order is
      // observable in the refetch.
      name: 'cp_remove_tag',
      family: 'connection_profile_tags',
      user: 'A',
      route: 'connProfileItem',
      method: 'POST',
      url: `${cbase(spec.profiles.gpt)}?action=remove-tag`,
      paramId: spec.profiles.gpt,
      body: { tagId: spec.tags.mystery },
      after: 'connProfiles',
    },
    {
      // Not held: the array does not shrink, so v4 skips the write entirely and
      // still answers `{success: true}`.
      name: 'cp_remove_tag_absent',
      family: 'connection_profile_tags',
      user: 'A',
      route: 'connProfileItem',
      method: 'POST',
      url: `${cbase(spec.profiles.gpt)}?action=remove-tag`,
      paramId: spec.profiles.gpt,
      body: { tagId: spec.tags.unused },
      after: 'connProfiles',
    },
    {
      // The DANGLING id is held by the profile but backs no tag row — remove-tag
      // has no existence check, so it must come out.
      name: 'cp_remove_tag_dangling',
      family: 'connection_profile_tags',
      user: 'A',
      route: 'connProfileItem',
      method: 'POST',
      url: `${cbase(spec.profiles.gpt)}?action=remove-tag`,
      paramId: spec.profiles.gpt,
      body: { tagId: spec.tags.dangling },
      after: 'connProfiles',
    },
    {
      name: 'cp_remove_tag_malformed',
      family: 'connection_profile_tags',
      user: 'A',
      route: 'connProfileItem',
      method: 'POST',
      url: `${cbase(spec.profiles.gpt)}?action=remove-tag`,
      paramId: spec.profiles.gpt,
      body: { tagId: 'not-a-uuid' },
    },
    {
      // RECORDED-ONLY: v4's POST action gate, naming all THREE v4 actions (the
      // third, `auto-configure`, is unported — no service, no consumer).
      name: 'cp_post_unknown_action',
      family: 'connection_profile_tags',
      user: 'A',
      route: 'connProfileItem',
      method: 'POST',
      url: `${cbase(spec.profiles.gpt)}?action=bogus`,
      paramId: spec.profiles.gpt,
      body: {},
      recorded: true,
    },
    // P4.D85 tier 2 — the PUT `baseUrl` arm P4.D86's poisoned-row clear depends
    // on. The CLAUDE profile is baked with a stale base URL precisely so `''`
    // has something to clear.
    {
      name: 'cp_update_base_url_empty',
      family: 'connection_profile_tags',
      user: 'A',
      route: 'connProfileItem',
      method: 'PUT',
      url: cbase(spec.profiles.claude),
      paramId: spec.profiles.claude,
      body: { baseUrl: '' },
      after: 'connProfiles',
    },
    {
      name: 'cp_update_base_url_value',
      family: 'connection_profile_tags',
      user: 'A',
      route: 'connProfileItem',
      method: 'PUT',
      url: cbase(spec.profiles.claude),
      paramId: spec.profiles.claude,
      body: { baseUrl: 'https://api.example.test/v1' },
      after: 'connProfiles',
    },
    {
      // The SAME clear against a row whose `baseUrl` is ALREADY SQL NULL. v4's
      // `_update` answers `validate({...existing, ...data})` and `existing` (a
      // DB read) has no `baseUrl` key at all, so this is where the explicit
      // `null` gets its POSITION decided — by Zod's shape order, not by the
      // spread. Measured, not assumed.
      name: 'cp_update_base_url_empty_already_null',
      family: 'connection_profile_tags',
      user: 'A',
      route: 'connProfileItem',
      method: 'PUT',
      url: cbase(spec.profiles.gpt),
      paramId: spec.profiles.gpt,
      body: { baseUrl: '' },
      after: 'connProfiles',
    },
    {
      // The other three keys the PUT can clear to an explicit null, all against
      // columns that are already SQL NULL: same in-memory-merge question.
      name: 'cp_update_clear_optionals',
      family: 'connection_profile_tags',
      user: 'A',
      route: 'connProfileItem',
      method: 'PUT',
      url: cbase(spec.profiles.gpt),
      paramId: spec.profiles.gpt,
      body: { apiKeyId: null, modelClass: null, maxContext: null },
      after: 'connProfiles',
    },
    // P4.55 (the merge-verb silent-keep sweep), the missing-`else` sub-family:
    // v5 reads `apiKeyId` as `if null … else if as_str …` with NO else, so a
    // present NON-string is silently dropped and the PUT answers 200. v4 has no
    // Zod schema on this route either — it falls straight into
    // `findApiKeyById(apiKeyId)`. These arms MEASURE what that does.
    {
      name: 'cp_update_api_key_id_number',
      family: 'connection_profiles',
      user: 'A',
      route: 'connProfileItem',
      method: 'PUT',
      url: cbase(spec.profiles.gpt),
      paramId: spec.profiles.gpt,
      body: { apiKeyId: 5 },
      after: 'connProfiles',
    },
    {
      name: 'cp_update_api_key_id_object',
      family: 'connection_profiles',
      user: 'A',
      route: 'connProfileItem',
      method: 'PUT',
      url: cbase(spec.profiles.gpt),
      paramId: spec.profiles.gpt,
      body: { apiKeyId: {} },
      after: 'connProfiles',
    },
    {
      // The sibling read one line below: `baseUrl || null`. A TRUTHY non-string
      // is stored verbatim by v4; v5's `as_str()` filter collapses it to null.
      name: 'cp_update_base_url_number',
      family: 'connection_profiles',
      user: 'A',
      route: 'connProfileItem',
      method: 'PUT',
      url: cbase(spec.profiles.gpt),
      paramId: spec.profiles.gpt,
      body: { baseUrl: 5 },
      after: 'connProfiles',
    },
    {
      // The courier gate sets apiKeyId AND baseUrl to null in one go.
      name: 'cp_update_courier_gate',
      family: 'connection_profile_tags',
      user: 'A',
      route: 'connProfileItem',
      method: 'PUT',
      url: cbase(spec.profiles.claude),
      paramId: spec.profiles.claude,
      body: { transport: 'courier' },
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
    // P4.56 threads the family through the `Request` enum's serde on the v5 side
    // and adds the seeded arms, so "kept the current value" is distinguishable
    // from "reset to the schema default".
    { name: 'dr_get_default', family: 'data_retention', user: 'A', route: 'dataRetention', method: 'GET', url: 'http://x/api/v1/settings/data-retention' },
    { name: 'dr_get_seeded', family: 'data_retention', user: 'A', route: 'dataRetention', method: 'GET', url: 'http://x/api/v1/settings/data-retention', seedDataRetention: 120 },
    { name: 'dr_put_valid', family: 'data_retention', user: 'A', route: 'dataRetention', method: 'PUT', url: 'http://x/api/v1/settings/data-retention', body: { staleChatDays: 90 }, after: 'dataRetention' },
    { name: 'dr_put_boundary_max', family: 'data_retention', user: 'A', route: 'dataRetention', method: 'PUT', url: 'http://x/api/v1/settings/data-retention', body: { staleChatDays: 3650 } },
    { name: 'dr_put_boundary_min', family: 'data_retention', user: 'A', route: 'dataRetention', method: 'PUT', url: 'http://x/api/v1/settings/data-retention', body: { staleChatDays: 1 } },
    { name: 'dr_put_empty_merge', family: 'data_retention', user: 'A', route: 'dataRetention', method: 'PUT', url: 'http://x/api/v1/settings/data-retention', body: {} },
    // An empty body over a SEEDED value must keep it, not fall back to the
    // schema default 30 — the arm `dr_put_empty_merge` alone cannot tell those
    // apart, because the unseeded current value IS 30.
    { name: 'dr_put_empty_merge_seeded', family: 'data_retention', user: 'A', route: 'dataRetention', method: 'PUT', url: 'http://x/api/v1/settings/data-retention', body: {}, seedDataRetention: 120, after: 'dataRetention' },
    // A non-object body: `{...current, ...body}` spreads a string into indexed
    // keys, contributing no `staleChatDays`, so the stored value survives.
    { name: 'dr_put_string_body', family: 'data_retention', user: 'A', route: 'dataRetention', method: 'PUT', url: 'http://x/api/v1/settings/data-retention', body: 'ninety' as never, seedDataRetention: 120, after: 'dataRetention' },
    { name: 'dr_put_too_big', family: 'data_retention', user: 'A', route: 'dataRetention', method: 'PUT', url: 'http://x/api/v1/settings/data-retention', body: { staleChatDays: 5000 } },
    { name: 'dr_put_too_small', family: 'data_retention', user: 'A', route: 'dataRetention', method: 'PUT', url: 'http://x/api/v1/settings/data-retention', body: { staleChatDays: 0 } },
    { name: 'dr_put_non_integer', family: 'data_retention', user: 'A', route: 'dataRetention', method: 'PUT', url: 'http://x/api/v1/settings/data-retention', body: { staleChatDays: 12.5 } },
    { name: 'dr_put_wrong_type', family: 'data_retention', user: 'A', route: 'dataRetention', method: 'PUT', url: 'http://x/api/v1/settings/data-retention', body: { staleChatDays: 'abc' } },
    // A rejected PUT writes NOTHING — the seeded value survives the 400.
    { name: 'dr_put_invalid_writes_nothing', family: 'data_retention', user: 'A', route: 'dataRetention', method: 'PUT', url: 'http://x/api/v1/settings/data-retention', body: { staleChatDays: 5000 }, seedDataRetention: 120, after: 'dataRetention' },
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
