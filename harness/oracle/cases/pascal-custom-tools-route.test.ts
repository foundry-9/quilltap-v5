/**
 * @jest-environment node
 *
 * P4.6ay unit 7 ORACLE: drives v4's REAL custom-tools route handlers
 * (`app/api/v1/chats/[id]/custom-tools/route.ts` GET + POST ?action=run) over a
 * FRESH copy of the committed `pascal-run-custom-{main,mount}.db` fixture per
 * case. GET emits the response body (the merged-per-perspective roster); POST
 * emits the body + the posted `chat_messages` system rows. Diffed against the
 * Rust dispatch (`api::custom_tools::{chat_custom_tools_list,
 * chat_custom_tool_run}`).
 *
 * The fixture's three characters each carry their OWN `ansible.tool.json`
 * (distinct files → distinct variants → the labelled branch), while `coin` and
 * `whispered` live only in char A's vault (seen by B/C via the participant tier
 * → one unlabelled row). All rolls are `min === max` (deterministic).
 *
 * Run (Node 24, from the v4 checkout — mirror the case file to /tmp; jest
 * ignores `.claude/`):
 *   QT_FIXTURE_PASCAL_MAIN=<v5>/crates/quilltap-web/tests/fixtures/pascal-run-custom-main.db \
 *   QT_FIXTURE_PASCAL_MOUNT=<v5>/crates/quilltap-web/tests/fixtures/pascal-run-custom-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-pascal-custom-tools-route.ndjson \
 *     npx jest --silent --roots "$PWD" --roots "$M/cases" -- pascal-custom-tools-route
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

const CHAT = 'c1000000-0000-4000-8000-000000000001';
const CHAR_A = 'a1000000-0000-4000-8000-00000000000a';
const CHAR_B = 'a1000000-0000-4000-8000-00000000000b';

interface CaseSpec {
  name: string;
  method: 'GET' | 'POST';
  body?: Record<string, unknown>;
}

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: body ? 'POST' : 'GET',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
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

  const work = mkdtempSync(join(scratch, 'route-'));
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
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite');

  await initializeDatabase();

  try {
    const params = { params: Promise.resolve({ id: CHAT }) };
    const base = `http://localhost/api/v1/chats/${CHAT}/custom-tools`;
    let response: { status: number; json: () => Promise<unknown> };
    if (c.method === 'GET') {
      const { GET } = await import('@/app/api/v1/chats/[id]/custom-tools/route');
      response = (await GET(mockRequest(base) as never, params as never)) as never;
    } else {
      const { POST } = await import('@/app/api/v1/chats/[id]/custom-tools/route');
      response = (await POST(
        mockRequest(`${base}?action=run`, c.body) as never,
        params as never,
      )) as never;
    }
    const status = response.status;
    const body = await response.json();

    let systemRows: unknown[] = [];
    if (c.method === 'POST') {
      const mdb = getRawDatabase();
      if (!mdb) throw new Error('main DB handle unavailable');
      const hasTable = mdb
        .prepare(`SELECT name FROM sqlite_master WHERE type='table' AND name='chat_messages'`)
        .get();
      if (hasTable) {
        const columns = (
          mdb.prepare(`PRAGMA table_info(chat_messages)`).all() as Array<{ name: string }>
        ).map((col) => col.name);
        const rawRows = mdb
          .prepare(`SELECT * FROM chat_messages WHERE systemSender IS NOT NULL ORDER BY createdAt`)
          .all() as Array<Record<string, unknown>>;
        systemRows = rawRows.map((r) => {
          const out: Record<string, unknown> = {};
          for (const col of columns) out[col] = canonValue(r[col]);
          return out;
        });
      }
    }
    return { name: c.name, status, body, systemRows };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'pascal-run-custom.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_PASCAL_MAIN ?? '',
    mount: process.env.QT_FIXTURE_PASCAL_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-pascal-route-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cases: CaseSpec[] = [
    { name: 'list', method: 'GET' },
    { name: 'run-coin-as-a', method: 'POST', body: { tool: 'coin', asCharacterId: CHAR_A } },
    { name: 'run-ansible-hit', method: 'POST', body: { tool: 'ansible', asCharacterId: CHAR_A } },
    { name: 'run-ansible-miss', method: 'POST', body: { tool: 'ansible', asCharacterId: CHAR_B } },
    { name: 'run-no-character', method: 'POST', body: { tool: 'coin' } },
    { name: 'run-private', method: 'POST', body: { tool: 'coin', asCharacterId: CHAR_A, private: true } },
    { name: 'run-unknown-tool', method: 'POST', body: { tool: 'nope', asCharacterId: CHAR_A } },
    { name: 'run-unknown-character', method: 'POST', body: { tool: 'coin', asCharacterId: 'a1000000-0000-4000-8000-0000000000ff' } },
    { name: 'run-error', method: 'POST', body: { tool: 'coin', asCharacterId: CHAR_A, parameters: { bad: 1 } } },
  ];

  const out = fs.createWriteStream(outPath);
  for (const c of cases) {
    const row = await runCase(spec, c, scratch, fixtures);
    out.write(JSON.stringify(row) + '\n');
  }
  out.end();
  await new Promise((r) => out.on('finish', r));
  rmSync(scratch, { recursive: true, force: true });
}

it('emits the pascal custom-tools route oracle', async () => {
  await main();
}, 120000);
