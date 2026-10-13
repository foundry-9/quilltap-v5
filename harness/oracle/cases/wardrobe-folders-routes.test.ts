/**
 * @jest-environment node
 *
 * P4.D281 WARDROBE-FOLDERS-ROUTES ORACLE (v4 `4a38222e6`): drives v4's REAL
 * route factory (`lib/wardrobe/routes/wardrobe-route-factory.ts`) through the
 * four tiers' registered route files — `characters/[id]/wardrobe`,
 * `wardrobe` (Quilltap General), `projects/[id]/wardrobe`,
 * `groups/[id]/wardrobe` (+ each `[itemId]` route) — for the five folder
 * actions (`GET ?action=folders`; `POST ?action=create-folder`,
 * `rename-folder`, `delete-folder`, `move-items`) and the collection POST /
 * item PUT carrying `folder`. Each case runs over a FRESH copy of the
 * keystone's four-tier seed (`build-wardrobe-keystone-fixture.ts`, consumed
 * read-only and built into this family's OWN `/tmp/p4d281`-style paths), in a
 * fresh jest module graph.
 *
 * THE CASE LIST LIVES IN `wardrobe-folders-routes.json#cases` — the corpus the
 * Rust differential (`wardrobe_folders_routes_equivalence`) reads too; it
 * asserts its row count against this NDJSON's. A case is a list of `steps`
 * run in order on the SAME database copy (a chained 409 — a same-named
 * subfolder — is made by a create-folder step first); EVERY step's
 * `{status, body}` is emitted, then the case's mount-index write-set: every
 * `doc_mount_file_links` relative path and every `doc_mount_folders` path,
 * per mount (paths only — the minted ids and the frontmatter's `updatedAt`
 * are not compared).
 *
 * Owner refs: `ada` (user A's character), `bo` (user B's — the ownership
 * gate), `zed` (archived), `missingChar`, `general`, `bare` (a project whose
 * store has no `Wardrobe/`), `null` (a project whose slim row's
 * `officialMountPointId` is NULL — v4's real `findById` throws), `full`
 * (project), `guild` (group), `missing`. Item refs name the seed's `items`
 * keys. `plants` run raw SQL on the MAIN copy pre-init (the unprovisioned
 * General: `DELETE FROM instance_settings WHERE key = 'generalMountPointId'`).
 *
 * Run (Node 24, from the PINNED v4 worktree — cp to a /tmp mirror; jest
 * ignores `.claude/` paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-wfr-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/wardrobe-folders-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/wardrobe-folders-routes.json" "$TMPO/fixtures/"
 *   cp "$V5W/harness/oracle/fixtures/wardrobe-keystone-tier2.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server   # (the pinned worktree while v4 is past the baseline)
 *   QT_FIXTURE_WKS_MAIN=/tmp/qt-wfr-main.db QT_FIXTURE_WKS_MOUNT=/tmp/qt-wfr-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-wardrobe-keystone-fixture.ts
 *   QT_FIXTURE_WFR_MAIN=/tmp/qt-wfr-main.db QT_FIXTURE_WFR_MOUNT=/tmp/qt-wfr-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-wardrobe-folders-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=600000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- wardrobe-folders-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Step {
  /** `folders` | `create-folder` | `rename-folder` | `delete-folder` |
   *  `move-items` | `create` | `update` | `list`. */
  op: string;
  body?: unknown;
  /** `update`: the item ref (a seed `items` key) or a literal id. */
  item?: string;
  /** `move-items`: item refs, expanded into `body.itemIds` (keys or literals). */
  items?: string[];
}
interface Case {
  name: string;
  tier: 'character' | 'general' | 'project' | 'group';
  owner?: string;
  steps: Step[];
  plants?: Array<{ sql: string; params: Array<string | number | null> }>;
}
interface Spec {
  ownerIds: Record<string, string>;
  cases: Case[];
}
interface SeedSpec {
  testPepperBase64: string;
  userAId: string;
  items: Record<string, { id: string }>;
}

function mockRequest(url: string, method: string, body?: unknown): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body),
  };
}

type Handler = (r: unknown, ctx: unknown) => Promise<{ status: number; json(): Promise<unknown> }>;

