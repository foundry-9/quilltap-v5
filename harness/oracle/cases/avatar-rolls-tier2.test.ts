/**
 * @jest-environment node
 *
 * P4.D185 AVATAR-ROLLS ORACLE: drives v4's REAL `lib/photos/avatar-rolls-
 * service.ts` (`4dcbe0d21`) over a fresh copy of the committed
 * `avatar-rolls-{main,mount}.db` pair, and emits each call's result PLUS a
 * whole-table census after it, so the Rust port
 * (`photos::avatar_rolls_service`) can be diffed on both what it answers and
 * what it wrote.
 *
 * Every case gets its own copy of the fixture, so the mutating save / set-avatar
 * / delete cases cannot contaminate their neighbours.
 *
 * ## The one un-mocked seam, and why
 *
 * `jest.setup` stubs `fileStorageManager.downloadFile` to
 * `Buffer.from('mock file content')` for the whole suite. This case un-mocks it
 * to the REAL module, because every roll in the fixture carries a `mount-blob:`
 * storage key and v4's real `downloadFile` serves those straight out of the
 * mount index (`manager.ts:386`) with no backend and no config. Keeping the stub
 * would have made every album save hash the same seventeen bytes — the saved
 * blob would then have a sha that no roll row names, and `classifyRollLinks`
 * could never see its own album copy afterwards. The Rust side reads the same
 * blob through its `FileBytesStore` seam, which is what the host does in
 * production.
 *
 * `Date` is frozen to the spec's `keptAt` for the same reason the photo-upload
 * oracle freezes it: `saveToCharacterGallery` mints `new Date().toISOString()`
 * and the Rust twin takes that stamp as a parameter.
 *
 * Run (Node 24, from a v4 checkout pinned at the lane's target — cp to a /tmp
 * mirror; jest ignores .claude/ paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-avatar-rolls-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/avatar-rolls-tier2.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/avatar-rolls.json"       "$TMPO/fixtures/"
 *   cd /tmp/qt-v4-pin-p4d185-31436bae4
 *   TZ=UTC \
 *   QT_FIXTURE_AR_MAIN=$V5W/crates/quilltap-web/tests/fixtures/avatar-rolls-main.db \
 *   QT_FIXTURE_AR_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/avatar-rolls-mount.db \
 *   QT_FIXTURE_AR_META=$V5W/crates/quilltap-web/tests/fixtures/avatar-rolls-main.db.meta.json \
 *   QT_ORACLE_OUT=/tmp/oracle-avatar-rolls.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "cases/avatar-rolls-tier2\.test\.ts$"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  keptAt: string;
}

interface Meta {
  rolfVault: string;
  sageVault: string;
  projectMp: string;
  rollNewLinkId: string;
  rollKeptLinkId: string;
  rollKeptAlbumLinkId: string;
  rollAlbumLinkId: string;
  rollLegacyLinkId: string;
}

const ROLF = 'a1000000-0000-4000-8000-000000000001';
const SAGE = 'a1000000-0000-4000-8000-000000000002';
const TALL = 'a1000000-0000-4000-8000-000000000003';
const NOBODY = 'a9000000-0000-4000-8000-0000000000ff';

const F_ROLL_NEW = 'f1000000-0000-4000-8000-000000000001';
const F_ROLL_KEPT = 'f1000000-0000-4000-8000-000000000002';
const F_ROLL_ALBUM = 'f1000000-0000-4000-8000-000000000003';
const F_ROLL_NOLINK = 'f1000000-0000-4000-8000-000000000004';
const F_ROLL_SAGE = 'f1000000-0000-4000-8000-000000000006';
const F_NOT_ROLL = 'f1000000-0000-4000-8000-000000000007';
const F_ROLL_TALL = 'f1000000-0000-4000-8000-000000000008';
const F_MISSING = '99999999-9999-4999-8999-999999999999';

type Op =
  | { kind: 'list'; characterId: string; limit?: number; offset?: number }
  | { kind: 'save'; characterId: string; fileId: string }
  | { kind: 'setAvatar'; characterId: string; fileId: string }
  | { kind: 'delete'; characterId: string; fileId: string }
  /** The REST edges, through v4's REAL route handlers — the Zod query gate, the
   *  `withActionDispatch` envelope and the whole error ladder, none of which the
   *  service-level cases can see. */
  | { kind: 'route'; method: 'GET' | 'POST' | 'DELETE'; url: string; params: Record<string, string> };

interface CaseSpec {
  name: string;
  op: Op;
  /** Census the write tables afterwards (every mutating case). */
  census?: boolean;
}

