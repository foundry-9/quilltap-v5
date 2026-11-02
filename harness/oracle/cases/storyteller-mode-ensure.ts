/**
 * Tier-2 oracle case — the Storyteller-mode boot ensure (P4.D341,
 * `db::storyteller_mode_repair`) against v4's REAL migration
 * `add-storyteller-mode-v1` (v4 `b15fe9404` — the EIGHTEENTH D23 move),
 * through v4's REAL `MigrationRunner` narrowed to it (`runV4Migrations({
 * runner: true })`).
 *
 * The generateDDL-current tables come from v4's REAL repositories at the pin
 * (the `dump-fresh-schema.ts` triggers: `chats.count()`, the chats repo's
 * `getMessageCount` for `chat_messages`, `storytellerRounds.count()` for the
 * table + its createdAt index), read back from `sqlite_master`; each removed
 * line is asserted present exactly once. Six shapes, written to
 * `QT_FIXTURE_OUT_DIR/storyteller-<mode>.db`, each with two chats and two
 * messages:
 *   a  BASELINE — the three column lines stripped, no `storyteller_rounds` →
 *      v4 runs all four steps;
 *   b  NO `chats` (only `chat_messages`) → `not needed`;
 *   c  ALREADY MIGRATED — (a) after one v4 run (ledger dropped) → `not needed`;
 *   d  GENERATEDDL-CURRENT — the columns at schema position, generateDDL's
 *      table + createdAt index + the migration's chat index (a fresh
 *      provision) → `not needed`, untouched;
 *   e  HALF — only `storytellerBid` missing, the table present → v4 appends
 *      the one column;
 *   f  TABLE-ONLY missing → v4 creates the table + the chat index.
 * `CREATED_AT_INDEX_ON_FIRST_TOUCH`: after the migration, on every mode with
 * `chats`, v4's REAL `StorytellerRoundsRepository` is touched once (`count()`
 * — what any v4 boot's first round read does), so its `ensureCollection`
 * makes generateDDL's `idx_storyteller_rounds_createdAt` — which v5's ensure
 * makes at boot (it has no lazy ensure). On (a) and (d) v4's REAL `create`
 * then writes one round (bids JSON, `selection` null); the Rust side writes
 * the same cells raw and both report every cell's `quote()` + `typeof`
 * (the storage-class leg — the v5 repository is P4.D343's).
 * Comparands: the report, `PRAGMA table_info` of the three tables,
 * `sqlite_master.sql` of the three tables and both indexes, the chat / message
 * / round cells, the `migrations_state` ids.
 *
 * Run from the pinned v4 worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   rm -rf /tmp/qt-storyteller-mode-ensure /tmp/oracle-storyteller-mode-ensure.ndjson
 *   QT_FIXTURE_OUT_DIR=/tmp/qt-storyteller-mode-ensure \
 *     $N/npx tsx $V5W/harness/oracle/cases/storyteller-mode-ensure.ts \
 *     > /tmp/oracle-storyteller-mode-ensure.ndjson
 */

import { copyFileSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { KEYSTONE_236_MIGRATIONS, runV4Migrations } from '../lib/v4-migrations';
import {
  PEPPER,
  cells,
  indexSql,
  insertRow,
  ledger,
  openDb,
  strip,
  sweep,
  tableInfo,
  tableSql,
} from '../lib/keystone-ensure-kit';

const TURN_MODE_LINE = `  "turnMode" TEXT,\n`;
const INVITE_LINE = `  "storytellerInviteFilter" INTEGER DEFAULT 0,\n`;
const BID_LINE = `  "storytellerBid" TEXT,\n`;
const CHAT_INDEX_SQL = `CREATE INDEX IF NOT EXISTS "idx_storyteller_rounds_chat" ON "storyteller_rounds" ("chatId", "status")`;
const MIGRATION = KEYSTONE_236_MIGRATIONS.filter((m) => m.exportName === 'addStorytellerModeMigration');
export const ROUND = {
  id: '00000000-0000-4000-8000-0000000000a1',
  chatId: 'c1',
  triggerMessageId: 'msg1',
  status: 'open',
  bids: [
    {
      participantId: 'p1',
      characterId: 'k1',
      characterName: 'Ada',
      status: 'bid',
      tier: 'simple',
      tools: [],
      plan: 'Say hello.',
      talkativeness: 0.5,
    },
  ],
  selection: null,
  createdAt: '2026-01-02T00:00:00.000Z',
  updatedAt: '2026-01-02T00:00:00.000Z',
};
const ROUND_COLUMNS = [
  'chatId',
  'triggerMessageId',
  'status',
  'bids',
  'selection',
  'closedReason',
  'createdAt',
  'updatedAt',
  'closedAt',
];

type Mode = 'a' | 'b' | 'c' | 'd' | 'e' | 'f';

async function main(): Promise<void> {
  const outDir = process.env.QT_FIXTURE_OUT_DIR;
  if (!outDir) throw new Error('QT_FIXTURE_OUT_DIR must name the dir for the base files');
  if (MIGRATION.length !== 1) throw new Error('KEYSTONE_236_MIGRATIONS lacks the storyteller ref');
  mkdirSync(outDir, { recursive: true });
  process.env.LOG_LEVEL = 'error';

  const scratch = mkdtempSync(join(tmpdir(), 'qt-storyteller-ensure-ddl-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.SQLITE_PATH = join(scratch, 'ddl.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  await initializeDatabase();
  const repos = getRepositories() as unknown as {
    chats: { count(): Promise<number>; getMessageCount(id: string): Promise<number> };
    storytellerRounds: { count(): Promise<number> };
  };
  await repos.chats.count();
  await repos.chats.getMessageCount('00000000-0000-0000-0000-000000000000');
  await repos.storytellerRounds.count();
  const sqlOf = async (type: string, name: string) =>
    ((await rawQuery(`SELECT sql FROM sqlite_master WHERE type = '${type}' AND name = '${name}'`)) as Array<{
      sql: string;
    }>)[0].sql;
  const chatsNow = await sqlOf('table', 'chats');
  const messagesNow = await sqlOf('table', 'chat_messages');
  const roundsNow = await sqlOf('table', 'storyteller_rounds');
  const roundsCreatedAt = await sqlOf('index', 'idx_storyteller_rounds_createdAt');
  await closeDatabase();
  const chatsBase = strip(chatsNow, [TURN_MODE_LINE, INVITE_LINE]);
  const messagesBase = strip(messagesNow, [BID_LINE]);

  const runOnCopy = async (base: string) => {
    const workDir = mkdtempSync(join(tmpdir(), 'qt-storyteller-ensure-'));
    mkdirSync(join(workDir, 'data'), { recursive: true });
    const work = join(workDir, 'work.db');
    copyFileSync(base, work);
    const report = await runV4Migrations({ dbPath: work, pepperBase64: PEPPER, migrations: MIGRATION, runner: true });
    return { workDir, work, report };
  };

  for (const mode of ['a', 'b', 'c', 'd', 'e', 'f'] as Mode[]) {
    const out = join(outDir, `storyteller-${mode}.db`);
    sweep(out);
    const base = openDb(out);
    base.pragma('journal_mode = TRUNCATE');
    const withChats = mode !== 'b';
    if (withChats) base.exec(mode === 'a' || mode === 'c' ? chatsBase : chatsNow);
    base.exec(mode === 'a' || mode === 'c' || mode === 'e' ? messagesBase : messagesNow);
    if (mode === 'd' || mode === 'e') {
      base.exec(roundsNow);
      base.exec(roundsCreatedAt);
      base.exec(CHAT_INDEX_SQL);
    }
    const t0 = '2026-01-01T00:00:00.000Z';
    if (withChats) {
      for (const id of ['c1', 'c2']) insertRow(base, 'chats', { id, userId: 'u1', title: id, createdAt: t0, updatedAt: t0 });
    }
    for (const id of ['msg1', 'msg2']) {
      insertRow(base, 'chat_messages', { id, chatId: 'c1', type: 'message', role: 'USER', content: id, createdAt: t0 });
    }
    base.close();
    if (mode === 'c') {
      const first = await runOnCopy(out);
      if (!first.report[0]?.startsWith('+RAN')) throw new Error(`(c) setup did not run: ${first.report}`);
      const migrated = openDb(first.work);
      migrated.exec('DROP TABLE IF EXISTS migrations_state; DROP TABLE IF EXISTS migrations_metadata');
      migrated.close();
      sweep(out);
      copyFileSync(first.work, out);
      rmSync(first.workDir, { recursive: true, force: true });
    }

    const { workDir, work, report } = await runOnCopy(out);
    let wrote: Record<string, unknown> | null = null;
    if (withChats) {
      // `CREATED_AT_INDEX_ON_FIRST_TOUCH` + (a) / (d) the REAL create.
      process.env.SQLITE_PATH = work;
      process.env.QUILLTAP_DATA_DIR = workDir;
      await initializeDatabase();
      const { StorytellerRoundsRepository } = await import(
        '@/lib/database/repositories/storyteller-rounds.repository'
      );
      const repo = new StorytellerRoundsRepository();
      await repo.count();
      if (mode === 'a' || mode === 'd') {
        const { id, createdAt, updatedAt, ...data } = ROUND;
        await repo.create(data as never, { id, createdAt, updatedAt });
      }
      await closeDatabase();
    }
    const db = openDb(work);
    if (mode === 'a' || mode === 'd') {
      wrote = db.prepare(`SELECT * FROM storyteller_rounds WHERE id = ?`).get(ROUND.id) as Record<string, unknown>;
    }
    const row = {
      case: 'storyteller-mode-ensure',
      mode,
      report,
      tableInfo: {
        chats: tableInfo(db, 'chats'),
        chat_messages: tableInfo(db, 'chat_messages'),
        storyteller_rounds: tableInfo(db, 'storyteller_rounds'),
      },
      sql: {
        chats: tableSql(db, 'chats'),
        chat_messages: tableSql(db, 'chat_messages'),
        storyteller_rounds: tableSql(db, 'storyteller_rounds'),
        idx_storyteller_rounds_chat: indexSql(db, 'idx_storyteller_rounds_chat'),
        idx_storyteller_rounds_createdAt: indexSql(db, 'idx_storyteller_rounds_createdAt'),
      },
      rows: {
        chats: cells(db, 'chats', ['turnMode', 'storytellerInviteFilter']),
        chat_messages: cells(db, 'chat_messages', ['storytellerBid']),
        storyteller_rounds: cells(db, 'storyteller_rounds', ROUND_COLUMNS),
      },
      wrote,
      ledger: ledger(db),
    };
    db.close();
    rmSync(workDir, { recursive: true, force: true });
    process.stdout.write(JSON.stringify(row) + '\n');
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`storyteller-mode-ensure oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
