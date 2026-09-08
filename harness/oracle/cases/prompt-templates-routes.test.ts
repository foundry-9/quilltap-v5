/**
 * @jest-environment node
 *
 * P4.83 prompt-templates ORACLE: drives v4's REAL route handlers for
 * `GET/POST /api/v1/prompt-templates` and `GET/PUT/DELETE
 * /api/v1/prompt-templates/[id]` over a FRESH copy of the /tmp fixture per case,
 * emitting status + body + the captured log lines + the whole `prompt_templates`
 * table so the v5 ports can be diffed.
 *
 * The middleware is REAL (`createContextHandler` → `buildRequestContext`), so
 * only the two seams the Rust side also neutralizes are mocked: the auth session
 * (per-case userId) and the startup gate. The DB stack is the real cipher
 * binding past jest.setup.
 *
 * ⚠ `initializePlugins()` runs per case, because the whole point of the family
 * is v4's LAZY seeding: `findAllForUser` calls `seedSamplePrompts`, which reads
 * `systemPromptRegistry`. An uninitialized registry is v4's "seed nothing" arm
 * and every list case would come back with one row. The 21-row assertion in
 * `list_first_seeds_21` is what refuses that silently-empty registry.
 *
 * Ids and timestamps minted by the seeding pass are normalized on the Rust side
 * (`<newid>` / `<ts>`), the `settings-routes` shape; the fixture's own ids stay
 * literal so the never-update and order pins stay legible.
 *
 * Run (Node 24, from the v4 checkout — mirror to /tmp; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   rm -rf /tmp/qt-prompt-templates-oracle
 *   mkdir -p /tmp/qt-prompt-templates-oracle/cases /tmp/qt-prompt-templates-oracle/fixtures
 *   cp $V5W/harness/oracle/cases/prompt-templates-routes.test.ts /tmp/qt-prompt-templates-oracle/cases/
 *   cp $V5W/harness/oracle/fixtures/prompt-templates-routes.json /tmp/qt-prompt-templates-oracle/fixtures/
 *   QT_FIXTURE_PT_ROUTES_MAIN=/tmp/qt-pt-routes-fixture.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-prompt-templates-routes-fixture.ts
 *   QT_FIXTURE_PT_ROUTES_MAIN=/tmp/qt-pt-routes-fixture.db \
 *   QT_ORACLE_OUT=/tmp/oracle-prompt-templates-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=600000 \
 *       --roots "$PWD" --roots /tmp/qt-prompt-templates-oracle/cases -- "prompt-templates-routes\.test\.ts$"
 */

import * as fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, copyFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  ts: string;
  userA: string;
  userB: string;
  userTemplateId: string;
  userTemplateName: string;
  staleBuiltInId: string;
  userCloneId: string;
  invalidRowId: string;
  collidingName: string;
  staleContent: string;
}

type Method = 'GET' | 'POST' | 'PUT' | 'DELETE';
type Seed = 'staleBuiltIn' | 'userClone' | 'invalidRow';

interface CaseSpec {
  name: string;
  route: 'collection' | 'item';
  method: Method;
  /** 'A' | 'B' — which fixture user the session carries. */
  user?: 'A' | 'B';
  paramId?: string;
  body?: unknown;
  seeds?: Seed[];
  /** Run the handler TWICE and emit the SECOND run (the "seeds nothing" pin). */
  runTwice?: boolean;
}

const MISSING_ID = '5e800000-0000-4000-8000-0000000000ff';
/** 101 code points (202 UTF-16 units) — passes a `.length` cap, fails Zod 4.5's. */
const ASTRAL_101 = '\u{1F600}'.repeat(101);
const ASTRAL_100 = '\u{1F600}'.repeat(100);

