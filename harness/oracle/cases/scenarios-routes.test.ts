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
    const out = await c.run();
    const payload: Record<string, unknown> = {
      name: c.name,
      status: out.status,
      body: out.body,
    };
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
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

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
