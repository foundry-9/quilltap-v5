/**
 * Tier-2 oracle case — the llm-logs round-id boot ensure (P4.D341,
 * `db::llm_logs_storyteller_round_id_repair`, the FIRST llm-logs column ensure)
 * against v4's REAL migration `add-llm-logs-storyteller-round-id-column-v1`
 * (v4 `b15fe9404` — the NINETEENTH D23 move; FILE
 * `add-llm-logs-storyteller-round-id-column.ts`), through v4's REAL
 * `MigrationRunner` narrowed to it (`runV4Migrations({ runner: true,
 * llmLogsPath })`). v4's module opens the logs FILE itself from
 * `SQLITE_LLM_LOGS_PATH`, so each mode is a PAIR — a main file (the runner's
 * ledger lives there) and a logs file — written to `QT_FIXTURE_OUT_DIR` as
 * `roundid-<mode>.db` / `roundid-<mode>-llm-logs.db`.
 *
 * The generateDDL-current `llm_logs` comes from v4's REAL `LLMLogsRepository`
 * at the pin (`count()` on a scratch logs partition, read back through
 * `getRawLLMLogsDatabase()`), with its generated indexes; the column line is
 * asserted present exactly once before the strip. Two log rows each.
 *   a  BASELINE — `"storytellerRoundId" TEXT,` removed → v4 RUNS: the column
 *      appended (`TEXT DEFAULT NULL`) + the index;
 *   b  FILE ABSENT → v4's `shouldRun` false; v5's host has no partition (`None`
 *      → skip): the comparand is the absence of any file;
 *   c  file present, `llm_logs` ABSENT → `not needed` both sides;
 *   d  ALREADY MIGRATED — (a) after one v4 run (ledger dropped) → `not needed`;
 *   e  GENERATEDDL-CURRENT (the column at schema position, NO round-id index)
 *      → v4's `shouldRun` is `!column` only, so `not needed` and the index stays
 *      ABSENT (`COLUMN_WITHOUT_INDEX_NOT_NEEDED` — the fresh path's index comes
 *      from `migration_indexes.json`).
 * Comparands: the report, `PRAGMA table_info`, `sqlite_master.sql` of the table
 * and of `idx_llm_logs_storytellerRoundId`, the rows' `storytellerRoundId`
 * (`quote()` + `typeof`), the main file's `migrations_state` ids, and the logs
 * file's journal mode AFTER (`journalMode` — through the runner helper, which
 * puts the header back) plus `rawJournalMode`: the mode a SEPARATE copy's
 * header holds after v4's module ran directly with NO restore — the measurement
 * of whether its migration-time opener's `journal_mode = WAL` persists in the
 * file (the both-ways carve `V4_LLM_LOGS_MIGRATION_WAL` on the Rust side).
 *
 * Run from the pinned v4 worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   rm -rf /tmp/qt-llm-logs-round-id-ensure /tmp/oracle-llm-logs-round-id-ensure.ndjson
 *   QT_FIXTURE_OUT_DIR=/tmp/qt-llm-logs-round-id-ensure \
 *     $N/npx tsx $V5W/harness/oracle/cases/llm-logs-storyteller-round-id-ensure.ts \
 *     > /tmp/oracle-llm-logs-round-id-ensure.ndjson
 */