function buildCases(spec: Spec): CaseSpec[] {
  return [
    // ── GET /api/v1/prompt-templates ─────────────────────────────────────────
    { name: 'list_first_seeds_21', route: 'collection', method: 'GET' },
    { name: 'list_second_seeds_nothing', route: 'collection', method: 'GET', runTwice: true },
    {
      name: 'list_stale_builtin_never_updated',
      route: 'collection',
      method: 'GET',
      seeds: ['staleBuiltIn'],
    },
    {
      name: 'list_user_named_like_builtin_still_seeds',
      route: 'collection',
      method: 'GET',
      seeds: ['userClone'],
    },
    {
      name: 'list_drops_schema_invalid_row',
      route: 'collection',
      method: 'GET',
      seeds: ['invalidRow'],
    },
    { name: 'list_user_b_sees_builtins_only', route: 'collection', method: 'GET', user: 'B' },

    // ── POST /api/v1/prompt-templates ────────────────────────────────────────
    {
      name: 'create_minimal_201',
      route: 'collection',
      method: 'POST',
      body: { name: 'Fresh', content: 'Fresh body.' },
    },
    {
      name: 'create_full_201',
      route: 'collection',
      method: 'POST',
      body: {
        name: 'Fresh Full',
        content: 'Fresh full body.',
        description: 'A description',
        category: 'COMPANION',
        modelHint: 'CLAUDE',
      },
    },
    {
      name: 'create_empty_optionals_become_null_201',
      route: 'collection',
      method: 'POST',
      body: { name: 'Blank Optionals', content: 'x', description: '', category: '', modelHint: '' },
    },
    { name: 'create_zod_missing_both_400', route: 'collection', method: 'POST', body: {} },
    {
      name: 'create_zod_name_empty_400',
      route: 'collection',
      method: 'POST',
      body: { name: '', content: 'x' },
    },
    {
      name: 'create_zod_content_empty_400',
      route: 'collection',
      method: 'POST',
      body: { name: 'n', content: '' },
    },
    {
      name: 'create_zod_name_101_astral_400',
      route: 'collection',
      method: 'POST',
      body: { name: ASTRAL_101, content: 'x' },
    },
    {
      name: 'create_zod_name_100_astral_201',
      route: 'collection',
      method: 'POST',
      body: { name: ASTRAL_100, content: 'x' },
    },
    {
      name: 'create_zod_description_501_400',
      route: 'collection',
      method: 'POST',
      body: { name: 'n', content: 'x', description: 'a'.repeat(501) },
    },
    {
      name: 'create_zod_name_number_400',
      route: 'collection',
      method: 'POST',
      body: { name: 42, content: 'x' },
    },
    { name: 'create_zod_body_null_400', route: 'collection', method: 'POST', body: null },
    {
      name: 'create_zod_body_array_400',
      route: 'collection',
      method: 'POST',
      body: [1, 2],
    },

    // ── GET /api/v1/prompt-templates/[id] ────────────────────────────────────
    {
      name: 'get_user_template_200',
      route: 'item',
      method: 'GET',
      paramId: spec.userTemplateId,
    },
    {
      name: 'get_builtin_200',
      route: 'item',
      method: 'GET',
      paramId: spec.staleBuiltInId,
      seeds: ['staleBuiltIn'],
    },
    { name: 'get_missing_404', route: 'item', method: 'GET', paramId: MISSING_ID },

    // ── PUT /api/v1/prompt-templates/[id] ────────────────────────────────────
    {
      name: 'put_user_template_200',
      route: 'item',
      method: 'PUT',
      paramId: spec.userTemplateId,
      body: { name: 'Renamed', content: 'Rewritten.' },
    },
    {
      name: 'put_clears_optionals_to_null_200',
      route: 'item',
      method: 'PUT',
      paramId: spec.userTemplateId,
      body: { description: '', category: '', modelHint: '' },
    },
    {
      name: 'put_builtin_403',
      route: 'item',
      method: 'PUT',
      paramId: spec.staleBuiltInId,
      body: { name: 'Hijacked' },
      seeds: ['staleBuiltIn'],
    },
    { name: 'put_missing_404', route: 'item', method: 'PUT', paramId: MISSING_ID, body: {} },
    {
      name: 'put_zod_name_empty_400',
      route: 'item',
      method: 'PUT',
      paramId: spec.userTemplateId,
      body: { name: '' },
    },
    {
      name: 'put_zod_beats_missing_404',
      route: 'item',
      method: 'PUT',
      paramId: MISSING_ID,
      body: { name: '' },
    },
    {
      name: 'put_zod_body_null_400',
      route: 'item',
      method: 'PUT',
      paramId: spec.userTemplateId,
      body: null,
    },

    // ── DELETE /api/v1/prompt-templates/[id] ─────────────────────────────────
    {
      name: 'delete_user_template_200',
      route: 'item',
      method: 'DELETE',
      paramId: spec.userTemplateId,
    },
    {
      name: 'delete_builtin_403',
      route: 'item',
      method: 'DELETE',
      paramId: spec.staleBuiltInId,
      seeds: ['staleBuiltIn'],
    },
    { name: 'delete_missing_404', route: 'item', method: 'DELETE', paramId: MISSING_ID },
  ];
}

function mockRequest(url: string, method: Method, body?: unknown): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body === undefined ? {} : body),
  };
}

function applyMocks(userId: string): void {
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
}

