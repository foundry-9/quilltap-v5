/**
 * @jest-environment node
 *
 * P4.D119 ROUTES ORACLE — the four `?action=instructions` GET/POST surfaces
 * (v4 `b86bb1a5`): `app/api/v1/characters/[id]/wardrobe/route.ts`,
 * `.../groups/[id]/wardrobe/route.ts`, `.../projects/[id]/wardrobe/route.ts`
 * and `.../wardrobe/route.ts`.
 *
 * Drives v4's REAL route handlers through the REAL middleware — the flat
 * `Validation error` 400 and the unknown-action envelope come out of THAT
 * layer, not the routes — against a FRESH copy of the committed
 * `wardrobe-instructions-{main,mount}.db` pair per case. Only the auth seam is
 * stubbed and the startup gate forced open; the whole DB stack is doMocked to
 * the REAL modules plus the real cipher driver (the `group-wardrobe` real-DB
 * idiom).
 *
 * THE CASE LIST IS NOT DECLARED HERE: both this oracle and the Rust
 * differential read `wardrobe-instructions.json#routeCases`, so neither side
 * can silently partial-pass.
 *
 * Emits one NDJSON line per case: `{name, status, body, tables}` — writes are
 * verified through the mount-index tables (the instructions file's bytes, and
 * its ABSENCE after a clear), not just the response body.
 *
 * Run (Node 24, from the v4 checkout or a pinned worktree — cp to a /tmp
 * mirror; jest ignores `.claude/` paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-wi-routes-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/wardrobe-instructions-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/wardrobe-instructions.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WI_MAIN=$V5W/crates/quilltap-web/tests/fixtures/wardrobe-instructions-main.db \
 *   QT_FIXTURE_WI_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/wardrobe-instructions-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-wardrobe-instructions-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- wardrobe-instructions-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

const INSTRUCTIONS_PATH = 'Wardrobe/instructions.md';

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}
function canonicalizeRows(opts: {
  table: string;
  columns: string[];
  rawRows: Array<Record<string, unknown>>;
  orderBy?: string;
}): { table: string; columns: string[]; rows: Array<Record<string, unknown>> } {
  const { table, columns, rawRows, orderBy = 'id' } = opts;
  const rows = rawRows
    .map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
      return out;
    })
    .sort((a, b) => {
      const av = String(a[orderBy] ?? '');
      const bv = String(b[orderBy] ?? '');
      return av < bv ? -1 : av > bv ? 1 : 0;
    });
  return { table, columns, rows };
}

interface Case {
  name: string;
  scope: 'character' | 'group' | 'project' | 'general';
  method: 'GET' | 'POST';
  target?: 'missing' | 'archived' | 'vaultless';
  action?: string;
  seed?: Record<string, string>;
  unprovisionGeneral?: boolean;
  body?: unknown;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  characterId: string;
  vaultlessCharacterId: string;
  archivedCharacterId: string;
  missingCharacterId: string;
  projectId: string;
  groupId: string;
  missingProjectId: string;
  missingGroupId: string;
  generalMountPointId: string;
  extraStores: Array<{ label: string; id: string; name: string }>;
  routeCases: Case[];
}

const MOUNT_TABLES: Array<{ key: string; table: string; orderBy: string }> = [
  { key: 'points', table: 'doc_mount_points', orderBy: 'name' },
  { key: 'files', table: 'doc_mount_files', orderBy: 'sha256' },
  { key: 'documents', table: 'doc_mount_documents', orderBy: 'contentSha256' },
  { key: 'links', table: 'doc_mount_file_links', orderBy: 'relativePath' },
  { key: 'folders', table: 'doc_mount_folders', orderBy: 'path' },
];

function mockRequest(url: string, method: string, body?: unknown): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body),
  };
}

async function runCase(
  spec: Spec,
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

  const work = mkdtempSync(join(scratch, 'case-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
  const { ensureFolderPath } = await import('@/lib/mount-index/folder-paths');

  await initializeDatabase();

  try {
    // Per-case seeding through the RAW document-store write: the helpers under
    // test never do the seeding.
    if (c.seed) {
      const repos = getRepositories();
      const labels: Record<string, string> = {
        general: spec.generalMountPointId,
      };
      const charRow = await repos.characters.findByIdRaw(spec.characterId);
      labels.charA = charRow?.characterDocumentMountPointId as string;
      const archRow = await repos.characters.findByIdRaw(spec.archivedCharacterId);
      labels.charC = archRow?.characterDocumentMountPointId as string;
      const projRow = await repos.projects.findByIdRaw(spec.projectId);
      labels.project = projRow?.officialMountPointId as string;
      const groupRow = await repos.groups.findByIdRaw(spec.groupId);
      labels.group = groupRow?.officialMountPointId as string;
      for (const s of spec.extraStores) labels[s.label] = s.id;
      for (const [label, content] of Object.entries(c.seed)) {
        const mp = labels[label];
        if (!mp) throw new Error(`no mount for seed label ${label}`);
        await ensureFolderPath(mp, 'Wardrobe');
        await writeDatabaseDocument(mp, INSTRUCTIONS_PATH, content);
      }
    }
    if (c.unprovisionGeneral) {
      await rawQuery('DELETE FROM "instance_settings" WHERE "key" = ?', ['generalMountPointId']);
    }

    const action = c.action ?? 'instructions';
    let response: { status: number; json(): Promise<unknown> };
    if (c.scope === 'general') {
      const route = (await import('@/app/api/v1/wardrobe/route')) as never as Record<
        string,
        (r: unknown) => Promise<{ status: number; json(): Promise<unknown> }>
      >;
      const url = `http://localhost/api/v1/wardrobe?action=${action}`;
      response = await route[c.method](mockRequest(url, c.method, c.body));
    } else {
      const id =
        c.scope === 'character'
          ? c.target === 'missing'
            ? spec.missingCharacterId
            : c.target === 'archived'
              ? spec.archivedCharacterId
              : c.target === 'vaultless'
                ? spec.vaultlessCharacterId
                : spec.characterId
          : c.scope === 'group'
            ? c.target === 'missing'
              ? spec.missingGroupId
              : spec.groupId
            : c.target === 'missing'
              ? spec.missingProjectId
              : spec.projectId;
      const folder =
        c.scope === 'character' ? 'characters' : c.scope === 'group' ? 'groups' : 'projects';
      const route = (await import(`@/app/api/v1/${folder}/[id]/wardrobe/route`)) as never as Record<
        string,
        (r: unknown, ctx: unknown) => Promise<{ status: number; json(): Promise<unknown> }>
      >;
      const url = `http://localhost/api/v1/${folder}/${id}/wardrobe?action=${action}`;
      response = await route[c.method](mockRequest(url, c.method, c.body), {
        params: Promise.resolve({ id }),
      });
    }

    const status: number = response.status;
    const body = await response.json();

    const midb = getRawMountIndexDatabase();
    if (!midb) throw new Error('mount-index DB handle unavailable for dump');
    const tables: Record<string, unknown> = {};
    for (const t of MOUNT_TABLES) {
      const columns = (
        midb.prepare(`PRAGMA table_info(${t.table})`).all() as Array<{ name: string }>
      ).map((col) => col.name);
      const rawRows = midb.prepare(`SELECT * FROM ${t.table}`).all() as Array<
        Record<string, unknown>
      >;
      tables[t.key] = canonicalizeRows({ table: t.table, columns, rawRows, orderBy: t.orderBy });
    }

    return { name: c.name, status, body, tables };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'wardrobe-instructions.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_WI_MAIN;
  const mountFixture = process.env.QT_FIXTURE_WI_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_WI_MAIN and QT_FIXTURE_WI_MOUNT must point at the fixture pair');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wi-routes-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const lines: string[] = [];
  for (const c of spec.routeCases) {
    lines.push(JSON.stringify(await runCase(spec, c, scratch, mainFixture, mountFixture)));
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(
    `wardrobe-instructions-routes oracle wrote ${outPath} (${lines.length} cases)\n`,
  );
}

test('wardrobe-instructions-routes tier-2 oracle', async () => {
  await main();
});
