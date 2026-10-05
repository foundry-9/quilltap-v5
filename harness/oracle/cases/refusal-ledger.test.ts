/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the P4.D225 refusal ledger (v4 `recordModerationRefusal` /
 * `maybeAutoSwitchAfterRefusal`, `lib/services/dangerous-content/
 * refusal-ledger.ts`, NEW at `49059fb14`, #74).
 *
 * Drives v4's REAL module over the baked fixture (`build-refusal-ledger-
 * fixture.ts`): the REAL chats + chat-settings repositories, the REAL
 * `applyConciergeFlip` and the REAL Concierge writer (the auto-flag bubble
 * lands in `chat_messages`). Per op it records the result and every line the
 * `ConciergeRefusalLedger` logger wrote (level, message, bag); at the end it
 * dumps `chats` + `chat_messages`.
 *
 * The re-read race (v4's own `does not overwrite an operator state set while
 * the check was reading`) is planted where v4's test plants it: inside the
 * check's settings read, the operator vouches for the chat. (v5's probe sits
 * after the ledger read; nothing between the two reads looks at the chat, so
 * the two points are observationally one.)
 *
 * Regen recipe: the header of
 * `crates/quilltap-harness/tests/refusal_ledger_tier3_equivalence.rs` (run it
 * through `harness/tools/recipe_sweep.py`).
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

interface RecordedLog {
  service: string | null;
  level: string;
  message: string;
  bag: Record<string, unknown>;
}

interface Op {
  id: string;
  kind: 'record' | 'check' | 'concurrentChecks' | 'operatorFlip';
  record?: Record<string, unknown>;
  /** `operatorFlip` (P4.D226): the state the operator asks for. */
  requested?: string;
  racePlant?: boolean;
  chatId?: string;
  lastRefusal?: Record<string, unknown> | null;
}
interface Spec {
  testPepperBase64: string;
  ops: Op[];
  ledgerPlants: Array<{ chatId: string; count: number; lastAt: string }>;
  /** P4.D226: the `conciergeMode` the race plant writes (was `raceOverride`). */
  raceMode: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'refusal-ledger.json'), 'utf8'),
  ) as Spec;

  const fixture = process.env.QT_FIXTURE_REFUSAL_LEDGER;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_REFUSAL_LEDGER must point at the seed fixture');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-refusal-ledger-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'refusal-ledger-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  delete process.env.QUILLTAP_JOB_CHILD;
  process.env.LOG_LEVEL = 'error';

  const logs: RecordedLog[] = [];
  jest.resetModules();
  jest.doMock('@/lib/logger', () => {
    const recorder = (service: string | null) => {
      const record = (level: string) => (message: string, bag?: Record<string, unknown>) =>
        logs.push({ service, level, message, bag: JSON.parse(JSON.stringify(bag ?? {})) });
      const self: Record<string, unknown> = {
        debug: record('debug'),
        info: record('info'),
        warn: record('warn'),
        error: record('error'),
        trace: record('trace'),
      };
      self.child = (ctx: Record<string, unknown>) =>
        recorder(typeof ctx?.service === 'string' ? ctx.service : service);
      return self;
    };
    return {
      __esModule: true,
      LogLevel: { ERROR: 'error', WARN: 'warn', INFO: 'info', DEBUG: 'debug', TRACE: 'trace' },
      logger: recorder(null),
    };
  });
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/services/concierge-notifications/writer', () =>
    jest.requireActual('@/lib/services/concierge-notifications/writer'),
  );

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { recordModerationRefusal, maybeAutoSwitchAfterRefusal } = await import(
    '@/lib/services/dangerous-content/refusal-ledger'
  );

  await initializeDatabase();
  const repos = getRepositories();

  for (const plant of spec.ledgerPlants) {
    await rawQuery(
      'UPDATE chats SET "moderationRefusalCount" = ?, "lastModerationRefusalAt" = ? WHERE id = ?',
      [plant.count, plant.lastAt, plant.chatId],
    );
  }

  // The race plant: armed per op, fired once inside the check's settings read.
  let raceChat: string | null = null;
  const findByUserId = repos.chatSettings.findByUserId.bind(repos.chatSettings);
  (repos.chatSettings as { findByUserId: unknown }).findByUserId = async (userId: string) => {
    const found = await findByUserId(userId);
    if (raceChat) {
      const id = raceChat;
      raceChat = null;
      // P4.D226 (v4 `4d370a90f`): the operator locks the chat mid-check.
      await rawQuery('UPDATE chats SET "conciergeMode" = ? WHERE id = ?', [spec.raceMode, id]);
    }
    return found;
  };

  const ledgerLines = () => {
    const mine = logs
      .filter((l) => l.service === 'ConciergeRefusalLedger')
      .map(({ level, message, bag }) => ({ level, message, bag }));
    logs.length = 0;
    return mine;
  };

  const lines: string[] = [];
  for (const op of spec.ops) {
    logs.length = 0;
    let result: unknown;
    if (op.kind === 'record') {
      if (op.racePlant) raceChat = String(op.record!.chatId);
      result = await recordModerationRefusal(op.record as never);
    } else if (op.kind === 'operatorFlip') {
      // P4.D226: the operator's own move through the real chokepoint.
      const chat = await repos.chats.findById(op.chatId!);
      if (!chat) throw new Error(`op ${op.id}: chat ${op.chatId} not found`);
      const { applyConciergeFlip } = await import('@/lib/services/dangerous-content/manual-flip');
      result = await applyConciergeFlip(op.chatId!, op.requested as never, chat);
    } else if (op.kind === 'check') {
      result = await maybeAutoSwitchAfterRefusal(op.chatId!, op.lastRefusal as never);
    } else {
      result = await Promise.all([
        maybeAutoSwitchAfterRefusal(op.chatId!, op.lastRefusal as never),
        maybeAutoSwitchAfterRefusal(op.chatId!, op.lastRefusal as never),
      ]);
    }
    if (raceChat) throw new Error(`op ${op.id}: the race plant never fired`);
    lines.push(JSON.stringify({ kind: 'op', id: op.id, result, logs: ledgerLines() }));
  }

  const dumpTable = async (table: string, orderBy: string) => {
    const columns = ((await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>).map(
      (c) => c.name,
    );
    const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
    const rows = rawRows
      .map((r) => {
        const out: Record<string, unknown> = {};
        for (const col of columns) out[col] = canonValue(r[col]);
        return out;
      })
      .sort((a, b) => {
        const av = String(a[orderBy] ?? '');
        const bv = String(b[orderBy] ?? '');
        return av < bv ? -1 : av > bv ? 1 : 0;
      });
    return { table, columns, rows };
  };

  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chats', 'id')) }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chat_messages', 'chatId')) }));

  await closeDatabase();
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`refusal-ledger oracle wrote ${outPath} (${spec.ops.length} ops)\n`);
}

test('refusal-ledger tier-3 oracle', async () => {
  await main();
});

// Remove the OS-temp scratch dir(s) once the oracle has written its NDJSON.
const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
