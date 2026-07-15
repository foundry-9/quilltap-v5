/**
 * @jest-environment node
 *
 * P4.6ar SYSTEM IMAGE-AESTHETICS route-surface ORACLE: drives v4's REAL
 * `app/api/v1/system/image-aesthetics` GET/PUT handlers over a FRESH copy of the
 * committed `inspector-*` fixture family per case, and emits each response
 * {status, body} — so the Rust port (`api::system::system_image_aesthetics_*`)
 * diffs byte-for-byte.
 *
 * ── THE TWO STORE STATES ─────────────────────────────────────────────────────
 * `instance_settings.generalMountPointId` is a SINGLETON row, so the
 * provisioned and unprovisioned arms cannot coexist in one main DB. The fixture
 * family ships two main files for exactly this: `inspector-main.db` (pointer
 * present, Quilltap General carrying `lantern-aesthetics.md` and NO
 * `aurora-aesthetics.md`) and `inspector-nostore-main.db` (the same DB with the
 * pointer row deleted). A case names which one it wants via `store`.
 *
 * Case coverage (12): the kind guard on both verbs (invalid + absent), GET over
 * an existing file, GET over an absent file → '', the PUT→GET round trip, the
 * PUT-empty → delete → GET '' arm, the MALFORMED-BODY PUT (which parses to {},
 * yields '', and therefore DELETES the file — v4's most surprising arm), the
 * non-string-content PUT (same landing), the whitespace-only PUT (writeStoreFile
 * tests `!content.trim()`, so this deletes too), and both unprovisioned-store
 * arms (GET succeeds with '', PUT refuses with a 500).
 *
 * The mutating cases re-GET afterwards and emit the resulting content as
 * `extra.after`, so the diff sees the store effect and not just the ack.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-aesthetics-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/image-aesthetics-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/inspector-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_INSP_MAIN=$V5W/crates/quilltap-web/tests/fixtures/inspector-main.db \
 *   QT_FIXTURE_INSP_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/inspector-mount.db \
 *   QT_FIXTURE_INSP_NOSTORE=$V5W/crates/quilltap-web/tests/fixtures/inspector-nostore-main.db \
 *   QT_ORACLE_OUT=/tmp/oracle-image-aesthetics.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- image-aesthetics-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  lanternAesthetic: string;
}

/** A PUT body that is deliberately un-parseable as JSON (v4's `.catch(() => ({}))`). */
const MALFORMED = Symbol('malformed');

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'PUT',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json:
      body === MALFORMED
        ? jest.fn().mockRejectedValue(new SyntaxError('Unexpected token'))
        : jest.fn().mockResolvedValue(body ?? {}),
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

async function loadRoute(
  path: string,
): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
  return (await import(path)) as never;
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  if (resp.status === 204) return { status: 204, body: null };
  return { status: resp.status, body: await resp.json() };
}

const ROUTE = '@/app/api/v1/system/image-aesthetics/route';
const B = 'http://localhost/api/v1/system/image-aesthetics';

const get = async (query: string) =>
  respond(await (await loadRoute(ROUTE)).GET(mockRequest(`${B}${query}`)));
const put = async (query: string, body: unknown) =>
  respond(await (await loadRoute(ROUTE)).PUT(mockRequest(`${B}${query}`, body)));

/** The content a follow-up GET reports (the store effect of a mutating case). */
const contentAfter = async (query: string) =>
  ((await get(query)).body as { content?: string })?.content;

interface CaseSpec {
  name: string;
  /** Which main DB this case wants: the provisioned one (default) or the pointer-less one. */
  store?: 'main' | 'nostore';
  run: (s: Spec) => Promise<{ status: number; body: unknown; extra?: unknown }>;
}

function buildCases(): CaseSpec[] {
  return [
    // ── The kind guard (parseAestheticKind: the literals, or a 400) ──
    { name: 'get_invalid_kind', run: () => get('?kind=bogus') },
    { name: 'get_absent_kind', run: () => get('') },
    { name: 'put_invalid_kind', run: () => put('?kind=bogus', { content: 'x' }) },

    // ── GET ──
    { name: 'get_lantern_existing', run: () => get('?kind=lantern') },
    {
      // No aurora file was ever seeded → readAesthetic coalesces absent to ''.
      name: 'get_aurora_absent',
      run: () => get('?kind=aurora'),
    },

    // ── PUT ──
    {
      name: 'put_aurora_then_get',
      run: async () => {
        const r = await put('?kind=aurora', { content: '# Aurora\n\nSilk, and too much of it.\n' });
        return { ...r, extra: { after: await contentAfter('?kind=aurora') } };
      },
    },
    {
      name: 'put_lantern_overwrite_then_get',
      run: async () => {
        const r = await put('?kind=lantern', { content: 'Replaced.' });
        return { ...r, extra: { after: await contentAfter('?kind=lantern') } };
      },
    },
    {
      // Empty content DELETES the file (restoring the fallback tier).
      name: 'put_lantern_empty_deletes',
      run: async () => {
        const r = await put('?kind=lantern', { content: '' });
        return { ...r, extra: { after: await contentAfter('?kind=lantern') } };
      },
    },
    {
      // writeStoreFile tests `!content.trim()` — whitespace-only deletes too.
      name: 'put_lantern_whitespace_deletes',
      run: async () => {
        const r = await put('?kind=lantern', { content: '   \n\t ' });
        return { ...r, extra: { after: await contentAfter('?kind=lantern') } };
      },
    },
    {
      // THE SURPRISING ARM: a malformed body → `.catch(() => ({}))` → safeParse
      // fails → `.data?.content ?? ''` → '' → the file is DELETED.
      name: 'put_lantern_malformed_body_deletes',
      run: async () => {
        const r = await put('?kind=lantern', MALFORMED);
        return { ...r, extra: { after: await contentAfter('?kind=lantern') } };
      },
    },
    {
      // Same landing by a different road: `content` present but not a string.
      name: 'put_lantern_non_string_content_deletes',
      run: async () => {
        const r = await put('?kind=lantern', { content: 42 });
        return { ...r, extra: { after: await contentAfter('?kind=lantern') } };
      },
    },

    // ── The unprovisioned Quilltap General store ──
    {
      // The GET SUCCEEDS with '' ("nothing to show").
      name: 'get_no_store',
      store: 'nostore',
      run: () => get('?kind=lantern'),
    },
    {
      // The PUT REFUSES — there is no store to write into.
      name: 'put_no_store',
      store: 'nostore',
      run: () => put('?kind=lantern', { content: 'x' }),
    },
  ];
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string; nostore: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec.userId);

  const work = mkdtempSync(join(scratch, 'aes-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(c.store === 'nostore' ? fixtures.nostore : fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();

  try {
    const out = await c.run(spec);
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      ...(out.extra !== undefined ? { extra: out.extra } : {}),
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
    fs.readFileSync(join(here, '..', 'fixtures', 'inspector-web.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_INSP_MAIN ?? '',
    mount: process.env.QT_FIXTURE_INSP_MOUNT ?? '',
    nostore: process.env.QT_FIXTURE_INSP_NOSTORE ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-aesthetics-oracle-'));
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
  process.stderr.write(
    `image-aesthetics-routes oracle wrote ${outPath} (${outLines.length} cases)\n`,
  );
}

test('image-aesthetics-routes oracle', async () => {
  await main();
});
