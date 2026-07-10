/**
 * @jest-environment node
 *
 * P4.6f characters sub-resource-mutation ORACLE: drives v4's REAL prompts /
 * scenarios / plugin-data route handlers (POST/PUT/DELETE) over a FRESH copy of
 * the committed characters fixture per case, and emits each response body. The
 * underlying repo ops are already differential-proven (characters_arrays_tier2 /
 * character_plugin_data_upsert_tier2); this pins the HANDLER assembly.
 *
 * Update/delete/set-default target an EXISTING baked sub-item, resolved by name
 * inside each case (Aria's "Backup" prompt / "Prologue" scenario / "com.example.
 * notes" plugin) so the ids are identical on both differential sides. Create
 * cases mint fresh ids/timestamps (normalized on the Rust side).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-characters-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/characters-subresources.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/characters.json"              "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARACTERS_MAIN=$V5W/crates/quilltap-web/tests/fixtures/characters-main.db \
 *   QT_FIXTURE_CHARACTERS_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/characters-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-characters-subresources.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- characters-subresources
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
const ARIA = 'a1000000-0000-4000-8000-000000000001';

interface CaseSpec {
  name: string;
  module: string;
  method: 'POST' | 'PUT' | 'DELETE';
  /** Resolve a sub-item id by name and route it into params/query. */
  target?: { kind: 'prompt' | 'scenario' | 'plugin'; byName: string };
  params?: (id: string) => Record<string, string>;
  query?: (id: string) => string;
  body: unknown;
}

function mockRequest(url: string, method: string, body: unknown): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body),
    formData: jest.fn(),
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
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () => jest.requireActual('@/lib/embedding/vector-store'));
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

  const work = mkdtempSync(join(scratch, 'chars-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
  const { getRepositories } = await import('@/lib/repositories/factory');
  await initializeDatabase();

  try {
    // Resolve a target id by name, if the case needs one.
    let targetId = '';
    if (c.target) {
      const character = await getRepositories().characters.findById(ARIA);
      if (c.target.kind === 'prompt') {
        targetId = (character?.systemPrompts ?? []).find((p: { name: string }) => p.name === c.target!.byName)?.id ?? '';
      } else if (c.target.kind === 'scenario') {
        targetId = (character?.scenarios ?? []).find((s: { title: string }) => s.title === c.target!.byName)?.id ?? '';
      } else {
        targetId = c.target.byName; // plugin name is the id
      }
    }
    let url = `http://localhost/api/v1/characters/${ARIA}/sub`;
    if (c.query) url += `?${c.query(targetId)}`;
    const params = c.params ? c.params(targetId) : { id: ARIA };
    const mod = (await import(c.module)) as Record<string, (...a: unknown[]) => Promise<unknown>>;
    const handler = mod[c.method];
    const response = (await handler(mockRequest(url, c.method, c.body), {
      params: Promise.resolve(params),
    })) as { status: number; json: () => Promise<unknown> };
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
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'characters.json'), 'utf8')) as Spec;
  const fixtures = {
    main: process.env.QT_FIXTURE_CHARACTERS_MAIN ?? '',
    mount: process.env.QT_FIXTURE_CHARACTERS_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-characters-subres-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const PROMPTS = '@/app/api/v1/characters/[id]/prompts/route';
  const PROMPT_ITEM = '@/app/api/v1/characters/[id]/prompts/[promptId]/route';
  const SCENARIOS = '@/app/api/v1/characters/[id]/scenarios/route';
  const PLUGIN = '@/app/api/v1/characters/[id]/plugin-data/route';
  const PLUGIN_ITEM = '@/app/api/v1/characters/[id]/plugin-data/[pluginName]/route';

  const cases: CaseSpec[] = [
    { name: 'prompt_create', module: PROMPTS, method: 'POST', body: { name: 'Extra', content: 'A third prompt.', isDefault: false } },
    { name: 'prompt_update', module: PROMPT_ITEM, method: 'PUT', target: { kind: 'prompt', byName: 'Backup' }, params: (id) => ({ id: ARIA, promptId: id }), body: { content: 'Revised backup prompt.' } },
    { name: 'prompt_delete', module: PROMPT_ITEM, method: 'DELETE', target: { kind: 'prompt', byName: 'Backup' }, params: (id) => ({ id: ARIA, promptId: id }), body: {} },
    { name: 'scenario_create', module: SCENARIOS, method: 'POST', body: { title: 'Epilogue', content: 'The voyage ends.' } },
    { name: 'scenario_update', module: SCENARIOS, method: 'PUT', target: { kind: 'scenario', byName: 'Prologue' }, query: (id) => `scenarioId=${id}`, body: { content: 'A revised prologue.' } },
    { name: 'scenario_delete', module: SCENARIOS, method: 'DELETE', target: { kind: 'scenario', byName: 'Interlude' }, query: (id) => `scenarioId=${id}`, body: {} },
    { name: 'plugin_upsert_existing', module: PLUGIN, method: 'POST', body: { pluginName: 'com.example.notes', data: { color: 'crimson' } } },
    { name: 'plugin_upsert_new', module: PLUGIN, method: 'POST', body: { pluginName: 'com.example.flags', data: { beta: true } } },
    { name: 'plugin_delete', module: PLUGIN_ITEM, method: 'DELETE', target: { kind: 'plugin', byName: 'com.example.notes' }, params: (id) => ({ id: ARIA, pluginName: id }), body: {} },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    outLines.push(JSON.stringify(await runCase(spec, c, scratch, fixtures)));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`characters-subresources oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('characters-subresources oracle', async () => {
  await main();
});
