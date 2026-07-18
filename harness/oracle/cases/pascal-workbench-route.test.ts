/**
 * @jest-environment node
 *
 * P4.6ay unit 12 ORACLE (part B): drives v4's REAL `/api/v1/custom-tools` route
 * handlers (GET library / `?action=destinations`, POST `?action=preview` /
 * `?action=audit`) over a FRESH copy of the committed
 * `workbench-{main,mount}.db` fixture per case. Emits `{status, body}` rows,
 * diffed against the Rust dispatch (`api::custom_tools::{custom_tools_library,
 * custom_tools_destinations, custom_tool_preview, custom_tool_audit}`).
 *
 * The case corpus lives in `harness/oracle/fixtures/workbench-route-cases.json`
 * and is read by BOTH sides, so the two can never drift apart. Every roll in it
 * is `min === max`, which is what makes preview and audit exactly comparable
 * across languages: v4's `simulateOutcomes` draws through the crypto with no
 * seam, so a draw-for-draw diff is impossible — a deterministic corpus never
 * consumes a byte and pins exact `runs`/`hits`/`share`/min/max/mean instead.
 * Stochastic spread is covered by the v5-side statistical tests mirroring v4's
 * own `custom-tools-simulate.test.ts`.
 *
 * Run (Node 24, from the v4 checkout — mirror the case file to /tmp; jest
 * ignores `.claude/`):
 *   QT_FIXTURE_WORKBENCH_MAIN=<v5>/crates/quilltap-web/tests/fixtures/workbench-main.db \
 *   QT_FIXTURE_WORKBENCH_MOUNT=<v5>/crates/quilltap-web/tests/fixtures/workbench-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-pascal-workbench-route.ndjson \
 *     npx jest --silent --roots "$PWD" --roots "$M/cases" -- pascal-workbench-route
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
  method: 'GET' | 'POST';
  action?: string;
  definition?: string;
  params?: Record<string, unknown>;
  private?: boolean;
  metadata?: Record<string, unknown>;
  metadataCharacter?: string;
  /** §B: the bench oracle, sent verbatim (an explicit null is a distinct arm). */
  llm?: unknown;
}

interface Corpus {
  characters: Record<string, string>;
  definitions: Record<string, unknown>;
  cases: CaseSpec[];
}

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: body === undefined ? 'GET' : 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

/** Build the POST body a case sends, resolving its definition + metadata refs. */
function bodyFor(c: CaseSpec, corpus: Corpus): Record<string, unknown> {
  const body: Record<string, unknown> = { definition: corpus.definitions[c.definition ?? ''] };
  if (c.params !== undefined) body.params = c.params;
  if (c.private !== undefined) body.private = c.private;
  if (c.metadata !== undefined) body.metadata = c.metadata;
  if (c.metadataCharacter !== undefined) {
    body.metadata = { characterId: corpus.characters[c.metadataCharacter] };
  }
  // §B: the bench oracle. `hasOwnProperty` rather than `!== undefined`, so a
  // case carrying an explicit `null` still SENDS the null — that is a distinct
  // arm (`.nullish()`) from omitting the field.
  if (Object.prototype.hasOwnProperty.call(c, 'llm')) body.llm = (c as {llm: unknown}).llm;
  return body;
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  corpus: Corpus,
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

  const work = mkdtempSync(join(scratch, 'wbr-'));
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
    const base = 'http://localhost/api/v1/custom-tools';
    const url = c.action ? `${base}?action=${c.action}` : base;
    let response: { status: number; json: () => Promise<unknown> };
    if (c.method === 'GET') {
      const { GET } = await import('@/app/api/v1/custom-tools/route');
      response = (await GET(mockRequest(url) as never, {} as never)) as never;
    } else {
      const { POST } = await import('@/app/api/v1/custom-tools/route');
      response = (await POST(mockRequest(url, bodyFor(c, corpus)) as never, {} as never)) as never;
    }
    return { name: c.name, status: response.status, body: await response.json() };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const fixturesDir = join(here, '..', 'fixtures');
  const spec = JSON.parse(fs.readFileSync(join(fixturesDir, 'workbench.json'), 'utf8')) as Spec;
  const corpus = JSON.parse(
    fs.readFileSync(join(fixturesDir, 'workbench-route-cases.json'), 'utf8'),
  ) as Corpus;

  const fixtures = {
    main: process.env.QT_FIXTURE_WORKBENCH_MAIN ?? '',
    mount: process.env.QT_FIXTURE_WORKBENCH_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-workbench-route-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const out = fs.createWriteStream(outPath);
  for (const c of corpus.cases) {
    const row = await runCase(spec, c, corpus, scratch, fixtures);
    out.write(JSON.stringify(row) + '\n');
  }
  out.end();
  await new Promise((r) => out.on('finish', r));
  rmSync(scratch, { recursive: true, force: true });
}

it('emits the pascal workbench route oracle', async () => {
  await main();
}, 300000);
