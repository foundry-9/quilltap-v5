/**
 * Cross-compat proof #4 (P4.4 unit 1): v4's REAL code opens a v5-provisioned
 * instance and reads it. The Rust differential
 * (`provisioning_equivalence.rs`) writes a v5 `Setup`-provisioned instance to
 * `QT_FIXTURE_V5_PROVISIONED` (all three encrypted partitions, keyed by the
 * shared test pepper); this script points v4's real repositories at it and
 * asserts the seed reads back — the single user, an empty chat list, and the
 * default BUILTIN embedding profile.
 *
 * Run from the v4 server checkout under Node 24, AFTER the Rust differential
 * has written the instance:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_V5_PROVISIONED=/tmp/qt-v5-provisioned \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/provision/verify-v5-provisioned.ts
 *
 * Exits 0 on success (prints OK), non-zero on any read/assert failure.
 */

import { existsSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';

const TEST_PEPPER = '3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=';
const SINGLE_USER_ID = 'ffffffff-ffff-ffff-ffff-ffffffffffff';

function assert(cond: unknown, msg: string): void {
  if (!cond) throw new Error(`ASSERT FAILED: ${msg}`);
}

async function main(): Promise<void> {
  const dir = process.env.QT_FIXTURE_V5_PROVISIONED;
  if (!dir) throw new Error('QT_FIXTURE_V5_PROVISIONED must point at the v5-provisioned instance dir');
  const mainPath = join(dir, 'quilltap.db');
  const miPath = join(dir, 'quilltap-mount-index.db');
  const llPath = join(dir, 'quilltap-llm-logs.db');
  for (const p of [mainPath, miPath, llPath]) {
    if (!existsSync(p)) throw new Error(`missing partition: ${p} (run the Rust differential first)`);
  }

  // v4's SQLite backend creates its instance lock under `<dataDir>/data/`.
  mkdirSync(join(dir, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;
  process.env.SQLITE_PATH = mainPath;
  process.env.SQLITE_MOUNT_INDEX_PATH = miPath;
  process.env.SQLITE_LLM_LOGS_PATH = llPath;
  process.env.QUILLTAP_DATA_DIR = dir;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();
  const repos = getRepositories() as any;

  // The single user reads back under v4's real UsersRepository.
  const user = await repos.users.findById(SINGLE_USER_ID);
  assert(user, 'v4 reads the single user from the v5 instance');
  assert(user.username === 'localUser', `username localUser (got ${user?.username})`);
  assert(user.email === 'user@localhost.localdomain', `email (got ${user?.email})`);

  // chats is empty (fresh instance).
  const chats = await repos.chats.findByUserId(SINGLE_USER_ID);
  assert(Array.isArray(chats) && chats.length === 0, `chats empty (got ${chats?.length})`);

  // chat_settings reads back and the nested defaults parse (v4 Zod).
  const settings = await repos.chatSettings.findByUserId(SINGLE_USER_ID);
  assert(settings, 'v4 reads chat_settings from the v5 instance');
  assert(settings.avatarDisplayMode === 'ALWAYS', 'avatarDisplayMode ALWAYS');

  // The default BUILTIN embedding profile is present + default.
  const profiles = await repos.embeddingProfiles.findAll();
  const builtin = profiles.find((p: any) => p.provider === 'BUILTIN');
  assert(builtin, 'v4 reads the default embedding profile from the v5 instance');
  assert(builtin.isDefault === true, 'embedding profile isDefault');
  assert(builtin.modelName === 'tfidf-bm25-v1', `modelName (got ${builtin?.modelName})`);

  // The mount-index + llm-logs partitions open under v4 too (raw handles).
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawLLMLogsDatabase } = await import('@/lib/database/backends/sqlite/llm-logs-client');
  const miTables = (getRawMountIndexDatabase()
    .prepare("SELECT count(*) c FROM sqlite_master WHERE type='table'")
    .get() as { c: number }).c;
  const llTables = (getRawLLMLogsDatabase()
    .prepare("SELECT count(*) c FROM sqlite_master WHERE type='table'")
    .get() as { c: number }).c;
  assert(miTables >= 10, `mount-index tables present (got ${miTables})`);
  assert(llTables >= 1, `llm-logs table present (got ${llTables})`);

  await closeDatabase();

  process.stderr.write('OK: v4 opened and read the v5-provisioned instance\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`v4-reads-v5 cross-compat FAILED: ${err?.stack ?? err}\n`);
  process.exit(1);
});
