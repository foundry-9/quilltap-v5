/**
 * @jest-environment node
 *
 * P4.D163 tier-2 ROUTES ORACLE for the five subprompt endpoints (v4 `2f4254b42`,
 * `app/api/v1/characters/[id]/subprompts/route.ts` GET/POST +
 * `[subpromptId]/route.ts` GET/PUT/DELETE), ported to
 * `quilltap_core::api::subprompts::*` (dispatch) + `quilltap-web`'s
 * `subprompts_routes.rs` (the REST edges).
 *
 * Drives v4's REAL route handlers through the real `createContextParamsHandler`
 * middleware (the Zod 400s are ITS `validationError` envelope — `{error:
 * 'Validation error', details}`) over a FRESH copy of the committed
 * `subprompts-{main,mount}.db` pair per case, and emits `{ name, status, body,
 * census?, recorded? }`. The recorded seams are the same two as the storage
 * family (the compiler + the realtime bus), so the routes' own
 * `publishRealtime('characters', id)` and the fan-out's `chats` publishes are
 * both comparands, and the recompile stays a recorder until P4.D164.
 *
 * THE GUARD ORDER IS MEASURED HERE, not assumed (the `a6870c5a` class): POST
 * and PUT parse the body BEFORE the character lookup (bad body + missing
 * character → 400); GET and DELETE validate the id first (bad id + missing
 * character → 400); a good everything + missing character → 404.
 *
 * Run (Node 24, from the v4 checkout — a pinned worktree while v4 HEAD is past
 * the baseline; cp to a /tmp mirror, jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-subprompts-routes-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/subprompts-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/subprompts.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SP_MAIN=$V5W/crates/quilltap-web/tests/fixtures/subprompts-main.db \
 *   QT_FIXTURE_SP_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/subprompts-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-subprompts-routes.ndjson TZ=UTC \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- subprompts-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  ids: Record<string, string>;
}
type Recorded = { compile: Array<[string, string]>; publish: Array<[string, string | null]> };

function mockRequest(url: string, method: string, body?: unknown): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body),
  };
}

function applyMocks(spec: Spec, recorded: Recorded): void {
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
  jest.doMock('@/lib/mount-index/character-vault', () =>
    jest.requireActual('@/lib/mount-index/character-vault'),
  );
  jest.doMock('@/lib/services/system-prompt-compiler/compiler', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/services/system-prompt-compiler/compiler'),
    compileIdentityStackForParticipant: async (chat: { id: string }, participantId: string) => {
      recorded.compile.push([chat.id, participantId]);
    },
  }));
  jest.doMock('@/lib/realtime/bus', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/realtime/bus'),
    publishRealtime: (topic: string, id?: string) => {
      recorded.publish.push([topic, id ?? null]);
    },
  }));
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
  });
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

const cmp = (a: string, b: string): number => (a < b ? -1 : a > b ? 1 : 0);
function ts(v: unknown, sentinel: string): unknown {
  if (typeof v === 'string' && /^\d{4}-\d{2}-\d{2}T/.test(v)) return v === sentinel ? v : '<ts>';
  return v ?? null;
}

/** A compact census: the vault links (path-keyed) + the chats participants. */
async function census(spec: Spec, vaults: Record<string, string>): Promise<Record<string, unknown>> {
  const { rawQuery } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const S = spec.seedTimestamp;
  const keyOf = new Map<string, string>(Object.entries(vaults).map(([k, v]) => [v, k]));
  const point = (id: unknown) => (typeof id === 'string' ? (keyOf.get(id) ?? '<new-point>') : null);
  const j = (v: unknown) => (typeof v === 'string' && v.length > 0 ? JSON.parse(v) : v ?? null);
  const links = (
    midb
      .prepare(
        'SELECT l.mountPointId, l.relativePath, fo.path AS folderPath, l.lastModified, d.content AS content ' +
          'FROM doc_mount_file_links l LEFT JOIN doc_mount_folders fo ON fo.id = l.folderId ' +
          'LEFT JOIN doc_mount_documents d ON d.fileId = l.fileId',
      )
      .all() as Array<Record<string, unknown>>
  )
    .map((r) => ({ point: point(r.mountPointId), relativePath: r.relativePath, folderPath: r.folderPath ?? null, lastModified: ts(r.lastModified, S), content: r.content ?? null }))
    .sort((a, b) => cmp(String(a.point), String(b.point)) || cmp(String(a.relativePath), String(b.relativePath)));
  const folders = (midb.prepare('SELECT mountPointId, path FROM doc_mount_folders').all() as Array<Record<string, unknown>>)
    .map((r) => ({ point: point(r.mountPointId), path: r.path }))
    .sort((a, b) => cmp(String(a.point), String(b.point)) || cmp(String(a.path), String(b.path)));
  const characters = ((await rawQuery('SELECT id, characterDocumentMountPointId FROM characters ORDER BY id')) as Array<Record<string, unknown>>)
    .map((r) => ({ id: r.id, vault: point(r.characterDocumentMountPointId) }));
  const chats = ((await rawQuery('SELECT id, participants FROM chats ORDER BY id')) as Array<Record<string, unknown>>)
    .map((r) => ({
      id: r.id,
      participants: (j(r.participants) as Array<Record<string, unknown>>).map((p) => ({ ...p, createdAt: ts(p.createdAt, S), updatedAt: ts(p.updatedAt, S) })),
    }));
  return { links, folders, characters, chats };
}

