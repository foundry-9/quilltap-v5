/**
 * @jest-environment node
 *
 * P4.6p ROLEPLAY-TEMPLATES route-surface ORACLE: drives v4's REAL roleplay-template
 * route handlers over a FRESH copy of the committed groups-projects fixture per
 * case, emitting each response `{status, body}` so `api::roleplay_templates::*`
 * can be diffed byte-for-byte. Built-in template ids (minted at fixture build) are
 * resolved once from the committed fixture and identical on both differential
 * sides.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-gp-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/roleplay-templates-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/groups-projects.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_GP_MAIN=$V5W/crates/quilltap-web/tests/fixtures/groups-projects-main.db \
 *   QT_FIXTURE_GP_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/groups-projects-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-roleplay-templates-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- roleplay-templates-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

const RT_USER_1 = 'a4000000-0000-4000-8000-000000000001'; // Bracket Prose (patterns present)
const RT_USER_2 = 'a4000000-0000-4000-8000-000000000002'; // Quiet Draft (delimiters + empty patterns)
const BOGUS = 'a4000000-0000-4000-8000-0000000000ff';

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'GET',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

function cipherDriverPath(): string {
  return require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
}

function applyMocks(spec: Spec): void {
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath()));
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

/** Resolve the built-in template ids (minted at fixture build) by opening the
 *  committed fixture main.db directly with the cipher driver. */
function resolveBuiltins(spec: Spec, mainPath: string): Record<string, string> {
  const Database = require(cipherDriverPath());
  const hex = Buffer.from(spec.testPepperBase64, 'base64').toString('hex');
  const db = new Database(mainPath, { readonly: true });
  db.pragma(`key = "x'${hex}'"`);
  const rows = db
    .prepare('SELECT id, name FROM roleplay_templates WHERE isBuiltIn = 1')
    .all() as Array<{ id: string; name: string }>;
  db.close();
  const map: Record<string, string> = {};
  for (const r of rows) map[r.name] = r.id;
  return map;
}

interface CaseSpec {
  name: string;
  run: () => Promise<{ status: number; body: unknown }>;
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