/**
 * The whole-table census a write case is judged on. Mount-point stat columns are
 * excluded deliberately: `refreshStats` recomputes them from the surviving rows,
 * which the link/blob tables already report, and v4 fires it best-effort.
 */
async function census(): Promise<Record<string, unknown>> {
  const { rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  // The mount-index client is created lazily on the first mount read, and the
  // refusal cases (`save_not_a_roll`, `save_missing_file`) throw before any —
  // so the raw handle is null unless the census opens it itself.
  await getRepositories().docMountPoints.findById('00000000-0000-4000-8000-000000000000');
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const j = (v: unknown): unknown =>
    typeof v === 'string' && v.length > 0 ? JSON.parse(v) : (v ?? null);

  const files = (await rawQuery(
    'SELECT "id", "originalFilename", "generationKey" IS NOT NULL AS "keyed", "tags" FROM "files" ORDER BY "id"',
  )) as Array<Record<string, unknown>>;
  const characters = (await rawQuery(
    'SELECT "id", "defaultImageId", "avatarOverrides" FROM "characters" ORDER BY "id"',
  )) as Array<Record<string, unknown>>;
  const chats = (await rawQuery(
    'SELECT "id", "characterAvatars" FROM "chats" ORDER BY "id"',
  )) as Array<Record<string, unknown>>;
  const links = midb
    .prepare(
      'SELECT "id", "mountPointId", "relativePath", "fileId" FROM "doc_mount_file_links" ORDER BY "mountPointId", "relativePath"',
    )
    .all() as Array<Record<string, unknown>>;
  const blobs = midb
    .prepare('SELECT "fileId" FROM "doc_mount_blobs" ORDER BY "fileId"')
    .all() as Array<Record<string, unknown>>;
  const mountFiles = midb
    .prepare('SELECT "id", "sha256" FROM "doc_mount_files" ORDER BY "id"')
    .all() as Array<Record<string, unknown>>;

  return {
    files: files.map((f) => ({ ...f, keyed: Number(f.keyed), tags: j(f.tags) })),
    characters: characters.map((c) => ({ ...c, avatarOverrides: j(c.avatarOverrides) })),
    chats: chats.map((c) => ({ ...c, characterAvatars: j(c.characterAvatars) })),
    links,
    blobs,
    mountFiles,
  };
}

function mockRequest(method: string, url: string): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue({}),
  };
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();

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
  // The vault bridge is globally mocked to a fake mount; un-mock it or every
  // vault lookup answers an empty stub and the album arms vanish.
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
  );
  // See the module doc: the real `downloadFile` serves `mount-blob:` keys out of
  // the mount index, so the album save hashes the roll's REAL bytes.
  jest.doMock('@/lib/file-storage/manager', () => jest.requireActual('@/lib/file-storage/manager'));
  // The REST cases run through `createContextParamsHandler`, which gates on the
  // session and the startup state.
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

  const work = mkdtempSync(join(scratch, 'ar-'));
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
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();

  const RealDate = Date;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  global.Date = class extends RealDate {
    constructor(...a: unknown[]) {
      if (a.length === 0) super(spec.keptAt);
      // @ts-expect-error forward variadic args
      else super(...a);
    }
    static now(): number {
      return RealDate.parse(spec.keptAt);
    }
  } as unknown as DateConstructor;

  try {
    const svc = await import('@/lib/photos/avatar-rolls-service');
    const repos = getRepositories();
    let result: unknown = null;
    let error: string | null = null;
    try {
      if (c.op.kind === 'list') {
        result = await svc.listAvatarRolls({
          characterId: c.op.characterId,
          limit: c.op.limit,
          offset: c.op.offset,
          repos,
        });
      } else if (c.op.kind === 'save') {
        result = await svc.saveAvatarRollToAlbum({
          characterId: c.op.characterId,
          fileId: c.op.fileId,
          repos,
        });
      } else if (c.op.kind === 'setAvatar') {
        result = await svc.setAvatarRollAsPortrait({
          characterId: c.op.characterId,
          fileId: c.op.fileId,
          repos,
        });
      } else if (c.op.kind === 'delete') {
        result = await svc.deleteAvatarRoll({
          characterId: c.op.characterId,
          fileId: c.op.fileId,
          repos,
        });
      } else {
        const mod = (await import(
          c.op.params.fileId
            ? '@/app/api/v1/characters/[id]/avatar-rolls/[fileId]/route'
            : '@/app/api/v1/characters/[id]/avatar-rolls/route'
        )) as Record<string, (...a: unknown[]) => Promise<unknown>>;
        const handler = mod[c.op.method];
        const response = (await handler(mockRequest(c.op.method, c.op.url), {
          params: Promise.resolve(c.op.params),
        })) as { status: number; json: () => Promise<unknown> };
        result = { status: response.status, body: await response.json() };
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
    const out: Record<string, unknown> = { name: c.name, result, error };
    if (c.census) out.state = await census();
    return out;
  } finally {
    global.Date = RealDate;
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'avatar-rolls.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_AR_MAIN ?? '',
    mount: process.env.QT_FIXTURE_AR_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const metaPath = process.env.QT_FIXTURE_AR_META;
  if (!metaPath || !existsSync(metaPath)) throw new Error(`meta sidecar missing: ${metaPath}`);
  JSON.parse(fs.readFileSync(metaPath, 'utf8')) as Meta;

  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-avatar-rolls-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const B = 'http://localhost/api/v1/characters';
  const rGet = (id: string, q: string): Op => ({
    kind: 'route',
    method: 'GET',
    url: `${B}/${id}/avatar-rolls${q}`,
    params: { id },
  });
  const rPost = (id: string, fileId: string, q: string): Op => ({
    kind: 'route',
    method: 'POST',
    url: `${B}/${id}/avatar-rolls/${fileId}${q}`,
    params: { id, fileId },
  });
  const rDelete = (id: string, fileId: string): Op => ({
    kind: 'route',
    method: 'DELETE',
    url: `${B}/${id}/avatar-rolls/${fileId}`,
    params: { id, fileId },
  });

  const cases: CaseSpec[] = [
    // ── listAvatarRolls ──────────────────────────────────────────────────────
    // The whole page: five rolls newest first, SAGE's excluded by the tag, the
    // unkeyed image excluded by the key, both link kinds, the legacy-URL
    // fallback, the portrait through the ALBUM link, and a usage COUNT of 2.
    { name: 'list_page', op: { kind: 'list', characterId: ROLF } },
    { name: 'list_limit_offset', op: { kind: 'list', characterId: ROLF, limit: 2, offset: 1 } },
    // `Math.max(1, Math.min(limit ?? 60, 200))` / `Math.max(0, offset ?? 0)`.
    { name: 'list_limit_clamped_low', op: { kind: 'list', characterId: ROLF, limit: 0, offset: -5 } },
    { name: 'list_limit_clamped_high', op: { kind: 'list', characterId: ROLF, limit: 5000 } },
    { name: 'list_offset_past_end', op: { kind: 'list', characterId: ROLF, offset: 99 } },
    // SAGE's portrait is a LEGACY `files.id` — the other half of the tolerance.
    { name: 'list_sage', op: { kind: 'list', characterId: SAGE } },
    // A vault-less character: `albumLink` can never resolve.
    { name: 'list_no_vault', op: { kind: 'list', characterId: TALL } },
    { name: 'list_missing_character', op: { kind: 'list', characterId: NOBODY } },
    // ── saveAvatarRollToAlbum ────────────────────────────────────────────────
    { name: 'save_new', op: { kind: 'save', characterId: ROLF, fileId: F_ROLL_NEW }, census: true },
    {
      name: 'save_idempotent',
      op: { kind: 'save', characterId: ROLF, fileId: F_ROLL_KEPT },
      census: true,
    },
    {
      name: 'save_not_a_roll',
      op: { kind: 'save', characterId: ROLF, fileId: F_NOT_ROLL },
      census: true,
    },
    {
      name: 'save_other_characters_roll',
      op: { kind: 'save', characterId: ROLF, fileId: F_ROLL_SAGE },
      census: true,
    },
    { name: 'save_missing_file', op: { kind: 'save', characterId: ROLF, fileId: F_MISSING } },
    // The vault rung of the ladder, reachable only through TALL's own roll.
    {
      name: 'save_no_vault',
      op: { kind: 'save', characterId: TALL, fileId: F_ROLL_TALL },
      census: true,
    },
    // ── setAvatarRollAsPortrait ──────────────────────────────────────────────
    {
      name: 'set_avatar_saves_first',
      op: { kind: 'setAvatar', characterId: ROLF, fileId: F_ROLL_NEW },
      census: true,
    },
    {
      name: 'set_avatar_already_kept',
      op: { kind: 'setAvatar', characterId: ROLF, fileId: F_ROLL_KEPT },
      census: true,
    },
    // ── deleteAvatarRoll ─────────────────────────────────────────────────────
    // Two chats wearing it, an `avatarOverrides` entry naming it, one link.
    {
      name: 'delete_scrubs_everything',
      op: { kind: 'delete', characterId: ROLF, fileId: F_ROLL_NEW },
      census: true,
    },
    // Two links over one blob: the album copy and its bytes survive.
    {
      name: 'delete_keeps_the_album_copy',
      op: { kind: 'delete', characterId: ROLF, fileId: F_ROLL_KEPT },
      census: true,
    },
    // The album copy is the only link: nothing is dropped but the cache row.
    {
      name: 'delete_album_only_roll',
      op: { kind: 'delete', characterId: ROLF, fileId: F_ROLL_ALBUM },
      census: true,
    },
    // No mount link at all: the `files` row goes and nothing else.
    {
      name: 'delete_unlinked_roll',
      op: { kind: 'delete', characterId: ROLF, fileId: F_ROLL_NOLINK },
      census: true,
    },
    // SAGE's `defaultImageId` is the LEGACY `files.id` shape — the arm a delete
    // clears (ROLF's album-link pointer is the arm it must NOT clear).
    {
      name: 'delete_clears_legacy_portrait',
      op: { kind: 'delete', characterId: SAGE, fileId: F_ROLL_SAGE },
      census: true,
    },
    {
      name: 'delete_miss_is_not_a_throw',
      op: { kind: 'delete', characterId: ROLF, fileId: F_NOT_ROLL },
      census: true,
    },
    {
      name: 'delete_other_characters_roll',
      op: { kind: 'delete', characterId: ROLF, fileId: F_ROLL_SAGE },
      census: true,
    },
    // ── the REST edges (v4's REAL route handlers) ───────────────────────────
    // The Zod query gate. v4 reads `has('limit') ? Number(get('limit')) :
    // undefined`, so `?limit=` is `Number('') === 0` and `?limit=abc` is NaN —
    // the messages are measured here, never transcribed.
    { name: 'route_list_ok', op: rGet(ROLF, '') },
    { name: 'route_list_paginated', op: rGet(ROLF, '?limit=2&offset=1') },
    { name: 'route_list_limit_zero', op: rGet(ROLF, '?limit=0') },
    { name: 'route_list_limit_empty', op: rGet(ROLF, '?limit=') },
    { name: 'route_list_limit_nan', op: rGet(ROLF, '?limit=abc') },
    { name: 'route_list_limit_float', op: rGet(ROLF, '?limit=1.5') },
    { name: 'route_list_limit_too_big', op: rGet(ROLF, '?limit=201') },
    { name: 'route_list_offset_negative', op: rGet(ROLF, '?offset=-1') },
    // Two issues → v4 joins the messages with `; `.
    { name: 'route_list_both_bad', op: rGet(ROLF, '?limit=0&offset=-1') },
    { name: 'route_list_missing_character', op: rGet(NOBODY, '') },
    // `withActionDispatch`'s two refusals — this route passes NO default
    // handler, so a missing action is `Action parameter required`.
    { name: 'route_post_no_action', op: rPost(ROLF, F_ROLL_NEW, '') },
    { name: 'route_post_unknown_action', op: rPost(ROLF, F_ROLL_NEW, '?action=bogus') },
    { name: 'route_post_empty_action', op: rPost(ROLF, F_ROLL_NEW, '?action=') },
    { name: 'route_post_save_to_album', op: rPost(ROLF, F_ROLL_NEW, '?action=save-to-album') },
    { name: 'route_post_set_avatar', op: rPost(ROLF, F_ROLL_ALBUM, '?action=set-avatar') },
    // The ladder: 404 for a non-roll, 400 for the vault rung.
    { name: 'route_post_not_a_roll', op: rPost(ROLF, F_NOT_ROLL, '?action=save-to-album') },
    { name: 'route_post_missing_character', op: rPost(NOBODY, F_ROLL_NEW, '?action=save-to-album') },
    { name: 'route_post_no_vault', op: rPost(TALL, F_ROLL_TALL, '?action=save-to-album') },
    { name: 'route_delete_ok', op: rDelete(ROLF, F_ROLL_NEW) },
    { name: 'route_delete_miss', op: rDelete(ROLF, F_NOT_ROLL) },
    { name: 'route_delete_missing_character', op: rDelete(NOBODY, F_ROLL_NEW) },
  ];

  const lines: string[] = [];
  for (const c of cases) {
    lines.push(JSON.stringify(await runCase(spec, c, scratch, fixtures)));
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(`avatar-rolls oracle wrote ${outPath} (${cases.length} cases)\n`);
}

it('emits the avatar-rolls oracle', async () => {
  await main();
}, 300_000);
