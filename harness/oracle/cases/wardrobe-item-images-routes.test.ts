/**
 * @jest-environment node
 *
 * Route ORACLE for the wardrobe item-images edge (P4.D263; v4
 * `app/api/v1/wardrobe/[itemId]/images/route.ts`, `7c8572869`), ported to
 * `quilltap-web::wardrobe_images_routes` + `quilltap-core::api::
 * wardrobe_item_images`.
 *
 * Drives v4's REAL route handlers (`GET`, and `POST` through
 * `withActionDispatch`) over the item-images tier-2 fixture: the DB stack
 * doMocked back to the real modules with the real cipher binding, the auth
 * seam stubbed to the fixture owner (v4's own single-user id), the startup
 * gate forced open. The provider is mocked BELOW v4's real generation
 * (`createImageProvider`) for the `direct` arms; no `http` arm reaches it.
 *
 * Per request: a FRESH module graph and fixture copy, the request's `sql`,
 * the symbolic `{ref}` file ids resolved against the baked state, then ONE
 * request built as v4's handlers read it (`nextUrl.searchParams`, `text()`,
 * `json()`, `formData()` — a multipart body is a real `FormData` of real
 * `File`s). Emits `{ name, status, body, followGet?, rows }` — `rows` the
 * item-scoped `files` rows (linked to a corpus item), the `Wardrobe/images/`
 * links and their blobs.
 *
 * Run (Node 24, from the v4 checkout — a /tmp mirror, jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-wiir-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/wardrobe-item-images-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/wardrobe-item-images-tier2.json" "$TMPO/fixtures/"
 *   cp "$V5W/harness/oracle/fixtures/wardrobe-item-images-routes.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WII_MAIN=/tmp/qt-wiir-main.db QT_FIXTURE_WII_MOUNT=/tmp/qt-wiir-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-wardrobe-item-images-fixture.ts
 *   QT_FIXTURE_WIIR_MAIN=/tmp/qt-wiir-main.db QT_FIXTURE_WIIR_MOUNT=/tmp/qt-wiir-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-wardrobe-item-images-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=600000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "cases/wardrobe-item-images-routes\.test\.ts$"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

type Ref = { item?: string; nth?: number; doc?: string };
interface Part {
  name: string;
  value?: string;
  file?: boolean;
  contentType?: string;
  filename?: string;
  webp?: number;
  size?: number;
  png?: boolean;
}
interface RouteRequest {
  name: string;
  method: 'GET' | 'POST';
  item: string;
  query: Record<string, string> | null;
  action?: string;
  body?: { type: 'json' | 'text' | 'multipart'; value?: unknown; parts?: Part[] };
  driver: 'http' | 'direct';
  provider?: 'ok' | 'refuseAll' | 'throw';
  sql?: string[];
  followGet?: boolean;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  characterId: string;
  archivedCharacterId: string;
  strangerCharacterId: string;
  projectId: string;
  groupId: string;
  items: Record<string, { id: string }>;
  webp: string[];
}

const NOWHERE_ID = '99999999-0000-4000-8000-000000000000';
const PLUGIN_DIRS = [
  'anthropic', 'openai', 'google', 'grok', 'deepseek',
  'z-ai', 'openrouter', 'ollama', 'openai-compatible', 'nanogpt',
];
const SAFETY = '400 Your request was rejected as a result of our safety system.';

function resolveId(spec: Spec, v: string): string {
  switch (v) {
    case '@character':
      return spec.characterId;
    case '@archived':
      return spec.archivedCharacterId;
    case '@stranger':
      return spec.strangerCharacterId;
    case '@project':
      return spec.projectId;
    case '@group':
      return spec.groupId;
    default:
      return v;
  }
}

async function runRequest(
  spec: Spec,
  pngB64: string,
  req: RouteRequest,
  scratch: string,
  mainFixture: string,
  mountFixture: string,
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
  jest.doMock('@/lib/repositories/factory', () =>
    jest.requireActual('@/lib/repositories/factory'),
  );
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
  );
  jest.doMock('@/lib/embedding/embedding-service', () =>
    jest.requireActual('@/lib/embedding/embedding-service'),
  );
  jest.doMock('@/lib/files/webp-conversion', () =>
    jest.requireActual('@/lib/files/webp-conversion'),
  );
  jest.doMock('@/lib/services/llm-logging.service', () =>
    jest.requireActual('@/lib/services/llm-logging.service'),
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
  jest.doMock('@/lib/llm/plugin-factory', () => {
    const actual = jest.requireActual('@/lib/llm/plugin-factory');
    return {
      __esModule: true,
      ...actual,
      createImageProvider: () => ({
        generateImage: async () => {
          if (req.driver !== 'direct') throw new Error('an http arm reached the provider');
          if (req.provider === 'throw') throw new Error('canned provider failure');
          if (req.provider === 'refuseAll') throw new Error(SAFETY);
          return {
            images: [{ data: spec.webp[0], mimeType: 'image/webp', revisedPrompt: 'a revised coat' }],
          };
        },
      }),
    };
  });

  const work = mkdtempSync(join(scratch, 'rq-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.SQLITE_LLM_LOGS_PATH = join(work, 'llm-logs.db');

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { initializeProviderRegistry } = await import('@/lib/plugins/provider-registry');
  await initializeProviderRegistry(
    PLUGIN_DIRS.map((d) => {
      const m = require(join(process.cwd(), 'plugins', 'dist', `qtap-plugin-${d}`, 'index.js'));
      return m.plugin || m.default?.plugin || m.default;
    }),
  );
  const ii = await import('@/lib/wardrobe/item-images');
  const route = (await import('@/app/api/v1/wardrobe/[itemId]/images/route')) as {
    GET: (r: unknown, c: unknown) => Promise<{ status: number; json: () => Promise<unknown> }>;
    POST: (r: unknown, c: unknown) => Promise<{ status: number; json: () => Promise<unknown> }>;
  };

  await initializeDatabase();
  const repos = getRepositories();
  const raw = getRawDatabase();
  const midb = getRawMountIndexDatabase();
  if (!raw || !midb) throw new Error('DB handles unavailable');
  for (const sql of req.sql ?? []) raw.exec(sql);

  const resolveRef = async (ref: Ref): Promise<string> => {
    if (ref.doc) {
      const row = raw
        .prepare(
          "SELECT f.id AS id FROM files f, json_each(f.linkedTo) j WHERE j.value = ? AND f.category = 'DOCUMENT'",
        )
        .get(spec.items[ref.doc].id) as { id: string };
      return row.id;
    }
    const list = await ii.listWardrobeItemImages(repos, spec.items[ref.item as string].id);
    return list[ref.nth ?? 0].id;
  };
  const resolveBody = async (v: unknown): Promise<unknown> => {
    if (v && typeof v === 'object' && !Array.isArray(v)) {
      const o = v as Record<string, unknown>;
      if (o.ref) return resolveRef(o.ref as Ref);
      const out: Record<string, unknown> = {};
      for (const [k, x] of Object.entries(o)) out[k] = await resolveBody(x);
      return out;
    }
    return v;
  };

  const itemId = req.item === '@nowhere' ? NOWHERE_ID : spec.items[req.item].id;
  const qs = new URLSearchParams();
  for (const [k, v] of Object.entries(req.query ?? {})) qs.append(k, resolveId(spec, v));
  if (req.action !== undefined) qs.append('action', req.action);
  const url = `http://localhost/api/v1/wardrobe/${itemId}/images${qs.toString() ? `?${qs}` : ''}`;

  const bodyValue = req.body?.type === 'json' ? await resolveBody(req.body.value) : undefined;
  const text =
    req.body?.type === 'json'
      ? JSON.stringify(bodyValue)
      : req.body?.type === 'text'
        ? (req.body.value as string)
        : '';
  const buildForm = (): FormData => {
    const fd = new FormData();
    for (const p of req.body?.parts ?? []) {
      if (p.file) {
        const bytes = p.size !== undefined
          ? Buffer.alloc(p.size)
          : p.png
            ? Buffer.from(pngB64, 'base64')
            : Buffer.from(spec.webp[p.webp ?? 0], 'base64');
        fd.append(p.name, new File([bytes], p.filename ?? 'f', { type: p.contentType ?? '' }));
      } else {
        fd.append(p.name, p.value ?? '');
      }
    }
    return fd;
  };
  const request = {
    method: req.method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({
      'content-type': req.body?.type === 'multipart' ? 'multipart/form-data' : 'application/json',
    }),
    text: async () => text,
    json: async () => JSON.parse(text),
    formData: async () => {
      if (req.body?.type !== 'multipart') throw new TypeError('Could not parse content as FormData.');
      return buildForm();
    },
  };
  const ctx = { params: Promise.resolve({ itemId }) };

  try {
    const response = req.method === 'GET' ? await route.GET(request, ctx) : await route.POST(request, ctx);
    const status = response.status;
    const body = JSON.parse(JSON.stringify(await response.json()));
    let followGet: unknown;
    if (req.followGet) {
      const getUrl = new URL(url);
      getUrl.searchParams.delete('action');
      const g = await route.GET(
        {
          method: 'GET',
          url: getUrl.toString(),
          nextUrl: getUrl,
          headers: new Headers(),
          text: async () => '',
          json: async () => ({}),
          formData: async () => {
            throw new TypeError('no form');
          },
        },
        ctx,
      );
      followGet = { status: g.status, body: JSON.parse(JSON.stringify(await g.json())) };
    }

    const itemIds = Object.values(spec.items).map((i) => i.id);
    const dump = (db: { prepare: (s: string) => { all: (...a: unknown[]) => unknown[] } }, sql: string, args: unknown[]) =>
      (db.prepare(sql).all(...args) as Array<Record<string, unknown>>).map((r) => {
        const out: Record<string, unknown> = {};
        for (const [k, v] of Object.entries(r)) out[k] = canonValue(v);
        return out;
      });
    const placeholders = itemIds.map(() => '?').join(',');
    const rows = {
      files: dump(
        raw as never,
        `SELECT DISTINCT f.* FROM files f, json_each(f.linkedTo) j WHERE j.value IN (${placeholders}) ORDER BY f.rowid`,
        itemIds,
      ),
      links: dump(
        midb as never,
        "SELECT * FROM doc_mount_file_links WHERE relativePath LIKE 'Wardrobe/images/%' ORDER BY rowid",
        [],
      ),
      blobs: dump(
        midb as never,
        "SELECT b.* FROM doc_mount_blobs b WHERE b.fileId IN (SELECT fileId FROM doc_mount_file_links WHERE relativePath LIKE 'Wardrobe/images/%') ORDER BY b.rowid",
        [],
      ),
    };
    return { name: req.name, status, body, ...(req.followGet ? { followGet } : {}), rows };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'wardrobe-item-images-tier2.json'), 'utf8'),
  ) as Spec;
  const corpus = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'wardrobe-item-images-routes.json'), 'utf8'),
  ) as { png: string; requests: RouteRequest[] };
  // Its OWN build of the tier-2 builder's fixture (`/tmp/qt-wiir-*`, the
  // `f5e953a3f` unification): sharing `/tmp/qt-wii-*` with the tier-2 family
  // let whichever regenerated last re-mint the other's picture ids.
  const mainFixture = process.env.QT_FIXTURE_WIIR_MAIN;
  const mountFixture = process.env.QT_FIXTURE_WIIR_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_WIIR_MAIN and QT_FIXTURE_WIIR_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wiir-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const out: string[] = [];
  for (const req of corpus.requests) {
    out.push(
      JSON.stringify(await runRequest(spec, corpus.png, req, scratch, mainFixture, mountFixture)),
    );
  }
  fs.writeFileSync(outPath, out.join('\n') + '\n');
  process.stderr.write(`wardrobe-item-images-routes oracle wrote ${outPath} (${out.length} requests)\n`);
}

test('wardrobe-item-images-routes oracle', async () => {
  await main();
});

const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
