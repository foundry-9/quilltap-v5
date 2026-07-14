/**
 * @jest-environment node
 *
 * P4.6ak TEXT-REPLACEMENTS + CHAT-GET-BACKGROUND route-surface ORACLE: drives
 * v4's REAL route handlers over a FRESH copy of the committed
 * text-replacements fixture per case, and emits each response {status, body} so
 * the Rust ports (`api::text_replacements::*` + `api::chat_media::
 * chat_get_background`) diff byte-for-byte.
 *
 * Cases: list (seed, ordered sortOrder-then-createdAt), create (+ conflict),
 * patch (+ missing + conflict), delete (204) (+ missing), bulk-replace (+ then
 * list ordering + list-empty), and the three get-background arms (+ chat-missing
 * 404). Minted ids/timestamps on create/bulk are blanked by the Rust harness;
 * the patch case blanks only the minted updatedAt.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-tr-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/text-replacements-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/text-replacements-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TR_MAIN=$V5W/crates/quilltap-web/tests/fixtures/text-replacements-main.db \
 *   QT_FIXTURE_TR_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/text-replacements-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-text-replacements.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- text-replacements-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  chatWithBackgroundId: string;
  chatNoBackgroundId: string;
  chatMissingFileId: string;
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

function applyMocks(spec: Spec): void {
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
  // The chat GET handler pulls the markdown renderer (unified/ESM) — mock it away
  // (the get-background body isn't rendered; the port omits renderedHtml).
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
}

async function loadRoute(
  path: string,
): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
  return (await import(path)) as never;
}

/** Read the response, tolerating an empty 204 body (v4 `noContent`). */
async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  if (resp.status === 204) return { status: 204, body: null };
  return { status: resp.status, body: await resp.json() };
}

const SETTINGS_ROUTE = '@/app/api/v1/settings/text-replacements/route';
const SETTINGS_ID_ROUTE = '@/app/api/v1/settings/text-replacements/[id]/route';
const CHAT_GET_ROUTE = '@/app/api/v1/chats/[id]/route';

// Pinned ids from the spec (shared verbatim with the Rust differential).
const R1 = '1a000000-0000-4000-8000-0000000000a1'; // teh (ci, so0, 01-01)
const R3 = '1a000000-0000-4000-8000-0000000000a3'; // dont (ci, so0, 01-03)
const MISSING = '99999999-9999-4999-8999-999999999999';

interface CaseSpec {
  name: string;
  blank?: string[];
  run: () => Promise<{ status: number; body: unknown }>;
}

const B = 'http://localhost/api/v1';

