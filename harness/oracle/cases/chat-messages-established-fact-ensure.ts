/**
 * Tier-2 oracle case — the `chat_messages.isEstablishedFact` boot ensure
 * (P4.D362, `db::chat_messages_established_fact_repair`) against v4's REAL
 * migration `add-established-fact-field-v1` (v4 `db6190b44` — the
 * TWENTY-SECOND D23 move), run through v4's REAL `MigrationRunner` NARROWED to
 * it (`runV4Migrations({ runner: true })` — the ledger is a comparand: v4
 * STAMPS the id when it runs, v5 never does).
 *
 * Four shapes, each base file built HERE at the pin from v4's own generateDDL
 * (the chats repository's first access materialises `chats` + `chat_messages`;
 * the `"isEstablishedFact" TEXT,` line asserted present exactly once), written
 * to `QT_FIXTURE_OUT_DIR/established-fact-<mode>.db` with two message rows:
 *   a  BASELINE — the line removed → v4 RUNS, appends `INTEGER DEFAULT NULL`;
 *   b  NO TABLE → `not needed`;
 *   c  MIGRATED — (a) after one v4 run (ledger dropped) → `not needed`;
 *   d  GENERATEDDL-CURRENT → `not needed`, untouched.
 * Comparands per mode: the report, `PRAGMA table_info` (order + text),
 * `sqlite_master.sql`, every row's `quote()`d / `typeof` flag, the
 * `migrations_state` ids. The STORAGE-CLASS leg on (c) and (d): a message
 * written through v4's REAL `ChatsRepository.addMessage({… isEstablishedFact:
 * true})` (and one with `false`) on a SECOND copy, its cell read back with
 * `typeof()` + `quote()`, then read back through v4's `getMessages` (the
 * flag's value on the hydrated row) — the Rust side writes the same rows
 * through `insert_message` and reads them through `get_messages`.
 *
 * Run from the pinned v4 worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   rm -rf /tmp/qt-established-fact-ensure /tmp/oracle-established-fact-ensure.ndjson
 *   QT_FIXTURE_OUT_DIR=/tmp/qt-established-fact-ensure \
 *     $N/npx tsx $V5W/harness/oracle/cases/chat-messages-established-fact-ensure.ts \
 *     > /tmp/oracle-established-fact-ensure.ndjson
 */

import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { PIN_624_MIGRATIONS, runV4Migrations } from '../lib/v4-migrations';
import { PEPPER, cells, insertRow, ledger, openDb, strip, sweep, tableInfo, tableSql } from '../lib/keystone-ensure-kit';

const LINE = `  "isEstablishedFact" TEXT,\n`;
const MIGRATION = PIN_624_MIGRATIONS.filter((m) => m.exportName === 'addEstablishedFactFieldMigration');
export const CHAT_ID = '6f1c3a52-0000-4000-8000-0000000000c1';
export const FACT_TRUE_ID = '6f1c3a52-0000-4000-8000-0000000000f1';
export const FACT_FALSE_ID = '6f1c3a52-0000-4000-8000-0000000000f0';

type Mode = 'a' | 'b' | 'c' | 'd';

