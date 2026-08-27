/**
 * @jest-environment node
 *
 * P4.9c USER-PROFILE ORACLE: drives v4's REAL route handlers
 * (`app/api/v1/user/profile/route.ts` GET / PUT / PATCH — imported directly,
 * never reimplemented) over a FRESH copy of the committed
 * `profile-{main,mount}.db` family per case. Emits each response
 * `{status, body}` plus the persisted `users` row, so the Rust port
 * (`api::user_profile`) diffs both the wire and the write.
 *
 * The theme-preference arms of the same multiplexed route are NOT exercised:
 * they are already ported in v5 through `theme.service` over chatSettings and
 * are explicitly out of this lane's scope.
 *
 * Case coverage (17):
 *   - GET: the primary profile; a session user with no row (v4's 500, NOT a 404).
 *   - PUT: name only; name + a new unique email; re-submitting MY OWN email
 *     (proves the uniqueness check compares IDs); the other user's email (400);
 *     an empty body (a no-op that still bumps updatedAt); `image` as a URL and
 *     as the empty string (which CLEARS the column via `|| null`).
 *   - PUT validation: `{name: null, email: null}` — what v4's OWN profile form
 *     sends for a cleared field, which `.optional()` rejects with a 400; a
 *     malformed email; a 201-character name; a non-string name.
 *   - PATCH set-avatar: an IMAGE file; an AVATAR file (v4's second accepted
 *     literal); null (clears); a DOCUMENT file (400, quoting the category); an
 *     absent file id (404).
 *
 * Run (Node 24, from the PINNED v4 worktree — the checkout is dirty; jest also
 * ignores .claude/ paths, so the case is copied to a /tmp mirror):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-profile-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/profile-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/profile-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_PROFILE_MAIN=$V5W/crates/quilltap-web/tests/fixtures/profile-main.db \
 *   QT_FIXTURE_PROFILE_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/profile-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-profile.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$TMPO/cases" -- profile-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  otherUserId: string;
  missingUserId: string;
  primaryEmail: string;
  otherEmail: string;
  imageFileId: string;
  avatarFileId: string;
  documentFileId: string;
  missingFileId: string;
}

function mockRequest(url: string, method: string, body?: unknown): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

function applyMocks(userId: string): void {
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

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

const BASE = 'http://localhost/api/v1/user/profile';

async function get(): Promise<{ status: number; body: unknown }> {
  const mod = (await import('@/app/api/v1/user/profile/route')) as {
    GET: (...a: unknown[]) => Promise<unknown>;
  };
  return respond(await mod.GET(mockRequest(BASE, 'GET')));
}

async function put(body: unknown): Promise<{ status: number; body: unknown }> {
  const mod = (await import('@/app/api/v1/user/profile/route')) as {
    PUT: (...a: unknown[]) => Promise<unknown>;
  };
  return respond(await mod.PUT(mockRequest(BASE, 'PUT', body)));
}

async function patch(body: unknown): Promise<{ status: number; body: unknown }> {
  const mod = (await import('@/app/api/v1/user/profile/route')) as {
    PATCH: (...a: unknown[]) => Promise<unknown>;
  };
  return respond(await mod.PATCH(mockRequest(`${BASE}?action=set-avatar`, 'PATCH', body)));
}

interface CaseSpec {
  name: string;
  user: (s: Spec) => string;
  run: (s: Spec) => Promise<{ status: number; body: unknown }>;
  /** The dispatch payload the Rust side replays (absent for GET-only cases). */
  payload?: (s: Spec) => Record<string, unknown>;
  verb: 'get' | 'update' | 'setAvatar';
}

