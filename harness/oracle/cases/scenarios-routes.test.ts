/**
 * @jest-environment node
 *
 * P4.6n SCENARIOS route-surface ORACLE: drives v4's REAL scenario route handlers
 * (groups / projects / general families + the participant-union) over a FRESH copy
 * of the committed groups-projects fixture per case, emitting each response's
 * {status, body} (+ where noted a post-mutation state) so the Rust ports
 * (`api::groups`/`api::projects`/`api::scenarios`) can be diffed byte-for-byte.
 *
 * Success bodies carry the full scenario DTOs; mutation cases that mint fresh
 * document timestamps (create/update) have those blanked on both sides in the
 * Rust harness. Error cases carry {status, body:{error}} — the Rust side asserts
 * the Error kind's HTTP status + message match.
 *
 * Only the auth session + startup gate are mocked; the DB stack is doMocked to the
 * REAL modules (past jest.setup) + the real cipher binding — the exact mold of
 * groups-routes.test.ts.
 *
 * P4.D295 (v4 `545e299bb` at the `83334b9a5` pin): every case also records the
 * scenario lines it logged (`lines` — the location's, the factory's L16–L18,
 * the catalogue's L12 / L19, the union's L20, the create hooks' L38 and the
 * `Project created` / `Group created` INFOs), rendered `LEVEL message k=v`
 * (an array / object field as `kJson`, the logger's error argument as
 * `error=<message>`), and the P4.D295 rows add the location's route edges
 * (Δ10–Δ13), the `%` rows of L-6 / R-Z, the catalogue and the by-ref read, the
 * union's per-group WARN and both create hooks. Minted ids in a create hook's
 * lines are masked to `<id>` / `<mount>`.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-gp-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/scenarios-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/groups-projects.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_GP_MAIN=$V5W/crates/quilltap-web/tests/fixtures/groups-projects-main.db \
 *   QT_FIXTURE_GP_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/groups-projects-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-scenarios-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- scenarios-routes
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

const ARIA = 'a1000000-0000-4000-8000-000000000001';
const BRAM = 'a1000000-0000-4000-8000-000000000002';
const GAMMA = 'a2000000-0000-4000-8000-000000000001';
const IOTA = 'a3000000-0000-4000-8000-000000000001';
const BOGUS = 'a1000000-0000-4000-8000-0000000000ff';

/** [P4.D120] The archived-scenario seed every archived case starts from. */
/** ARIA's group — the participant-union's only hit. */
const BEACON = 'a2000000-0000-4000-8000-000000000003';

const ARCHIVED_FILE = '---\nname: Mothballed\narchived: true\n---\n\nA scene put away.';

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'GET',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

/** [P4.D295] The scenario lines a case logs (see the header). */
const LINE_PREFIXES = [
  '[ScenarioLocation] ',
  '[ScenariosEverywhere] ',
  '[General v1] ',
  '[Projects v1] ',
  '[Groups v1] ',
  'Scenario file body is empty; skipping',
  'Scenarios: multiple isDefault: true entries',
  'Failed to demote a previously-default scenario',
];
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

/** [P4.D295] A raw mount-index statement on the fresh per-case copy. */
async function mountExec(sql: string, ...args: unknown[]): Promise<void> {
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const midb = getRawMountIndexDatabase() as unknown as {
    prepare: (s: string) => { run: (...a: unknown[]) => unknown };
    exec: (s: string) => unknown;
  };
  if (args.length === 0) midb.exec(sql);
  else midb.prepare(sql).run(...args);
}
async function mainExec(sql: string, ...args: unknown[]): Promise<void> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const main = getRawDatabase() as unknown as {
    prepare: (s: string) => { run: (...a: unknown[]) => unknown };
  };
  main.prepare(sql).run(...args);
}
/** Drop a store's `<folder>/` row so a probe can be seen NOT to heal it. */
function dropFolder(scope: 'group' | 'project' | 'general' | 'beacon', folder: string) {
  return async () => {
    const mp = await scopeMount(scope);
    await mountExec(
      'DELETE FROM "doc_mount_folders" WHERE "mountPointId" = ? AND LOWER("path") = ?',
      mp,
      folder.toLowerCase(),
    );
  };
}
/** Make any NEW `Scenarios/` folder row fail to insert — the heal's failure. */
const plantFolderFailure = () =>
  mountExec(
    `CREATE TRIGGER "qt_p4d295_folder_failure" BEFORE INSERT ON "doc_mount_folders"
     WHEN LOWER(NEW."path") = 'scenarios'
     BEGIN SELECT RAISE(ABORT, 'planted folder failure'); END`,
  );
