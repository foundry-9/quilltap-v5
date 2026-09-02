/**
 * Tier-2 fixture builder for the `folders` MINTED-VALUES (remap) case.
 *
 * Same machinery as build-folders-fixture.ts, but the seed is EMPTY: the only
 * rows under test are the two minted-id folders created by the op sequence
 * (cases/folders-remap-tier2.ts + the Rust harness). The builder still runs v4's
 * own `ensureCollection('folders', FolderSchema)` so the table DDL is identical
 * to production, then writes the encrypted seed-only DB under the throwaway test
 * pepper.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-folders-remap-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-folders-remap-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedRow {
  id: string;
  userId: string;
  path: string;
  name: string;
  parentFolderId: string | null;
  projectId: string | null;
  createdAt: string;
  updatedAt: string;
}

interface Spec {
  testPepperBase64: string;
  seed?: SeedRow[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'folders-remap-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the fixture .db to write');
  }

  // Fresh output: drop any prior fixture so we never seed on top of stale state.
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  // Throwaway data dir absorbs v4's operational scaffolding (instance lock,
  // startup physical backup, sibling DBs). The MAIN db lands at SQLITE_PATH.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-folders-remap-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE; // writable path uses journal_mode = TRUNCATE
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { FolderSchema } = await import('@/lib/schemas/folder.types');

  await initializeDatabase();
  await ensureCollection('folders', FolderSchema);

  // THE TWO-VINTAGE SEAM (v4 `a5df98b3f`, bug 114). `ensureCollection` builds
  // the table from `generateDDL`, which builds indexes from a plain column list
  // and CANNOT express `COALESCE(...)` — so the fixture is pre-index by
  // construction and the `ensureByPath` constraint arm would be unreachable.
  // This is the statement v4's `sqlite-initial-schema` `SQLITE_TABLES` now runs
  // for a fresh instance, and byte-for-byte the one v5's boot ensure creates.
  await rawQuery(
    `CREATE UNIQUE INDEX IF NOT EXISTS "idx_folders_userId_projectId_path" ` +
      `ON "folders" ("userId", COALESCE("projectId", ''), "path")`
  );

  // One seeded row whose `projectId` is the EMPTY STRING. It is the only shape
  // where `findByPath` (which reads `projectId IS NULL`) and the index (which
  // reads `COALESCE(projectId, '')`) disagree, so it is what makes
  // `ensureByPath`'s "unique conflict with nothing to reconcile to" arm
  // reachable from a plain sequential op list — no race required. Without it
  // that arm is unreachable on BOTH sides and v4's catch/re-read/rethrow branch
  // is never driven by any differential.
  //
  // Seeded with RAW SQL, not the repository: MEASURED at the `a5df98b3f` pin —
  // v4's `FolderSchema.projectId` is a UUID-validated string, so `repo.create`
  // refuses `''` with a ZodError. The row is therefore synthetic (v4 cannot
  // write one), but SQLite can hold it and both sides face the identical bytes,
  // which is what the arm needs.
  for (const row of spec.seed ?? []) {
    await rawQuery(
      `INSERT INTO folders (id, userId, path, name, parentFolderId, projectId, ` +
        `createdAt, updatedAt) VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
      [
        row.id,
        row.userId,
        row.path,
        row.name,
        row.parentFolderId,
        row.projectId,
        row.createdAt,
        row.updatedAt,
      ]
    );
  }

  await closeDatabase();

  process.stderr.write(
    `built folders remap fixture: ${out} (${(spec.seed ?? []).length} seed rows, unique index)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
