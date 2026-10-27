/**
 * @jest-environment node
 *
 * P4.D325 INVENTORY OBJECT-IMAGES ROUTE ORACLE: drives v4's REAL
 * `app/api/v1/inventory/[itemId]/images/route.ts` `GET` / `POST` at the
 * `322a5488e` pin (each wrapped in v4's real context middleware and
 * `withActionDispatch`) over a FRESH copy of the committed
 * `inventory-routes-{main,mount}.db` pair per case — the routes family's pair
 * (it carries the keystone's pictures: two on Vivienne's sword, one on the
 * General lantern archetype its vault instance inherits). Emits `{name,
 * status, body}`; `followGet` adds `${name}__get` (the list on the same copy).
 *
 * THE CASE LIST IS NOT DECLARED HERE: both this oracle and the Rust family
 * (`crates/quilltap-web/tests/inventory_item_images_routes_equivalence.rs`)
 * read `inventory-item-images-routes.json#cases`.
 *
 * Body refs resolve on the copy, both sides alike: `{ref: {item, nth}}` is the
 * object's `nth` picture newest-first (`files` linked to the item, `createdAt`
 * DESC — the pair pins them a second apart); `{vault: <characterId>}` is that
 * character's vault mount id.
 *
 * NO provider is reached: the pair has no image profile, so every `generate`
 * that passes its gates answers v4's one `NoInventoryImageProfileError` 400
 * (the 201 / 422 / 502 outcomes are the keystone's
 * `inventory_item_image_generation_tier3` family and the edge's `render`
 * unit pins). The title lint is stubbed as the routes oracle stubs it.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-iiir-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures" "$TMPO/lib"
 *   cp "$V5W/harness/oracle/cases/inventory-item-images-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/lib/v4-migrations.ts" "$TMPO/lib/"
 *   cp "$V5W/harness/oracle/fixtures/inventory-item-images-routes.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_IIIR_MAIN=$V5W/crates/quilltap-web/tests/fixtures/inventory-routes-main.db \
 *   QT_FIXTURE_IIIR_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/inventory-routes-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-inventory-item-images-routes.ndjson TZ=UTC \
 *     $N/npx jest --silent --watchman=false --testTimeout=240000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- inventory-item-images-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Part {
  name: string;
  value?: string;
  file?: boolean;
  filename?: string;
  contentType?: string;
  size?: number;
  png?: boolean;
}
interface CaseEntry {
  name: string;
  method: 'GET' | 'POST';
  item: string;
  query: Array<[string, string]>;
  body?: { type: 'json'; value: unknown } | { type: 'text'; value: string } | { type: 'multipart'; parts: Part[] };
  followGet?: boolean;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  png: string;
  cases: CaseEntry[];
}

type Handler = (...a: unknown[]) => Promise<unknown>;

function applyMocks(spec: Spec): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  for (const p of [
    '@/lib/database/manager',
    '@/lib/database/repositories',
    '@/lib/repositories/factory',
    '@/lib/file-storage/character-vault-bridge',
    '@/lib/embedding/vector-store',
    '@/lib/file-storage/manager',
  ]) {
    jest.doMock(p, () => jest.requireActual(p));
  }
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
  // The Inventory title lint runs on both sides since the union (P4.D340 landed).
}

async function resolveRefs(v: unknown): Promise<unknown> {
  const { rawQuery } = await import('@/lib/database/manager');
  if (Array.isArray(v)) return Promise.all(v.map(resolveRefs));
  if (v && typeof v === 'object') {
    const o = v as Record<string, unknown>;
    if (o.ref && typeof o.ref === 'object') {
      const r = o.ref as { item: string; nth?: number };
      const rows = (await rawQuery(
        `SELECT id FROM files WHERE category = 'IMAGE' AND "linkedTo" LIKE ? ORDER BY "createdAt" DESC, rowid`,
        [`%${r.item}%`],
      )) as Array<{ id: string }>;
      return rows[r.nth ?? 0].id;
    }
    if (typeof o.vault === 'string') {
      const rows = (await rawQuery('SELECT "characterDocumentMountPointId" AS m FROM characters WHERE id = ?', [
        o.vault,
      ])) as Array<{ m: string }>;
      return rows[0].m;
    }
    const out: Record<string, unknown> = {};
    for (const [k, e] of Object.entries(o)) out[k] = await resolveRefs(e);
    return out;
  }
  return v;
}

async function buildRequest(spec: Spec, c: CaseEntry, url: string): Promise<unknown> {
  let text = '';
  let form: FormData | null = null;
  if (c.body?.type === 'json') text = JSON.stringify(await resolveRefs(c.body.value));
  if (c.body?.type === 'text') text = c.body.value;
  if (c.body?.type === 'multipart') {
    form = new FormData();
    for (const p of c.body.parts) {
      if (p.file) {
        const bytes = p.png ? Buffer.from(spec.png, 'base64') : Buffer.alloc(p.size ?? 0);
        form.append(p.name, new File([bytes], p.filename ?? 'f', { type: p.contentType }));
      } else {
        form.append(p.name, p.value ?? '');
      }
    }
  }
  return {
    method: c.method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': form ? 'multipart/form-data' : 'application/json' }),
    text: async () => text,
    json: async () => JSON.parse(text),
    formData: async () => {
      if (!form) throw new TypeError('Content-Type was not one of "multipart/form-data"');
      return form;
    },
  };
}

function urlFor(c: CaseEntry, query: Array<[string, string]>): string {
  const params = new URLSearchParams();
  for (const [k, v] of query) params.append(k, v);
  const qs = params.toString();
  return `http://localhost/api/v1/inventory/${c.item}/images${qs ? `?${qs}` : ''}`;
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

async function runCase(
  spec: Spec,
  c: CaseEntry,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Array<Record<string, unknown>>> {
  jest.resetModules();
  applyMocks(spec);
  const work = mkdtempSync(join(scratch, 'iiir-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  const { closeDatabase, initializeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
  await initializeDatabase();
  const { plantChatWardrobeHistoryTable } = await import('../lib/v4-migrations');
  await plantChatWardrobeHistoryTable();
  try {
    const mod = (await import('@/app/api/v1/inventory/[itemId]/images/route')) as Record<string, Handler>;
    const params = { params: Promise.resolve({ itemId: c.item }) };
    const rows: Array<Record<string, unknown>> = [];
    const out = await respond(await mod[c.method](await buildRequest(spec, c, urlFor(c, c.query)), params));
    rows.push({ name: c.name, status: out.status, body: out.body });
    await new Promise((r) => setTimeout(r, 100));
    if (c.followGet) {
      const getQuery = c.query.filter(([k]) => k !== 'action');
      const follow = await respond(
        await mod.GET(await buildRequest(spec, { ...c, method: 'GET', body: undefined }, urlFor(c, getQuery)), params),
      );
      rows.push({ name: `${c.name}__get`, status: follow.status, body: follow.body });
    }
    return rows;
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'inventory-item-images-routes.json'), 'utf8'),
  ) as Spec;
  const fixtures = {
    main: process.env.QT_FIXTURE_IIIR_MAIN ?? '',
    mount: process.env.QT_FIXTURE_IIIR_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  const scratch = mkdtempSync(join(tmpdir(), 'qt-iiir-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';
  const outLines: string[] = [];
  for (const c of spec.cases) {
    for (const p of await runCase(spec, c, scratch, fixtures)) outLines.push(JSON.stringify(p));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`inventory-item-images-routes oracle wrote ${outPath} (${outLines.length} rows)\n`);
}

test('inventory-item-images-routes oracle', async () => {
  await main();
});

const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
