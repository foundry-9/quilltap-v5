/**
 * @jest-environment node
 *
 * P4.6f characters action-verbs ORACLE: drives v4's REAL
 * `characters/[id]/handlers/post.ts` action handlers (favorite,
 * toggle-controlled-by, toggle-carina, set-default-partner incl. its guards,
 * avatar set/clear, add-tag incl. the missing-tag guard, remove-tag) over a
 * FRESH copy of the committed characters fixture per case, and emits each
 * response body PLUS the post-op slim character row (`findByIdRaw`) so the Rust
 * ports (`api::characters::character_*`) can be diffed byte-for-byte (identical
 * baked ids → no remap; only the op-minted `updatedAt` + the read-time-minted
 * `physicalDescription` timestamps are normalized on the Rust side).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-characters-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/characters-actions.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/characters.json"         "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARACTERS_MAIN=$V5W/crates/quilltap-web/tests/fixtures/characters-main.db \
 *   QT_FIXTURE_CHARACTERS_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/characters-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-characters-actions.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- characters-actions
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
const BRAM = 'a1000000-0000-4000-8000-000000000002';
const CLEO = 'a1000000-0000-4000-8000-000000000003';
const DAX_IMG = 'f0000002-0000-4000-8000-000000000002';
const TAG_MYSTERY = '70000002-0000-4000-8000-000000000002';
const TAG_ADVENTURE = '70000001-0000-4000-8000-000000000001';

interface CaseSpec {
  name: string;
  action: string;
  body: unknown;
}

function mockRequest(url: string, body: unknown): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body),
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
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();

  try {
    const url = `http://localhost/api/v1/characters/${ARIA}?action=${c.action}`;
    const { POST } = (await import('@/app/api/v1/characters/[id]/route')) as {
      POST: (...a: unknown[]) => Promise<unknown>;
    };
    const response = (await POST(mockRequest(url, c.body), {
      params: Promise.resolve({ id: ARIA }),
    })) as { status: number; json: () => Promise<unknown> };
    const status = response.status;
    const body = await response.json();
    const raw = await getRepositories().characters.findByIdRaw(ARIA);
    return { name: c.name, status, body, raw };
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-characters-actions-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cases: CaseSpec[] = [
    { name: 'favorite', action: 'favorite', body: {} },
    { name: 'toggle_controlled_by', action: 'toggle-controlled-by', body: {} },
    { name: 'toggle_carina', action: 'toggle-carina', body: {} },
    { name: 'set_partner_valid', action: 'set-default-partner', body: { partnerId: CLEO } },
    { name: 'set_partner_clear', action: 'set-default-partner', body: { partnerId: null } },
    { name: 'set_partner_llm_fail', action: 'set-default-partner', body: { partnerId: BRAM } },
    { name: 'set_partner_self_fail', action: 'set-default-partner', body: { partnerId: ARIA } },
    { name: 'avatar_set', action: 'avatar', body: { imageId: DAX_IMG } },
    { name: 'avatar_clear', action: 'avatar', body: { imageId: null } },
    { name: 'add_tag', action: 'add-tag', body: { tagId: TAG_MYSTERY } },
    { name: 'remove_tag', action: 'remove-tag', body: { tagId: TAG_ADVENTURE } },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`characters-actions oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('characters-actions oracle', async () => {
  await main();
});
