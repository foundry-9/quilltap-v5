/**
 * Differential oracle — the three built-in mount stores (P4.4u3, family 2).
 *
 * Drives v4's REAL provisioning migrations (`provisionLanternBackgroundsMount` /
 * `provisionUserUploadsMount` / `provisionGeneralMount`) `run()` functions over
 * one pre-state (selected by `QT_STATE`) and dumps `doc_mount_points` +
 * `doc_mount_folders` (mount-index db) + `instance_settings` (main db). The Rust
 * port remaps the minted mount/folder ids to their names/paths and diffs.
 *
 *   - `empty`    : no pointers, no rows → mint all three fresh + subfolders.
 *   - `dangling` : pointers set to non-existent ids → re-provision fresh.
 *   - `live`     : run once (establish), then AGAIN → adopt (no new rows).
 *
 * Run once per state (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   : > /tmp/oracle-builtin-mounts.ndjson
 *   for S in empty dangling live; do
 *     QT_STATE=$S $N/npx tsx <this> >> /tmp/oracle-builtin-mounts.ndjson
 *   done
 */

import Database from 'better-sqlite3';
import { mkdtempSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const TEST_PEPPER = '3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=';
const INSTANCE_SETTINGS_DDL =
  'CREATE TABLE IF NOT EXISTS "instance_settings" (\n' +
  '  "key" TEXT PRIMARY KEY,\n' +
  '  "value" TEXT NOT NULL\n' +
  ')';

function dumpTable(db: import('better-sqlite3').Database, table: string): {
  table: string;
  columns: string[];
  rows: Array<Record<string, unknown>>;
} {
  const columns = (db.prepare(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>).map(
    (c) => c.name,
  );
  const rows = db.prepare(`SELECT * FROM "${table}"`).all() as Array<Record<string, unknown>>;
  return { table, columns, rows };
}

async function main(): Promise<void> {
  const stateName = process.env.QT_STATE;
  if (!['empty', 'dangling', 'live'].includes(stateName ?? '')) {
    throw new Error('QT_STATE must be one of: empty, dangling, live');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-builtin-mounts-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainPath = join(scratch, 'quilltap.db');
  const miPath = join(scratch, 'data', 'quilltap-mount-index.db');

  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;
  process.env.SQLITE_PATH = mainPath;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, rawQuery, closeDatabase } = await import('@/lib/database/manager');
  const { provisionLanternBackgroundsMountMigration } = await import(
    '@/migrations/scripts/provision-lantern-backgrounds-mount'
  );
  const { provisionUserUploadsMountMigration } = await import(
    '@/migrations/scripts/provision-user-uploads-mount'
  );
  const { provisionGeneralMountMigration } = await import(
    '@/migrations/scripts/provision-general-mount'
  );

  await initializeDatabase();
  await rawQuery(INSTANCE_SETTINGS_DDL);

  if (stateName === 'dangling') {
    // Pointers set to ids that have no matching mount row → re-provision.
    for (const [key, val] of [
      ['lanternBackgroundsMountPointId', 'dangling-lantern'],
      ['userUploadsMountPointId', 'dangling-uploads'],
      ['generalMountPointId', 'dangling-general'],
    ]) {
      await rawQuery(
        'INSERT INTO "instance_settings" ("key","value") VALUES (?,?) ON CONFLICT("key") DO UPDATE SET "value" = excluded."value"',
        [key, val],
      );
    }
  }

  const runAll = async () => {
    // Registration order Lantern → Uploads → General (matches v5).
    await provisionLanternBackgroundsMountMigration.run();
    await provisionUserUploadsMountMigration.run();
    await provisionGeneralMountMigration.run();
  };

  await runAll();
  if (stateName === 'live') {
    // Second pass over an already-provisioned instance → adopt (no duplicates).
    await runAll();
  }

  // instance_settings lives in the manager-held main db — read it through rawQuery.
  const isColumns = (
    (await rawQuery('PRAGMA table_info(instance_settings)')) as Array<{ name: string }>
  ).map((c) => c.name);
  const isRows = (await rawQuery('SELECT * FROM "instance_settings"')) as Array<
    Record<string, unknown>
  >;
  const instanceSettings = { table: 'instance_settings', columns: isColumns, rows: isRows };

  await closeDatabase();

  // The mount-index db was opened + closed by the migrations; re-open it fresh.
  const mi = openKeyed(miPath);
  const docMountPoints = dumpTable(mi, 'doc_mount_points');
  const docMountFolders = dumpTable(mi, 'doc_mount_folders');
  mi.close();

  process.stdout.write(
    JSON.stringify({
      case: 'builtin-mounts',
      state: stateName,
      docMountPoints,
      docMountFolders,
      instanceSettings,
    }) + '\n',
  );
  process.exit(0);
}

/** Open an encrypted db read-only-ish with the test pepper (ChaCha20/sqleet). */
function openKeyed(path: string): import('better-sqlite3').Database {
  const db = new Database(path);
  const keyHex = Buffer.from(TEST_PEPPER, 'base64').toString('hex');
  db.pragma(`key = "x'${keyHex}'"`);
  return db;
}

main().catch((err) => {
  process.stderr.write(`builtin-mounts oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
