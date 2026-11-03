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
 * P4.D351 (v4 `778b97b2b`, the Novel template): every case also emits the v4
 * log lines it wrote (`lines`, rendered `LEVEL message k=v`) whose message
 * starts with `Roleplay template ` — the two NEW DEBUGs (`… patterns resolved
 * for create`, `… patterns regenerated for update`) and the pre-existing INFOs.
 * v4's `initializeDatabase()` does NOT seed the built-ins — production runs
 * `repos.roleplayTemplates.seedBuiltInTemplates()` from `lib/startup/
 * seed-initial-data.ts` at boot — so each case calls that REAL seed on its copy
 * right after `initializeDatabase()` (the Rust family runs v5's
 * `seed_built_in_templates` on its copy the same way). The seed inserts Novel
 * with an id / stamps minted per copy, so a case that needs Novel's id resolves
 * it on the case copy AFTER the seed.
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

/** The v4 lines a case logs that the Rust family compares. */
const LINE_PREFIXES = ['Roleplay template '];
let capturedLines: string[] = [];

function renderField(k: string, v: unknown): string {
  if (v === null || v === undefined) return ` ${k}=null`;
  if (Array.isArray(v) || typeof v === 'object') return ` ${k}Json=${JSON.stringify(v)}`;
  return ` ${k}=${String(v)}`;
}

async function installCapture(): Promise<void> {
  capturedLines = [];
  const { logger } = await import('@/lib/logger');
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as {
        level: string;
        message: string;
        context?: Record<string, unknown>;
        error?: { message?: string };
      };
      if (!LINE_PREFIXES.some((p) => data.message.startsWith(p))) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      let line = `${data.level.toUpperCase()} ${data.message}`;
      for (const [k, v] of Object.entries(fields)) line += renderField(k, v);
      if (data.error) line += ` error=${data.error.message}`;
      capturedLines.push(line);
    },
  });
}

/** The seeded Novel built-in's id on THIS case's copy (minted by the seed). */
function novelIdOnCopy(spec: Spec): string {
  const Database = require(cipherDriverPath());
  const hex = Buffer.from(spec.testPepperBase64, 'base64').toString('hex');
  const db = new Database(process.env.SQLITE_PATH, { readonly: true });
  db.pragma(`key = "x'${hex}'"`);
  const row = db
    .prepare("SELECT id FROM roleplay_templates WHERE isBuiltIn = 1 AND name = 'Novel'")
    .get() as { id: string } | undefined;
  db.close();
  if (!row) throw new Error('the seeded Novel row is missing on the case copy');
  return row.id;
}

