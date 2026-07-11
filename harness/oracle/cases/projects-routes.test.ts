/**
 * @jest-environment node
 *
 * P4.6k PROJECTS route-surface ORACLE: drives v4's REAL projects route handlers
 * over a FRESH copy of the committed groups-projects fixture per case, and emits
 * each response body (+ post-mutation table dumps) so the Rust ports
 * (`api::projects::*`) can be diffed byte-for-byte.
 *
 * Unit-2 coverage: list (O(n²) _count), detail (rich roster + empty), roster
 * list, list-chats (sort fallback + pagination), get-state, mount-points
 * (dangling filtered + empty); create (default injection), update, delete
 * (chats/files nulled, mount links UNTOUCHED), roster add/remove, chat
 * add/remove, set/reset-state, tool-settings, mount link/unlink.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-gp-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/projects-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/groups-projects.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_GP_MAIN=$V5W/crates/quilltap-web/tests/fixtures/groups-projects-main.db \
 *   QT_FIXTURE_GP_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/groups-projects-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-projects-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- projects-routes
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

const IOTA = 'a3000000-0000-4000-8000-000000000001';
const KAPPA = 'a3000000-0000-4000-8000-000000000003';
const ARIA = 'a1000000-0000-4000-8000-000000000001';
const BRAM = 'a1000000-0000-4000-8000-000000000002';
const GAMMA_EXTRA_MP = 'b0000000-0000-4000-8000-000000000001';
const IOTA_DANGLING_MP = 'b0000000-0000-4000-8000-0000000000df';
const CHAT_A = 'c1000000-0000-4000-8000-000000000001';

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
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
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

/** Dump project slim rows + chats/files projectId + project links (baked ids). */
async function dumpProjectTables(): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const main = getRawDatabase() as unknown as { prepare: (s: string) => { all: () => unknown } };
  const mount = getRawMountIndexDatabase() as unknown as {
    prepare: (s: string) => { all: () => unknown };
  };
  return {
    projects: main.prepare('SELECT id, name FROM projects ORDER BY id').all(),
    chats: main.prepare('SELECT id, projectId FROM chats ORDER BY id').all(),
    files: main.prepare('SELECT id, projectId FROM files ORDER BY id').all(),
    links: mount
      .prepare('SELECT projectId, mountPointId FROM project_doc_mount_links ORDER BY projectId, mountPointId')
      .all(),
  };
}

interface CaseSpec {
  name: string;
  run: () => Promise<{ status: number; body: unknown; tables?: unknown }>;
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-gp-proj-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const B = 'http://localhost/api/v1/projects';
  const idRoute = '@/app/api/v1/projects/[id]/route';
  const mpRoute = '@/app/api/v1/projects/[id]/mount-points/route';
  const p = (id: string) => ({ params: Promise.resolve({ id }) });

  const cases: CaseSpec[] = [
    // --- Reads ---
    { name: 'list', run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).GET(mockRequest(B))) },
    { name: 'get_iota', run: async () => respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}`), p(IOTA))) },
    { name: 'get_kappa', run: async () => respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${KAPPA}`), p(KAPPA))) },
    { name: 'list_characters', run: async () => respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}?action=list-characters`), p(IOTA))) },
    { name: 'list_chats', run: async () => respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}?action=list-chats`), p(IOTA))) },
    { name: 'list_chats_page', run: async () => respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}?action=list-chats&limit=1&offset=0`), p(IOTA))) },
    { name: 'get_state', run: async () => respond(await (await loadRoute(idRoute)).GET(mockRequest(`${B}/${IOTA}?action=get-state`), p(IOTA))) },
    { name: 'mount_points_iota', run: async () => respond(await (await loadRoute(mpRoute)).GET(mockRequest(`${B}/${IOTA}/mount-points`), p(IOTA))) },
    { name: 'mount_points_kappa', run: async () => respond(await (await loadRoute(mpRoute)).GET(mockRequest(`${B}/${KAPPA}/mount-points`), p(KAPPA))) },
    // --- Mutations ---
    {
      name: 'create',
      // NB icon is provided so the create echo does not exercise the null-vs-absent
      // open-JSON seam (v4's route injects `icon: x || null`; v5's ProjectProperties
      // folds null→absent — a pre-existing, documented repo-layer divergence).
      run: async () => respond(await (await loadRoute('@/app/api/v1/projects/route')).POST(
        mockRequest(B, { name: 'Mu', description: 'A new project', allowAnyCharacter: true, characterRoster: [ARIA], color: '#abcdef', icon: 'rocket' }),
      )),
    },
    { name: 'update', run: async () => respond(await (await loadRoute(idRoute)).PUT(mockRequest(`${B}/${IOTA}`, { name: 'Iota Renamed', backgroundDisplayMode: 'theme' }), p(IOTA))) },
    {
      name: 'delete',
      run: async () => {
        const r = await (await loadRoute(idRoute)).DELETE(mockRequest(`${B}/${IOTA}`), p(IOTA));
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpProjectTables() };
      },
    },
    {
      name: 'add_character',
      run: async () => {
        const r = await (await loadRoute(idRoute)).POST(mockRequest(`${B}/${KAPPA}?action=add-character`, { characterId: BRAM }), p(KAPPA));
        const { status, body } = await respond(r);
        return { status, body };
      },
    },
    {
      name: 'remove_character',
      run: async () => {
        const r = await (await loadRoute(idRoute)).DELETE(mockRequest(`${B}/${IOTA}?action=remove-character`, { characterId: ARIA }), p(IOTA));
        return respond(r);
      },
    },
    {
      name: 'add_chat',
      run: async () => {
        const r = await (await loadRoute(idRoute)).POST(mockRequest(`${B}/${KAPPA}?action=add-chat`, { chatId: CHAT_A }), p(KAPPA));
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpProjectTables() };
      },
    },
    {
      name: 'remove_chat',
      run: async () => {
        const r = await (await loadRoute(idRoute)).DELETE(mockRequest(`${B}/${IOTA}?action=remove-chat`, { chatId: CHAT_A }), p(IOTA));
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpProjectTables() };
      },
    },
    { name: 'set_state', run: async () => respond(await (await loadRoute(idRoute)).PUT(mockRequest(`${B}/${IOTA}?action=set-state`, { state: { mood: 'tense', turns: 9 } }), p(IOTA))) },
    { name: 'reset_state', run: async () => respond(await (await loadRoute(idRoute)).DELETE(mockRequest(`${B}/${IOTA}?action=reset-state`), p(IOTA))) },
    { name: 'tool_settings', run: async () => respond(await (await loadRoute(idRoute)).POST(mockRequest(`${B}/${IOTA}?action=update-tool-settings`, { defaultDisabledTools: ['web_search'], defaultDisabledToolGroups: ['danger'] }), p(IOTA))) },
    { name: 'mount_link', run: async () => respond(await (await loadRoute(mpRoute)).POST(mockRequest(`${B}/${IOTA}/mount-points`, { mountPointId: GAMMA_EXTRA_MP }), p(IOTA))) },
    {
      name: 'mount_unlink',
      run: async () => {
        const r = await (await loadRoute(mpRoute)).DELETE(mockRequest(`${B}/${IOTA}/mount-points`, { mountPointId: IOTA_DANGLING_MP }), p(IOTA));
        const { status, body } = await respond(r);
        return { status, body, tables: await dumpProjectTables() };
      },
    },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`projects-routes oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('projects-routes oracle', async () => {
  await main();
});
