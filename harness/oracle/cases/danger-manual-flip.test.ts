/**
 * @jest-environment node
 *
 * Tier-2 ORACLE for the W4.2 manual Concierge flip (v4 `applyConciergeFlip`,
 * `lib/services/dangerous-content/manual-flip.ts`).
 *
 * Drives v4's REAL `applyConciergeFlip` over a baked `chats` fixture (one chat
 * per flip scenario), then dumps `chats` + `chat_messages`. P4.D226 (v4
 * `4d370a90f`, #75) REWROTE the corpus for the three states: v4's own
 * `manual-flip.test.ts` cases as planted rows (the six transitions, the no-ops,
 * the provenance adoptions, both Concierge switches, the refused Concierge
 * moves, a compare-and-set miss via a stale `snapshot`), and every op now
 * records its `ConciergeManualFlip` log lines plus the repository's two DEBUGs
 * (`Concierge state write`, `Chat danger classification recorded`) through a
 * `@/lib/logger` recorder. The synthetic
 * Concierge announcement (`postConciergeManualAnnouncement`) runs REAL now
 * (W4.6b): every changed flip posts the manual bubble into `chat_messages`
 * (bumping the chat's `updatedAt`/`lastMessageAt`/`messageCount` via
 * `repos.chats.addMessage`), matching the ported `RealConciergeAnnouncer`.
 *
 * The real DB stack is wired back in past `jest.setup`'s global DB mocks
 * (`[[jest-real-db-oracle]]`).
 *
 * Run from the v4 checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-danger-manual-flip.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-danger-manual-flip-fixture.ts
 *   QT_FIXTURE_MANUAL_FLIP=/tmp/qt-danger-manual-flip.db \
 *   QT_ORACLE_OUT=/tmp/oracle-danger-manual-flip.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5W/harness/oracle/cases" \
 *       -- danger-manual-flip
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

interface Op {
  id: string;
  chatId: string;
  requested: 'moderated' | 'unmoderated' | 'locked';
  /** P4.D225 (v4 `49059fb14`): `applyConciergeFlip`'s fourth argument. */
  options?: Record<string, unknown>;
  /** P4.D226: spread over the row read before the flip (a stale snapshot). */
  snapshot?: Record<string, unknown>;
}
interface RecordedLog {
  service: string | null;
  level: string;
  message: string;
  bag: Record<string, unknown>;
}
/** The repository's own lines this family pins (root logger, no service). */
const REPO_LINES = new Set(['Concierge state write', 'Chat danger classification recorded']);
interface Spec {
  testPepperBase64: string;
  ops: Op[];
  ledgerPlants?: Array<{ chatId: string; count: number; lastAt: string }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'danger-manual-flip.json'), 'utf8')
  ) as Spec;

  const fixture = process.env.QT_FIXTURE_MANUAL_FLIP;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_MANUAL_FLIP must point at the seed fixture');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-danger-manual-flip-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'manual-flip-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
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
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories')
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  // Run v4's REAL Concierge writer (W4.6b): every changed flip posts the manual
  // bubble into `chat_messages`, matching the ported `RealConciergeAnnouncer`.
  jest.doMock('@/lib/services/concierge-notifications/writer', () =>
    jest.requireActual('@/lib/services/concierge-notifications/writer')
  );

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { applyConciergeFlip } = await import(
    '@/lib/services/dangerous-content/manual-flip'
  );

  await initializeDatabase();
  const repos = getRepositories();

  // P4.D225: plant the refusal ledgers (both sides do the same raw UPDATE).
  for (const plant of spec.ledgerPlants ?? []) {
    await rawQuery(
      'UPDATE chats SET "moderationRefusalCount" = ?, "lastModerationRefusalAt" = ? WHERE id = ?',
      [plant.count, plant.lastAt, plant.chatId],
    );
  }

  const lines: string[] = [];
  for (const op of spec.ops) {
    const read = await repos.chats.findById(op.chatId);
    if (!read) throw new Error(`op ${op.id}: chat ${op.chatId} not found`);
    const chat = { ...read, ...(op.snapshot ?? {}) };
    logs.length = 0;
    const result = await applyConciergeFlip(op.chatId, op.requested, chat as never, (op.options ?? {}) as never);
    const opLogs = logs
      .filter((l) => l.service === 'ConciergeManualFlip' || (l.service === null && REPO_LINES.has(l.message)))
      .map(({ service, level, message, bag }) => ({ service, level, message, bag }));
    lines.push(
      JSON.stringify({ kind: 'op', id: op.id, newState: result.newState, changed: result.changed, logs: opLogs })
    );
  }

  const dumpTable = async (table: string, orderBy: string) => {
    const columns = (
      (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
    ).map((c) => c.name);
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
  // The manual Concierge bubble (one per changed flip) lands here.
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chat_messages', 'chatId')) }));

  await closeDatabase();
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`danger-manual-flip oracle wrote ${outPath}\n`);
}

test('danger-manual-flip tier-2 oracle', async () => {
  await main();
});
