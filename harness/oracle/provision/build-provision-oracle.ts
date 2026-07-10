/**
 * Provisioning differential oracle (P4.4 unit 1, the provisioning proof).
 *
 * Builds a v4 FRESH instance the tier-2-fixture way — v4's real repositories
 * create the generateDDL schema across all three partitions on first access,
 * then v4's real `getOrCreateSingleUser` + the seed-embedding-profile path write
 * the deterministic first-boot seed (the sample-content import + roleplay
 * templates + built-in mount stores are the P4.4 named deferrals) — and emits:
 *
 *   - QT_ORACLE_PROVISION (JSON): `{ schema: {main, mountIndex, llmLogs},
 *     seed: { users, chatSettings, embeddingProfile } }`. The schema is the LIVE
 *     `sqlite_master` (so the differential catches v4 drift, not just replay
 *     fidelity); the seed rows have their minted id/createdAt/updatedAt stripped
 *     (the harness compares the deterministic remainder).
 *   - QT_V4_FRESH_OUT (dir): the three encrypted v4-fresh `.db` files, so the
 *     Rust differential can prove a v4-built instance opens under the v5 engine
 *     (cross-compat, one direction; the other — v4 reads a v5-provisioned
 *     instance — is `verify-v5-provisioned.ts`).
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_PROVISION=/tmp/oracle-provision.json \
 *   QT_V4_FRESH_OUT=/tmp/qt-v4-fresh \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/provision/build-provision-oracle.ts
 */

import { mkdtempSync, mkdirSync, writeFileSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const TEST_PEPPER = '3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=';
const SINGLE_USER_ID = 'ffffffff-ffff-ffff-ffff-ffffffffffff';

const INSTANCE_SETTINGS_DDL =
  'CREATE TABLE IF NOT EXISTS "instance_settings" (\n' +
  '  "key" TEXT PRIMARY KEY,\n' +
  '  "value" TEXT NOT NULL\n' +
  ')';

interface SchemaRow {
  type: string;
  name: string;
  sql: string;
}

function dumpPartition(db: import('better-sqlite3').Database): string[] {
  const rows = db
    .prepare(
      `SELECT type, name, sql FROM sqlite_master
       WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%'`,
    )
    .all() as SchemaRow[];
  const cmp = (a: SchemaRow, b: SchemaRow) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0);
  const tables = rows.filter((r) => r.type === 'table').sort(cmp).map((r) => r.sql);
  const indexes = rows.filter((r) => r.type === 'index').sort(cmp).map((r) => r.sql);
  return [...tables, ...indexes];
}

function stripMinted(row: Record<string, unknown>, extra: string[] = []): Record<string, unknown> {
  const drop = new Set(['id', 'createdAt', 'updatedAt', ...extra]);
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(row)) {
    if (!drop.has(k)) out[k] = v ?? null;
  }
  return out;
}

