/**
 * Tier-2 fixture builder — materializes the shared starting DB for the
 * `wardrobe` repo (Phase-2) over the `wardrobe_items` table, from the committed
 * plaintext spec (`wardrobe-tier2.json`).
 *
 * Same shape as build-prompt-templates-fixture.ts, with ONE difference:
 * v4's `WardrobeRepository` overrides create/update/delete to be VAULT-ONLY (no
 * SQL write mirror — they throw without a document-store mount). So this builder
 * cannot seed through the public repo. Instead it drives v4's REAL base
 * repository SQL CRUD via a thin local subclass (`WardrobeSqlRepo`) that exposes
 * the protected `_create`. That is the actual SQL marshaling path the
 * schema-translator + base repository define for `wardrobe_items` (and that the
 * table's reads — `findByCharacterIdRaw` -> `findByFilter` — consume).
 *
 * The `wardrobe_items` table is created by v4's OWN
 * `ensureCollection('wardrobe_items', WardrobeItemSchema)` so the DDL (column
 * set/order, the two JSON-array columns, the two boolean columns, the nullable
 * `archivedAt` timestamp) is identical to production by construction. Seed rows
 * are inserted with id + timestamps pinned (CreateOptions), so the starting state
 * is fully deterministic.
 *
 * The output file is the SEED-ONLY starting state. The op sequence under test is
 * applied later, by `cases/wardrobe-tier2.ts` (oracle) and the Rust harness, each
 * on its own fresh copy.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-wardrobe-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-wardrobe-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedRow {
  id: string;
  characterId: string | null;
  title: string;
  description: string | null;
  imagePrompt: string | null;
  types: string[];
  componentItemIds: string[];
  appropriateness: string | null;
  isDefault: boolean;
  replace: boolean;
  migratedFromClothingRecordId: string | null;
  archivedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

interface Spec {
  testPepperBase64: string;
  seed: SeedRow[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'wardrobe-tier2.json');
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
  const scratch = mkdtempSync(join(tmpdir(), 'qt-wardrobe-fixture-build-'));
  // v4 nests its working files under `<dataDir>/data/` (instance lock, sibling
  // DBs). Pre-create it so `acquireInstanceLock` can open the lock file.
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
  const { AbstractBaseRepository } = await import(
    '@/lib/database/repositories/base.repository'
  );
  const { WardrobeItemSchema } = await import('@/lib/schemas/wardrobe.types');

  // v4's WardrobeRepository is vault-only; to exercise the REAL base-repository
  // SQL CRUD against `wardrobe_items` we subclass the abstract base directly and
  // expose its protected internals. This runs v4's own `_create` marshaling
  // (documentToRow / prepareForStorage / the schema-translator column mapping).
  class WardrobeSqlRepo extends AbstractBaseRepository<any> {
    constructor() {
      super('wardrobe_items', WardrobeItemSchema);
    }
    // Subclasses must implement the abstract public CRUD; the base internals are
    // what we actually drive.
    async create(data: any, options?: any) {
      return this._create(data, options);
    }
    async update(id: string, data: any) {
      return this._update(id, data);
    }
    async delete(id: string) {
      return this._delete(id);
    }
  }

  await initializeDatabase();
  await ensureCollection('wardrobe_items', WardrobeItemSchema);

  const repo = new WardrobeSqlRepo();
  for (const row of spec.seed) {
    await repo.create(
      {
        characterId: row.characterId,
        title: row.title,
        description: row.description,
        imagePrompt: row.imagePrompt,
        types: row.types,
        componentItemIds: row.componentItemIds,
        appropriateness: row.appropriateness,
        isDefault: row.isDefault,
        replace: row.replace,
        migratedFromClothingRecordId: row.migratedFromClothingRecordId,
        archivedAt: row.archivedAt,
      },
      { id: row.id, createdAt: row.createdAt, updatedAt: row.updatedAt }
    );
  }

  await closeDatabase();

  process.stderr.write(
    `built wardrobe_items fixture: ${out} (${spec.seed.length} seed rows)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
