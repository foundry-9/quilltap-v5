/**
 * @jest-environment node
 *
 * P4.9E3B per-message reattribute ORACLE: drives v4's REAL
 * `POST /api/v1/messages/[id]?action=reattribute`
 * (`app/api/v1/messages/[id]/route.ts` `handleReattributeAction`) over a FRESH
 * copy of the committed chat-dialogs fixture per case, and emits each response
 * body plus post-mutation table dumps (SR_CHAT's chat row + messages + every
 * memory) so the Rust port (`services::message_reattribute`) diffs
 * byte-for-byte.
 *
 * **Must run under `TZ=UTC`.**
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-cd-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/chat-dialogs-reattribute.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/chat-dialogs-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/chat-dialogs-main.db \
 *   QT_FIXTURE_CD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/chat-dialogs-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-message-reattribute.ndjson TZ=UTC \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- chat-dialogs-reattribute
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  frozenNowMs: number;
}

const SR_CHAT = 'c2000000-0000-4000-8000-000000000003';
const R1 = 'd5000000-0000-4000-8000-000000000001';
const R4 = 'd5000000-0000-4000-8000-000000000004';
const P3_PIP = 'e2000000-0000-4000-8000-000000000012';
const P3_VERA = 'e2000000-0000-4000-8000-000000000013';
// A participant of a DIFFERENT chat — "not found in THIS chat" with a
// well-formed uuid.
const P_NORA_EXPORT = 'e2000000-0000-4000-8000-000000000001';
const MISSING_ID = '99999999-9999-4999-8999-999999999999';

const RealDate = Date;

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
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
  );
  jest.doMock('@/lib/mount-index/character-vault', () =>
    jest.requireActual('@/lib/mount-index/character-vault'),
  );
  jest.doMock('@/lib/background-jobs/processor', () => ({
    __esModule: true,
    ensureProcessorRunning: () => {},
    startProcessor: () => {},
    stopProcessor: () => {},
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

async function readTables(): Promise<unknown> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const repos = getRepositories();
  const chat = (await repos.chats.findById(SR_CHAT)) ?? null;
  const messages = await repos.chats.getMessages(SR_CHAT);
  const memories = ((await repos.memories.findAll()) as Array<Record<string, unknown>>)
    .map((m) => ({
      id: m.id,
      characterId: m.characterId,
      chatId: m.chatId ?? null,
      sourceMessageId: m.sourceMessageId ?? null,
    }))
    .sort((a, b) => String(a.id).localeCompare(String(b.id)));
  return { chat, messages, memories };
}

interface CaseSpec {
  name: string;
  messageId: string;
  body: unknown;
  dump?: boolean;
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'ra-'));
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

  let tick = 0;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  global.Date = class extends RealDate {
    constructor(...a: unknown[]) {
      if (a.length === 0) super(spec.frozenNowMs + tick++);
      // @ts-expect-error forward variadic args
      else super(...a);
    }
    static now(): number {
      return spec.frozenNowMs + tick++;
    }
  } as unknown as DateConstructor;

  try {
    const route = (await import('@/app/api/v1/messages/[id]/route')) as never as Record<
      string,
      (...a: unknown[]) => Promise<unknown>
    >;
    const resp = (await route.POST(
      mockRequest(
        `http://localhost/api/v1/messages/${c.messageId}?action=reattribute`,
        c.body,
      ),
      { params: Promise.resolve({ id: c.messageId }) },
    )) as { status: number; body?: unknown; json?: () => Promise<unknown> };
    const body =
      resp.body !== undefined && typeof resp.body === 'object'
        ? resp.body
        : await (resp as { json: () => Promise<unknown> }).json();
    return {
      name: c.name,
      status: resp.status,
      body,
      ...(c.dump ? { tables: await readTables() } : {}),
    };
  } finally {
    global.Date = RealDate;
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(
      `chat-dialogs-reattribute oracle must run under TZ=UTC (getTimezoneOffset=${offset})`,
    );
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'chat-dialogs-web.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_CD_MAIN ?? '',
    mount: process.env.QT_FIXTURE_CD_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ra-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cases: CaseSpec[] = [
    // R4 carries TWO sourced memories — both deleted; the row's participantId
    // moves to Pip; the chat's updatedAt is PRESERVED (v4's `update({})`).
    {
      name: 'reattribute_with_memories',
      messageId: R4,
      body: { newParticipantId: P3_PIP },
      dump: true,
    },
    {
      name: 'reattribute_no_memories',
      messageId: R1,
      body: { newParticipantId: P3_VERA },
      dump: true,
    },
    {
      name: 'reattribute_target_not_in_chat',
      messageId: R4,
      body: { newParticipantId: P_NORA_EXPORT },
      dump: true,
    },
    {
      name: 'reattribute_message_missing',
      messageId: MISSING_ID,
      body: { newParticipantId: P3_PIP },
    },
    {
      name: 'reattribute_bad_uuid',
      messageId: R4,
      body: { newParticipantId: 'not-a-uuid' },
    },
  ];

  const lines: string[] = [];
  for (const c of cases) {
    lines.push(JSON.stringify(await runCase(spec, c, scratch, fixtures)));
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(
    `wrote ${lines.length} chat-dialogs-reattribute oracle rows to ${outPath}\n`,
  );
}

test('chat-dialogs-reattribute oracle', async () => {
  await main();
});