import { copyFileSync, existsSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { KEYSTONE_236_MIGRATIONS, runV4Migrations } from '../lib/v4-migrations';
import {
  PEPPER,
  cells,
  indexSql,
  insertRow,
  journalMode,
  ledger,
  openDb,
  strip,
  sweep,
  tableInfo,
  tableSql,
} from '../lib/keystone-ensure-kit';

const LINE = `  "storytellerRoundId" TEXT,\n`;
const INDEX = 'idx_llm_logs_storytellerRoundId';
const MIGRATION = KEYSTONE_236_MIGRATIONS.filter(
  (m) => m.exportName === 'addLLMLogsStorytellerRoundIdColumnMigration',
);

type Mode = 'a' | 'b' | 'c' | 'd' | 'e';

async function main(): Promise<void> {
  const outDir = process.env.QT_FIXTURE_OUT_DIR;
  if (!outDir) throw new Error('QT_FIXTURE_OUT_DIR must name the dir for the base files');
  if (MIGRATION.length !== 1) throw new Error('KEYSTONE_236_MIGRATIONS lacks the llm-logs ref');
  mkdirSync(outDir, { recursive: true });
  process.env.LOG_LEVEL = 'error';

  const scratch = mkdtempSync(join(tmpdir(), 'qt-roundid-ensure-ddl-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.SQLITE_PATH = join(scratch, 'ddl.db');
  process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'ddl-llm-logs.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = join(scratch, 'ddl-mount-index.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawLLMLogsDatabase } = await import('@/lib/database/backends/sqlite/llm-logs-client');
  await initializeDatabase();
  await (getRepositories() as unknown as { llmLogs: { count(): Promise<number> } }).llmLogs.count();
  const raw = getRawLLMLogsDatabase()!;
  const current = (raw.prepare(`SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'llm_logs'`).get() as {
    sql: string;
  }).sql;
  const genIndexes = (
    raw
      .prepare(`SELECT sql FROM sqlite_master WHERE type = 'index' AND tbl_name = 'llm_logs' AND sql IS NOT NULL`)
      .all() as Array<{ sql: string }>
  ).map((r) => r.sql);
  await closeDatabase();
  delete process.env.SQLITE_LLM_LOGS_PATH;
  delete process.env.SQLITE_MOUNT_INDEX_PATH;
  const baseline = strip(current, [LINE]);

  const migrationModule = (await import(pathToFileURL(join(process.cwd(), MIGRATION[0].file)).href))[
    MIGRATION[0].exportName
  ];

  const runOnCopy = async (main: string, logs: string) => {
    const workDir = mkdtempSync(join(tmpdir(), 'qt-roundid-ensure-'));
    const work = join(workDir, 'work.db');
    const workLogs = join(workDir, 'work-llm-logs.db');
    copyFileSync(main, work);
    if (existsSync(logs)) copyFileSync(logs, workLogs);
    const report = await runV4Migrations({
      dbPath: work,
      pepperBase64: PEPPER,
      migrations: MIGRATION,
      runner: true,
      llmLogsPath: workLogs,
    });
    return { workDir, work, workLogs, report };
  };

  /** v4's module DIRECTLY on a separate copy, its header left as v4 leaves it. */
  const rawRun = async (logs: string): Promise<string | null> => {
    if (!existsSync(logs)) return null;
    const dir = mkdtempSync(join(tmpdir(), 'qt-roundid-raw-'));
    const copy = join(dir, 'raw-llm-logs.db');
    copyFileSync(logs, copy);
    const saved = process.env.SQLITE_LLM_LOGS_PATH;
    process.env.SQLITE_LLM_LOGS_PATH = copy;
    try {
      if (await migrationModule.shouldRun()) {
        const r = await migrationModule.run();
        if (!r.success) throw new Error(`raw run failed: ${r.error}`);
      }
      return journalMode(copy);
    } finally {
      if (saved === undefined) delete process.env.SQLITE_LLM_LOGS_PATH;
      else process.env.SQLITE_LLM_LOGS_PATH = saved;
      rmSync(dir, { recursive: true, force: true });
    }
  };

  for (const mode of ['a', 'b', 'c', 'd', 'e'] as Mode[]) {
    const main = join(outDir, `roundid-${mode}.db`);
    const logs = join(outDir, `roundid-${mode}-llm-logs.db`);
    sweep(main);
    sweep(logs);
    const m = openDb(main);
    m.pragma('journal_mode = TRUNCATE');
    m.exec('CREATE TABLE "unrelated" ("id" TEXT PRIMARY KEY NOT NULL)');
    m.close();
    if (mode !== 'b') {
      const l = openDb(logs);
      l.pragma('journal_mode = TRUNCATE');
      if (mode === 'c') {
        l.exec('CREATE TABLE "unrelated" ("id" TEXT PRIMARY KEY NOT NULL)');
      } else {
        l.exec(mode === 'e' ? current : baseline);
        for (const s of genIndexes) l.exec(s);
        for (const id of ['l1', 'l2']) {
          insertRow(l, 'llm_logs', {
            id,
            userId: 'u1',
            type: 'CHAT_MESSAGE',
            provider: 'OPENAI',
            modelName: 'gpt',
            request: '{}',
            response: '{}',
            createdAt: '2026-01-01T00:00:00.000Z',
            updatedAt: '2026-01-01T00:00:00.000Z',
          });
        }
      }
      l.close();
    }
    if (mode === 'd') {
      const first = await runOnCopy(main, logs);
      if (!first.report[0]?.startsWith('+RAN')) throw new Error(`(d) setup did not run: ${first.report}`);
      sweep(logs);
      copyFileSync(first.workLogs, logs);
      rmSync(first.workDir, { recursive: true, force: true });
    }

    const rawJournalMode = await rawRun(logs);
    const { workDir, work, workLogs, report } = await runOnCopy(main, logs);
    const mdb = openDb(work, true);
    const mainLedger = ledger(mdb);
    mdb.close();
    let row: Record<string, unknown>;
    if (existsSync(workLogs)) {
      const db = openDb(workLogs, true);
      row = {
        fileExists: true,
        tableInfo: tableInfo(db, 'llm_logs'),
        sql: tableSql(db, 'llm_logs'),
        indexSql: indexSql(db, INDEX),
        rows: cells(db, 'llm_logs', ['storytellerRoundId']),
      };
      db.close();
      row.journalMode = journalMode(workLogs);
    } else {
      row = { fileExists: false, tableInfo: [], sql: null, indexSql: null, rows: [], journalMode: null };
    }
    rmSync(workDir, { recursive: true, force: true });
    process.stdout.write(
      JSON.stringify({
        case: 'llm-logs-storyteller-round-id-ensure',
        mode,
        report,
        ...row,
        rawJournalMode,
        ledger: mainLedger,
      }) + '\n',
    );
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`llm-logs-storyteller-round-id-ensure oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
