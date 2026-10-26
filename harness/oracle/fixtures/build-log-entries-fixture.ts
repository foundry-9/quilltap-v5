/**
 * Tier-2 fixture builder (P4.D331) — materializes the starting LLM-LOGS
 * sibling DB for the Log's repository (`log_entries`) from the committed spec
 * `log-entries-tier2.json`, through v4's REAL `LogEntriesRepository` at the
 * `322a5488e` pin.
 *
 * The llm-logs machinery is `build-llm-logs-fixture.ts`'s: SQLITE_LLM_LOGS_PATH
 * at the file we KEEP (`QT_FIXTURE_OUT`), throwaway main + mount-index DBs in
 * scratch, and `closeDatabase()` flushing the llm-logs client. The table
 * arrives through the repository's OWN ensure (generateDDL + `onTableEnsured`),
 * so it carries v4's three indexes in v4's spellings — the builder never runs
 * DDL of its own. Seed ids and stamps are pinned through `CreateOptions`; the
 * file is rebuilt by every recipe (never committed).
 *
 * Run from the pinned v4 checkout under Node 24:
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-log-entries-fixture.db \
 *     npx tsx $V5W/harness/oracle/fixtures/build-log-entries-fixture.ts
 * (`QT_FIXTURE_SEED=tools` for log_tools_equivalence's seed.)
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

type Msg = string | { repeat: string; count: number; then?: string };

function expandMessage(m: Msg): string {
  return typeof m === 'string' ? m : m.repeat.repeat(m.count) + (m.then ?? '');
}

interface Spec {
  testPepperBase64: string;
  seed: Array<{
    id: string;
    message: Msg;
    chatId: string;
    characterId: string | null;
    createdAt: string;
  }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const whole = JSON.parse(readFileSync(join(here, 'log-entries-tier2.json'), 'utf8')) as Spec & {
    tools: Spec;
  };
  // `QT_FIXTURE_SEED=tools` builds log_tools_equivalence's file instead: the
  // `tools` seed, keyed with the chat-dialogs pair's pepper.
  const spec: Spec = process.env.QT_FIXTURE_SEED === 'tools' ? whole.tools : whole;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) throw new Error('QT_FIXTURE_OUT must point at the llm-logs fixture .db to write');
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    if (existsSync(out + suffix)) rmSync(out + suffix);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-log-entries-build-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = join(scratch, 'data', 'mount-index.db');
  process.env.SQLITE_LLM_LOGS_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { LogEntriesRepository } = await import(
    '@/lib/database/repositories/log-entries.repository'
  );

  await initializeDatabase();
  const repo = new LogEntriesRepository();
  for (const row of spec.seed) {
    await repo.create(
      { message: expandMessage(row.message), chatId: row.chatId, characterId: row.characterId },
      { id: row.id, createdAt: row.createdAt, updatedAt: row.createdAt },
    );
  }
  await closeDatabase();

  process.stderr.write(`built log_entries llm-logs fixture: ${out} (${spec.seed.length} seed rows)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
