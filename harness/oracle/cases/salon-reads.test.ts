/**
 * @jest-environment node
 *
 * P4.6a read-surface ORACLE: drives v4's REAL route handlers for the three Salon
 * reads — the chat-settings GET, the enriched chat list (`handleList`), and the
 * single-chat GET (`handleGet`) — over a fresh copy of the committed salon fixture,
 * and emits each response body so the Rust ports (`api::salon::{chat_settings,
 * list_chats, chat_get}`) can be diffed byte-for-byte (identical baked ids on both
 * sides → no remap).
 *
 * Only the seams the Rust harness also neutralizes are mocked: the auth session
 * (getServerSession → the fixture user), the startup gate (forced ready), and the
 * markdown pre-render (`renderMarkdownToHtml` → null — the port OMITS `renderedHtml`,
 * the locked divergence; the harness strips it from the v4 side). The DB stack is
 * doMocked to the REAL modules (past jest.setup) + the real cipher binding, so the
 * reads run genuinely against a FRESH copy of the baked fixture per case.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-salon-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/salon-reads.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/salon.json"       "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SALON_MAIN=$V5W/crates/quilltap-web/tests/fixtures/salon-main.db \
 *   QT_FIXTURE_SALON_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/salon-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-salon-reads.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- salon-reads
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  tags?: Array<{ id: string; name: string }>;
  chats: Array<{ id: string; tags?: string[] }>;
}

interface CaseSpec {
  name: string;
  kind: 'settings' | 'list' | 'get';
  url: string;
  chatId?: string;
  /** P4.D60 (bug 51): inject live impersonation state into the fresh fixture copy
   * before the GET, so the projection exercises the non-default (`[]`/`null`) arm.
   * Written as a raw UPDATE on the `chats` row (both differential sides mirror it). */
  setImpersonation?: {
    chatId: string;
    impersonatingParticipantIds: string[];
    activeTypingParticipantId: string | null;
  };
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
  // The port OMITS renderedHtml (locked divergence — the renderer is npm-native
  // ESM that jest can't compile). Fully stub the module so `canPreRenderMessage`
  // is false → renderedHtml stays null (stripped from the v4 side by the harness).
  jest.doMock('@/lib/services/markdown-renderer.service', () => ({
    __esModule: true,
    renderMarkdownToHtml: async () => null,
    canPreRenderMessage: () => false,
  }));

  // Fresh fixture copy → case independence.
  const work = mkdtempSync(join(scratch, 'salon-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );

  await initializeDatabase();

  // P4.D60 (bug 51): inject live impersonation state so the GET projection
  // exercises the non-default arm (the columns are stored as JSON string / text).
  if (c.setImpersonation) {
    rawQuery('UPDATE "chats" SET "impersonatingParticipantIds" = ?, "activeTypingParticipantId" = ? WHERE "id" = ?', [
      JSON.stringify(c.setImpersonation.impersonatingParticipantIds),
      c.setImpersonation.activeTypingParticipantId,
      c.setImpersonation.chatId,
    ]);
  }

  try {
    let response: { status: number; json: () => Promise<unknown> };
    if (c.kind === 'settings') {
      const { GET } = await import('@/app/api/v1/settings/chat/route');
      response = (await GET(mockRequest(c.url) as never)) as never;
    } else if (c.kind === 'list') {
      const { GET } = await import('@/app/api/v1/chats/route');
      response = (await GET(mockRequest(c.url) as never)) as never;
    } else {
      const { GET } = await import('@/app/api/v1/chats/[id]/route');
      response = (await GET(mockRequest(c.url) as never, {
        params: Promise.resolve({ id: c.chatId }),
      } as never)) as never;
    }
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-salon-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const soloId = spec.chats[0].id;
  const groupId = spec.chats[1].id;
  const tag = spec.chats[0].tags?.[0] ?? '';
  // P4.65: the "Mystery" tag lives ONLY on a character (Elm, in the third
  // chat, which carries no chat-level tag) — excluding it exercises the
  // participant half of `_allTagIds` in isolation.
  const characterTag = spec.tags?.[1]?.id ?? '';
  const cases: CaseSpec[] = [
    { name: 'settings', kind: 'settings', url: 'http://localhost/api/v1/settings/chat' },
    { name: 'list_all', kind: 'list', url: 'http://localhost/api/v1/chats' },
    {
      name: 'list_exclude_tag',
      kind: 'list',
      url: `http://localhost/api/v1/chats?excludeTagIds=${tag}`,
    },
    {
      name: 'list_exclude_character_tag',
      kind: 'list',
      url: `http://localhost/api/v1/chats?excludeTagIds=${characterTag}`,
    },
    { name: 'list_limit1', kind: 'list', url: 'http://localhost/api/v1/chats?limit=1' },
    { name: 'get_solo', kind: 'get', url: `http://localhost/api/v1/chats/${soloId}`, chatId: soloId },
    {
      name: 'get_group',
      kind: 'get',
      url: `http://localhost/api/v1/chats/${groupId}`,
      chatId: groupId,
    },
    // P4.D60 (bug 51): the group chat with live impersonation state — the human
    // impersonates its first (LLM) seat, so the GET must project the seat in
    // `impersonatingParticipantIds` + `activeTypingParticipantId`.
    {
      name: 'get_impersonated',
      kind: 'get',
      url: `http://localhost/api/v1/chats/${groupId}`,
      chatId: groupId,
      setImpersonation: {
        chatId: groupId,
        impersonatingParticipantIds: ['b2000000-0000-4000-8000-000000000001'],
        activeTypingParticipantId: 'b2000000-0000-4000-8000-000000000001',
      },
    },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`salon-reads oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('salon-reads oracle', async () => {
  await main();
});
