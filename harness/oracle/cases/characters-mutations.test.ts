/**
 * @jest-environment node
 *
 * P4.6f (slice 4) characters MUTATIONS ORACLE: drives v4's REAL create /
 * quick-create (`characters/handlers/post.ts`) and update
 * (`characters/[id]/handlers/put.ts`) handlers over a FRESH copy of the committed
 * characters fixture per case, and emits each response body (the create/update
 * ECHO). The Rust ports (`api::characters::{character_create,
 * character_quick_create, character_update}`) are diffed against these echoes.
 *
 * The echo is a full OVERLAY re-read of the freshly-written vault
 * (v4 `create` reloads via `findById`; `update` returns
 * `applyDocumentStoreOverlayOne`), so the echo diff transitively proves the vault
 * round-trip in composition — every managed field (identity/description/
 * scenarios/systemPrompts/physicalDescription/…) is read back from the vault the
 * handler just wrote. The raw storage rows themselves are already byte-proven by
 * the standing `characters_create_tier2` / `vault_character_update` tier-2
 * differentials, so they are not re-dumped here.
 *
 * Minted ids + timestamps (the created character id, its mount-point FK, and the
 * per-array scenario/prompt/physical ids/timestamps) are position-blanked on both
 * sides (keys `id` / `createdAt` / `updatedAt` / `characterDocumentMountPointId`);
 * everything else is diffed byte-exact.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-characters-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/characters-mutations.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/characters.json"           "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARACTERS_MAIN=$V5W/crates/quilltap-web/tests/fixtures/characters-main.db \
 *   QT_FIXTURE_CHARACTERS_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/characters-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-characters-mutations.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- characters-mutations
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
const CONN = 'c0000001-0000-4000-8000-000000000001';

interface CaseSpec {
  name: string;
  kind: 'create' | 'quick-create' | 'update';
  id?: string;
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
  // The update handler calls `revalidatePath('/')` (Next.js ISR cache), which
  // throws outside a Next request context (jest) — a harness artifact, not a v4
  // behavior. Stub it to a no-op so the handler completes normally.
  jest.doMock(
    'next/cache',
    () => ({
      __esModule: true,
      revalidatePath: () => {},
      revalidateTag: () => {},
    }),
    { virtual: true },
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

  await initializeDatabase();

  try {
    let status: number;
    let body: unknown;
    if (c.kind === 'update') {
      const url = `http://localhost/api/v1/characters/${c.id}`;
      const { PUT } = (await import('@/app/api/v1/characters/[id]/route')) as {
        PUT: (...a: unknown[]) => Promise<unknown>;
      };
      const response = (await PUT(mockRequest(url, c.body), {
        params: Promise.resolve({ id: c.id }),
      })) as { status: number; json: () => Promise<unknown> };
      status = response.status;
      body = await response.json();
    } else {
      const action = c.kind === 'quick-create' ? '?action=quick-create' : '';
      const url = `http://localhost/api/v1/characters${action}`;
      const { POST } = (await import('@/app/api/v1/characters/route')) as {
        POST: (...a: unknown[]) => Promise<unknown>;
      };
      const response = (await POST(mockRequest(url, c.body))) as {
        status: number;
        json: () => Promise<unknown>;
      };
      status = response.status;
      body = await response.json();
    }
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-characters-mutations-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cases: CaseSpec[] = [
    {
      name: 'create_full',
      kind: 'create',
      body: {
        name: 'Fenwick',
        title: 'The Cartographer',
        identity: 'A wandering mapmaker.',
        description: 'Ink-stained and curious.',
        manifesto: 'Every coast deserves a name.',
        personality: 'Meticulous, wry.',
        firstMessage: 'Ah — a fellow traveller.',
        exampleDialogues: 'User: Where to?\nFenwick: North, always north.',
        controlledBy: 'llm',
        npc: false,
        defaultConnectionProfileId: CONN,
        scenarios: [
          { title: 'The Harbor', content: 'Fog rolls over the docks.' },
          { title: 'The Summit', content: 'The last ridge before the map ends.' },
        ],
        systemPrompts: [
          {
            id: 'd0000001-0000-4000-8000-000000000001',
            name: 'Terse',
            content: 'Answer in few words.',
            isDefault: true,
            createdAt: '2020-01-01T00:00:00.000Z',
            updatedAt: '2020-01-01T00:00:00.000Z',
          },
        ],
        physicalDescription: {
          id: 'e0000001-0000-4000-8000-000000000001',
          name: 'Fenwick body',
          shortPrompt: 'weathered coat',
          mediumPrompt: 'a weathered coat and a brass spyglass',
          fullDescription: 'Tall, lean, ink on the fingers.',
          createdAt: '2020-01-01T00:00:00.000Z',
          updatedAt: '2020-01-01T00:00:00.000Z',
        },
      },
    },
    { name: 'create_minimal', kind: 'create', body: { name: 'Mimsy' } },
    { name: 'quick_create', kind: 'quick-create', body: { name: 'Quill' } },
    {
      name: 'update_managed',
      kind: 'update',
      id: ARIA,
      body: {
        title: 'Renamed Title',
        identity: 'A rewritten identity.',
        description: 'A rewritten description.',
        personality: 'Newly bold.',
        firstMessage: 'A fresh greeting.',
        scenarios: [{ title: 'Only Scenario', content: 'The reprojected scene.' }],
        physicalDescription: {
          name: 'Aria body v2',
          shortPrompt: 'silver cloak',
          longPrompt: 'a flowing silver cloak with embroidered stars',
        },
      },
    },
    {
      name: 'update_slim',
      kind: 'update',
      id: ARIA,
      body: {
        name: 'Aria Reforged',
        controlledBy: 'user',
        npc: true,
        canBeCarina: false,
      },
    },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(
    `characters-mutations oracle wrote ${outPath} (${outLines.length} cases)\n`,
  );
}

test('characters-mutations oracle', async () => {
  await main();
});
