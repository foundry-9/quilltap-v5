/**
 * @jest-environment node
 *
 * P4.D331 log-tools ORACLE: drives v4's REAL `executeToolCallWithContext`
 * (`lib/chat/tool-executor.ts:303`) → the three log handlers
 * (`lib/tools/handlers/log-handler.ts`) at the `322a5488e` pin, over a FRESH
 * copy per case of the committed chat-dialogs pair (characters Nora, Pip,
 * Vera, Wren — the names `toViews` and the resolver read) plus an llm-logs
 * file `build-log-entries-fixture.ts` seeded (`QT_FIXTURE_SEED=tools`, the
 * pair's pepper), or — for the degraded rows — a garbage file in its place.
 *
 * Each row records the executor's envelope (`{toolName, success, result,
 * error}`), every logger call made during the call (a `Logger.prototype` spy:
 * `LEVEL message key=value …`, the child logger's bindings first, an object
 * as `<key>Json=`), and after a create the whole `log_entries` table. Minted
 * ids are `<minted>`, their stamps `<now>`.
 *
 * Run (Node 24, from the pinned v4 checkout — cp to a /tmp mirror; jest
 * ignores .claude/ paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-log-tools-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/log-tools.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SEED=tools QT_FIXTURE_OUT=/tmp/qt-log-tools-llmlogs.db PATH=$N:$PATH \
 *     npx tsx $V5W/harness/oracle/fixtures/build-log-entries-fixture.ts
 *   QT_FIXTURE_CD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/chat-dialogs-main.db \
 *   QT_FIXTURE_CD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/chat-dialogs-mount.db \
 *   QT_FIXTURE_LOG_TOOLS_LLMLOGS=/tmp/qt-log-tools-llmlogs.db \
 *   QT_ORACLE_OUT=/tmp/oracle-log-tools.ndjson TZ=UTC PATH=$N:$PATH \
 *     npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- log-tools
 */

