/**
 * @jest-environment node
 *
 * Route + repository ORACLE for the gallery's Wardrobe Pictures (P4.D321; v4
 * `app/api/v1/characters/[id]/wardrobe-images/route.ts` +
 * `lib/wardrobe/gallery-images.ts` + `FilesRepository.findByLinkedToAny`,
 * `1cfc9b504`), ported to `quilltap-web::characters_routes::wardrobe_images_get`
 * over `quilltap-core::api::characters::character_wardrobe_images` →
 * `services::wardrobe_gallery_images` → `FilesRepository::find_by_linked_to_any`.
 *
 * ONE corpus, two op kinds (the order's R-8):
 *   - `repo` — v4's REAL `repos.files.findByLinkedToAny(ids)` (the `$in`
 *     translation over `json_each("linkedTo")`, `findByFilter`'s Zod drop):
 *     the ids it answers, in order — the row-order + dedupe proof the route
 *     relies on. The `repo` ops run FIRST.
 *   - `route` — v4's REAL route GET behind the real middleware: status + body.
 *
 * Both record v4's log lines (the logger's transports spliced — nothing
 * reaches stdout): level, message, the context keys in order, the scalar
 * context values, and whether an `Error` rode the third argument.
 *
 * The DB stack is doMocked back to the real modules with the real cipher
 * binding, the auth seam stubbed to the fixture owner, the startup gate forced
 * open. Per request: a FRESH module graph and fixture copy, the request's
 * `sql` (main) / `mountSql` (mount index) plants, then ONE call.
 *
 * The fixture is the item-images tier-2 builder's, built into THIS family's
 * OWN `/tmp/qt-wgi-*` paths (never `/tmp/qt-wii-*` / `/tmp/qt-wiir-*`: a
 * shared build re-mints the other families' picture ids).
 *
 * Run (Node 24, from the v4 checkout — a /tmp mirror, jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-wgi-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/wardrobe-images-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/wardrobe-item-images-tier2.json" "$TMPO/fixtures/"
 *   cp "$V5W/harness/oracle/fixtures/wardrobe-images-routes.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WII_MAIN=/tmp/qt-wgi-main.db QT_FIXTURE_WII_MOUNT=/tmp/qt-wgi-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-wardrobe-item-images-fixture.ts
 *   QT_FIXTURE_WGI_MAIN=/tmp/qt-wgi-main.db QT_FIXTURE_WGI_MOUNT=/tmp/qt-wgi-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-wardrobe-images-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=600000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "cases/wardrobe-images-routes\.test\.ts$"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Op {
  name: string;
  op: 'route' | 'repo';
  /** `route`: `@character` / `@archived` / `@stranger` or a literal id. */
  character?: string;
  /** `repo`: item keys (`spec.items`) or literal ids, in argument order. */
  ids?: string[];
  sql?: string[];
  mountSql?: string[];
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  characterId: string;
  archivedCharacterId: string;
  strangerCharacterId: string;
  items: Record<string, { id: string }>;
}

/** Every line this family records (by message prefix). */
const LINE_PREFIXES = [
  '[Characters/WardrobeImages v1]',
  '[WardrobeGallery]',
  '[WearablePool]',
  'Data validation failed',
  'Safe validation failed',
  'Error finding',
];

function resolveCharacter(spec: Spec, v: string): string {
  switch (v) {
    case '@character':
      return spec.characterId;
    case '@archived':
      return spec.archivedCharacterId;
    case '@stranger':
      return spec.strangerCharacterId;
    default:
      return v;
  }
}

async function runOp(
  spec: Spec,
  op: Op,
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
  const { logger } = await import('@/lib/logger');
  const lines: Array<Record<string, unknown>> = [];
  (logger as unknown as { transports: Array<{ write: (d: unknown) => void }> }).transports.splice(
    0,
    (logger as unknown as { transports: unknown[] }).transports.length,
    {
      write: (d: unknown) => {
        const data = d as {
          level: string;
          message: string;
          context?: Record<string, unknown>;
          error?: { message?: string };
        };
        if (!LINE_PREFIXES.some((p) => data.message.startsWith(p))) return;
        const { service: _s, environment: _e, ...fields } = data.context ?? {};
        const values: Record<string, unknown> = {};
        for (const [k, v] of Object.entries(fields)) {
          if (v === null || ['string', 'number', 'boolean'].includes(typeof v)) values[k] = v;
        }
        lines.push({
          level: data.level,
          message: data.message,
          keys: Object.keys(fields),
          values,
          ...(data.error ? { error: data.error.message ?? null } : {}),
        });
      },
    },
  );
  const route = (await import('@/app/api/v1/characters/[id]/wardrobe-images/route')) as {
    GET: (r: unknown, c: unknown) => Promise<{ status: number; json: () => Promise<unknown> }>;
  };

  await initializeDatabase();
  const repos = getRepositories();
  const raw = getRawDatabase();
  const midb = getRawMountIndexDatabase();
  if (!raw || !midb) throw new Error('DB handles unavailable');
  for (const sql of op.sql ?? []) raw.exec(sql);
  for (const sql of op.mountSql ?? []) midb.exec(sql);

  try {
    if (op.op === 'repo') {
      const ids = (op.ids ?? []).map((k) => spec.items[k]?.id ?? k);
      const files = await repos.files.findByLinkedToAny(ids);
      return { name: op.name, op: 'repo', ids: files.map((f) => f.id), lines };
    }
    const id = resolveCharacter(spec, op.character as string);
    const url = `http://localhost/api/v1/characters/${id}/wardrobe-images`;
    const request = {
      method: 'GET',
      url,
      nextUrl: new URL(url),
      headers: new Headers(),
      text: async () => '',
      json: async () => ({}),
    };
    const response = await route.GET(request, { params: Promise.resolve({ id }) });
    const status = response.status;
    const body = JSON.parse(JSON.stringify(await response.json()));
    return { name: op.name, op: 'route', status, body, lines };
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
    fs.readFileSync(join(here, '..', 'fixtures', 'wardrobe-images-routes.json'), 'utf8'),
  ) as { ops: Op[] };
  const mainFixture = process.env.QT_FIXTURE_WGI_MAIN;
  const mountFixture = process.env.QT_FIXTURE_WGI_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_WGI_MAIN and QT_FIXTURE_WGI_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wgi-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  // DEBUG so the `Listed …` / `Archived …` / `[WearablePool]` lines reach the
  // capture (spliced, silent).
  process.env.LOG_LEVEL = 'debug';

  const out: string[] = [];
  for (const op of corpus.ops) {
    out.push(JSON.stringify(await runOp(spec, op, scratch, mainFixture, mountFixture)));
  }
  fs.writeFileSync(outPath, out.join('\n') + '\n');
  process.stderr.write(`wardrobe-images-routes oracle wrote ${outPath} (${out.length} ops)\n`);
}

test('wardrobe-images-routes oracle', async () => {
  await main();
});

const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
