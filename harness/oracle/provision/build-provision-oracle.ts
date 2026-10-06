/**
 * Provisioning differential oracle (P4.4 unit 1, the provisioning proof).
 *
 * Builds a v4 FRESH instance the way v4's REAL first boot does (P4.153 —
 * `migrations-first.ts`: v4's real `MigrationRunner` over an empty data dir,
 * then the repositories' first-access `ensureCollection` pass), then v4's real `getOrCreateSingleUser` + the seed-embedding-profile path write
 * the deterministic first-boot seed (the sample-content import + roleplay
 * templates + built-in mount stores are the P4.4 named deferrals) — and emits:
 *
 *   - QT_ORACLE_PROVISION (JSON): `{ schema, indexes, columns, migrations,
 *     seed, seeded }`. `schema` is the LIVE `sqlite_master` (tables now in the
 *     MIGRATION text, which v5 deliberately does not reproduce — P4.153 R-A);
 *     `indexes` is every named index per partition `{name, tbl_name, sql}` (both
 *     v4 families, the comparand of the index arm); `columns` is each table's
 *     sorted column names (the table arm compares column SETS); `migrations`
 *     is the runner's run/skipped/deferred/failed report; the seed rows have their minted id/createdAt/updatedAt stripped
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

import { buildMigrationsFirst, indexRows, migrationsFirstEnv } from './migrations-first';

const SINGLE_USER_ID = 'ffffffff-ffff-ffff-ffff-ffffffffffff';

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
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  // P4.153: the real-boot layout — all three partitions under `data/`, so the
  // manager and the mount-provisioning migrations (which open
  // `getMountIndexDatabasePath()` themselves) resolve to the SAME files.
  migrationsFirstEnv(scratch);
  const mainPath = process.env.SQLITE_PATH as string;
  const miPath = process.env.SQLITE_MOUNT_INDEX_PATH as string;
  const llPath = process.env.SQLITE_LLM_LOGS_PATH as string;

  // P4.153 (dogfood #149): v4's fresh instance is built the way its REAL first
  // boot builds it — `MigrationRunner.runMigrations()` FIRST (PHASE 1), then the
  // repositories' `ensureCollection` pass. Before P4.153 this oracle drove the
  // repositories alone, so its `sqlite_master` was the generateDDL surface and
  // the migration-created index family (`idx_chat_messages_chatId`, …) was
  // invisible to the differential.
  const report = await buildMigrationsFirst();
  const { closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const anyRepos = getRepositories() as Record<string, any>;

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

  // --- P4.4u3 family 1: built-in roleplay templates ---
  // v4's every-startup `seedBuiltInTemplates` (a repo op through the manager).
  // Family 2 (the three built-in mount stores) needs no by-hand call any more:
  // the three `provision-*-mount` migrations ran inside the runner above, as on
  // a real first boot.
  await anyRepos.roleplayTemplates.seedBuiltInTemplates();

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
  // P4.153: every named index per partition (name, table, SQL) and every
  // table's column-name list — the Rust side compares the index family by name
  // + SQL and the tables by column SET (the TABLE text is the migration's here,
  // the generateDDL one in v5 — the recorded asymmetry the order's R-A keeps).
  const columnsOf = (db: import('better-sqlite3').Database) => {
    const out: Record<string, string[]> = {};
    const tables = db
      .prepare(`SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'`)
      .all() as { name: string }[];
    for (const { name } of tables.sort((a, b) => (a.name < b.name ? -1 : 1))) {
      out[name] = (db.prepare(`PRAGMA table_info("${name}")`).all() as { name: string }[])
        .map((c) => c.name)
        .sort();
    }
    return out;
  };
  const indexes = {
    main: indexRows(getRawDatabase()),
    mountIndex: indexRows(getRawMountIndexDatabase()),
    llmLogs: indexRows(getRawLLMLogsDatabase()),
  };
  const columns = {
    main: columnsOf(getRawDatabase()),
    mountIndex: columnsOf(getRawMountIndexDatabase()),
    llmLogs: columnsOf(getRawLLMLogsDatabase()),
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

  writeFileSync(oracleOut, JSON.stringify({ schema, indexes, columns, migrations: report, seed, seeded }, null, 2));

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
      `llm-logs=${schema.llmLogs.length} DDL; migrations run=${report.migrationsRun} ` +
      `skipped=${report.migrationsSkipped} deferred=[${report.deferred.join(', ')}]; seed rows: users/chat_settings/embedding → ${oracleOut}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`provision oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