import * as fs from 'fs';
import { join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

const PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
const USER = 'f0000000-0000-4000-8000-00000000000f';
const CHAT_PROJ = 'c2000000-0000-4000-8000-000000000004';
const CHAT_BARE = 'c2000000-0000-4000-8000-000000000006';
const NORA = 'a2000000-0000-4000-8000-000000000001';

interface Case {
  name: string;
  tool: 'log_create_entry' | 'log_grep' | 'log_read';
  args: Record<string, unknown>;
  chatId?: string;
  /** The calling character; `''` exercises the executor's falsy guard. */
  characterId?: string;
  llm?: 'fixture' | 'garbage';
}

const CASES: Case[] = [
  // log_create_entry
  { name: 'create-basic', tool: 'log_create_entry', args: { message: '  Picked a second lock  ' }, characterId: NORA },
  { name: 'create-no-character', tool: 'log_create_entry', args: { message: 'Operator note' } },
  { name: 'create-empty-character-id', tool: 'log_create_entry', args: { message: 'Empty seat' }, characterId: '' },
  { name: 'create-blank', tool: 'log_create_entry', args: { message: '   ' }, characterId: NORA },
  { name: 'create-missing-message', tool: 'log_create_entry', args: {} },
  { name: 'create-number', tool: 'log_create_entry', args: { message: 5 } },
  { name: 'create-4000-astral', tool: 'log_create_entry', args: { message: '\u{1f600}'.repeat(4000) }, characterId: NORA },
  { name: 'create-4001', tool: 'log_create_entry', args: { message: 'q'.repeat(4001) } },
  { name: 'create-degraded', tool: 'log_create_entry', args: { message: 'Into the void' }, characterId: NORA, llm: 'garbage' },
  // log_grep
  { name: 'grep-case-sensitive', tool: 'log_grep', args: { pattern: 'lock(pick|smith)' } },
  { name: 'grep-ignore-case', tool: 'log_grep', args: { pattern: 'lock(pick|smith)', ignore_case: true } },
  { name: 'grep-day', tool: 'log_grep', args: { pattern: '^Day \\d+' } },
  { name: 'grep-lookbehind', tool: 'log_grep', args: { pattern: '(?<=Lock)(pick|smith)' } },
  { name: 'grep-stranger', tool: 'log_grep', args: { pattern: 'stranger' } },
  { name: 'grep-bad-regex', tool: 'log_grep', args: { pattern: '(unclosed' } },
  { name: 'grep-limit-string', tool: 'log_grep', args: { pattern: 'e', limit: '2' } },
  { name: 'grep-limit-fraction', tool: 'log_grep', args: { pattern: 'e', limit: '5.5' } },
  { name: 'grep-limit-zero', tool: 'log_grep', args: { pattern: 'e', limit: 0 } },
  { name: 'grep-limit-201', tool: 'log_grep', args: { pattern: 'e', limit: 201 } },
  { name: 'grep-empty-pattern', tool: 'log_grep', args: { pattern: '' } },
  { name: 'grep-501', tool: 'log_grep', args: { pattern: 'a'.repeat(501) } },
  { name: 'grep-ignore-case-string', tool: 'log_grep', args: { pattern: 'lock', ignore_case: 'yes' } },
  { name: 'grep-ignore-case-false', tool: 'log_grep', args: { pattern: 'Lock', ignore_case: false } },
  { name: 'grep-degraded', tool: 'log_grep', args: { pattern: 'lock' }, llm: 'garbage' },
  // log_read
  { name: 'read-everything', tool: 'log_read', args: {} },
  { name: 'read-current', tool: 'log_read', args: { conversationId: 'current' } },
  { name: 'read-explicit-chat', tool: 'log_read', args: { conversationId: CHAT_BARE } },
  { name: 'read-non-uuid-chat', tool: 'log_read', args: { conversationId: 'not-a-uuid' } },
  { name: 'read-empty-chat', tool: 'log_read', args: { conversationId: '' } },
  { name: 'read-by-name-lowercase', tool: 'log_read', args: { character: 'nora' } },
  { name: 'read-by-id', tool: 'log_read', args: { character: NORA } },
  { name: 'read-user-character', tool: 'log_read', args: { character: 'Vera' } },
  { name: 'read-unknown-spaced', tool: 'log_read', args: { character: ' bob ' } },
  { name: 'read-name-and-current', tool: 'log_read', args: { character: 'Nora', conversationId: 'current' } },
  { name: 'read-same-bare-day', tool: 'log_read', args: { from: '2026-10-19', to: '2026-10-19' } },
  { name: 'read-loose-from', tool: 'log_read', args: { from: 'October 1, 2026' } },
  { name: 'read-feb-31-to', tool: 'log_read', args: { to: '2026-02-31' } },
  { name: 'read-feb-31-from', tool: 'log_read', args: { from: '2026-02-31' } },
  { name: 'read-offset-to', tool: 'log_read', args: { to: '2026-10-02T12:00:00+02:00' } },
  { name: 'read-expanded-from', tool: 'log_read', args: { from: '+275760-09-13' } },
  { name: 'read-expanded-from-past-to', tool: 'log_read', args: { from: '+275760-09-13', to: '2026-01-01' } },
  { name: 'read-negative-year-from', tool: 'log_read', args: { from: '-000001-01-01', to: '2026-03-01' } },
  { name: 'read-backwards', tool: 'log_read', args: { from: '2026-10-20', to: '2026-10-19' } },
  { name: 'read-garbage-from', tool: 'log_read', args: { from: 'garbage' } },
  { name: 'read-garbage-from-with-to', tool: 'log_read', args: { from: 'garbage', to: '2026-01-01' } },
  { name: 'read-limit-string', tool: 'log_read', args: { limit: '2' } },
  { name: 'read-limit-null', tool: 'log_read', args: { limit: null } },
  { name: 'read-degraded', tool: 'log_read', args: {}, llm: 'garbage' },
  { name: 'read-degraded-unknown-character', tool: 'log_read', args: { character: 'nobody' }, llm: 'garbage' },
];

function applyMocks(): void {
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
  jest.doMock('@/lib/mount-index/character-vault', () =>
    jest.requireActual('@/lib/mount-index/character-vault'),
  );
  jest.doMock('@/lib/background-jobs/processor', () => ({
    __esModule: true,
    ensureProcessorRunning: () => {},
    startProcessor: () => {},
    stopProcessor: () => {},
  }));
}

/** v4's line as the Rust side renders it (root bindings dropped). */
function render(level: string, message: string, bound: Record<string, unknown>, context?: Record<string, unknown>) {
  const parts = [level.toUpperCase(), message];
  const merged: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(bound)) {
    if (k === 'environment' || (k === 'service' && v === 'quilltap')) continue;
    merged[k] = v;
  }
  Object.assign(merged, context ?? {});
  for (const [k, v] of Object.entries(merged)) {
    if (v === undefined) continue;
    if (v !== null && typeof v === 'object') parts.push(`${k}Json=${JSON.stringify(v)}`);
    else if (typeof v === 'number') parts.push(`${k}=${JSON.stringify(v)}`);
    else parts.push(`${k}=${v === null ? 'null' : String(v)}`);
  }
  return parts.join(' ');
}