async function main(): Promise<void> {
  const outDir = process.env.QT_FIXTURE_OUT_DIR;
  if (!outDir) throw new Error('QT_FIXTURE_OUT_DIR must name the dir for the base files');
  if (MIGRATION.length !== 1) throw new Error('PIN_624_MIGRATIONS lacks the established-fact ref');
  mkdirSync(outDir, { recursive: true });
  process.env.LOG_LEVEL = 'error';

  const scratch = mkdtempSync(join(tmpdir(), 'qt-established-fact-ddl-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.SQLITE_PATH = join(scratch, 'ddl.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  const manager = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  await manager.initializeDatabase();
  await getRepositories().chats.count();
  await getRepositories().chats.getMessageCount('00000000-0000-0000-0000-000000000000');
  const ddl = async (name: string) =>
    ((await manager.rawQuery(`SELECT sql FROM sqlite_master WHERE type = 'table' AND name = '${name}'`)) as Array<{
      sql: string;
    }>)[0].sql;
  const current = await ddl('chat_messages');
  const chatsDdl = await ddl('chats');
  await manager.closeDatabase();
  const baseline = strip(current, [LINE]);

  const seed = (db: ReturnType<typeof openDb>) => {
    for (const id of ['m1', 'm2']) {
      insertRow(db, 'chat_messages', {
        id,
        chatId: CHAT_ID,
        type: 'message',
        role: 'USER',
        content: `c-${id}`,
        createdAt: '2026-01-01T00:00:00.000Z',
      });
    }
  };
  const runOnCopy = async (base: string) => {
    const workDir = mkdtempSync(join(tmpdir(), 'qt-established-fact-ensure-'));
    const work = join(workDir, 'work.db');
    copyFileSync(base, work);
    const report = await runV4Migrations({ dbPath: work, pepperBase64: PEPPER, migrations: MIGRATION, runner: true });
    return { workDir, work, report };
  };

  /** v4's REAL write + read over a copy of the post-ensure file. */
  const storageLeg = async (work: string) => {
    const legDir = mkdtempSync(join(tmpdir(), 'qt-established-fact-leg-'));
    mkdirSync(join(legDir, 'data'), { recursive: true });
    const leg = join(legDir, 'leg.db');
    copyFileSync(work, leg);
    const db0 = openDb(leg);
    db0.exec('DROP TABLE IF EXISTS migrations_state; DROP TABLE IF EXISTS migrations_metadata');
    db0.close();
    // v4's manager is a process singleton whose collection cache goes stale
    // across a close + re-init (rows then fail Zod silently) — so the REAL
    // write + read runs in a fresh child process per leg.
    const child = spawnSync(process.execPath, [...process.execArgv, process.argv[1]], {
      env: { ...process.env, QT_EF_LEG: leg, QT_EF_LEG_DIR: legDir },
      encoding: 'utf8',
    });
    if (child.status !== 0) throw new Error(`storage leg failed: ${child.stderr}`);
    const hydrated = JSON.parse(child.stdout.trim().split('\n').pop() as string);
    const db = openDb(leg, true);
    const stored = db
      .prepare(
        `SELECT id, quote("isEstablishedFact") AS q, typeof("isEstablishedFact") AS t FROM chat_messages WHERE id IN (?, ?) ORDER BY id`,
      )
      .all(FACT_TRUE_ID, FACT_FALSE_ID);
    db.close();
    rmSync(legDir, { recursive: true, force: true });
    return { stored, hydrated };
  };

  for (const mode of ['a', 'b', 'c', 'd'] as Mode[]) {
    const out = join(outDir, `established-fact-${mode}.db`);
    sweep(out);
    const base = openDb(out);
    base.pragma('journal_mode = TRUNCATE');
    if (mode === 'b') {
      base.exec('CREATE TABLE "unrelated" ("id" TEXT PRIMARY KEY NOT NULL)');
    } else {
      base.exec(chatsDdl);
      base.exec(mode === 'a' || mode === 'c' ? baseline : current);
      seed(base);
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
    const db = openDb(work);
    const row = {
      case: 'chat-messages-established-fact-ensure',
      mode,
      report,
      tableInfo: tableInfo(db, 'chat_messages'),
      sql: tableSql(db, 'chat_messages'),
      rows: cells(db, 'chat_messages', ['isEstablishedFact']),
      ledger: ledger(db),
    };
    db.close();
    const storage = mode === 'c' || mode === 'd' ? await storageLeg(work) : null;
    rmSync(workDir, { recursive: true, force: true });
    process.stdout.write(JSON.stringify({ ...row, storage }) + '\n');
  }
  process.exit(0);
}

/** The storage leg's child: v4's REAL `addMessage` ×2 then `getMessages`. */
async function legChild(leg: string, legDir: string): Promise<void> {
  process.env.LOG_LEVEL = 'error';
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.SQLITE_PATH = leg;
  process.env.QUILLTAP_DATA_DIR = legDir;
  delete process.env.SQLITE_WAL_MODE;
  const manager = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  await manager.initializeDatabase();
  const repos = getRepositories();
  for (const [id, flag] of [
    [FACT_TRUE_ID, true],
    [FACT_FALSE_ID, false],
  ] as const) {
    await repos.chats.addMessage(CHAT_ID, {
      type: 'message',
      id,
      role: 'USER',
      content: `fact ${flag}`,
      // Distinct times — a createdAt TIE orders differently across the two
      // read plans, which is not this family's question.
      createdAt: flag ? '2026-01-02T00:00:00.000Z' : '2026-01-02T00:00:01.000Z',
      isEstablishedFact: flag,
    } as never);
  }
  const read = (await repos.chats.getMessages(CHAT_ID)) as Array<Record<string, unknown>>;
  const hydrated = read
    .filter((m) => m.id === FACT_TRUE_ID || m.id === FACT_FALSE_ID)
    .map((m) => ({ id: m.id, isEstablishedFact: m.isEstablishedFact === undefined ? '<absent>' : m.isEstablishedFact }));
  await manager.closeDatabase();
  process.stdout.write(JSON.stringify(hydrated) + '\n');
  process.exit(0);
}

if (process.env.QT_EF_LEG) {
  legChild(process.env.QT_EF_LEG, process.env.QT_EF_LEG_DIR as string).catch((err) => {
    process.stderr.write(`storage leg child failed: ${err?.stack ?? err}\n`);
    process.exit(1);
  });
} else
main().catch((err) => {
  process.stderr.write(`chat-messages-established-fact-ensure oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