function buildCases(spec: Spec): CaseSpec[] {
  return [
    // ── Text-replacement rules ──
    {
      name: 'list',
      run: async () => respond(await (await loadRoute(SETTINGS_ROUTE)).GET(mockRequest(`${B}/settings/text-replacements`))),
    },
    {
      name: 'create',
      blank: ['id', 'createdAt', 'updatedAt'],
      run: async () =>
        respond(
          await (await loadRoute(SETTINGS_ROUTE)).POST(
            mockRequest(`${B}/settings/text-replacements`, {
              fromText: 'recieve',
              toText: 'receive',
              caseSensitive: false,
              sortOrder: 5,
            }),
          ),
        ),
    },
    {
      name: 'create_conflict',
      run: async () =>
        respond(
          await (await loadRoute(SETTINGS_ROUTE)).POST(
            mockRequest(`${B}/settings/text-replacements`, {
              fromText: 'TEH',
              toText: 'the',
              caseSensitive: false,
            }),
          ),
        ),
    },
    {
      name: 'patch',
      blank: ['updatedAt'],
      run: async () =>
        respond(
          await (await loadRoute(SETTINGS_ID_ROUTE)).PATCH(
            mockRequest(`${B}/settings/text-replacements/${R1}`, { toText: 'THE', enabled: false }),
            { params: Promise.resolve({ id: R1 }) },
          ),
        ),
    },
    {
      name: 'patch_missing',
      run: async () =>
        respond(
          await (await loadRoute(SETTINGS_ID_ROUTE)).PATCH(
            mockRequest(`${B}/settings/text-replacements/${MISSING}`, { toText: 'x' }),
            { params: Promise.resolve({ id: MISSING }) },
          ),
        ),
    },
    {
      name: 'patch_conflict',
      run: async () =>
        respond(
          await (await loadRoute(SETTINGS_ID_ROUTE)).PATCH(
            mockRequest(`${B}/settings/text-replacements/${R3}`, {
              fromText: 'adn',
              caseSensitive: true,
            }),
            { params: Promise.resolve({ id: R3 }) },
          ),
        ),
    },
    {
      name: 'delete',
      run: async () =>
        respond(
          await (await loadRoute(SETTINGS_ID_ROUTE)).DELETE(
            mockRequest(`${B}/settings/text-replacements/${R1}`),
            { params: Promise.resolve({ id: R1 }) },
          ),
        ),
    },
    {
      name: 'delete_missing',
      run: async () =>
        respond(
          await (await loadRoute(SETTINGS_ID_ROUTE)).DELETE(
            mockRequest(`${B}/settings/text-replacements/${MISSING}`),
            { params: Promise.resolve({ id: MISSING }) },
          ),
        ),
    },
    {
      name: 'bulk_replace',
      blank: ['id', 'createdAt', 'updatedAt'],
      run: async () =>
        respond(
          await (await loadRoute(SETTINGS_ROUTE)).POST(
            mockRequest(`${B}/settings/text-replacements?action=bulk-replace`, {
              rules: [
                { fromText: 'alpha', toText: 'ALPHA', sortOrder: 2 },
                { fromText: 'bravo', toText: 'BRAVO', caseSensitive: true, sortOrder: 0 },
                { fromText: 'charlie', toText: 'CHARLIE', enabled: false, sortOrder: 1 },
              ],
            }),
          ),
        ),
    },
    {
      // bulk-replace, THEN list — proves the transactional replace + the list
      // resort (input order 2/0/1 → list order 0/1/2). Minted ids/ts blanked.
      name: 'bulk_then_list',
      blank: ['id', 'createdAt', 'updatedAt'],
      run: async () => {
        const settings = await loadRoute(SETTINGS_ROUTE);
        await settings.POST(
          mockRequest(`${B}/settings/text-replacements?action=bulk-replace`, {
            rules: [
              { fromText: 'alpha', toText: 'ALPHA', sortOrder: 2 },
              { fromText: 'bravo', toText: 'BRAVO', sortOrder: 0 },
              { fromText: 'charlie', toText: 'CHARLIE', sortOrder: 1 },
            ],
          }),
        );
        return respond(await settings.GET(mockRequest(`${B}/settings/text-replacements`)));
      },
    },
    {
      // bulk-replace to EMPTY, then list → {rules:[], count:0}.
      name: 'list_empty',
      run: async () => {
        const settings = await loadRoute(SETTINGS_ROUTE);
        await settings.POST(
          mockRequest(`${B}/settings/text-replacements?action=bulk-replace`, { rules: [] }),
        );
        return respond(await settings.GET(mockRequest(`${B}/settings/text-replacements`)));
      },
    },
    // ── chatGetBackground ──
    {
      name: 'bg_resolved',
      run: async () =>
        respond(
          await (await loadRoute(CHAT_GET_ROUTE)).GET(
            mockRequest(`${B}/chats/${spec.chatWithBackgroundId}?action=get-background`),
            { params: Promise.resolve({ id: spec.chatWithBackgroundId }) },
          ),
        ),
    },
    {
      name: 'bg_no_id',
      run: async () =>
        respond(
          await (await loadRoute(CHAT_GET_ROUTE)).GET(
            mockRequest(`${B}/chats/${spec.chatNoBackgroundId}?action=get-background`),
            { params: Promise.resolve({ id: spec.chatNoBackgroundId }) },
          ),
        ),
    },
    {
      name: 'bg_file_missing',
      run: async () =>
        respond(
          await (await loadRoute(CHAT_GET_ROUTE)).GET(
            mockRequest(`${B}/chats/${spec.chatMissingFileId}?action=get-background`),
            { params: Promise.resolve({ id: spec.chatMissingFileId }) },
          ),
        ),
    },
    {
      name: 'bg_chat_missing',
      run: async () =>
        respond(
          await (await loadRoute(CHAT_GET_ROUTE)).GET(
            mockRequest(`${B}/chats/${MISSING}?action=get-background`),
            { params: Promise.resolve({ id: MISSING }) },
          ),
        ),
    },
  ];
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'tr-'));
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
    const out = await c.run();
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      ...(c.blank ? { blank: c.blank } : {}),
    };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'text-replacements-web.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_TR_MAIN ?? '',
    mount: process.env.QT_FIXTURE_TR_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-tr-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const outLines: string[] = [];
  for (const c of buildCases(spec)) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`text-replacements-routes oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('text-replacements-routes oracle', async () => {
  await main();
});
