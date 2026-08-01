/**
 * @jest-environment node
 *
 * P4.6c skipUserTurn ORACLE (the minted-values sibling of `salon-mutations`):
 * drives v4's REAL `handleTurnAction` `skipUserTurn` over a FRESH copy of the
 * committed salon GROUP fixture per case, emitting the response body + the MAIN-db
 * `chats` / `chat_messages` dumps so the Rust port (`api::salon::turn_action`) can
 * be diffed. Unlike `salon-mutations` (zero-mint), the successful skip posts a Host
 * turn-pass announcement (minted id + timestamps), so the Rust harness normalizes:
 * the minted turn-pass row's id → `<HOSTPASS>`, its createdAt + the chats
 * lastMessageAt/updatedAt bumps → `<ts>`.
 *
 * Two branches:
 *   - `skip_success`: a user-controlled participant (Cleo) skips in the group scene
 *     (`qualifiesForTurnSkipping` = 3 active chars) with no prior passes → the skip
 *     applies: a Host turn-pass row lands, `spokenThisCycle` folds, the turn queue +
 *     `lastTurnParticipantId` persist.
 *   - `skip_refusal`: two Host turn-pass rows for the LLM participants (Aria + Bram)
 *     are seeded after the last substantive message, so `all-others-skipped` fires
 *     and v4 refuses the human skip with the exact copy. The refusal writes nothing.
 *
 * Same seams as `salon-mutations` (auth, startup gate, Math.random, the real cipher
 * binding + vector store past jest.setup).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-salon-skip-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp $V5W/harness/oracle/cases/salon-skip.test.ts "$TMPO/cases/"
 *   cp $V5W/harness/oracle/fixtures/salon.json      "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SALON_MAIN=$V5W/crates/quilltap-web/tests/fixtures/salon-main.db \
 *   QT_FIXTURE_SALON_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/salon-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-salon-skip.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- salon-skip
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

interface SeedTurnPass {
  id: string;
  participantId: string; // the passing (LLM) participant, into hostEvent
  createdAt: string;
}

interface CaseSpec {
  name: string;
  paramId: string;
  body: Record<string, unknown>;
  random01: number;
  seed?: SeedTurnPass[];
}

const TABLES = [
  { key: 'chats', table: 'chats' },
  { key: 'chatMessages', table: 'chat_messages' },
];

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'POST',
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
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
  );
  jest.doMock('@/lib/services/markdown-renderer.service', () => ({
    __esModule: true,
    renderMarkdownToHtml: async () => null,
    canPreRenderMessage: () => false,
  }));
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

  const work = mkdtempSync(join(scratch, 'salon-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const origRandom = Math.random;
  Math.random = () => c.random01;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite');
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();

  try {
    // Seed the prior Host turn-pass records (refusal case) via the REAL repo, with
    // explicit ids + createdAt AFTER the last substantive message so the walk-back
    // stall guard sees them.
    if (c.seed && c.seed.length > 0) {
      const repos = getRepositories();
      await repos.chats.addMessages(
        c.paramId,
        c.seed.map((s) => ({
          id: s.id,
          type: 'message',
          role: 'ASSISTANT',
          content: 'A pass, quietly noted.',
          participantId: undefined,
          createdAt: s.createdAt,
          attachments: [],
          systemSender: 'host',
          systemKind: 'turn-pass',
          hostEvent: { participantId: s.participantId },
        })) as never,
      );
    }

    const url = `http://localhost/api/v1/chats/${c.paramId}?action=turn`;
    const params = { params: Promise.resolve({ id: c.paramId }) };
    const { POST } = await import('@/app/api/v1/chats/[id]/route');
    const response = (await POST(mockRequest(url, c.body) as never, params as never)) as {
      status: number;
      json: () => Promise<unknown>;
    };
    const status = response.status;
    const body = await response.json();

    const mdb = getRawDatabase();
    if (!mdb) throw new Error('main DB handle unavailable');
    const tables: Record<string, unknown> = {};
    for (const t of TABLES) {
      const columns = (
        mdb.prepare(`PRAGMA table_info(${t.table})`).all() as Array<{ name: string }>
      ).map((col) => col.name);
      const rawRows = mdb.prepare(`SELECT * FROM ${t.table}`).all() as Array<
        Record<string, unknown>
      >;
      const rows = rawRows.map((r) => {
        const out: Record<string, unknown> = {};
        for (const col of columns) out[col] = canonValue(r[col]);
        return out;
      });
      tables[t.key] = { table: t.table, columns, rows };
    }
    return { name: c.name, status, body, tables };
  } finally {
    Math.random = origRandom;
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'salon.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_SALON_MAIN ?? '',
    mount: process.env.QT_FIXTURE_SALON_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-salon-skip-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const GROUP = 'c1000000-0000-4000-8000-000000000002';
  const USER_P = 'b2000000-0000-4000-8000-000000000003'; // Cleo (controlledBy user)
  const ARIA_P = 'b2000000-0000-4000-8000-000000000001'; // Aria (llm)
  const BRAM_P = 'b2000000-0000-4000-8000-000000000002'; // Bram (llm)

  const cases: CaseSpec[] = [
    {
      name: 'skip_success',
      paramId: GROUP,
      body: { action: 'skipUserTurn', participantId: USER_P },
      random01: 0.42,
    },
    {
      name: 'skip_refusal',
      paramId: GROUP,
      body: { action: 'skipUserTurn', participantId: USER_P },
      random01: 0.42,
      seed: [
        { id: 'aa000000-0000-4000-8000-000000000001', participantId: ARIA_P, createdAt: '2026-02-02T00:00:00.000Z' },
        { id: 'aa000000-0000-4000-8000-000000000002', participantId: BRAM_P, createdAt: '2026-02-03T00:00:00.000Z' },
      ],
    },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`salon-skip oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('salon-skip oracle', async () => {
  await main();
});
