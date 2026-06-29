/**
 * Tier-2 fixture builder — materializes the shared starting **mount-index**
 * sibling DB for the `doc_mount_points` repo from the committed plaintext spec
 * (`doc-mount-points-tier2.json`).
 *
 * ── THE SIBLING-DB MACHINERY (mirrors the group_character_members pilot) ──────
 * This repo's data lives in v4's dedicated mount-index database
 * (`quilltap-mount-index.db`), NOT the main DB. v4 resolves that file from
 * `SQLITE_MOUNT_INDEX_PATH` (or `<dataDir>/quilltap-mount-index.db`), keyed with
 * the SAME `ENCRYPTION_MASTER_PEPPER`. So the recipe is:
 *   - point SQLITE_MOUNT_INDEX_PATH at the fixture we want to KEEP (`QT_FIXTURE_OUT`);
 *   - point SQLITE_PATH at a THROWAWAY main DB in the scratch dir (initializeDatabase
 *     still stands the main backend up first; we just never read it);
 *   - seed through the REAL `DocMountPointsRepository`, whose overridden
 *     `getCollection()` creates the table in — and writes to — the mount-index DB
 *     (lazy `CREATE TABLE IF NOT EXISTS` on first access; no explicit
 *     ensureCollection needed, unlike the main-DB builders).
 * Then flush the mount-index handle explicitly: the backend disconnect closes the
 * main + llm-logs DBs but NOT the mount-index client, so we call
 * `closeMountIndexSQLiteClient()` ourselves to checkpoint + release before exit.
 *
 * FRESH-FIXTURE NOTE: getCollection()'s four runtime ALTER-TABLE migrations
 * (totalSizeBytes, conversionStatus, conversionError, storeType) are NO-OPS here —
 * generateDDL emits all four columns from the current schema, so every migration
 * guard is false. The fixture has EXACTLY the 18 schema columns.
 *
 * journal_mode is TRUNCATE on both DBs (SQLITE_WAL_MODE unset → walMode=false →
 * journalMode default 'truncate'), so each committed transaction is self-contained
 * in the `.db` file — the Rust `Writer::open_writable` (also TRUNCATE) then opens
 * the fixture copy directly.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-dmp-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-doc-mount-points-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedRow {
  id: string;
  name: string;
  basePath: string;
  mountType: string;
  storeType: string;
  includePatterns: string[];
  excludePatterns: string[];
  enabled: boolean;
  lastScannedAt: string | null;
  scanStatus: string;
  lastScanError: string | null;
  conversionStatus: string;
  conversionError: string | null;
  fileCount: number;
  chunkCount: number;
  totalSizeBytes: number;
  createdAt: string;
  updatedAt: string;
}

interface Spec {
  testPepperBase64: string;
  seed: SeedRow[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'doc-mount-points-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the mount-index fixture .db to write');
  }

  // Fresh output: drop any prior fixture so we never seed on top of stale state.
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  // Throwaway data dir absorbs v4's operational scaffolding (instance lock,
  // startup physical backups, the THROWAWAY main + llm-logs DBs). A unique dir
  // per run avoids stale-lock collisions. The MOUNT-INDEX db lands at QT_FIXTURE_OUT.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-dmp-fixture-build-'));
  // v4 nests working files under `<dataDir>/data/`; pre-create it so the
  // instance lock + throwaway main DB have a home.
  mkdirSync(join(scratch, 'data'), { recursive: true });

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db'); // throwaway main DB
  process.env.SQLITE_MOUNT_INDEX_PATH = out; // the fixture we KEEP
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE; // both DBs use journal_mode = TRUNCATE
  process.env.LOG_LEVEL = 'error'; // keep stdout/stderr quiet for clean runs

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { DocMountPointsRepository } = await import(
    '@/lib/database/repositories/doc-mount-points.repository'
  );

  await initializeDatabase();

  // The repo's overridden getCollection() routes to the mount-index DB and
  // creates the table on first access — no explicit ensureCollection needed.
  const repo = new DocMountPointsRepository();
  for (const row of spec.seed) {
    await repo.create(
      {
        name: row.name,
        basePath: row.basePath,
        mountType: row.mountType,
        storeType: row.storeType,
        includePatterns: row.includePatterns,
        excludePatterns: row.excludePatterns,
        enabled: row.enabled,
        lastScannedAt: row.lastScannedAt,
        scanStatus: row.scanStatus,
        lastScanError: row.lastScanError,
        conversionStatus: row.conversionStatus,
        conversionError: row.conversionError,
        fileCount: row.fileCount,
        chunkCount: row.chunkCount,
        totalSizeBytes: row.totalSizeBytes,
      } as never,
      { id: row.id, createdAt: row.createdAt, updatedAt: row.updatedAt }
    );
  }

  // Flush + close the mount-index handle ourselves (backend disconnect doesn't).
  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built doc_mount_points mount-index fixture: ${out} (${spec.seed.length} seed rows)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