interface CaseSpec {
  name: string;
  method: 'GET' | 'POST' | 'PUT' | 'DELETE';
  characterId: string;
  subpromptId?: string;
  body?: unknown;
  census?: boolean;
}

const COLLECTION = '@/app/api/v1/characters/[id]/subprompts/route';
const ITEM = '@/app/api/v1/characters/[id]/subprompts/[subpromptId]/route';

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
  vaults: Record<string, string>,
): Promise<Record<string, unknown>> {
  jest.resetModules();
  const recorded: Recorded = { compile: [], publish: [] };
  applyMocks(spec, recorded);
  const work = mkdtempSync(join(scratch, 'spr-'));
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
    const B = `http://localhost/api/v1/characters/${c.characterId}/subprompts`;
    let response: { status: number; json(): Promise<unknown> };
    if (c.subpromptId === undefined) {
      const route = (await import(COLLECTION)) as Record<string, (r: unknown, ctx: unknown) => Promise<{ status: number; json(): Promise<unknown> }>>;
      const ctx = { params: Promise.resolve({ id: c.characterId }) };
      response = await route[c.method](mockRequest(B, c.method, c.body), ctx);
    } else {
      const route = (await import(ITEM)) as Record<string, (r: unknown, ctx: unknown) => Promise<{ status: number; json(): Promise<unknown> }>>;
      const ctx = { params: Promise.resolve({ id: c.characterId, subpromptId: c.subpromptId }) };
      response = await route[c.method](mockRequest(`${B}/${encodeURIComponent(c.subpromptId)}`, c.method, c.body), ctx);
    }
    const status = response.status;
    const body = await response.json();
    const out: Record<string, unknown> = { name: c.name, status, body, recorded };
    if (c.census) out.census = await census(spec, vaults);
    return out;
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) throw new Error(`must run under TZ=UTC (getTimezoneOffset=${offset})`);
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'subprompts.json'), 'utf8')) as Spec;
  const I = spec.ids;
  const fixtures = { main: process.env.QT_FIXTURE_SP_MAIN ?? '', mount: process.env.QT_FIXTURE_SP_MOUNT ?? '' };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const vaults = (JSON.parse(fs.readFileSync(fixtures.main + '.meta.json', 'utf8')) as { vaults: Record<string, string> }).vaults;
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  const scratch = mkdtempSync(join(tmpdir(), 'qt-subprompts-routes-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const A = I.charA, B = I.charB, C = I.charC, D = I.charD, X = I.missing;
  const cases: CaseSpec[] = [
    // ── GET list ───────────────────────────────────────────────────────────
    { name: 'list_a', method: 'GET', characterId: A },
    { name: 'list_b_no_folder', method: 'GET', characterId: B },
    { name: 'list_c_no_vault', method: 'GET', characterId: C },
    { name: 'list_d_archived_reads', method: 'GET', characterId: D },
    { name: 'list_missing_404', method: 'GET', characterId: X },
    // ── POST create ────────────────────────────────────────────────────────
    { name: 'create_a_201', method: 'POST', characterId: A, body: { title: 'New one', content: 'Say less.' }, census: true },
    { name: 'create_a_collision_201', method: 'POST', characterId: A, body: { title: 'Be terse', content: 'again' }, census: true },
    { name: 'create_b_ensures_folder_201', method: 'POST', characterId: B, body: { title: 'First', content: 'x' }, census: true },
    { name: 'create_c_provisions_201', method: 'POST', characterId: C, body: { title: 'Fresh', content: 'x' }, census: true },
    { name: 'create_d_archived_409', method: 'POST', characterId: D, body: { title: 'Nope', content: 'x' }, census: true },
    { name: 'create_missing_404', method: 'POST', characterId: X, body: { title: 'T', content: 'x' } },
    // Zod (the middleware envelope, `details` carried) — and the guard order:
    // the body parses BEFORE the character lookup.
    { name: 'create_zod_title_101_code_points_400', method: 'POST', characterId: A, body: { title: 'x'.repeat(101), content: 'x' } },
    { name: 'create_zod_title_empty_400', method: 'POST', characterId: A, body: { title: '', content: 'x' } },
    { name: 'create_zod_content_missing_400', method: 'POST', characterId: A, body: { title: 'T' } },
    { name: 'create_zod_title_null_400', method: 'POST', characterId: A, body: { title: null, content: 'x' } },
    { name: 'create_zod_body_null_400', method: 'POST', characterId: A, body: null },
    { name: 'create_zod_both_bad_two_issues_400', method: 'POST', characterId: A, body: { title: '', content: '' } },
    { name: 'create_zod_unknown_key_stripped_201', method: 'POST', characterId: A, body: { title: 'Stripped', content: 'x', bogus: 1 }, census: true },
    { name: 'create_bad_body_missing_character_400_not_404', method: 'POST', characterId: X, body: { title: '', content: 'x' } },
    // The SERVICE validators, AFTER Zod: 100 astral code points pass Zod and
    // fail v4's UTF-16 `.length` rule; a whitespace-only title passes `min(1)`
    // and fails the service's trim.
    { name: 'create_service_title_100_astral_400', method: 'POST', characterId: A, body: { title: '😀'.repeat(100), content: 'x' } },
    { name: 'create_service_title_blank_400', method: 'POST', characterId: A, body: { title: '   ', content: 'x' } },
    { name: 'create_service_content_blank_400', method: 'POST', characterId: A, body: { title: 'T', content: ' \n ' } },
    { name: 'create_service_title_blank_missing_character_404', method: 'POST', characterId: X, body: { title: '   ', content: 'x' } },
    // ── GET one ────────────────────────────────────────────────────────────
    { name: 'get_a_terse', method: 'GET', characterId: A, subpromptId: 'terse' },
    { name: 'get_a_verse_lower_case', method: 'GET', characterId: A, subpromptId: 'verse' },
    { name: 'get_a_gone_404', method: 'GET', characterId: A, subpromptId: 'gone' },
    { name: 'get_a_bad_id_400', method: 'GET', characterId: A, subpromptId: 'a/b' },
    { name: 'get_a_bad_id_dotdot_400', method: 'GET', characterId: A, subpromptId: '..' },
    { name: 'get_bad_id_missing_character_400_not_404', method: 'GET', characterId: X, subpromptId: 'a/b' },
    { name: 'get_good_id_missing_character_404', method: 'GET', characterId: X, subpromptId: 'terse' },
    { name: 'get_d_keep_archived_reads', method: 'GET', characterId: D, subpromptId: 'keep' },
    { name: 'get_c_no_vault_404', method: 'GET', characterId: C, subpromptId: 'terse' },
    // ── PUT update ─────────────────────────────────────────────────────────
    { name: 'update_a_title_only', method: 'PUT', characterId: A, subpromptId: 'terse', body: { title: 'Be brief' }, census: true },
    { name: 'update_a_content_only', method: 'PUT', characterId: A, subpromptId: 'terse', body: { content: 'Two lines.' }, census: true },
    { name: 'update_a_both', method: 'PUT', characterId: A, subpromptId: 'terse', body: { title: 'Brief', content: 'Short.' }, census: true },
    { name: 'update_a_empty_body_noop_rewrite', method: 'PUT', characterId: A, subpromptId: 'terse', body: {}, census: true },
    { name: 'update_a_gone_404', method: 'PUT', characterId: A, subpromptId: 'gone', body: { title: 'x' } },
    { name: 'update_a_bad_id_400', method: 'PUT', characterId: A, subpromptId: 'a/b', body: { title: 'x' } },
    { name: 'update_d_archived_409', method: 'PUT', characterId: D, subpromptId: 'keep', body: { title: 'x' }, census: true },
    { name: 'update_missing_404', method: 'PUT', characterId: X, subpromptId: 'terse', body: { title: 'x' } },
    { name: 'update_zod_title_null_400', method: 'PUT', characterId: A, subpromptId: 'terse', body: { title: null } },
    { name: 'update_zod_title_101_400', method: 'PUT', characterId: A, subpromptId: 'terse', body: { title: 'x'.repeat(101) } },
    { name: 'update_zod_content_empty_400', method: 'PUT', characterId: A, subpromptId: 'terse', body: { content: '' } },
    { name: 'update_zod_body_null_400', method: 'PUT', characterId: A, subpromptId: 'terse', body: null },
    { name: 'update_bad_body_bad_id_400_zod_first', method: 'PUT', characterId: A, subpromptId: 'a/b', body: { title: null } },
    { name: 'update_bad_body_missing_character_400_not_404', method: 'PUT', characterId: X, subpromptId: 'terse', body: { title: null } },
    { name: 'update_good_body_bad_id_missing_character_400', method: 'PUT', characterId: X, subpromptId: 'a/b', body: { title: 'x' } },
    { name: 'update_service_title_100_astral_400', method: 'PUT', characterId: A, subpromptId: 'terse', body: { title: '😀'.repeat(100) } },
    { name: 'update_service_title_blank_400', method: 'PUT', characterId: A, subpromptId: 'terse', body: { title: '  ' } },
    { name: 'update_a_verse_lower_case_writes_lower_path', method: 'PUT', characterId: A, subpromptId: 'verse', body: { title: 'In verse' }, census: true },
    // ── DELETE ─────────────────────────────────────────────────────────────
    { name: 'delete_a_terse_fans_out', method: 'DELETE', characterId: A, subpromptId: 'terse', census: true },
    { name: 'delete_a_VERSE_case_insensitive_strip', method: 'DELETE', characterId: A, subpromptId: 'Verse', census: true },
    { name: 'delete_a_gone_404_no_fanout', method: 'DELETE', characterId: A, subpromptId: 'gone', census: true },
    { name: 'delete_a_bad_id_400', method: 'DELETE', characterId: A, subpromptId: 'a/b' },
    { name: 'delete_bad_id_missing_character_400_not_404', method: 'DELETE', characterId: X, subpromptId: 'a/b' },
    { name: 'delete_good_id_missing_character_404', method: 'DELETE', characterId: X, subpromptId: 'terse' },
    { name: 'delete_d_archived_409', method: 'DELETE', characterId: D, subpromptId: 'keep', census: true },
    { name: 'delete_c_no_vault_provisions_then_404', method: 'DELETE', characterId: C, subpromptId: 'terse', census: true },
  ];

  const norm = (v: unknown): unknown => {
    if (Array.isArray(v)) return v.map(norm);
    if (v && typeof v === 'object') {
      const o = v as Record<string, unknown>;
      const out: Record<string, unknown> = {};
      for (const [k, val] of Object.entries(o)) out[k] = k === 'updatedAt' ? ts(val, spec.seedTimestamp) : norm(val);
      return out;
    }
    return v;
  };
  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, c, scratch, fixtures, vaults);
    payload.body = norm(payload.body);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`subprompts-routes oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('subprompts-routes tier-2 oracle', async () => {
  await main();
});
