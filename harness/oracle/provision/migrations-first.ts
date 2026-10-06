/**
 * A v4 fresh instance built the way v4's REAL first boot builds it (P4.153,
 * dogfood #149): `MigrationRunner.runMigrations()` FIRST — v4's PHASE 1,
 * `instrumentation.ts:404-437`, before any repository is touched — THEN the
 * repositories' `ensureCollection` pass `dump-fresh-schema.ts` makes, which
 * adds the generateDDL indexes with `CREATE INDEX IF NOT EXISTS` (so a name
 * the migrations already made keeps the MIGRATION's text, as on every real
 * v4 instance).
 *
 * Shared by `dump-migration-indexes.ts` (the committed artifact) and
 * `build-provision-oracle.ts` (the differential's migrations-first arm), so the
 * two can never build v4's instance two different ways.
 *
 * The caller sets the env block BEFORE importing anything from `@/` (v4 reads
 * its paths at module load): `ENCRYPTION_MASTER_PEPPER`, `QUILLTAP_DATA_DIR`,
 * the three `SQLITE_*_PATH`s at their default locations under
 * `<QUILLTAP_DATA_DIR>/data/` (the layout a real boot uses — the
 * mount-provisioning migrations open `getMountIndexDatabasePath()` themselves,
 * so the manager's path must be the same file), `LOG_LEVEL`.
 */

import { join } from 'node:path';

export const TEST_PEPPER = '3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=';

/** The env block a migrations-first build needs, rooted at `scratch`. */
export function migrationsFirstEnv(scratch: string): void {
  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;
  process.env.QUILLTAP_DATA_DIR = scratch;
  process.env.SQLITE_PATH = join(scratch, 'data', 'quilltap.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = join(scratch, 'data', 'quilltap-mount-index.db');
  process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'data', 'quilltap-llm-logs.db');
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = process.env.LOG_LEVEL ?? 'error';
}

export interface MigrationsFirstReport {
  migrationsRun: number;
  migrationsSkipped: number;
  deferred: string[];
  failed: string[];
  drove: string[];
  skipped: string[];
}

// Raw instance_settings DDL — v4 creates this key/value store by hand; the
// migration that needs it creates it first on a real boot, so this is a no-op
// there and kept only for parity with `dump-fresh-schema.ts`.
const INSTANCE_SETTINGS_DDL =
  'CREATE TABLE IF NOT EXISTS "instance_settings" (\n' +
  '  "key" TEXT PRIMARY KEY,\n' +
  '  "value" TEXT NOT NULL\n' +
  ')';

/**
 * Run v4's REAL migration runner over an empty data dir, then drive every
 * repository's first-access DDL. Throws if any migration failed (a real boot
 * exits 1 there, `instrumentation.ts:419-429`).
 */
export async function buildMigrationsFirst(): Promise<MigrationsFirstReport> {
  const { initializeDatabase, rawQuery } = await import('@/lib/database/manager');
  await initializeDatabase();

  const { MigrationRunner } = await import('@/migrations');
  const result = await new MigrationRunner().runMigrations();
  if (!result.success) {
    throw new Error(
      `v4 migrations failed on an empty data dir: ${JSON.stringify(result.failed)} ${result.error ?? ''}`,
    );
  }

  await rawQuery(INSTANCE_SETTINGS_DDL);

  const { getRepositories } = await import('@/lib/repositories/factory');
  const repos = getRepositories() as Record<string, unknown>;
  const skip = new Set(['wardrobe']);
  const drove: string[] = [];
  const skipped: string[] = [];
  for (const [key, repo] of Object.entries(repos)) {
    if (skip.has(key)) {
      skipped.push(key);
      continue;
    }
    const r = repo as { count?: () => Promise<number>; findAll?: () => Promise<unknown[]> };
    try {
      if (typeof r.count === 'function') await r.count();
      else if (typeof r.findAll === 'function') await r.findAll();
      else {
        skipped.push(`${key}(no-trigger)`);
        continue;
      }
      drove.push(key);
    } catch (err) {
      skipped.push(`${key}(${err instanceof Error ? err.message.slice(0, 60) : String(err)})`);
    }
  }
  // Secondary tables (multi-table / hand-written repos with no base count) —
  // the same four triggers `dump-fresh-schema.ts` uses.
  const anyRepos = repos as Record<string, any>;
  const zero = '00000000-0000-0000-0000-000000000000';
  await anyRepos.chats?.getMessageCount(zero);
  await anyRepos.connections?.getApiKeysByUserId(zero);
  await anyRepos.vectorIndices?.findMetaByCharacterId(zero);
  await anyRepos.docMountBlobs?.findByFileId(zero);

  return {
    migrationsRun: result.migrationsRun,
    migrationsSkipped: result.migrationsSkipped,
    deferred: result.deferred ?? [],
    failed: result.failed ?? [],
    drove,
    skipped,
  };
}

export interface IndexRow {
  name: string;
  tbl_name: string;
  sql: string;
}

/** Every named secondary index in one partition (autoindexes excluded), by name. */
export function indexRows(db: import('better-sqlite3').Database): IndexRow[] {
  return (
    db
      .prepare(
        `SELECT name, tbl_name, sql FROM sqlite_master
         WHERE type = 'index' AND sql IS NOT NULL AND name NOT LIKE 'sqlite_%'`,
      )
      .all() as IndexRow[]
  ).sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
}

/** Every table name in one partition. */
export function tableNames(db: import('better-sqlite3').Database): Set<string> {
  return new Set(
    (
      db
        .prepare(`SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'`)
        .all() as { name: string }[]
    ).map((r) => r.name),
  );
}