  const work = mkdtempSync(join(scratch, 'rt-'));
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
  try {
    const out = await c.run();
    return { name: c.name, status: out.status, body: out.body };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

const B = 'http://localhost/api/v1/roleplay-templates';
const collGET = () => loadRoute('@/app/api/v1/roleplay-templates/route');
const idRoute = () => loadRoute('@/app/api/v1/roleplay-templates/[id]/route');
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

  const builtins = resolveBuiltins(spec, fixtures.main);
  const STANDARD = builtins['Standard'];
  const QUILLTAP_RP = builtins['Quilltap RP'];
  if (!STANDARD || !QUILLTAP_RP) throw new Error(`built-in ids unresolved: ${JSON.stringify(builtins)}`);

  const scratch = mkdtempSync(join(tmpdir(), 'qt-rt-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cases: CaseSpec[] = [
    { name: 'list', run: async () => respond(await (await collGET()).GET(mockRequest(B))) },
    {
      name: 'create_happy',
      run: async () =>
        respond(
          await (await collGET()).POST(
            mockRequest(B, {
              name: 'Fresh Template',
              description: '  A fresh one.  ',
              systemPrompt: '  Write freshly.  ',
              narrationDelimiters: '*',
              delimiters: [
                { kind: 'wrap', name: 'Emph', buttonName: 'Em', delimiters: '~', style: 'qt-rp-emph' },
                { kind: 'linePrefix', name: 'OOC', buttonName: 'O', marker: '// ', style: 'qt-rp-ooc' },
              ],
            }),
          ),
        ),
    },
    {
      name: 'create_narration_array',
      run: async () =>
        respond(
          await (await collGET()).POST(
            mockRequest(B, {
              name: 'Bracketed',
              systemPrompt: 'Use brackets.',
              narrationDelimiters: ['[', ']'],
              delimiters: [
                { kind: 'wrap', name: 'Nar', buttonName: 'N', delimiters: ['[', ']'], style: 'qt-rp-nar' },
              ],
            }),
          ),
        ),
    },
    {
      name: 'create_name_required',
      run: async () => respond(await (await collGET()).POST(mockRequest(B, {}))),
    },
    {
      name: 'create_sysprompt_required',
      run: async () => respond(await (await collGET()).POST(mockRequest(B, { name: 'X' }))),
    },
    {
      name: 'create_narration_required',
      run: async () =>
        respond(await (await collGET()).POST(mockRequest(B, { name: 'X', systemPrompt: 'Y' }))),
    },
    {
      name: 'create_dup',
      run: async () =>
        respond(
          await (await collGET()).POST(
            mockRequest(B, { name: 'Bracket Prose', systemPrompt: 'Y', narrationDelimiters: '*' }),
          ),
        ),
    },
    {
      name: 'get_user',
      run: async () =>
        respond(await (await idRoute()).GET(mockRequest(`${B}/${RT_USER_1}`), params(RT_USER_1))),
    },
    {
      name: 'get_regen',
      run: async () =>
        respond(await (await idRoute()).GET(mockRequest(`${B}/${RT_USER_2}`), params(RT_USER_2))),
    },
    {
      name: 'get_builtin',
      run: async () =>
        respond(await (await idRoute()).GET(mockRequest(`${B}/${STANDARD}`), params(STANDARD))),
    },
    {
      name: 'get_missing',
      run: async () =>
        respond(await (await idRoute()).GET(mockRequest(`${B}/${BOGUS}`), params(BOGUS))),
    },
    {
      name: 'update_happy',
      run: async () =>
        respond(
          await (await idRoute()).PUT(
            mockRequest(`${B}/${RT_USER_1}`, { name: 'Renamed Prose', systemPrompt: '  Trimmed.  ' }),
            params(RT_USER_1),
          ),
        ),
    },
    {
      name: 'update_builtin_403',
      run: async () =>
        respond(
          await (await idRoute()).PUT(
            mockRequest(`${B}/${STANDARD}`, { name: 'Nope' }),
            params(STANDARD),
          ),
        ),
    },
    {
      name: 'update_dup',
      run: async () =>
        respond(
          await (await idRoute()).PUT(
            mockRequest(`${B}/${RT_USER_2}`, { name: 'Bracket Prose' }),
            params(RT_USER_2),
          ),
        ),
    },
    {
      name: 'update_regen_delims',
      run: async () =>
        respond(
          await (await idRoute()).PUT(
            mockRequest(`${B}/${RT_USER_1}`, {
              name: 'Bracket Prose',
              systemPrompt: 'Keep prose.',
              delimiters: [
                { kind: 'wrap', name: 'Q', buttonName: 'Q', delimiters: '"', style: 'qt-rp-q' },
              ],
            }),
            params(RT_USER_1),
          ),
        ),
    },
    {
      name: 'update_regen_narration',
      run: async () =>
        respond(
          await (await idRoute()).PUT(
            mockRequest(`${B}/${RT_USER_1}`, {
              name: 'Bracket Prose',
              systemPrompt: 'Keep prose.',
              narrationDelimiters: '|',
            }),
            params(RT_USER_1),
          ),
        ),
    },
    {
      name: 'update_null_description',
      run: async () =>
        respond(
          await (await idRoute()).PUT(
            mockRequest(`${B}/${RT_USER_1}`, {
              name: 'Bracket Prose',
              systemPrompt: 'Keep prose.',
              description: null,
            }),
            params(RT_USER_1),
          ),
        ),
    },
    {
      name: 'update_missing_required',
      run: async () =>
        respond(
          await (await idRoute()).PUT(
            mockRequest(`${B}/${RT_USER_1}`, {
              delimiters: [
                { kind: 'wrap', name: 'Q', buttonName: 'Q', delimiters: '"', style: 'qt-rp-q' },
              ],
            }),
            params(RT_USER_1),
          ),
        ),
    },
    {
      name: 'delete_happy',
      run: async () =>
        respond(
          await (await idRoute()).DELETE(mockRequest(`${B}/${RT_USER_2}`), params(RT_USER_2)),
        ),
    },
    {
      name: 'delete_builtin_403',
      run: async () =>
        respond(
          await (await idRoute()).DELETE(mockRequest(`${B}/${QUILLTAP_RP}`), params(QUILLTAP_RP)),
        ),
    },
    {
      name: 'delete_missing',
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
  process.stderr.write(`roleplay-templates oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('roleplay-templates-routes oracle', async () => {
  await main();
});