async function folderState(scope: 'group' | 'project' | 'general' | 'beacon') {
  const mp = await scopeMount(scope);
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const midb = getRawMountIndexDatabase() as unknown as {
    prepare: (s: string) => { get: (...a: unknown[]) => { n: number } };
  };
  const has = (f: string) =>
    midb
      .prepare('SELECT COUNT(*) AS n FROM "doc_mount_folders" WHERE "mountPointId" = ? AND LOWER("path") = ?')
      .get(mp, f).n > 0;
  return { scenarios: has('scenarios'), knowledge: has('knowledge') };
}
/** ARIA's vault mount (the character `?action=read` row). */
async function ariaVault(): Promise<string> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const row = (await getRepositories().characters.findByIdRaw(ARIA)) as {
    characterDocumentMountPointId?: string | null;
  } | null;
  const mp = row?.characterDocumentMountPointId;
  if (!mp) throw new Error('ARIA has no vault');
  return mp;
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

interface CaseSpec {
  name: string;
  run: () => Promise<{ status: number; body: unknown }>;
  /** Runs after initializeDatabase, before `run` — e.g. to unprovision the
   *  general mount (delete instance_settings.generalMountPointId). */
  prep?: () => Promise<void>;
  /** [P4.D120] After `run`, emit the RAW content of `<scope>/Scenarios/<file>`
   *  so a write is proven through the file BYTES (frontmatter key order
   *  included), not just the response body. `null` when the file is gone. */
  readBack?: { scope: 'group' | 'project' | 'general' | 'beacon'; file: string };
  /** [P4.D295] After `run`, emit which of `Scenarios/` / `Knowledge/` the
   *  scope's store holds — a probe must not heal, a write must. */
  folderAfter?: 'group' | 'project' | 'general' | 'beacon';
  /** [P4.D295] Mask the created entity's minted ids in the lines (a create
   *  hook) — `<entity>.id` → `<id>`, `.officialMountPointId` → `<mount>`. */
  maskEntity?: 'project' | 'group';
}

/** [P4.D120] The mount id behind each scenario scope, resolved at run time. */
async function scopeMount(scope: 'group' | 'project' | 'general' | 'beacon'): Promise<string> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  if (scope === 'general') {
    const { getGeneralMountPointId } = await import('@/lib/instance-settings');
    const mp = await getGeneralMountPointId();
    if (!mp) throw new Error('general mount unprovisioned');
    return mp;
  }
  const repos = getRepositories();
  const row =
    scope === 'group'
      ? await repos.groups.findByIdRaw(GAMMA)
      : scope === 'beacon'
        ? await repos.groups.findByIdRaw(BEACON)
        : await repos.projects.findByIdRaw(IOTA);
  const mp = row?.officialMountPointId as string;
  if (!mp) throw new Error(`no official store for ${scope}`);
  return mp;
}

/**
 * [P4.D120] Seed one `Scenarios/*.md` file on the fresh per-case copy through
 * the RAW document-store write — never through the route under test. The
 * committed `groups-projects-*` fixture pair is NOT rebuilt (that would mint
 * fresh ids and invalidate every sibling family reading it).
 */
async function seedScenario(
  scope: 'group' | 'project' | 'general' | 'beacon',
  file: string,
  content: string,
): Promise<void> {
  const mp = await scopeMount(scope);
  const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
  const { ensureFolderPath } = await import('@/lib/mount-index/folder-paths');
  await ensureFolderPath(mp, 'Scenarios');
  await writeDatabaseDocument(mp, `Scenarios/${file}`, content);
}

async function readScenarioBytes(
  scope: 'group' | 'project' | 'general' | 'beacon',
  file: string,
): Promise<string | null> {
  const mp = await scopeMount(scope);
  const { getRepositories } = await import('@/lib/repositories/factory');
  const doc = (await getRepositories().docMountDocuments.findByMountPointAndPath(
    mp,
    `Scenarios/${file}`,
  )) as { content?: string } | null;
  return doc?.content ?? null;
}

/** Delete the general mount pointer to exercise the pre-provision race arms. */
async function unprovisionGeneral(): Promise<void> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const main = getRawDatabase() as unknown as {
    prepare: (s: string) => { run: (...a: unknown[]) => unknown };
  };
  main.prepare('DELETE FROM "instance_settings" WHERE "key" = ?').run('generalMountPointId');
}

