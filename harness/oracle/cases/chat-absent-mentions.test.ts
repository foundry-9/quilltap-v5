/**
 * @jest-environment node
 *
 * P4.D347 CHAT-ABSENT-MENTIONS ORACLE: drives v4's REAL
 * `GET /api/v1/chats/[id]?action=absent-mentions` (`9a88f862f` —
 * `app/api/v1/chats/[id]/actions/absent-mentions.ts`'s `handleGetAbsentMentions`,
 * the function the chat GET dispatcher's thunk calls) over a FRESH copy of the
 * widened build-context pair per case, and emits each `(status, body)` so the
 * Rust port (`api::chat_absent_mentions::chat_absent_mentions`) diffs
 * byte-for-byte, key order included.
 *
 * Each case's plant SQL is EMITTED with its row, so the Rust side applies the
 * very same statements to its own copy (no second transcription).
 *
 * Cases: two absent characters first named in one message (one by her alias —
 * `text` is the alias as written — and the pending list's REVERSE order within a
 * message), a later repeat (not a second mention) and a Staff row (skipped);
 * only seated characters named (`{"mentions":[]}`); the persona named in a Salon
 * (in the room by name) and in an autonomous room (absent — bug 172); a missing
 * chat (`notFound('Chat')`). v4's `[Chats v1] Read absent-character mentions`
 * DEBUG is captured off the logger singleton per case (a 404 logs none).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-cam-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/chat-absent-mentions.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/build-context-tier3.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   TZ=UTC QT_FIXTURE_OUT=/tmp/qt-build-context-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-build-context-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-context-tier3-fixture.ts
 *   TZ=UTC QT_FIXTURE_BC_MAIN=/tmp/qt-build-context-main.db \
 *   QT_FIXTURE_BC_MOUNT=/tmp/qt-build-context-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-chat-absent-mentions.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "chat-absent-mentions\.test\.ts$"
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

const CHAT = 'cc000000-0000-4000-8000-0000000000c1';
const NO_CHAT = 'cc000000-0000-4000-8000-0000000000ff';
const ADA = 'ca000000-0000-4000-8000-0000000000a1';
const BEA = 'ca000000-0000-4000-8000-0000000000a2';

function msg(id: string, role: string, content: string, ts: string, extra = ''): string {
  const q = content.replace(/'/g, "''");
  const cols = extra ? ', systemSender, systemKind' : '';
  const vals = extra ? `, ${extra}` : '';
  return `INSERT INTO chat_messages (id, chatId, type, role, content, attachments, createdAt, participantId${cols}) VALUES ('${id}', '${CHAT}', 'message', '${role}', '${q}', '[]', '${ts}', NULL${vals})`;
}

function seat(id: string, characterId: string, controlledBy: string): Record<string, unknown> {
  return {
    id,
    type: 'CHARACTER',
    characterId,
    controlledBy,
    status: 'active',
    hasHistoryAccess: true,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
  };
}

interface CaseSpec {
  name: string;
  chatId: string;
  plantSql: string[];
}

const CASES: CaseSpec[] = [
  {
    name: 'two_absent_one_by_alias',
    chatId: CHAT,
    plantSql: [
      msg('e3471000-0000-4000-8000-000000000001', 'USER', 'Abby and Bea went out to the hives.', '2024-06-16T10:00:00.000Z'),
      msg('e3471000-0000-4000-8000-000000000002', 'ASSISTANT', 'Abigail came back alone; Bea stayed with the bees.', '2024-06-16T10:01:00.000Z'),
      msg('e3471000-0000-4000-8000-000000000003', 'ASSISTANT', 'Ada is named here, but by the Host.', '2024-06-16T10:02:00.000Z', "'host', 'timestamp'"),
    ],
  },
  {
    name: 'only_seated_named',
    chatId: CHAT,
    plantSql: [
      `UPDATE chats SET participants = '${JSON.stringify([
        seat('ba000000-0000-4000-8000-0000000000f1', ADA, 'llm'),
        seat('ba000000-0000-4000-8000-0000000000f2', BEA, 'llm'),
      ])}' WHERE id = '${CHAT}'`,
      msg('e3471000-0000-4000-8000-000000000011', 'USER', 'Ada and Bea, a word.', '2024-06-16T10:00:00.000Z'),
    ],
  },
  {
    name: 'persona_named_in_a_salon',
    chatId: CHAT,
    plantSql: [
      `UPDATE chats SET "chatType" = 'salon' WHERE id = '${CHAT}'`,
      msg('e3471000-0000-4000-8000-000000000021', 'ASSISTANT', 'Charlie, you are late again.', '2024-06-16T10:00:00.000Z'),
    ],
  },
  {
    name: 'persona_named_in_an_autonomous_room',
    chatId: CHAT,
    plantSql: [
      `UPDATE chats SET "chatType" = 'autonomous' WHERE id = '${CHAT}'`,
      msg('e3471000-0000-4000-8000-000000000031', 'ASSISTANT', 'Charlie, you are late again.', '2024-06-16T10:00:00.000Z'),
    ],
  },
  { name: 'missing_chat', chatId: NO_CHAT, plantSql: [] },
];

function applyMocks(spec: Spec): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () => jest.requireActual('@/lib/embedding/vector-store'));
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
  );
  // The chat GET handler module graph pulls the markdown renderer (ESM).
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

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'cam-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
  await initializeDatabase();
  for (const sql of c.plantSql) await rawQuery(sql);

  const { logger } = await import('@/lib/logger');
  const lines: Array<{ level: string; message: string; fields: unknown }> = [];
  const priorDebug = (logger as unknown as { debug: (...a: unknown[]) => unknown }).debug;
  (logger as unknown as { debug: unknown }).debug = (message: string, fields?: unknown) => {
    if (typeof message === 'string' && message.startsWith('[Chats v1] Read absent-character mentions')) {
      lines.push(JSON.parse(JSON.stringify({ level: 'debug', message, fields: fields ?? {} })));
    }
    return priorDebug.call(logger, message, fields);
  };

  try {
    // The REAL handler the chat GET dispatcher's `'absent-mentions'` thunk
    // calls, with the RequestContext the middleware would build (the widened
    // build-context pair carries no `users` row, so the middleware's user
    // lookup is bypassed — the handler reads only `user.id` and `repos`).
    const { handleGetAbsentMentions } = (await import(
      '@/app/api/v1/chats/[id]/actions/absent-mentions'
    )) as unknown as {
      handleGetAbsentMentions: (
        chatId: string,
        ctx: unknown,
      ) => Promise<{ status: number; json: () => Promise<unknown> }>;
    };
    const { getRepositories } = await import('@/lib/repositories/factory');
    const resp = await handleGetAbsentMentions(c.chatId, {
      user: { id: spec.userId },
      repos: getRepositories(),
    });
    // `JSON.stringify` of the parsed body keeps v4's construction key order.
    const raw = JSON.stringify(await resp.json());
    const llmLogs = (await rawQuery('SELECT COUNT(*) AS n FROM llm_logs').catch(() => [{ n: 0 }])) as Array<{ n: number }>;
    if ((llmLogs[0]?.n ?? 0) !== 0) throw new Error(`${c.name}: the oracle wrote llm_logs rows`);
    return { name: c.name, chatId: c.chatId, plantSql: c.plantSql, status: resp.status, raw, logs: lines };
  } finally {
    (logger as unknown as { debug: unknown }).debug = priorDebug;
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'build-context-tier3.json'), 'utf8'),
  ) as Spec;
  const fixtures = {
    main: process.env.QT_FIXTURE_BC_MAIN ?? '',
    mount: process.env.QT_FIXTURE_BC_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cam-oracle-'));
  try {
    mkdirSync(join(scratch, 'data'), { recursive: true });
    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    const out: string[] = [];
    for (const c of CASES) out.push(JSON.stringify(await runCase(spec, c, scratch, fixtures)));
    fs.writeFileSync(outPath, out.join('\n') + '\n');
    process.stderr.write(`chat-absent-mentions oracle wrote ${outPath} (${out.length} cases)\n`);
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

test('chat-absent-mentions oracle', async () => {
  await main();
});
