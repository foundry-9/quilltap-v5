/**
 * @jest-environment node
 *
 * Tier-2 ORACLE for the W4.2 manual Concierge flip (v4 `applyConciergeFlip`,
 * `lib/services/dangerous-content/manual-flip.ts`).
 *
 * Drives v4's REAL `applyConciergeFlip` over a baked `chats` fixture (one chat
 * per flip scenario), then dumps the `chats` table. The synthetic Concierge
 * announcement (`postConciergeManualAnnouncement`) is mocked to a no-op — it is
 * a personified-system writer seamed in the Rust port (a W4.6 deferral); mocking
 * it matches the port (neither side posts, so `chat_messages` / the chat
 * metadata are untouched by the announcement).
 *
 * Beyond the announcement mock, the real DB stack is wired back in past
 * `jest.setup`'s global DB mocks (`[[jest-real-db-oracle]]`).
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

interface Op { id: string; chatId: string; requested: 'safe' | 'flagged' | 'off' }
interface Spec { testPepperBase64: string; ops: Op[] }

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

  jest.resetModules();
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
  // Seam the personified Concierge announcement writer to no-ops (W4.6).
  jest.doMock('@/lib/services/concierge-notifications/writer', () => ({
    __esModule: true,
    postConciergeManualAnnouncement: async () => undefined,
    postConciergeDangerAnnouncement: async () => undefined,
  }));

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { applyConciergeFlip } = await import(
    '@/lib/services/dangerous-content/manual-flip'
  );

  await initializeDatabase();
  const repos = getRepositories();

  const lines: string[] = [];
  for (const op of spec.ops) {
    const chat = await repos.chats.findById(op.chatId);
    if (!chat) throw new Error(`op ${op.id}: chat ${op.chatId} not found`);
    const result = await applyConciergeFlip(op.chatId, op.requested, chat);
    lines.push(
      JSON.stringify({ kind: 'op', id: op.id, newState: result.newState, changed: result.changed })
    );
  }

  const columns = (
    (await rawQuery('PRAGMA table_info(chats)')) as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM chats')) as Array<Record<string, unknown>>;
  const rows = rawRows
    .map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
      return out;
    })
    .sort((a, b) => (String(a.id) < String(b.id) ? -1 : String(a.id) > String(b.id) ? 1 : 0));
  lines.push(JSON.stringify({ kind: 'table', table: 'chats', columns, rows }));

  await closeDatabase();
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`danger-manual-flip oracle wrote ${outPath}\n`);
}

test('danger-manual-flip tier-2 oracle', async () => {
  await main();
});