/** The two ported log families: the seed line and the route's `[Prompt Templates v1]` lines. */
function isPortedLine(message: unknown): boolean {
  return (
    typeof message === 'string' &&
    (message === 'Sample prompt template seeded from plugin' ||
      message.startsWith('[Prompt Templates v1] '))
  );
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtureMain: string,
): Promise<Record<string, unknown>> {
  jest.resetModules();
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  const userId = c.user === 'B' ? spec.userB : spec.userA;
  applyMocks(userId);

  const work = mkdtempSync(join(scratch, 'pt-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtureMain, mainWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = work;
  fs.mkdirSync(join(work, 'data'), { recursive: true });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  await initializeDatabase();

  // The REAL bundled-plugin inventory: `seedSamplePrompts` reads
  // `systemPromptRegistry`, so an uninitialized registry is v4's seed-nothing arm.
  const { initializePlugins } = await import('@/lib/startup/plugin-initialization');
  await initializePlugins();

  const logLines: Array<{ level: string; message: string; context: unknown }> = [];
  const loggerModule = (await import('@/lib/logger')) as {
    logger: Record<string, (...a: unknown[]) => unknown>;
  };
  const spies: Array<{ mockRestore: () => void }> = [];
  for (const level of ['info', 'warn', 'error', 'debug']) {
    const original = loggerModule.logger[level];
    spies.push(
      jest.spyOn(loggerModule.logger as never, level as never).mockImplementation(((
        message: string,
        context?: unknown,
        err?: unknown,
      ) => {
        if (isPortedLine(message)) {
          logLines.push({ level, message, context: context ?? null });
        }
        return original.call(loggerModule.logger, message, context, err);
      }) as never),
    );
  }

  try {
    const { getRepositories } = await import('@/lib/repositories/factory');
    const repos = getRepositories();

    for (const seed of c.seeds ?? []) {
      if (seed === 'staleBuiltIn') {
        await repos.promptTemplates.create(
          {
            userId: null,
            name: spec.collidingName,
            content: spec.staleContent,
            description: 'A stale plant',
            isBuiltIn: true,
            category: 'GENERAL',
            modelHint: 'MODERN',
            tags: [],
          } as never,
          { id: spec.staleBuiltInId, createdAt: spec.ts, updatedAt: spec.ts } as never,
        );
      } else if (seed === 'userClone') {
        await repos.promptTemplates.create(
          {
            userId: spec.userA,
            name: spec.collidingName,
            content: 'A user template that happens to share a sample prompt name.',
            description: null,
            isBuiltIn: false,
            category: null,
            modelHint: null,
            tags: [],
          } as never,
          { id: spec.userCloneId, createdAt: spec.ts, updatedAt: spec.ts } as never,
        );
      } else if (seed === 'invalidRow') {
        // Deliberately schema-INVALID (a 101-code-point name), so v4's repo
        // `create` cannot write it — the only shape that measures whether the
        // list's `validateSafe` gate drops a stored row. Raw SQL is the point
        // here, not a shortcut.
        await rawQuery(
          'INSERT INTO "prompt_templates" ("id","userId","name","content","description",' +
            '"isBuiltIn","category","modelHint","tags","createdAt","updatedAt") ' +
            'VALUES (?,?,?,?,?,?,?,?,?,?,?)',
          [
            spec.invalidRowId,
            null,
            ASTRAL_101,
            'A row v4 stores but its schema refuses.',
            null,
            1,
            null,
            null,
            '[]',
            spec.ts,
            spec.ts,
          ],
        );
      }
    }

    const url =
      c.route === 'collection'
        ? 'http://x/api/v1/prompt-templates'
        : `http://x/api/v1/prompt-templates/${c.paramId}`;
    const params = { params: Promise.resolve({ id: c.paramId ?? '' }) };

    const mod =
      c.route === 'collection'
        ? ((await import('@/app/api/v1/prompt-templates/route')) as never)
        : ((await import('@/app/api/v1/prompt-templates/[id]/route')) as never);
    const fn = (mod as Record<string, unknown>)[c.method] as (
      req: unknown,
      ctx?: unknown,
    ) => Promise<{ status: number; json: () => Promise<unknown> }>;

    const call = async () =>
      c.route === 'collection'
        ? fn(mockRequest(url, c.method, c.body) as never)
        : fn(mockRequest(url, c.method, c.body) as never, params as never);

    if (c.runTwice) {
      await call();
      logLines.length = 0; // Only the SECOND run's lines are the comparand.
    }
    const response = await call();
    const status = response.status;
    const body = await response.json();

    const table = (await rawQuery(
      'SELECT "id","userId","name","content","description","isBuiltIn","category",' +
        '"modelHint","tags","createdAt","updatedAt" FROM "prompt_templates"',
    )) as unknown;

    return {
      name: c.name,
      req: {
        route: c.route,
        method: c.method,
        user: c.user ?? 'A',
        paramId: c.paramId ?? null,
        body: c.body === undefined ? null : c.body,
        bodyAbsent: c.body === undefined,
        seeds: c.seeds ?? [],
        runTwice: c.runTwice ?? false,
      },
      status,
      body,
      logs: logLines,
      table,
    };
  } finally {
    for (const s of spies) s.mockRestore();
    await closeDatabase();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec: Spec = JSON.parse(
    fs.readFileSync(join(here, '../fixtures/prompt-templates-routes.json'), 'utf8'),
  );
  const fixtureMain = process.env.QT_FIXTURE_PT_ROUTES_MAIN as string;
  const outPath = process.env.QT_ORACLE_OUT as string;
  if (!fixtureMain) throw new Error('set QT_FIXTURE_PT_ROUTES_MAIN');
  if (!outPath) throw new Error('set QT_ORACLE_OUT');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-prompt-templates-oracle-'));
  const lines: string[] = [];
  for (const c of buildCases(spec)) {
    lines.push(JSON.stringify(await runCase(spec, c, scratch, fixtureMain)));
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(`wrote ${lines.length} prompt-templates oracle rows to ${outPath}\n`);
}

test('prompt-templates-routes oracle', async () => {
  await main();
}, 600000);