/** Call a route twice in one case (create then GET), so the body carries both. */
async function both(
  first: () => Promise<{ status: number; body: unknown }>,
  second: (firstBody: Record<string, unknown>) => Promise<{ status: number; body: unknown }>,
): Promise<{ status: number; body: unknown }> {
  const a = await first();
  const b = await second(a.body as Record<string, unknown>);
  return { status: b.status, body: { first: a, second: b } };
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
  const { getRepositories } = await import('@/lib/database/repositories');
  await getRepositories().roleplayTemplates.seedBuiltInTemplates();
  await installCapture();
  try {
    const out = await c.run();
    return { name: c.name, status: out.status, body: out.body, lines: capturedLines.slice() };
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
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  // DEBUG so the two P4.D351 lines reach the capture transport.
  process.env.LOG_LEVEL = 'debug';

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
    // --- P4.D351: the Novel template's route arms (v4 778b97b2b) ---
    {
      name: 'create_null_narration',
      run: async () =>
        respond(
          await (await collGET()).POST(
            mockRequest(B, {
              name: 'Plain Prose',
              systemPrompt: 'Write plainly.',
              narrationDelimiters: null,
              delimiters: [
                { kind: 'wrap', name: 'Speech', buttonName: 'S', delimiters: '"', style: 'qt-chat-dialogue' },
              ],
            }),
          ),
        ),
    },
    {
      name: 'create_narration_empty_string',
      run: async () =>
        respond(await (await collGET()).POST(mockRequest(B, { name: 'X', systemPrompt: 'Y', narrationDelimiters: '' }))),
    },
    {
      name: 'create_narration_zero',
      run: async () =>
        respond(await (await collGET()).POST(mockRequest(B, { name: 'X', systemPrompt: 'Y', narrationDelimiters: 0 }))),
    },
    {
      name: 'create_narration_false',
      run: async () =>
        respond(
          await (await collGET()).POST(mockRequest(B, { name: 'X', systemPrompt: 'Y', narrationDelimiters: false })),
        ),
    },
    {
      name: 'create_narration_object',
      run: async () =>
        respond(
          await (await collGET()).POST(mockRequest(B, { name: 'X', systemPrompt: 'Y', narrationDelimiters: { a: 1 } })),
        ),
    },
    {
      name: 'create_unmarked_style_bad_empty',
      run: async () =>
        respond(
          await (await collGET()).POST(
            mockRequest(B, { name: 'X', systemPrompt: 'Y', narrationDelimiters: null, unmarkedStyle: { style: '' } }),
          ),
        ),
    },
    {
      name: 'create_unmarked_style_bad_type',
      run: async () =>
        respond(
          await (await collGET()).POST(
            mockRequest(B, { name: 'X', systemPrompt: 'Y', narrationDelimiters: null, unmarkedStyle: 5 }),
          ),
        ),
    },
    {
      name: 'create_unmarked_style_missing_style',
      run: async () =>
        respond(
          await (await collGET()).POST(
            mockRequest(B, { name: 'X', systemPrompt: 'Y', narrationDelimiters: null, unmarkedStyle: {} }),
          ),
        ),
    },
    {
      name: 'create_unmarked_style_too_long',
      run: async () =>
        respond(
          await (await collGET()).POST(
            // 51 two-UTF-16-unit glyphs: 51 code points, 102 UTF-16 units.
            mockRequest(B, {
              name: 'X',
              systemPrompt: 'Y',
              narrationDelimiters: null,
              unmarkedStyle: { style: '\u{1D4D2}'.repeat(51) },
            }),
          ),
        ),
    },
    {
      name: 'create_unmarked_style_fifty_code_points_ok',
      run: async () =>
        respond(
          await (await collGET()).POST(
            mockRequest(B, {
              name: 'Fifty Glyphs',
              systemPrompt: 'Y',
              narrationDelimiters: null,
              unmarkedStyle: { style: '\u{1D4D2}'.repeat(50) },
            }),
          ),
        ),
    },
    {
      name: 'create_unmarked_style_bad_addons',
      run: async () =>
        respond(
          await (await collGET()).POST(
            mockRequest(B, { name: 'X', systemPrompt: 'Y', narrationDelimiters: null, unmarkedStyle: { style: 'x', addOns: 5 } }),
          ),
        ),
    },
    {
      name: 'create_unmarked_style_bad_addons_enum',
      run: async () =>
        respond(
          await (await collGET()).POST(
            mockRequest(B, {
              name: 'X',
              systemPrompt: 'Y',
              narrationDelimiters: null,
              unmarkedStyle: { style: 'x', addOns: { underline: 'wavy' } },
            }),
          ),
        ),
    },
    {
      name: 'create_unmarked_null_explicit',
      run: async () =>
        respond(
          await (await collGET()).POST(
            mockRequest(B, { name: 'Null Unmarked', systemPrompt: 'Y', narrationDelimiters: '*', unmarkedStyle: null }),
          ),
        ),
    },
    {
      name: 'create_unmarked_partial_addons',
      run: async () =>
        respond(
          await (await collGET()).POST(
            mockRequest(B, {
              name: 'Partial Addons',
              systemPrompt: 'Y',
              narrationDelimiters: null,
              unmarkedStyle: { style: 'qt-roleplay-plain', addOns: { italic: true } },
            }),
          ),
        ),
    },
    {
      name: 'create_markdown_asterisks_not_bool',
      run: async () =>
        respond(
          await (await collGET()).POST(
            mockRequest(B, { name: 'X', systemPrompt: 'Y', narrationDelimiters: '*', markdownAsterisks: 'yes' }),
          ),
        ),
    },
    {
      name: 'create_markdown_asterisks_null',
      run: async () =>
        respond(
          await (await collGET()).POST(
            mockRequest(B, { name: 'X', systemPrompt: 'Y', narrationDelimiters: '*', markdownAsterisks: null }),
          ),
        ),
    },
    {
      name: 'create_markdown_false_regen',
      run: async () =>
        respond(
          await (await collGET()).POST(
            mockRequest(B, {
              name: 'No Markdown Stars',
              systemPrompt: 'Y',
              narrationDelimiters: '*',
              markdownAsterisks: false,
              delimiters: [{ kind: 'wrap', name: 'Emph', buttonName: 'E', delimiters: '_', style: 'qt-rp-emph' }],
            }),
          ),
        ),
    },
    {
      name: 'create_unmarked_only_regen',
      run: async () =>
        both(
          async () =>
            respond(
              await (await collGET()).POST(
                mockRequest(B, {
                  name: 'Unmarked Only',
                  systemPrompt: 'Y',
                  narrationDelimiters: null,
                  unmarkedStyle: { style: 'qt-roleplay-plain' },
                }),
              ),
            ),
          async (created) =>
            respond(
              await (await idRoute()).GET(
                mockRequest(`${B}/${created.id as string}`),
                params(created.id as string),
              ),
            ),
        ),
    },
    {
      name: 'create_empty_patterns_no_regen_input',
      run: async () =>
        respond(
          await (await collGET()).POST(
            mockRequest(B, { name: 'Nothing To Derive', systemPrompt: 'Y', narrationDelimiters: null }),
          ),
        ),
    },
    {
      name: 'create_novel_like',
      run: async () => {
        const { NOVEL_TEMPLATE } = await import('@/lib/chat/novel-template');
        return respond(
          await (await collGET()).POST(
            mockRequest(B, {
              name: 'My Novel',
              description: NOVEL_TEMPLATE.description,
              systemPrompt: NOVEL_TEMPLATE.systemPrompt,
              delimiters: NOVEL_TEMPLATE.delimiters,
              dialogueDetection: NOVEL_TEMPLATE.dialogueDetection,
              narrationDelimiters: NOVEL_TEMPLATE.narrationDelimiters,
              unmarkedStyle: NOVEL_TEMPLATE.unmarkedStyle,
              markdownAsterisks: NOVEL_TEMPLATE.markdownAsterisks,
            }),
          ),
        );
      },
    },
    {
      name: 'create_wrap_curly_not_bool',
      run: async () =>
        respond(
          await (await collGET()).POST(
            mockRequest(B, {
              name: 'X',
              systemPrompt: 'Y',
              narrationDelimiters: '*',
              delimiters: [
                { kind: 'wrap', name: 'Q', buttonName: 'Q', delimiters: '"', style: 'qt-rp-q', matchCurlyQuotes: 'yes' },
              ],
            }),
          ),
        ),
    },
    {
      name: 'create_wrap_close_optional_null',
      run: async () =>
        respond(
          await (await collGET()).POST(
            mockRequest(B, {
              name: 'X',
              systemPrompt: 'Y',
              narrationDelimiters: '*',
              delimiters: [
                { kind: 'wrap', name: 'Q', buttonName: 'Q', delimiters: '"', style: 'qt-rp-q', closeOptional: null },
              ],
            }),
          ),
        ),
    },
    {
      name: 'create_wrap_flags_kept',
      run: async () =>
        respond(
          await (await collGET()).POST(
            mockRequest(B, {
              name: 'Flagged Wrap',
              systemPrompt: 'Y',
              narrationDelimiters: null,
              delimiters: [
                { kind: 'wrap', name: 'Q', buttonName: 'Q', delimiters: '"', style: 'qt-rp-q', matchCurlyQuotes: true, closeOptional: false },
              ],
            }),
          ),
        ),
    },
    {
      name: 'get_builtin_novel',
      run: async () => {
        const id = novelIdOnCopy(spec);
        return respond(await (await idRoute()).GET(mockRequest(`${B}/${id}`), params(id)));
      },
    },
    {
      name: 'update_narration_null',
      run: async () =>
        respond(
          await (await idRoute()).PUT(
            mockRequest(`${B}/${RT_USER_1}`, {
              name: 'Bracket Prose',
              systemPrompt: 'Keep prose.',
              narrationDelimiters: null,
            }),
            params(RT_USER_1),
          ),
        ),
    },
    {
      name: 'update_regen_unmarked_only',
      run: async () =>
        respond(
          await (await idRoute()).PUT(
            mockRequest(`${B}/${RT_USER_1}`, {
              name: 'Bracket Prose',
              systemPrompt: 'Keep prose.',
              unmarkedStyle: { style: 'qt-roleplay-plain', addOns: { bold: true } },
            }),
            params(RT_USER_1),
          ),
        ),
    },
    {
      name: 'update_regen_markdown_only',
      run: async () =>
        respond(
          await (await idRoute()).PUT(
            mockRequest(`${B}/${RT_USER_2}`, {
              name: 'Quiet Draft',
              systemPrompt: 'Keep quiet.',
              markdownAsterisks: false,
            }),
            params(RT_USER_2),
          ),
        ),
    },
    {
      name: 'update_unmarked_null_clears',
      run: async () =>
        both(
          async () =>
            respond(
              await (await idRoute()).PUT(
                mockRequest(`${B}/${RT_USER_1}`, {
                  name: 'Bracket Prose',
                  systemPrompt: 'Keep prose.',
                  unmarkedStyle: { style: 'qt-roleplay-plain' },
                }),
                params(RT_USER_1),
              ),
            ),
          async () =>
            respond(
              await (await idRoute()).PUT(
                mockRequest(`${B}/${RT_USER_1}`, {
                  name: 'Bracket Prose',
                  systemPrompt: 'Keep prose.',
                  unmarkedStyle: null,
                }),
                params(RT_USER_1),
              ),
            ),
        ),
    },
    {
      name: 'update_keeps_unmarked_when_absent',
      run: async () =>
        both(
          async () =>
            respond(
              await (await idRoute()).PUT(
                mockRequest(`${B}/${RT_USER_1}`, {
                  name: 'Bracket Prose',
                  systemPrompt: 'Keep prose.',
                  unmarkedStyle: { style: 'qt-roleplay-plain' },
                  markdownAsterisks: false,
                }),
                params(RT_USER_1),
              ),
            ),
          async () =>
            respond(
              await (await idRoute()).PUT(
                mockRequest(`${B}/${RT_USER_1}`, {
                  name: 'Bracket Prose',
                  systemPrompt: 'Keep prose.',
                  narrationDelimiters: '~',
                }),
                params(RT_USER_1),
              ),
            ),
        ),
    },
    {
      name: 'update_narration_empty_string',
      run: async () =>
        respond(
          await (await idRoute()).PUT(
            mockRequest(`${B}/${RT_USER_1}`, { name: 'Bracket Prose', systemPrompt: 'K', narrationDelimiters: '' }),
            params(RT_USER_1),
          ),
        ),
    },
    {
      name: 'update_narration_bad_pair',
      run: async () =>
        respond(
          await (await idRoute()).PUT(
            mockRequest(`${B}/${RT_USER_1}`, { name: 'Bracket Prose', systemPrompt: 'K', narrationDelimiters: ['[', ''] }),
            params(RT_USER_1),
          ),
        ),
    },
    {
      name: 'update_narration_number',
      run: async () =>
        respond(
          await (await idRoute()).PUT(
            mockRequest(`${B}/${RT_USER_1}`, { name: 'Bracket Prose', systemPrompt: 'K', narrationDelimiters: 7 }),
            params(RT_USER_1),
          ),
        ),
    },
    {
      name: 'update_narration_triple',
      run: async () =>
        respond(
          await (await idRoute()).PUT(
            mockRequest(`${B}/${RT_USER_1}`, { name: 'Bracket Prose', systemPrompt: 'K', narrationDelimiters: ['a', 'b', 'c'] }),
            params(RT_USER_1),
          ),
        ),
    },
    {
      name: 'update_narration_single_elem',
      run: async () =>
        respond(
          await (await idRoute()).PUT(
            mockRequest(`${B}/${RT_USER_1}`, { name: 'Bracket Prose', systemPrompt: 'K', narrationDelimiters: ['a'] }),
            params(RT_USER_1),
          ),
        ),
    },
    {
      name: 'update_narration_pair_nonstring',
      run: async () =>
        respond(
          await (await idRoute()).PUT(
            mockRequest(`${B}/${RT_USER_1}`, { name: 'Bracket Prose', systemPrompt: 'K', narrationDelimiters: [1, ''] }),
            params(RT_USER_1),
          ),
        ),
    },
    {
      name: 'create_unmarked_style_addons_null',
      run: async () =>
        respond(
          await (await collGET()).POST(
            mockRequest(B, { name: 'X', systemPrompt: 'Y', narrationDelimiters: null, unmarkedStyle: { style: 'x', addOns: null } }),
          ),
        ),
    },
    {
      name: 'update_unmarked_style_bad',
      run: async () =>
        respond(
          await (await idRoute()).PUT(
            mockRequest(`${B}/${RT_USER_1}`, { name: 'Bracket Prose', systemPrompt: 'K', unmarkedStyle: { style: '' } }),
            params(RT_USER_1),
          ),
        ),
    },
    {
      name: 'update_markdown_asterisks_not_bool',
      run: async () =>
        respond(
          await (await idRoute()).PUT(
            mockRequest(`${B}/${RT_USER_1}`, { name: 'Bracket Prose', systemPrompt: 'K', markdownAsterisks: 'yes' }),
            params(RT_USER_1),
          ),
        ),
    },
    {
      name: 'update_two_new_key_refusals',
      run: async () =>
        respond(
          await (await idRoute()).PUT(
            mockRequest(`${B}/${RT_USER_1}`, {
              name: 'Bracket Prose',
              systemPrompt: 'K',
              narrationDelimiters: '',
              markdownAsterisks: 1,
            }),
            params(RT_USER_1),
          ),
        ),
    },
    {
      name: 'update_patterns_sent_no_regen',
      run: async () =>
        respond(
          await (await idRoute()).PUT(
            mockRequest(`${B}/${RT_USER_1}`, {
              name: 'Bracket Prose',
              systemPrompt: 'Keep prose.',
              markdownAsterisks: false,
              renderingPatterns: [{ pattern: 'x', className: 'qt-x' }],
            }),
            params(RT_USER_1),
          ),
        ),
    },
    {
      name: 'update_name_only_no_regen',
      run: async () =>
        respond(
          await (await idRoute()).PUT(
            mockRequest(`${B}/${RT_USER_1}`, { name: 'Renamed Prose', systemPrompt: 'Keep prose.' }),
            params(RT_USER_1),
          ),
        ),
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

// Remove the OS-temp scratch dir(s) once the oracle has written its NDJSON.
const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