describe('P4.D331 log tools oracle', () => {
  it('records every case', async () => {
    const out = process.env.QT_ORACLE_OUT;
    const main = process.env.QT_FIXTURE_CD_MAIN;
    const mount = process.env.QT_FIXTURE_CD_MOUNT;
    const logs = process.env.QT_FIXTURE_LOG_TOOLS_LLMLOGS;
    if (!out || !main || !mount || !logs) throw new Error('QT_ORACLE_OUT / QT_FIXTURE_CD_MAIN / _MOUNT / QT_FIXTURE_LOG_TOOLS_LLMLOGS');
    const scratch = mkdtempSync(join(tmpdir(), 'qt-log-tools-'));
    const lines: string[] = [];
    process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;

    for (const c of CASES) {
      jest.resetModules();
      applyMocks();
      const work = mkdtempSync(join(scratch, 'case-'));
      mkdirSync(join(work, 'data'), { recursive: true });
      process.env.QUILLTAP_DATA_DIR = work;
      copyFileSync(main, join(work, 'main.db'));
      copyFileSync(mount, join(work, 'mount.db'));
      const llmPath = join(work, 'llm-logs.db');
      if ((c.llm ?? 'fixture') === 'garbage') writeFileSync(llmPath, Buffer.alloc(4096, 0x5a));
      else copyFileSync(logs, llmPath);
      process.env.SQLITE_PATH = join(work, 'main.db');
      process.env.SQLITE_MOUNT_INDEX_PATH = join(work, 'mount.db');
      process.env.SQLITE_LLM_LOGS_PATH = llmPath;

      const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
      const { getRawLLMLogsDatabase } = await import('@/lib/database/backends/sqlite/llm-logs-client');
      const { executeToolCallWithContext } = await import('@/lib/chat/tool-executor');
      const { Logger } = await import('@/lib/logger');
      await initializeDatabase();

      let sink: string[] | null = null;
      for (const level of ['error', 'warn', 'info', 'debug'] as const) {
        const original = Logger.prototype[level];
        Logger.prototype[level] = function (this: { context?: Record<string, unknown> }, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
          sink?.push(render(level, message, this.context ?? {}, context));
          return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
        } as never;
      }

      const captured: string[] = [];
      sink = captured;
      const result = await executeToolCallWithContext(
        { name: c.tool, arguments: c.args } as never,
        {
          chatId: c.chatId ?? CHAT_PROJ,
          userId: USER,
          ...(c.characterId !== undefined ? { characterId: c.characterId } : {}),
        } as never,
      );
      sink = null;

      const minted: string[] = [];
      const r = JSON.parse(JSON.stringify(result)) as Record<string, unknown>;
      const entry = (r.result as Record<string, unknown> | null)?.entry as Record<string, unknown> | undefined;
      if (entry) {
        minted.push(String(entry.id));
        entry.id = '<minted>';
        entry.createdAt = '<now>';
        entry.message = typeof entry.message === 'string' ? entry.message : entry.message;
      }
      const mask = (s: string) => minted.reduce((acc, id) => acc.split(id).join('<minted>'), s);
      const row: Record<string, unknown> = {
        case: c.name,
        tool: c.tool,
        args: c.args,
        chatId: c.chatId ?? CHAT_PROJ,
        ...(c.characterId !== undefined ? { characterId: c.characterId } : {}),
        llm: c.llm ?? 'fixture',
        result: {
          success: r.success,
          result: r.result ?? null,
          error: r.error ?? null,
        },
        lines: captured.map(mask),
      };
      if (c.tool === 'log_create_entry' && (c.llm ?? 'fixture') === 'fixture') {
        const db = getRawLLMLogsDatabase();
        const rows = db!
          .prepare('SELECT "id", "chatId", "characterId", "createdAt", "updatedAt", hex("message") AS "messageHex" FROM log_entries ORDER BY "id"')
          .all() as Array<Record<string, unknown>>;
        row.dump = rows
          .map((x) => (minted.includes(String(x.id)) ? { ...x, id: '<minted>', createdAt: '<now>', updatedAt: '<now>' } : x))
          .sort((a, b) => (String(a.id) < String(b.id) ? -1 : String(a.id) > String(b.id) ? 1 : 0));
      }
      lines.push(JSON.stringify(row));
      await closeDatabase();
    }

    fs.writeFileSync(out, lines.join('\n') + '\n');
    rmSync(scratch, { recursive: true, force: true });
  });
});