async function runCase(
  spec: Spec,
  seed: SeedSpec,
  c: Case,
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
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: seed.userAId } }),
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

  const work = mkdtempSync(join(scratch, 'case-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);
  if (c.plants && c.plants.length) {
    const nodeRequire = require('node:module').createRequire(join(process.cwd(), 'noop.js'));
    const Database = nodeRequire(cipherDriverPath);
    const conn = new Database(mainWork);
    conn.pragma(`key = "x'${Buffer.from(seed.testPepperBase64, 'base64').toString('hex')}'"`);
    try {
      for (const p of c.plants) conn.prepare(p.sql).run(...p.params);
    } finally {
      conn.close();
    }
  }
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const routes: Record<string, { coll: Record<string, Handler>; item: Record<string, Handler>; base: string }> = {
    character: {
      coll: (await import('@/app/api/v1/characters/[id]/wardrobe/route')) as unknown as Record<string, Handler>,
      item: (await import('@/app/api/v1/characters/[id]/wardrobe/[itemId]/route')) as unknown as Record<string, Handler>,
      base: 'characters',
    },
    general: {
      coll: (await import('@/app/api/v1/wardrobe/route')) as unknown as Record<string, Handler>,
      item: (await import('@/app/api/v1/wardrobe/[itemId]/route')) as unknown as Record<string, Handler>,
      base: '',
    },
    project: {
      coll: (await import('@/app/api/v1/projects/[id]/wardrobe/route')) as unknown as Record<string, Handler>,
      item: (await import('@/app/api/v1/projects/[id]/wardrobe/[itemId]/route')) as unknown as Record<string, Handler>,
      base: 'projects',
    },
    group: {
      coll: (await import('@/app/api/v1/groups/[id]/wardrobe/route')) as unknown as Record<string, Handler>,
      item: (await import('@/app/api/v1/groups/[id]/wardrobe/[itemId]/route')) as unknown as Record<string, Handler>,
      base: 'groups',
    },
  };

  await initializeDatabase();

  const itemId = (ref: string): string => seed.items[ref]?.id ?? ref;

  try {
    const r = routes[c.tier];
    const owner = c.tier === 'general' ? null : spec.ownerIds[c.owner ?? ''] ?? c.owner ?? '';
    const B = owner === null
      ? 'http://localhost/api/v1/wardrobe'
      : `http://localhost/api/v1/${r.base}/${owner}/wardrobe`;
    const collCtx = { params: Promise.resolve(owner === null ? {} : { id: owner }) };

    const steps: Array<Record<string, unknown>> = [];
    for (const s of c.steps) {
      let response: { status: number; json(): Promise<unknown> };
      switch (s.op) {
        case 'folders':
          response = await r.coll.GET(mockRequest(`${B}?action=folders`, 'GET'), collCtx);
          break;
        case 'list':
          response = await r.coll.GET(mockRequest(B, 'GET'), collCtx);
          break;
        case 'create-folder':
        case 'rename-folder':
        case 'delete-folder':
          response = await r.coll.POST(mockRequest(`${B}?action=${s.op}`, 'POST', s.body), collCtx);
          break;
        case 'move-items': {
          const body = s.items
            ? { ...(s.body as Record<string, unknown>), itemIds: s.items.map(itemId) }
            : s.body;
          response = await r.coll.POST(mockRequest(`${B}?action=move-items`, 'POST', body), collCtx);
          break;
        }
        case 'create':
          response = await r.coll.POST(mockRequest(B, 'POST', s.body), collCtx);
          break;
        case 'update': {
          const iid = itemId(s.item ?? '');
          const ctx = { params: Promise.resolve(owner === null ? { itemId: iid } : { id: owner, itemId: iid }) };
          response = await r.item.PUT(mockRequest(`${B}/${iid}`, 'PUT', s.body), ctx);
          break;
        }
        default:
          throw new Error(`unknown step op ${s.op}`);
      }
      const status = response.status;
      const body = await response.json();
      steps.push({ op: s.op, status, body });
    }

    for (let i = 0; i < 10; i++) {
      await new Promise((resolve) => setImmediate(resolve));
    }

    const midb = getRawMountIndexDatabase();
    if (!midb) throw new Error('mount-index DB handle unavailable for dump');
    const links = midb
      .prepare(
        'SELECT "mountPointId", "relativePath" FROM "doc_mount_file_links" ORDER BY "mountPointId", "relativePath"',
      )
      .all();
    const folders = midb
      .prepare('SELECT "mountPointId", "path" FROM "doc_mount_folders" ORDER BY "mountPointId", "path"')
      .all();
    return { name: c.name, steps, links, folders };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'wardrobe-folders-routes.json'), 'utf8'),
  ) as Spec;
  const seed = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'wardrobe-keystone-tier2.json'), 'utf8'),
  ) as SeedSpec;

  const mainFixture = process.env.QT_FIXTURE_WFR_MAIN;
  const mountFixture = process.env.QT_FIXTURE_WFR_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_WFR_MAIN and QT_FIXTURE_WFR_MOUNT must point at the keystone seed (built by build-wardrobe-keystone-fixture.ts)',
    );
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wfr-oracle-'));
  try {
    mkdirSync(join(scratch, 'data'), { recursive: true });
    process.env.ENCRYPTION_MASTER_PEPPER = seed.testPepperBase64;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    const outLines: string[] = [];
    for (const c of spec.cases) {
      outLines.push(JSON.stringify(await runCase(spec, seed, c, scratch, mainFixture, mountFixture)));
    }
    fs.writeFileSync(outPath, outLines.join('\n') + '\n');
    process.stderr.write(`wardrobe-folders-routes oracle wrote ${outPath} (${outLines.length} cases)\n`);
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

test('wardrobe-folders-routes tier-2 oracle', async () => {
  await main();
});