async function loadRoute(path: string): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
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

  const work = mkdtempSync(join(scratch, 'gp-'));
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
    if (c.prep) await c.prep();
    await installCapture();
    const out = await c.run();
    let lines = capturedLines;
    if (c.maskEntity) {
      const ent = (out.body as Record<string, Record<string, string | null>> | null)?.[c.maskEntity];
      for (const [value, token] of [
        [ent?.id, '<id>'],
        [ent?.officialMountPointId, '<mount>'],
      ] as Array<[string | null | undefined, string]>) {
        if (value) lines = lines.map((l) => l.split(value).join(token));
      }
    }
    const payload: Record<string, unknown> = {
      name: c.name,
      status: out.status,
      body: out.body,
      lines,
    };
    if (c.folderAfter) payload.folders = await folderState(c.folderAfter);
    if (c.readBack) {
      payload.fileBytes = await readScenarioBytes(c.readBack.scope, c.readBack.file);
    }
    return payload;
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

// Route loaders (thin wrappers so cases read cleanly).
const B = 'http://localhost/api/v1';

async function groupCollection(
  method: 'GET' | 'POST',
  id: string,
  body?: unknown,
  query = '',
) {
  const route = await loadRoute('@/app/api/v1/groups/[id]/scenarios/route');
  const req = mockRequest(`${B}/groups/${id}/scenarios${query}`, body);
  const ctx = { params: Promise.resolve({ id }) };
  return respond(await route[method](req, ctx));
}
async function groupItem(
  method: 'GET' | 'PUT' | 'POST' | 'DELETE',
  id: string,
  scenarioPath: string,
  body?: unknown,
  query = '',
) {
  const route = await loadRoute('@/app/api/v1/groups/[id]/scenarios/[scenarioPath]/route');
  const req = mockRequest(`${B}/groups/${id}/scenarios/${scenarioPath}${query}`, body);
  const ctx = { params: Promise.resolve({ id, scenarioPath }) };
  return respond(await route[method](req, ctx));
}
async function groupsUnion(characterIds: string, query = '') {
  const route = await loadRoute('@/app/api/v1/groups/scenarios/route');
  const req = mockRequest(`${B}/groups/scenarios?characterIds=${characterIds}${query}`);
  return respond(await route.GET(req));
}
async function projectCollection(
  method: 'GET' | 'POST',
  id: string,
  body?: unknown,
  query = '',
) {
  const route = await loadRoute('@/app/api/v1/projects/[id]/scenarios/route');
  const req = mockRequest(`${B}/projects/${id}/scenarios${query}`, body);
  const ctx = { params: Promise.resolve({ id }) };
  return respond(await route[method](req, ctx));
}
async function projectItem(
  method: 'GET' | 'PUT' | 'POST' | 'DELETE',
  id: string,
  scenarioPath: string,
  body?: unknown,
  query = '',
) {
  const route = await loadRoute('@/app/api/v1/projects/[id]/scenarios/[scenarioPath]/route');
  const req = mockRequest(`${B}/projects/${id}/scenarios/${scenarioPath}${query}`, body);
  const ctx = { params: Promise.resolve({ id, scenarioPath }) };
  return respond(await route[method](req, ctx));
}
async function projectsRoot(body: unknown) {
  const route = await loadRoute('@/app/api/v1/projects/route');
  const req = mockRequest(`${B}/projects`, body);
  return respond(await route.POST(req));
}
async function groupsRoot(body: unknown) {
  const route = await loadRoute('@/app/api/v1/groups/route');
  const req = mockRequest(`${B}/groups`, body);
  return respond(await route.POST(req));
}
async function generalCollection(method: 'GET' | 'POST', body?: unknown, query = '') {
  const route = await loadRoute('@/app/api/v1/scenarios/route');
  const req = mockRequest(`${B}/scenarios${query}`, body);
  return respond(await route[method](req));
}
async function generalItem(
  method: 'GET' | 'PUT' | 'POST' | 'DELETE',
  scenarioPath: string,
  body?: unknown,
  query = '',
) {
  const route = await loadRoute('@/app/api/v1/scenarios/[scenarioPath]/route');
  const req = mockRequest(`${B}/scenarios/${scenarioPath}${query}`, body);
  const ctx = { params: Promise.resolve({ scenarioPath }) };
  return respond(await route[method](req, ctx));
}

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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-gp-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  // [P4.D295] DEBUG: the location's L1 and its write lines are DEBUG.
  process.env.LOG_LEVEL = 'debug';

  const MISSING_PROJECT = 'a3000000-0000-4000-8000-0000000000ff';
  const readQuery = (mp: string | null, path: string | null) =>
    `?action=read${mp === null ? '' : `&mountPointId=${mp}`}${path === null ? '' : `&path=${encodeURIComponent(path)}`}`;

  const cases: CaseSpec[] = [
    // --- Groups: reads ---
    { name: 'group_list', run: () => groupCollection('GET', GAMMA) },
    { name: 'group_get', run: () => groupItem('GET', GAMMA, 'prologue.md') },
    { name: 'group_get_missing', run: () => groupItem('GET', GAMMA, 'ghost.md') },
    { name: 'group_get_nested', run: () => groupItem('GET', GAMMA, 'sub/deep.md') },
    // --- Groups: create ---
    {
      name: 'group_create',
      run: () =>
        groupCollection('POST', GAMMA, {
          filename: 'Fresh: Take',
          name: 'Fresh Take',
          isDefault: true,
          body: 'Anew.',
        }),
    },
    {
      name: 'group_create_collision',
      run: () => groupCollection('POST', GAMMA, { filename: 'prologue', body: 'dup' }),
    },
    // --- Groups: update ---
    {
      name: 'group_update',
      run: () => groupItem('PUT', GAMMA, 'interlude.md', { name: 'Interlude II', body: 'Changed.' }),
    },
    {
      name: 'group_update_emptybody',
      run: () => groupItem('PUT', GAMMA, 'interlude.md', { body: '' }),
    },
    {
      name: 'group_update_missing',
      run: () => groupItem('PUT', GAMMA, 'ghost.md', { body: 'x' }),
    },
    // --- Groups: rename ---
    {
      name: 'group_rename',
      run: () =>
        groupItem('POST', GAMMA, 'interlude.md', { newFilename: 'interlude-2' }, '?action=rename'),
    },
    {
      name: 'group_rename_noop',
      run: () =>
        groupItem('POST', GAMMA, 'interlude.md', { newFilename: 'interlude' }, '?action=rename'),
    },
    {
      name: 'group_rename_conflict',
      run: () =>
        groupItem('POST', GAMMA, 'interlude.md', { newFilename: 'prologue' }, '?action=rename'),
    },
    // NB: v4's "?action != rename → 400 Unknown action" is an HTTP-routing concern
    // with no v5 dispatch equivalent (the rename variant carries no action), so it
    // is intentionally not in this differential.
    // --- Groups: delete ---
    { name: 'group_delete', run: () => groupItem('DELETE', GAMMA, 'interlude.md') },
    { name: 'group_delete_missing', run: () => groupItem('DELETE', GAMMA, 'ghost.md') },
    // --- Groups: participant-union ---
    { name: 'union_aria', run: () => groupsUnion(ARIA) },
    { name: 'union_bram', run: () => groupsUnion(BRAM) },
    { name: 'union_empty', run: () => groupsUnion('') },
    { name: 'union_unknown', run: () => groupsUnion(BOGUS) },
    // --- Projects (Iota: opening[default], climax) ---
    { name: 'project_list', run: () => projectCollection('GET', IOTA) },
    { name: 'project_get', run: () => projectItem('GET', IOTA, 'opening.md') },
    { name: 'project_get_missing', run: () => projectItem('GET', IOTA, 'ghost.md') },
    { name: 'project_get_nested', run: () => projectItem('GET', IOTA, 'sub/deep.md') },
    {
      name: 'project_create',
      run: () =>
        projectCollection('POST', IOTA, {
          filename: 'Act Two',
          description: 'The middle',
          isDefault: true,
          body: 'It deepens.',
        }),
    },
    {
      name: 'project_create_collision',
      run: () => projectCollection('POST', IOTA, { filename: 'opening', body: 'dup' }),
    },
    {
      name: 'project_update',
      run: () => projectItem('PUT', IOTA, 'climax.md', { name: 'Climax!', body: 'Peak.' }),
    },
    {
      name: 'project_update_emptybody',
      run: () => projectItem('PUT', IOTA, 'climax.md', { body: '' }),
    },
    {
      name: 'project_rename',
      run: () =>
        projectItem('POST', IOTA, 'climax.md', { newFilename: 'climax-2' }, '?action=rename'),
    },
    {
      name: 'project_rename_conflict',
      run: () =>
        projectItem('POST', IOTA, 'climax.md', { newFilename: 'opening' }, '?action=rename'),
    },
    { name: 'project_delete', run: () => projectItem('DELETE', IOTA, 'climax.md') },
    { name: 'project_delete_missing', run: () => projectItem('DELETE', IOTA, 'ghost.md') },
    // ── P4.D120 / v4 `d25dacc1` — archived scenarios ─────────────────────
    // Every case seeds its own file on the fresh copy through the RAW
    // document-store write; the committed fixture pair is untouched.
    {
      name: 'group_list_hides_archived',
      prep: () => seedScenario('group', 'mothballed.md', ARCHIVED_FILE),
      run: () => groupCollection('GET', GAMMA),
    },
    {
      name: 'group_list_shows_archived_with_the_flag',
      prep: () => seedScenario('group', 'mothballed.md', ARCHIVED_FILE),
      run: () => groupCollection('GET', GAMMA, undefined, '?includeArchived=true'),
    },
    {
      name: 'group_list_bare_include_archived_spelling',
      prep: () => seedScenario('group', 'mothballed.md', ARCHIVED_FILE),
      run: () => groupCollection('GET', GAMMA, undefined, '?includeArchived'),
    },
    {
      name: 'group_list_rejects_other_include_archived_spellings',
      prep: () => seedScenario('group', 'mothballed.md', ARCHIVED_FILE),
      run: () => groupCollection('GET', GAMMA, undefined, '?includeArchived=1'),
    },
    {
      name: 'group_list_string_true_frontmatter_is_archived',
      prep: () =>
        seedScenario('group', 'stringy.md', '---\nname: Stringy\narchived: "true"\n---\n\nA quoted flag.'),
      run: () => groupCollection('GET', GAMMA),
    },
    {
      name: 'group_list_other_archived_values_are_active',
      prep: () =>
        seedScenario('group', 'yesish.md', '---\nname: Yesish\narchived: yes\n---\n\nNot the flag.'),
      run: () => groupCollection('GET', GAMMA),
    },
    {
      // `aardvark.md` sorts FIRST, claims the default AND is archived: it must
      // neither win nor be named as the warning's winner, listed or not.
      name: 'group_list_archived_cannot_win_the_default',
      prep: () =>
        seedScenario(
          'group',
          'aardvark.md',
          '---\nname: Aardvark\nisDefault: true\narchived: true\n---\n\nFirst alphabetically.',
        ),
      run: () => groupCollection('GET', GAMMA, undefined, '?includeArchived=true'),
    },
    {
      name: 'group_create_archived_writes_the_flag',
      run: () =>
        groupCollection('POST', GAMMA, {
          filename: 'Put Away',
          name: 'Put Away',
          description: 'Filed for later.',
          archived: true,
          body: 'Stored.',
        }),
      readBack: { scope: 'group', file: 'Put Away.md' },
    },
    {
      // ⚠ The collection-POST quirk: the fresh list is refreshed with
      // `{includeArchived: validated.archived === true}` — the BODY, not the
      // query param. Creating an ACTIVE scenario with "Show archived" ticked
      // returns an archived-free list.
      name: 'group_create_active_ignores_the_query_flag',
      prep: () => seedScenario('group', 'mothballed.md', ARCHIVED_FILE),
      run: () =>
        groupCollection(
          'POST',
          GAMMA,
          { filename: 'Still Here', body: 'Active.' },
          '?includeArchived=true',
        ),
    },
    {
      name: 'group_update_omitting_archived_preserves_it',
      prep: () => seedScenario('group', 'mothballed.md', ARCHIVED_FILE),
      run: () =>
        groupItem(
          'PUT',
          GAMMA,
          'mothballed.md',
          { name: 'Mothballed', body: 'Edited while archived.' },
          '?includeArchived=true',
        ),
      readBack: { scope: 'group', file: 'mothballed.md' },
    },
    {
      name: 'group_update_restoring_deletes_the_key',
      prep: () => seedScenario('group', 'mothballed.md', ARCHIVED_FILE),
      run: () =>
        groupItem('PUT', GAMMA, 'mothballed.md', {
          name: 'Mothballed',
          archived: false,
          body: 'Back in play.',
        }),
      readBack: { scope: 'group', file: 'mothballed.md' },
    },
    {
      name: 'group_update_archiving_an_active_scenario',
      run: () =>
        groupItem('PUT', GAMMA, 'interlude.md', {
          name: 'Interlude',
          archived: true,
          body: 'Put away.',
        }),
      readBack: { scope: 'group', file: 'interlude.md' },
    },
    {
      name: 'group_delete_fresh_list_honours_the_flag',
      prep: () => seedScenario('group', 'mothballed.md', ARCHIVED_FILE),
      run: () => groupItem('DELETE', GAMMA, 'interlude.md', undefined, '?includeArchived=true'),
    },
    {
      name: 'group_rename_fresh_list_honours_the_flag',
      prep: () => seedScenario('group', 'mothballed.md', ARCHIVED_FILE),
      run: () =>
        groupItem(
          'POST',
          GAMMA,
          'interlude.md',
          { newFilename: 'interlude-3' },
          '?action=rename&includeArchived=true',
        ),
    },
    {
      name: 'union_honours_the_flag',
      prep: () => seedScenario('beacon', 'mothballed.md', ARCHIVED_FILE),
      run: () => groupsUnion(ARIA, '&includeArchived=true'),
    },
    {
      name: 'union_hides_archived_by_default',
      prep: () => seedScenario('beacon', 'mothballed.md', ARCHIVED_FILE),
      run: () => groupsUnion(ARIA),
    },
    {
      name: 'project_list_hides_archived',
      prep: () => seedScenario('project', 'mothballed.md', ARCHIVED_FILE),
      run: () => projectCollection('GET', IOTA),
    },
    {
      name: 'project_list_shows_archived_with_the_flag',
      prep: () => seedScenario('project', 'mothballed.md', ARCHIVED_FILE),
      run: () => projectCollection('GET', IOTA, undefined, '?includeArchived=true'),
    },
    {
      name: 'project_update_omitting_archived_preserves_it',
      prep: () => seedScenario('project', 'mothballed.md', ARCHIVED_FILE),
      run: () =>
        projectItem(
          'PUT',
          IOTA,
          'mothballed.md',
          { name: 'Mothballed', body: 'Edited while archived.' },
          '?includeArchived=true',
        ),
      readBack: { scope: 'project', file: 'mothballed.md' },
    },
    {
      name: 'general_list_hides_archived',
      prep: () => seedScenario('general', 'mothballed.md', ARCHIVED_FILE),
      run: () => generalCollection('GET'),
    },
    {
      name: 'general_list_shows_archived_with_the_flag',
      prep: () => seedScenario('general', 'mothballed.md', ARCHIVED_FILE),
      run: () => generalCollection('GET', undefined, '?includeArchived=true'),
    },
    {
      name: 'general_create_archived_writes_the_flag',
      run: () =>
        generalCollection('POST', {
          filename: 'Put Away',
          name: 'Put Away',
          archived: true,
          body: 'Stored.',
        }),
      readBack: { scope: 'general', file: 'Put Away.md' },
    },
    {
      name: 'general_update_omitting_archived_preserves_it',
      prep: () => seedScenario('general', 'mothballed.md', ARCHIVED_FILE),
      run: () =>
        generalItem(
          'PUT',
          'mothballed.md',
          { name: 'Mothballed', body: 'Edited while archived.' },
          '?includeArchived=true',
        ),
      readBack: { scope: 'general', file: 'mothballed.md' },
    },
    {
      // The explicit-null refusal (unify §3): `z.boolean().optional()` accepts
      // an ABSENT key, never null — v4 answers 400 with Zod 4's own sentence
      // through the route's ZodError catch, and WRITES NOTHING. Pre-fix v5
      // silently preserved the stored flag and answered 200.
      name: 'general_update_null_archived_refuses',
      prep: () => seedScenario('general', 'mothballed.md', ARCHIVED_FILE),
      run: () =>
        generalItem('PUT', 'mothballed.md', {
          name: 'Mothballed',
          body: 'Never lands.',
          archived: null,
        }),
      readBack: { scope: 'general', file: 'mothballed.md' },
    },
    {
      // The wrong-type sibling: the arm's sentence must be Zod 4's
      // (`…received string`), not an invented spelling.
      name: 'general_update_string_archived_refuses',
      prep: () => seedScenario('general', 'mothballed.md', ARCHIVED_FILE),
      run: () =>
        generalItem('PUT', 'mothballed.md', {
          name: 'Mothballed',
          body: 'Never lands.',
          archived: 'yes',
        }),
      readBack: { scope: 'general', file: 'mothballed.md' },
    },

    // --- General (aurora[default] + dusk[default] → the default-conflict) ---
    { name: 'general_list', run: () => generalCollection('GET') },
    { name: 'general_get', run: () => generalItem('GET', 'aurora.md') },
    { name: 'general_get_missing', run: () => generalItem('GET', 'ghost.md') },
    {
      name: 'general_create',
      run: () =>
        generalCollection('POST', { filename: 'Twilight', isDefault: true, body: 'Between.' }),
    },
    {
      name: 'general_create_collision',
      run: () => generalCollection('POST', { filename: 'aurora', body: 'dup' }),
    },
    {
      name: 'general_update',
      run: () => generalItem('PUT', 'aurora.md', { name: 'Aurora II', body: 'Brighter.' }),
    },
    {
      name: 'general_rename',
      run: () => generalItem('POST', 'dusk.md', { newFilename: 'evening' }, '?action=rename'),
    },
    { name: 'general_delete', run: () => generalItem('DELETE', 'dusk.md') },
    // --- General: pre-provision race arms (mount pointer deleted) ---
    { name: 'general_list_unprov', prep: unprovisionGeneral, run: () => generalCollection('GET') },
    {
      name: 'general_create_unprov',
      prep: unprovisionGeneral,
      run: () => generalCollection('POST', { filename: 'x', body: 'y' }),
    },
    {
      name: 'general_get_unprov',
      prep: unprovisionGeneral,
      run: () => generalItem('GET', 'aurora.md'),
    },
    // ── P4.D295 (v4 `545e299bb`): the location's route edges ────────────────
    {
      // Δ12: `body.min(1)` passes a whitespace-only body; the file is WRITTEN,
      // then the re-read parses to nothing → 404 `Scenario not found`.
      name: 'project_create_whitespace_body_written_then_404',
      run: () => projectCollection('POST', IOTA, { filename: 'ws', body: '   ' }),
      readBack: { scope: 'project', file: 'ws.md' },
    },
    {
      // Δ13: the collection create parses the body FIRST — an invalid body for
      // a missing project is 400, not 404.
      name: 'project_create_invalid_body_missing_project_is_400',
      run: () => projectCollection('POST', MISSING_PROJECT, { filename: 'x', body: '' }),
    },
    {
      name: 'project_create_valid_body_missing_project_is_404',
      run: () => projectCollection('POST', MISSING_PROJECT, { filename: 'x', body: 'y' }),
    },
    {
      // Δ11: a same-name rename of a MISSING file is 404 (it used to answer 200).
      name: 'project_rename_same_name_missing_is_404',
      run: () => projectItem('POST', IOTA, 'ghost.md', { newFilename: 'ghost' }, '?action=rename'),
    },
    {
      // The same-name rename of an EXISTING file now logs `Renamed`.
      name: 'project_rename_same_name_existing_logs',
      run: () => projectItem('POST', IOTA, 'opening.md', { newFilename: 'opening' }, '?action=rename'),
    },
    {
      name: 'project_rename_empty_new_filename_is_400',
      run: () => projectItem('POST', IOTA, 'opening.md', { newFilename: '' }, '?action=rename'),
    },
    {
      // A GET is a probe: a store whose `Scenarios/` row is gone is NOT healed.
      name: 'project_get_no_folder_never_heals',
      prep: dropFolder('project', 'Scenarios'),
      run: () => projectItem('GET', IOTA, 'opening.md'),
      folderAfter: 'project',
    },
    {
      // ...and a write heals it.
      name: 'project_put_heals_folder',
      prep: dropFolder('project', 'Scenarios'),
      run: () => projectItem('PUT', IOTA, 'opening.md', { name: 'Opening', body: 'Healed.' }),
      folderAfter: 'project',
    },
    {
      // The group's `afterEnsure` heals `Knowledge/` on a write.
      name: 'group_put_heals_knowledge',
      prep: dropFolder('group', 'Knowledge'),
      run: () => groupItem('PUT', GAMMA, 'prologue.md', { name: 'Prologue', body: 'Again.' }),
      folderAfter: 'group',
    },
    {
      // ...but never on a GET.
      name: 'group_get_never_heals_knowledge',
      prep: dropFolder('group', 'Knowledge'),
      run: () => groupItem('GET', GAMMA, 'prologue.md'),
      folderAfter: 'group',
    },
    {
      // L-6: v4 decodes the route segment (here the HTTP encoding of the
      // dispatch string `a%20b.md`) and the location decodes again — v5's
      // single inner decode reads the same `Scenarios/a b.md`.
      name: 'project_get_percent_decoded_once',
      prep: () => seedScenario('project', 'a b.md', '---\nname: Spaced\n---\n\nA spaced file.'),
      run: () => projectItem('GET', IOTA, encodeURIComponent('a%20b.md')),
    },
    {
      // R-Z: a malformed escape THROWS inside the location's read — the GET's
      // catch logs L17 and answers 500.
      name: 'project_get_malformed_percent_500',
      run: () => projectItem('GET', IOTA, encodeURIComponent('%E0.md')),
    },
    {
      name: 'project_put_malformed_percent_500',
      run: () => projectItem('PUT', IOTA, encodeURIComponent('%E0.md'), { body: 'x' }),
    },
    {
      // The collection ensure's folder heal fails: the location's L2, then L18.
      name: 'project_list_folder_failure_500',
      prep: async () => {
        await dropFolder('project', 'Scenarios')();
        await plantFolderFailure();
      },
      run: () => projectCollection('GET', IOTA),
    },
    {
      // The union: a group whose folder heal fails is skipped with L20.
      name: 'union_folder_failure_skips_with_warn',
      prep: async () => {
        await dropFolder('group', 'Scenarios')();
        await dropFolder('beacon', 'Scenarios')();
        await plantFolderFailure();
      },
      run: () => groupsUnion(ARIA),
    },
    {
      // An empty group name: v4's row schema refuses it, so the raw read drops
      // the group and the union skips it (`name || 'Group'` is unreachable
      // through the real repositories).
      name: 'union_empty_group_name_dropped',
      prep: () => mainExec('UPDATE "groups" SET "name" = ? WHERE "id" = ?', '', BEACON),
      run: () => groupsUnion(ARIA),
    },
    // --- the catalogue + the by-ref read (`scenarios/route.ts:47-69`) ---
    { name: 'general_everywhere', run: () => generalCollection('GET', undefined, '?action=everywhere') },
    {
      name: 'general_everywhere_archived',
      prep: () => seedScenario('project', 'mothballed.md', ARCHIVED_FILE),
      run: () => generalCollection('GET', undefined, '?action=everywhere&includeArchived=true'),
    },
    {
      name: 'general_read_missing_mount_point_id',
      run: () => generalCollection('GET', undefined, readQuery(null, 'Scenarios/aurora.md')),
    },
    {
      name: 'general_read_missing_path',
      run: async () => generalCollection('GET', undefined, readQuery(await scopeMount('general'), null)),
    },
    {
      name: 'general_read_refused_bad_path',
      run: async () => generalCollection('GET', undefined, readQuery(await scopeMount('general'), '../x.md')),
    },
    {
      name: 'general_read_refused_not_an_owner',
      run: () => generalCollection('GET', undefined, readQuery('a9000000-0000-4000-8000-0000000000ff', 'Scenarios/x.md')),
    },
    {
      name: 'general_read_missing_file',
      run: async () => generalCollection('GET', undefined, readQuery(await scopeMount('general'), 'Scenarios/ghost.md')),
    },
    {
      name: 'general_read_ok_general_bare_name',
      run: async () => generalCollection('GET', undefined, readQuery(await scopeMount('general'), 'aurora')),
    },
    {
      name: 'general_read_ok_project',
      run: async () => generalCollection('GET', undefined, readQuery(await scopeMount('project'), 'Scenarios/opening.md')),
    },
    {
      name: 'general_read_ok_character',
      prep: async () => {
        const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
        await writeDatabaseDocument(await ariaVault(), 'Scenarios/Arrival.md', '# The Arrival\n\nDawn at the station.');
      },
      run: async () => generalCollection('GET', undefined, readQuery(await ariaVault(), 'Scenarios/Arrival.md')),
    },
    {
      // R-Z: no try/catch on `?action=read` — a malformed escape is the
      // middleware's 500.
      name: 'general_read_malformed_percent',
      run: async () => generalCollection('GET', undefined, readQuery(await scopeMount('general'), 'Scenarios/%E0.md')),
    },
    // --- the create hooks (`projects/route.ts:85-99`, `groups/route.ts:100-115`) ---
    {
      name: 'project_create_hook',
      run: () => projectsRoot({ name: 'Hooked' }),
      maskEntity: 'project',
    },
    {
      // The hook's folder heal fails: the create still succeeds; the
      // location's L2, then the route's L38.
      name: 'project_create_hook_folder_failure',
      prep: plantFolderFailure,
      run: () => projectsRoot({ name: 'Hooked' }),
      maskEntity: 'project',
    },
    {
      name: 'group_create_hook',
      run: () => groupsRoot({ name: 'Hooked' }),
      maskEntity: 'group',
    },
    {
      name: 'group_create_hook_folder_failure',
      prep: plantFolderFailure,
      run: () => groupsRoot({ name: 'Hooked' }),
      maskEntity: 'group',
    },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`scenarios-routes oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('scenarios-routes oracle', async () => {
  await main();
});

// Remove the OS-temp scratch dir(s) once the oracle has written its NDJSON.
const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
