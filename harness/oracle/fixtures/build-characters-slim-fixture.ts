/**
 * Tier-2 fixture builder — materializes the shared starting DB for the
 * `characters` SLIM ROW (Phase-2, main DB), from the committed plaintext spec
 * (`characters-slim-tier2.json`).
 *
 * Same shape as build-wardrobe-fixture.ts: v4's `CharactersRepository` public
 * create/update orchestrate the character vault, so we cannot seed through the
 * public repo. Instead we drive v4's REAL protected slim-row internals
 * (`_create`) via a thin local subclass (`CharactersSqlRepo`) that extends the
 * real repo and exposes them. `_create` is v4's vault-aware override that strips
 * MANAGED_FIELDS before the INSERT — exactly the slim-row marshaling under test.
 *
 * The `characters` table is created by v4's OWN
 * `ensureCollection('characters', CharacterSchema)` so the DDL (column set/order,
 * the JSON columns, the boolean/enum columns) is identical to production by
 * construction. The managed columns exist in the DDL but both v4 and the Rust
 * port omit them from every write, so they sit at their DDL defaults identically.
 * Seed rows are inserted with id + timestamps pinned (CreateOptions), so the
 * starting state is fully deterministic.
 *
 * The output file is the SEED-ONLY starting state. The op sequence under test is
 * applied later, by `cases/characters-slim.ts` (oracle) and the Rust harness,
 * each on its own fresh copy.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-characters-slim-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-characters-slim-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedEntry {
  data: Record<string, unknown>;
  options: { id: string; createdAt: string; updatedAt: string };
}

interface Spec {
  testPepperBase64: string;
  seed: SeedEntry[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'characters-slim-tier2.json');
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
  // startup physical backup, sibling llm-logs / mount-index DBs). A unique dir
  // per run avoids stale-lock collisions. The MAIN db still lands at SQLITE_PATH.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-characters-slim-fixture-build-'));
  // v4 nests its working files under `<dataDir>/data/` (instance lock, sibling DBs).
  mkdirSync(join(scratch, 'data'), { recursive: true });

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE; // writable path uses journal_mode = TRUNCATE
  process.env.LOG_LEVEL = 'error'; // keep stdout/stderr quiet for clean runs

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { CharactersRepository } = await import(
    '@/lib/database/repositories/characters.repository'
  );
  const { CharacterSchema } = await import('@/lib/schemas/types');

  // v4's public create/update orchestrate the vault; to exercise the REAL
  // slim-row SQL CRUD we subclass the real repo and expose its protected
  // internals. `_create` is the vault-aware override (strips MANAGED_FIELDS).
  class CharactersSqlRepo extends CharactersRepository {
    async createSlim(data: unknown, options: unknown) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      return (this as any)._create(data, options);
    }
    async updateSlim(id: string, data: unknown) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      return (this as any)._update(id, data);
    }
    async deleteSlim(id: string) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      return (this as any)._delete(id);
    }
  }

  await initializeDatabase();
  await ensureCollection('characters', CharacterSchema);

  const repo = new CharactersSqlRepo();
  for (const entry of spec.seed) {
    await repo.createSlim(entry.data, entry.options);
  }

  await closeDatabase();

  process.stderr.write(
    `built characters slim fixture: ${out} (${spec.seed.length} seed rows)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
