/**
 * @jest-environment node
 *
 * P4.9H2A EMBEDDING-PROFILES route-surface ORACLE: drives v4's REAL
 * embedding-profile route handlers over a FRESH copy of the committed
 * `embedding-profiles-{main,mount}.db` fixture per case, emitting each response
 * `{status, body(, tables)}` so `api::embedding_profiles::*` can be diffed
 * byte-for-byte. The matrix cases dump `background_jobs` types + `embedding_status`
 * counts (the matrix claim is state, not prose). No plugin registration needed —
 * the embedding CRUD does NO provider probe (unlike image-profiles), and
 * list-providers/list-models are covered by Rust unit tests (v4 returns [] in the
 * sandbox where no plugins are registered).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-ep-routes-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp $V5W/harness/oracle/cases/embedding-profiles-routes.test.ts "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   QT_EP_MGMT_MAIN=$V5W/crates/quilltap-web/tests/fixtures/embedding-profiles-main.db \
 *   QT_EP_MGMT_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/embedding-profiles-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-embedding-profiles-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- embedding-profiles-routes
 */

import * as fs from 'fs';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

const PEPPER = 'ZpjI5jcj5CYsyBA6zPH90G4frQEbv2WsAhERvEKrjJk=';
const USER = 'aa000000-0000-4000-8000-0000000000aa';
const APIKEY = 'ca000000-0000-4000-8000-0000000000c1';
const EP_DEFAULT = 'e0000000-0000-4000-8000-000000000001';
const EP_BUILTIN = 'e0000000-0000-4000-8000-000000000002';
const EP_TRUNC = 'e0000000-0000-4000-8000-000000000003';
const EP_TRUNCFREE = 'e0000000-0000-4000-8000-000000000004';
const BOGUS = 'e0000000-0000-4000-8000-0000000000ff';

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'GET',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

// A request whose json() REJECTS — v4's reindex legacy no-body parse guard.
function noBodyRequest(url: string): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockRejectedValue(new Error('Unexpected end of JSON input')),
  };
}

function applyMocks(): void {
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
  // The PUT matrix + reindex call `invalidateAllEmbeddings` — jest returns an
  // INCOMPLETE embedding-service module (the export reads `undefined`) unless it
  // is requireActual'd. (image-profiles never exercised this path.)
  jest.doMock('@/lib/embedding/embedding-service', () =>
    jest.requireActual('@/lib/embedding/embedding-service'),
  );
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: USER } }),
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

async function dumpState(): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const main = getRawDatabase() as unknown as {
    prepare: (s: string) => { all: () => unknown[] };
  };
  const jobs = (main.prepare('SELECT type FROM background_jobs ORDER BY type').all() as Array<{
    type: string;
  }>).map((j) => j.type);
  const status = main
    .prepare('SELECT status, COUNT(*) as n FROM embedding_status GROUP BY status ORDER BY status')
    .all();
  return { jobs, status };
}

async function loadRoute(path: string): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
  return (await import(path)) as never;
}
async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

interface CaseSpec {
  name: string;
  run: () => Promise<{ status: number; body: unknown; tables?: unknown }>;
}

