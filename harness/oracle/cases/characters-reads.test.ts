/**
 * @jest-environment node
 *
 * P4.6f characters read-surface ORACLE: drives v4's REAL characters route
 * handlers over a fresh copy of the committed characters fixture, and emits each
 * response body so the Rust ports (`api::characters::*`) can be diffed
 * byte-for-byte (identical baked ids on both sides → no remap).
 *
 * Cases: the list (plain + npc/controlledBy filters), the detail GET, the
 * default-partner / get-tags read actions, and the prompts / scenarios /
 * wardrobe / plugin-data (map + item) sub-resource GETs.
 *
 * Only the seams the Rust harness also neutralizes are mocked: the auth session
 * (getServerSession → the fixture user) and the startup gate (forced ready). The
 * DB stack is doMocked to the REAL modules (past jest.setup) + the real cipher
 * binding, so the reads run against a FRESH copy of the baked fixture per case.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-characters-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/characters-reads.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/characters.json"       "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARACTERS_MAIN=$V5W/crates/quilltap-web/tests/fixtures/characters-main.db \
 *   QT_FIXTURE_CHARACTERS_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/characters-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-characters-reads.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- characters-reads
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

interface CaseSpec {
  name: string;
  module: string;
  url: string;
  params?: Record<string, string>;
}

function mockRequest(url: string): unknown {
  return {
    method: 'GET',
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
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
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

  // Fresh fixture copy → case independence.
  const work = mkdtempSync(join(scratch, 'chars-'));
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
    const mod = (await import(c.module)) as { GET: (...a: unknown[]) => Promise<unknown> };
    const args: unknown[] = [mockRequest(c.url)];
    if (c.params) args.push({ params: Promise.resolve(c.params) });
    const response = (await mod.GET(...args)) as { status: number; json: () => Promise<unknown> };
    const status = response.status;
    const body = await response.json();
    return { name: c.name, status, body };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'characters.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_CHARACTERS_MAIN ?? '',
    mount: process.env.QT_FIXTURE_CHARACTERS_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-characters-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const aria = 'a1000000-0000-4000-8000-000000000001';
  const B = 'http://localhost/api/v1/characters';
  const cases: CaseSpec[] = [
    { name: 'list_all', module: '@/app/api/v1/characters/route', url: B },
    { name: 'list_npc_true', module: '@/app/api/v1/characters/route', url: `${B}?npc=true` },
    { name: 'list_npc_false', module: '@/app/api/v1/characters/route', url: `${B}?npc=false` },
    { name: 'list_user', module: '@/app/api/v1/characters/route', url: `${B}?controlledBy=user` },
    { name: 'list_llm', module: '@/app/api/v1/characters/route', url: `${B}?controlledBy=llm` },
    { name: 'get_aria', module: '@/app/api/v1/characters/[id]/route', url: `${B}/${aria}`, params: { id: aria } },
    { name: 'default_partner', module: '@/app/api/v1/characters/[id]/route', url: `${B}/${aria}?action=default-partner`, params: { id: aria } },
    { name: 'get_tags', module: '@/app/api/v1/characters/[id]/route', url: `${B}/${aria}?action=get-tags`, params: { id: aria } },
    { name: 'prompts', module: '@/app/api/v1/characters/[id]/prompts/route', url: `${B}/${aria}/prompts`, params: { id: aria } },
    { name: 'scenarios', module: '@/app/api/v1/characters/[id]/scenarios/route', url: `${B}/${aria}/scenarios`, params: { id: aria } },
    { name: 'wardrobe', module: '@/app/api/v1/characters/[id]/wardrobe/route', url: `${B}/${aria}/wardrobe`, params: { id: aria } },
    { name: 'plugin_data_map', module: '@/app/api/v1/characters/[id]/plugin-data/route', url: `${B}/${aria}/plugin-data`, params: { id: aria } },
    { name: 'plugin_data_item', module: '@/app/api/v1/characters/[id]/plugin-data/[pluginName]/route', url: `${B}/${aria}/plugin-data/com.example.notes`, params: { id: aria, pluginName: 'com.example.notes' } },
    { name: 'stats', module: '@/app/api/v1/characters/[id]/route', url: `${B}/${aria}?action=stats`, params: { id: aria } },
    { name: 'depiction_guidelines', module: '@/app/api/v1/characters/[id]/route', url: `${B}/${aria}?action=depiction-guidelines`, params: { id: aria } },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`characters-reads oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('characters-reads oracle', async () => {
  await main();
});
