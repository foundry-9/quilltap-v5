/**
 * @jest-environment node
 *
 * P4.6k GROUPS route-surface ORACLE: drives v4's REAL groups route handlers over
 * a FRESH copy of the committed groups-projects fixture per case, and emits each
 * response body (+ post-mutation table dumps) so the Rust ports (`api::groups::*`)
 * can be diffed byte-for-byte.
 *
 * Reads: list (+ _count.members), detail (rich + empty), members (rich + empty),
 * mount-points (rich → dangling filtered + empty). Mutations: create (+ the
 * Scenarios/Knowledge folder-ensure dumped as DB state), update, delete (+ the
 * memberships/links/rows dump — the official store SURVIVES), addMember,
 * removeMember, mount link (idempotent echo), mount unlink (+ link dump).
 *
 * Only the seams the Rust harness also neutralizes are mocked: the auth session
 * and the startup gate. The DB stack is doMocked to the REAL modules (past
 * jest.setup) + the real cipher binding.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-gp-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/groups-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/groups-projects.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_GP_MAIN=$V5W/crates/quilltap-web/tests/fixtures/groups-projects-main.db \
 *   QT_FIXTURE_GP_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/groups-projects-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-groups-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- groups-routes
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

const GAMMA = 'a2000000-0000-4000-8000-000000000001';
const DELTA = 'a2000000-0000-4000-8000-000000000002';
const ARIA = 'a1000000-0000-4000-8000-000000000001';
const BRAM = 'a1000000-0000-4000-8000-000000000002';
const CLEO = 'a1000000-0000-4000-8000-000000000003';
/** P4.D63: the archived character (fixture extension). */
const EDDA = 'a1000000-0000-4000-8000-000000000005';
const GAMMA_EXTRA_MP = 'b0000000-0000-4000-8000-000000000001';

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

/** Dump the group slim rows + memberships + links (baked ids → no remap). */
async function dumpGroupTables(): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const main = getRawDatabase() as unknown as {
    prepare: (s: string) => { all: () => unknown };
  };
  const mount = getRawMountIndexDatabase() as unknown as {
    prepare: (s: string) => { all: () => unknown };
  };
  return {
    groups: main.prepare('SELECT id, name FROM groups ORDER BY id').all(),
    members: mount
      .prepare('SELECT groupId, characterId FROM group_character_members ORDER BY groupId, characterId')
      .all(),
    links: mount
      .prepare('SELECT groupId, mountPointId FROM group_doc_mount_links ORDER BY groupId, mountPointId')
      .all(),
    // Whether the official store mount point survives delete (name-only — the id
    // is minted at fixture build, but identical on both differential sides).
    mountPointNames: mount.prepare('SELECT name FROM doc_mount_points ORDER BY name').all(),
  };
}

/** After create, dump the created group's official store folder rows (proves the
 *  Scenarios/ + Knowledge/ folder-ensure side effect; the minted mount id is not
 *  selected — the relativePaths are deterministic). */
async function dumpCreatedGroupFolders(createdMountPointId: string): Promise<unknown> {
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const mount = getRawMountIndexDatabase() as unknown as {
    prepare: (s: string) => { all: (...a: unknown[]) => unknown };
  };
  return {
    folders: mount
      .prepare('SELECT path FROM doc_mount_folders WHERE mountPointId = ? ORDER BY path')
      .all(createdMountPointId),
  };
}

interface CaseSpec {
  name: string;
  run: (mods: Record<string, unknown>) => Promise<{ status: number; body: unknown; tables?: unknown }>;
}