function buildCases(): CaseSpec[] {
  const u = (s: Spec) => s.userId;
  return [
    // ── GET ──
    { name: 'get_primary', user: u, verb: 'get', run: () => get() },
    {
      // No `users` row for the session id — v4 answers serverError(500), which
      // is NOT what the PATCH's identical condition answers.
      name: 'get_missing_user',
      user: (s) => s.missingUserId,
      verb: 'get',
      run: () => get(),
    },

    // ── PUT: the happy paths ──
    {
      name: 'put_name_only',
      user: u,
      verb: 'update',
      payload: () => ({ name: 'Reginald Jeeves' }),
      run: () => put({ name: 'Reginald Jeeves' }),
    },
    {
      name: 'put_name_and_new_email',
      user: u,
      verb: 'update',
      payload: () => ({ name: 'Reginald Jeeves', email: 'reginald@foundry-9.com' }),
      run: () => put({ name: 'Reginald Jeeves', email: 'reginald@foundry-9.com' }),
    },
    {
      // The uniqueness check compares IDs, so my own address is not a clash.
      name: 'put_my_own_email',
      user: u,
      verb: 'update',
      payload: (s) => ({ email: s.primaryEmail }),
      run: (s) => put({ email: s.primaryEmail }),
    },
    {
      name: 'put_empty_body',
      user: u,
      verb: 'update',
      payload: () => ({}),
      run: () => put({}),
    },
    {
      name: 'put_image_url',
      user: u,
      verb: 'update',
      payload: () => ({ image: 'https://example.com/portrait.png' }),
      run: () => put({ image: 'https://example.com/portrait.png' }),
    },
    {
      // `updateData.image = validatedData.image || null` — '' CLEARS.
      name: 'put_image_empty_string',
      user: u,
      verb: 'update',
      payload: () => ({ image: '' }),
      run: () => put({ image: '' }),
    },

    // ── PUT: the rejections ──
    {
      name: 'put_email_already_in_use',
      user: u,
      verb: 'update',
      payload: (s) => ({ email: s.otherEmail }),
      run: (s) => put({ email: s.otherEmail }),
    },
    {
      // EXACTLY what v4's own ProfileEditSection sends for a cleared field.
      name: 'put_explicit_nulls',
      user: u,
      verb: 'update',
      payload: () => ({ name: null, email: null }),
      run: () => put({ name: null, email: null }),
    },
    {
      name: 'put_malformed_email',
      user: u,
      verb: 'update',
      payload: () => ({ email: 'not-an-email' }),
      run: () => put({ email: 'not-an-email' }),
    },
    {
      name: 'put_name_too_long',
      user: u,
      verb: 'update',
      payload: () => ({ name: 'x'.repeat(201) }),
      run: () => put({ name: 'x'.repeat(201) }),
    },
    {
      name: 'put_name_wrong_type',
      user: u,
      verb: 'update',
      payload: () => ({ name: 42 }),
      run: () => put({ name: 42 }),
    },

    // ── PATCH set-avatar ──
    {
      name: 'patch_avatar_image_category',
      user: u,
      verb: 'setAvatar',
      payload: (s) => ({ imageId: s.imageFileId }),
      run: (s) => patch({ imageId: s.imageFileId }),
    },
    {
      // The SECOND accepted literal — a port that only checked IMAGE fails here.
      name: 'patch_avatar_avatar_category',
      user: u,
      verb: 'setAvatar',
      payload: (s) => ({ imageId: s.avatarFileId }),
      run: (s) => patch({ imageId: s.avatarFileId }),
    },
    {
      name: 'patch_avatar_null_clears',
      user: u,
      verb: 'setAvatar',
      payload: () => ({ imageId: null }),
      run: () => patch({ imageId: null }),
    },
    {
      name: 'patch_avatar_wrong_category',
      user: u,
      verb: 'setAvatar',
      payload: (s) => ({ imageId: s.documentFileId }),
      run: (s) => patch({ imageId: s.documentFileId }),
    },
    {
      name: 'patch_avatar_missing_file',
      user: u,
      verb: 'setAvatar',
      payload: (s) => ({ imageId: s.missingFileId }),
      run: (s) => patch({ imageId: s.missingFileId }),
    },
  ];
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(c.user(spec));

  const work = mkdtempSync(join(scratch, 'profile-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();

  try {
    const out = await c.run(spec);
    // The persisted row — so the differential diffs the WRITE too, not just the
    // response body (a handler could answer correctly and store the wrong thing).
    const rows = (await rawQuery(
      'SELECT "id", "username", "email", "name", "image", "createdAt", "updatedAt" ' +
        'FROM "users" WHERE "id" = ?',
      [spec.userId],
    )) as unknown[];
    return {
      name: c.name,
      verb: c.verb,
      payload: c.payload ? c.payload(spec) : null,
      status: out.status,
      body: out.body,
      row: (rows as Array<Record<string, unknown>>)[0] ?? null,
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
    fs.readFileSync(join(here, '..', 'fixtures', 'profile-web.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_PROFILE_MAIN ?? '',
    mount: process.env.QT_FIXTURE_PROFILE_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-profile-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const outLines: string[] = [];
  for (const c of buildCases()) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`profile-routes oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('profile-routes oracle', async () => {
  await main();
});