async function runCase(
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks();

  const work = mkdtempSync(require('node:path').join(scratch, 'ep-'));
  const mainWork = require('node:path').join(work, 'main.db');
  const mountWork = require('node:path').join(work, 'mount.db');
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

const B = 'http://localhost/api/v1/embedding-profiles';
const coll = () => loadRoute('@/app/api/v1/embedding-profiles/route');
const idRoute = () => loadRoute('@/app/api/v1/embedding-profiles/[id]/route');
const params = (id: string) => ({ params: Promise.resolve({ id }) });

/** POST an ?action= route, dumping the post-op state tables. */
async function postAction(
  id: string,
  action: string,
  body: unknown,
): Promise<{ status: number; body: unknown; tables: unknown }> {
  const r = await (await idRoute()).POST(mockRequest(`${B}/${id}?action=${action}`, body), params(id));
  const res = await respond(r);
  return { ...res, tables: await dumpState() };
}

/** PUT the [id] route, dumping the post-op state tables (the matrix claim). */
async function putProfile(
  id: string,
  body: unknown,
): Promise<{ status: number; body: unknown; tables: unknown }> {
  const r = await (await idRoute()).PUT(mockRequest(`${B}/${id}`, body), params(id));
  const res = await respond(r);
  return { ...res, tables: await dumpState() };
}

async function main(): Promise<void> {
  const fixtures = {
    main: process.env.QT_EP_MGMT_MAIN ?? '',
    mount: process.env.QT_EP_MGMT_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(require('node:path').join(tmpdir(), 'qt-ep-routes-oracle-'));
  mkdirSync(require('node:path').join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cases: CaseSpec[] = [
    // ── reads ──────────────────────────────────────────────────────────────
    { name: 'list', run: async () => respond(await (await coll()).GET(mockRequest(B))) },
    {
      name: 'get',
      run: async () => respond(await (await idRoute()).GET(mockRequest(`${B}/${EP_DEFAULT}`), params(EP_DEFAULT))),
    },
    {
      name: 'get_builtin',
      run: async () => respond(await (await idRoute()).GET(mockRequest(`${B}/${EP_BUILTIN}`), params(EP_BUILTIN))),
    },
    {
      name: 'get_404',
      run: async () => respond(await (await idRoute()).GET(mockRequest(`${B}/${BOGUS}`), params(BOGUS))),
    },
    // ── create ─────────────────────────────────────────────────────────────
    {
      name: 'create_happy',
      run: async () =>
        respond(
          await (await coll()).POST(
            mockRequest(B, {
              name: 'New External',
              provider: 'OPENAI',
              apiKeyId: APIKEY,
              modelName: 'text-embedding-3-small',
              dimensions: 1536,
            }),
          ),
        ),
    },
    {
      name: 'create_default_triggers_reindex',
      run: async () => {
        const r = await (await coll()).POST(
          mockRequest(B, {
            name: 'New Default',
            provider: 'OPENAI',
            modelName: 'text-embedding-3-small',
            isDefault: true,
          }),
        );
        return { ...(await respond(r)), tables: await dumpState() };
      },
    },
    {
      name: 'create_dup_409',
      run: async () =>
        respond(
          await (await coll()).POST(
            mockRequest(B, { name: 'OpenAI Default', provider: 'OPENAI', modelName: 'x' }),
          ),
        ),
    },
    {
      name: 'create_missing_name_400',
      run: async () =>
        respond(await (await coll()).POST(mockRequest(B, { provider: 'OPENAI', modelName: 'x' }))),
    },
    {
      name: 'create_bad_dimensions_400',
      run: async () =>
        respond(
          await (await coll()).POST(
            mockRequest(B, { name: 'Z', provider: 'OPENAI', modelName: 'x', dimensions: -5 }),
          ),
        ),
    },
    {
      name: 'create_apikey_404',
      run: async () =>
        respond(
          await (await coll()).POST(
            mockRequest(B, { name: 'Z', provider: 'OPENAI', modelName: 'x', apiKeyId: BOGUS }),
          ),
        ),
    },
    // ── update: the trigger matrix ───────────────────────────────────────────
    // Branch A: an already-default profile's model changed -> full reindex.
    { name: 'update_default_model_full_reindex', run: () => putProfile(EP_DEFAULT, { modelName: 'text-embedding-3-large' }) },
    // Branch A via becameDefault, BUILTIN -> refit (which triggers reindex).
    { name: 'update_builtin_became_default_refit', run: () => putProfile(EP_BUILTIN, { isDefault: true }) },
    // Branch A via becameDefault, external -> reindex-all (truncate change moot).
    { name: 'update_became_default_full_reindex', run: () => putProfile(EP_TRUNC, { isDefault: true, truncateToDimensions: 256 }) },
    // Branch B narrow: already-default, truncate null(->eff dims 1536)->512 <= 1536.
    { name: 'update_default_narrow_reapply', run: () => putProfile(EP_DEFAULT, { truncateToDimensions: 512 }) },
    // Branch B widen: already-default, truncate null(->eff dims 1536)->3000 > 1536.
    { name: 'update_default_widen_reindex', run: () => putProfile(EP_DEFAULT, { truncateToDimensions: 3000 }) },
    // Non-default profile edits -> nothing fires (reembeddingTriggered false).
    { name: 'update_nondefault_model_no_job', run: () => putProfile(EP_TRUNC, { modelName: 'text-embedding-3-small' }) },
    { name: 'update_default_normalizeL2_only_no_job', run: () => putProfile(EP_DEFAULT, { normalizeL2: false }) },
    { name: 'update_clear_apikey', run: () => putProfile(EP_DEFAULT, { apiKeyId: null }) },
    // Explicit-null clears on the NUMERIC nullables: Zod keeps the cleared keys
    // present-as-null in the PUT echo (the §3 unify review's blind spot — v5
    // dropped the keys). EP_TRUNC is non-default, so no matrix branch fires.
    { name: 'update_clear_truncate_dims_null', run: () => putProfile(EP_TRUNC, { truncateToDimensions: null, dimensions: null }) },
    // P4.55 (the merge-verb silent-keep sweep), the missing-`else` sub-family:
    // v5 read `apiKeyId` as `if null … else if as_str …` with NO else, so a
    // present non-string was silently dropped and the PUT answered 200. v4 has
    // no Zod schema here either — it falls into `findApiKeyById(apiKeyId)`,
    // which answers null for any non-string. These arms MEASURE that; the
    // `tables` dump proves nothing landed either way.
    { name: 'update_apikey_non_string', run: () => putProfile(EP_DEFAULT, { apiKeyId: 5 }) },
    { name: 'update_apikey_object', run: () => putProfile(EP_DEFAULT, { apiKeyId: {} }) },
    // The sibling read: `baseUrl || null`. A TRUTHY non-string is assigned
    // verbatim by v4; v5's `as_str()` filter collapsed it to null, silently
    // CLEARING the column.
    { name: 'update_baseurl_non_string', run: () => putProfile(EP_DEFAULT, { baseUrl: 5 }) },
    {
      name: 'update_dup_409',
      run: async () => respond(await (await idRoute()).PUT(mockRequest(`${B}/${EP_TRUNC}`, { name: 'OpenAI Default' }), params(EP_TRUNC))),
    },
    {
      name: 'update_404',
      run: async () => respond(await (await idRoute()).PUT(mockRequest(`${B}/${BOGUS}`, { name: 'X' }), params(BOGUS))),
    },
    // ── delete ───────────────────────────────────────────────────────────────
    {
      name: 'delete',
      run: async () => respond(await (await idRoute()).DELETE(mockRequest(`${B}/${EP_TRUNCFREE}`), params(EP_TRUNCFREE))),
    },
    {
      name: 'delete_404',
      run: async () => respond(await (await idRoute()).DELETE(mockRequest(`${B}/${BOGUS}`), params(BOGUS))),
    },
    // ── refit ────────────────────────────────────────────────────────────────
    { name: 'refit_builtin', run: () => postAction(EP_BUILTIN, 'refit', {}) },
    {
      name: 'refit_non_builtin_400',
      run: async () => respond(await (await idRoute()).POST(mockRequest(`${B}/${EP_DEFAULT}?action=refit`, {}), params(EP_DEFAULT))),
    },
    {
      name: 'refit_404',
      run: async () => respond(await (await idRoute()).POST(mockRequest(`${B}/${BOGUS}?action=refit`, {}), params(BOGUS))),
    },
    // ── reindex ──────────────────────────────────────────────────────────────
    { name: 'reindex_all', run: () => postAction(EP_DEFAULT, 'reindex', { scope: 'all' }) },
    {
      name: 'reindex_legacy_no_body',
      run: async () => {
        const r = await (await idRoute()).POST(noBodyRequest(`${B}/${EP_DEFAULT}?action=reindex`), params(EP_DEFAULT));
        return { ...(await respond(r)), tables: await dumpState() };
      },
    },
    { name: 'reindex_mismatched', run: () => postAction(EP_TRUNC, 'reindex', { scope: 'mismatched-dim' }) },
    {
      name: 'reindex_mismatched_no_target_400',
      run: async () =>
        respond(await (await idRoute()).POST(mockRequest(`${B}/${EP_BUILTIN}?action=reindex`, { scope: 'mismatched-dim' }), params(EP_BUILTIN))),
    },
    {
      name: 'reindex_bad_scope_400',
      run: async () =>
        respond(await (await idRoute()).POST(mockRequest(`${B}/${EP_DEFAULT}?action=reindex`, { scope: 'nonsense' }), params(EP_DEFAULT))),
    },
    // ── reapply ──────────────────────────────────────────────────────────────
    { name: 'reapply_has_trunc', run: () => postAction(EP_TRUNC, 'reapply', {}) },
    {
      name: 'reapply_no_trunc_400',
      run: async () => respond(await (await idRoute()).POST(mockRequest(`${B}/${EP_TRUNCFREE}?action=reapply`, {}), params(EP_TRUNCFREE))),
    },
    {
      name: 'reapply_404',
      run: async () => respond(await (await idRoute()).POST(mockRequest(`${B}/${BOGUS}?action=reapply`, {}), params(BOGUS))),
    },
  ];

  const lines: string[] = [];
  for (const c of cases) {
    lines.push(JSON.stringify(await runCase(c, scratch, fixtures)));
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  rmSync(scratch, { recursive: true, force: true });
}

it('embedding-profiles routes oracle', async () => {
  await main();
}, 120000);