async function main(): Promise<void> {
  const oracleOut = process.env.QT_ORACLE_PROVISION;
  const v4FreshOut = process.env.QT_V4_FRESH_OUT;
  if (!oracleOut) throw new Error('QT_ORACLE_PROVISION must point at the JSON to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-provision-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainPath = join(scratch, 'quilltap.db');
  // The mount-index sits under `data/` so `SQLITE_MOUNT_INDEX_PATH` (the manager)
  // and `getMountIndexDatabasePath()` (the mount-provisioning migrations) resolve
  // to the SAME file — otherwise the migrations would write a different db than
  // the manager reads.
  const miPath = join(scratch, 'data', 'quilltap-mount-index.db');
  const llPath = join(scratch, 'quilltap-llm-logs.db');

  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;
  process.env.SQLITE_PATH = mainPath;
  process.env.SQLITE_MOUNT_INDEX_PATH = miPath;
  process.env.SQLITE_LLM_LOGS_PATH = llPath;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, rawQuery, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();
  await rawQuery(INSTANCE_SETTINGS_DDL);

  const repos = getRepositories() as Record<string, unknown>;
  const skip = new Set(['wardrobe']);
  for (const [key, repo] of Object.entries(repos)) {
    if (skip.has(key)) continue;
    const r = repo as { count?: () => Promise<number>; findAll?: () => Promise<unknown[]> };
    try {
      if (typeof r.count === 'function') await r.count();
      else if (typeof r.findAll === 'function') await r.findAll();
    } catch {
      /* vault-only / no table — not part of a fresh schema */
    }
  }
  // Secondary tables (multi-table / hand-written repos with no base count).
  const anyRepos = repos as Record<string, any>;
  const zero = '00000000-0000-0000-0000-000000000000';
  await anyRepos.chats?.getMessageCount(zero);
  await anyRepos.connections?.getApiKeysByUserId(zero);
  await anyRepos.vectorIndices?.findMetaByCharacterId(zero);
  await anyRepos.docMountBlobs?.findByFileId(zero);

  // --- the deterministic first-boot seed ---
  // v4's `users.create` (called with no options.id) MINTS an id, so a single
  // getOrCreateSingleUser stores a minted-id user; v4's real boot calls it
  // multiple times (reconcileFilesystem + the FS watcher), and the 2nd call
  // finds the row by email and migrates it to SINGLE_USER_ID. Two calls converge
  // to the stable state a real v4 instance carries (id = SINGLE_USER_ID), which
  // is what the v5 core provisions directly.
  const { getOrCreateSingleUser } = await import('@/lib/auth/single-user');
  await getOrCreateSingleUser();
  await getOrCreateSingleUser(); // users (migrated to SINGLE_USER_ID) + chat_settings
  const { getSeedEmbeddingProfiles, prepareSeedEmbeddingProfile } = await import('@/first-startup');
  await anyRepos.embeddingProfiles.create(
    prepareSeedEmbeddingProfile(getSeedEmbeddingProfiles()[0], SINGLE_USER_ID),
  );

  // --- P4.4u3 families 1 & 2: built-in roleplay templates + mount stores ---
  // v4's every-startup `seedBuiltInTemplates` (a repo op through the manager) and
  // the three mount-provisioning migrations' `run()` (they read/write the settings
  // pointers via getSQLiteDatabase and open their own mount-index connection at
  // getMountIndexDatabasePath, aligned above with SQLITE_MOUNT_INDEX_PATH).
  await anyRepos.roleplayTemplates.seedBuiltInTemplates();
  const { provisionLanternBackgroundsMountMigration } = await import(
    '@/migrations/scripts/provision-lantern-backgrounds-mount'
  );
  const { provisionUserUploadsMountMigration } = await import(
    '@/migrations/scripts/provision-user-uploads-mount'
  );
  const { provisionGeneralMountMigration } = await import(
    '@/migrations/scripts/provision-general-mount'
  );
  await provisionLanternBackgroundsMountMigration.run();
  await provisionUserUploadsMountMigration.run();
  await provisionGeneralMountMigration.run();

  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawLLMLogsDatabase } = await import('@/lib/database/backends/sqlite/llm-logs-client');

  const schema = {
    main: dumpPartition(getRawDatabase()),
    mountIndex: dumpPartition(getRawMountIndexDatabase()),
    llmLogs: dumpPartition(getRawLLMLogsDatabase()),
  };

  const main = getRawDatabase();
  const userRow = main.prepare('SELECT * FROM users WHERE id = ?').get(SINGLE_USER_ID) as Record<
    string,
    unknown
  >;
  const settingsRow = main
    .prepare('SELECT * FROM chat_settings WHERE userId = ?')
    .get(SINGLE_USER_ID) as Record<string, unknown>;
  const epRow = main
    .prepare("SELECT * FROM embedding_profiles WHERE provider = 'BUILTIN'")
    .get() as Record<string, unknown>;

  const seed = {
    // users id IS fixed (SINGLE_USER_ID) — keep it; strip timestamps only.
    users: stripMinted(userRow, []),
    chatSettings: stripMinted(settingsRow, ['id', 'userId']),
    embeddingProfile: stripMinted(epRow, ['id']),
  };
  // Re-add the fixed userId to users for an explicit compare.
  (seed.users as Record<string, unknown>).id = SINGLE_USER_ID;

  // --- P4.4u3 seeded tables (raw dumps; the Rust side remaps minted ids) ---
  const dumpRawTable = (db: import('better-sqlite3').Database, table: string) => {
    const columns = (
      db.prepare(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>
    ).map((c) => c.name);
    const rows = db.prepare(`SELECT * FROM "${table}"`).all() as Array<Record<string, unknown>>;
    return { table, columns, rows };
  };
  const mi = getRawMountIndexDatabase();
  const seeded = {
    roleplayTemplates: dumpRawTable(main, 'roleplay_templates'),
    docMountPoints: dumpRawTable(mi, 'doc_mount_points'),
    docMountFolders: dumpRawTable(mi, 'doc_mount_folders'),
    instanceSettings: dumpRawTable(main, 'instance_settings'),
  };

  writeFileSync(oracleOut, JSON.stringify({ schema, seed, seeded }, null, 2));

  await closeDatabase();

  // Copy the encrypted v4-fresh .db files for the cross-compat (v5 reads v4).
  if (v4FreshOut) {
    mkdirSync(v4FreshOut, { recursive: true });
    for (const [src, name] of [
      [mainPath, 'quilltap.db'],
      [miPath, 'quilltap-mount-index.db'],
      [llPath, 'quilltap-llm-logs.db'],
    ] as [string, string][]) {
      const dest = join(v4FreshOut, name);
      if (existsSync(dest)) rmSync(dest);
      copyFileSync(src, dest);
    }
    process.stderr.write(`wrote v4-fresh instance → ${v4FreshOut}\n`);
  }

  process.stderr.write(
    `provision oracle: main=${schema.main.length} mount-index=${schema.mountIndex.length} ` +
      `llm-logs=${schema.llmLogs.length} DDL; seed rows: users/chat_settings/embedding → ${oracleOut}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`provision oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