async function loadRoute(path: string): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
  return (await import(path)) as never;
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
    const out = await c.run({});
    return { name: c.name, status: out.status, body: out.body, ...(out.tables !== undefined ? { tables: out.tables } : {}) };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function respond(
  r: unknown,
): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
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

  const B = 'http://localhost/api/v1/groups';
  const cases: CaseSpec[] = [
    // --- Reads ---
    {
      name: 'list',
      run: async () => respond(await (await loadRoute('@/app/api/v1/groups/route')).GET(mockRequest(B))),
    },
    {
      name: 'get_gamma',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/groups/[id]/route')).GET(mockRequest(`${B}/${GAMMA}`), {
            params: Promise.resolve({ id: GAMMA }),
          }),
        ),
    },
    {
      name: 'get_delta',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/groups/[id]/route')).GET(mockRequest(`${B}/${DELTA}`), {
            params: Promise.resolve({ id: DELTA }),
          }),
        ),
    },
    {
      name: 'members_gamma',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/groups/[id]/route')).GET(
            mockRequest(`${B}/${GAMMA}?action=members`),
            { params: Promise.resolve({ id: GAMMA }) },
          ),
        ),
    },
    {
      name: 'members_delta',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/groups/[id]/route')).GET(
            mockRequest(`${B}/${DELTA}?action=members`),
            { params: Promise.resolve({ id: DELTA }) },
          ),
        ),
    },
    {
      name: 'mount_points_gamma',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/groups/[id]/mount-points/route')).GET(
            mockRequest(`${B}/${GAMMA}/mount-points`),
            { params: Promise.resolve({ id: GAMMA }) },
          ),
        ),
    },
    {
      name: 'mount_points_delta',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/groups/[id]/mount-points/route')).GET(
            mockRequest(`${B}/${DELTA}/mount-points`),
            { params: Promise.resolve({ id: DELTA }) },
          ),
        ),
    },
    // --- Mutations ---
    {
      name: 'create',
      run: async () => {
        const r = await (await loadRoute('@/app/api/v1/groups/route')).POST(
          mockRequest(B, { name: 'Epsilon', description: 'A new group', color: '#abcdef', icon: 'gear' }),
        );
        const { status, body } = await respond(r);
        const mp = (body as { group?: { officialMountPointId?: string } })?.group?.officialMountPointId;
        const tables = mp ? await dumpCreatedGroupFolders(mp) : null;
        return { status, body, tables };
      },
    },
    // ---- P4.D103 (v4 `8f868109`): the `instructions` field + BOTH validators.
    {
      // Create WITH instructions — the hydrated create response carries them.
      name: 'create_with_instructions',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/groups/route')).POST(
            mockRequest(B, { name: 'Zeta', description: 'z', instructions: 'Zeta rule: cite your sources.', color: '#abcdef', icon: 'gear' }),
          ),
        ),
    },
    {
      // `instructions: ''` → `validatedData.instructions || null` on CREATE, so
      // the empty string normalizes to null. (The UPDATE path has no `|| null`
      // — see `update_instructions_empty_string_reads_back_null` below.)
      name: 'create_empty_instructions_normalizes_to_null',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/groups/route')).POST(
            mockRequest(B, { name: 'Eta', description: 'e', instructions: '', color: '#abcdef', icon: 'gear' }),
          ),
        ),
    },
    {
      // `.min(1)` is on the RAW string: a whitespace-only name is length 3 and
      // PASSES. v5 used to reject it with a hand-rolled `trim().isEmpty()` check.
      name: 'create_whitespace_name_passes',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/groups/route')).POST(
            mockRequest(B, { name: '   ', description: 'w', color: '#abcdef', icon: 'gear' }),
          ),
        ),
    },
    {
      // An EMPTY name fails `.min(1)`. The middleware's sentence is the flat
      // `Validation error` — `'Name is required'` only ever appears in `details`.
      name: 'create_empty_name_400',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/groups/route')).POST(
            mockRequest(B, { name: '' }),
          ),
        ),
    },
    {
      name: 'create_instructions_over_cap_400',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/groups/route')).POST(
            mockRequest(B, { name: 'Theta', instructions: 'x'.repeat(10001) }),
          ),
        ),
    },
    {
      name: 'create_instructions_at_cap_ok',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/groups/route')).POST(
            mockRequest(B, { name: 'Iota', description: 'i', instructions: 'y'.repeat(10000), color: '#abcdef', icon: 'gear' }),
          ),
        ),
    },
    {
      name: 'create_non_string_instructions_400',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/groups/route')).POST(
            mockRequest(B, { name: 'Kappa', instructions: 42 }),
          ),
        ),
    },
    {
      name: 'update_set_instructions',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/groups/[id]/route')).PUT(
            mockRequest(`${B}/${GAMMA}`, { instructions: 'Gamma rule: no exclamation marks.' }),
            { params: Promise.resolve({ id: GAMMA }) },
          ),
        ),
    },
    {
      name: 'update_clear_instructions_null',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/groups/[id]/route')).PUT(
            mockRequest(`${B}/${GAMMA}`, { instructions: null }),
            { params: Promise.resolve({ id: GAMMA }) },
          ),
        ),
    },
    {
      // MEASURED, not assumed: the PUT path has no `|| null` (unlike CREATE), so
      // the empty string DOES reach the store — but `instructions` lives in a
      // markdown FILE and the overlay's reader is
      // `markdownToNullable(content) = content === '' ? null : content`
      // (`lib/database/document-store-overlay.ts:98`), so the round trip answers
      // `null` either way. The client's `instructions || null` compensation is
      // therefore invisible on the READ path; it still decides what bytes land in
      // the file. This row pins the observable.
      name: 'update_instructions_empty_string_reads_back_null',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/groups/[id]/route')).PUT(
            mockRequest(`${B}/${GAMMA}`, { instructions: '' }),
            { params: Promise.resolve({ id: GAMMA }) },
          ),
        ),
    },
    {
      name: 'update_instructions_over_cap_400',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/groups/[id]/route')).PUT(
            mockRequest(`${B}/${GAMMA}`, { instructions: 'z'.repeat(10001) }),
            { params: Promise.resolve({ id: GAMMA }) },
          ),
        ),
    },
    {
      // `updateGroupSchema`'s `name` is `.optional()` but NOT `.nullable()`.
      name: 'update_null_name_400',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/groups/[id]/route')).PUT(
            mockRequest(`${B}/${GAMMA}`, { name: null }),
            { params: Promise.resolve({ id: GAMMA }) },
          ),
        ),
    },
    {
      name: 'update_bad_color_400',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/groups/[id]/route')).PUT(
            mockRequest(`${B}/${GAMMA}`, { color: 'rebeccapurple' }),
            { params: Promise.resolve({ id: GAMMA }) },
          ),
        ),
    },
    {
      // The non-strict `z.object` STRIPS unknown keys before the patch reaches
      // `repos.groups.update`, so `state` here must NOT be written. v5's PUT was
      // a raw passthrough until P4.D103 and would have written it.
      name: 'update_unknown_key_stripped',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/groups/[id]/route')).PUT(
            mockRequest(`${B}/${GAMMA}`, { icon: 'anchor', state: { planted: true } }),
            { params: Promise.resolve({ id: GAMMA }) },
          ),
        ),
    },
    {
      // v4 checks EXISTENCE first (`findById` → `notFound('Group')`) and parses
      // the body only after, so a missing group answers 404 even for a garbage
      // patch. Caught by the §3 unification review: the lane's first v5 port ran
      // the parse before the find and answered 400 here.
      name: 'update_missing_group_invalid_body_404',
      run: async () => {
        const missing = '00000000-0000-4000-8000-00000000dead';
        return respond(
          await (await loadRoute('@/app/api/v1/groups/[id]/route')).PUT(
            mockRequest(`${B}/${missing}`, { color: 'notacolor' }),
            { params: Promise.resolve({ id: missing }) },
          ),
        );
      },
    },
    {
      name: 'update',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/groups/[id]/route')).PUT(
            mockRequest(`${B}/${GAMMA}`, { name: 'Gamma Renamed', color: '#010203' }),
            { params: Promise.resolve({ id: GAMMA }) },
          ),
        ),
    },
    {
      name: 'delete',
      run: async () => {
        const r = await (await loadRoute('@/app/api/v1/groups/[id]/route')).DELETE(
          mockRequest(`${B}/${GAMMA}`),
          { params: Promise.resolve({ id: GAMMA }) },
        );
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpGroupTables() };
      },
    },
    {
      name: 'add_member',
      run: async () => {
        const r = await (await loadRoute('@/app/api/v1/groups/[id]/route')).POST(
          mockRequest(`${B}/${GAMMA}?action=addMember`, { characterId: CLEO }),
          { params: Promise.resolve({ id: GAMMA }) },
        );
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpGroupTables() };
      },
    },
    {
      // P4.D63 (v4 `d553f72a`): an archived character cannot join a group.
      // The tables dump proves the membership row was NOT written.
      name: 'add_member_archived',
      run: async () => {
        const r = await (await loadRoute('@/app/api/v1/groups/[id]/route')).POST(
          mockRequest(`${B}/${GAMMA}?action=addMember`, { characterId: EDDA }),
          { params: Promise.resolve({ id: GAMMA }) },
        );
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpGroupTables() };
      },
    },
    {
      name: 'remove_member',
      run: async () => {
        const r = await (await loadRoute('@/app/api/v1/groups/[id]/route')).DELETE(
          mockRequest(`${B}/${GAMMA}?action=removeMember`, { characterId: BRAM }),
          { params: Promise.resolve({ id: GAMMA }) },
        );
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpGroupTables() };
      },
    },
    {
      name: 'mount_link',
      run: async () =>
        respond(
          await (await loadRoute('@/app/api/v1/groups/[id]/mount-points/route')).POST(
            mockRequest(`${B}/${GAMMA}/mount-points`, { mountPointId: GAMMA_EXTRA_MP }),
            { params: Promise.resolve({ id: GAMMA }) },
          ),
        ),
    },
    {
      name: 'mount_unlink',
      run: async () => {
        const r = await (await loadRoute('@/app/api/v1/groups/[id]/mount-points/route')).DELETE(
          mockRequest(`${B}/${GAMMA}/mount-points`, { mountPointId: GAMMA_EXTRA_MP }),
          { params: Promise.resolve({ id: GAMMA }) },
        );
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpGroupTables() };
      },
    },
    // P4.23: the corrupted-store 503 envelope arm. Malformed bytes are planted
    // into GAMMA's official store's properties.json through the REAL
    // writeDatabaseDocument; the GET's hydrating findById then throws
    // GroupStoreUnavailableError and the middleware answers the deliberate,
    // contextful 503 (`context.ts:176-205`) — status AND body are recorded.
    {
      name: 'get_store_corrupt',
      run: async () => {
        const { rawQuery } = await import('@/lib/database/manager');
        const rows = (await rawQuery(
          `SELECT officialMountPointId AS mp FROM groups WHERE id = '${GAMMA}'`,
        )) as Array<{ mp: string | null }>;
        const mp = rows[0]?.mp;
        if (!mp) throw new Error('gamma has no officialMountPointId');
        const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
        await writeDatabaseDocument(mp, 'properties.json', '{');
        return respond(
          await (await loadRoute('@/app/api/v1/groups/[id]/route')).GET(
            mockRequest(`${B}/${GAMMA}`),
            { params: Promise.resolve({ id: GAMMA }) },
          ),
        );
      },
    },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`groups-routes oracle wrote ${outPath} (${outLines.length} cases)\n`);
  // Silence unused-import warnings for ids referenced only in some cases.
  void ARIA;
}

test('groups-routes oracle', async () => {
  await main();
});
