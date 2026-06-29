/**
 * Tier-2 oracle case — the `wardrobe` repo (Phase-2) over the `wardrobe_items`
 * table.
 *
 * Proves what state v4 leaves the database in after a fixed create / update /
 * delete sequence, so the Rust port can be diffed against it (structural DB
 * diff).
 *
 * WHY THE BASE SQL CRUD, NOT THE PUBLIC OVERRIDES: v4's `WardrobeRepository`
 * overrides create/update/delete to be VAULT-ONLY (they write into the character
 * document store and throw without a mount — there is no SQL write mirror). They
 * cannot run against a bare fixture DB. This case instead drives v4's REAL base
 * repository SQL CRUD (`_create`/`_update`/`_delete`) against `wardrobe_items`
 * via a thin local subclass (`WardrobeSqlRepo`) that exposes those protected
 * internals — the same code path the schema-translator + base marshaling define
 * for this table (and that its reads, `findByCharacterIdRaw` -> `findByFilter`,
 * consume). v4 stays the oracle of truth: we run v4's own `_create`/`_update`/
 * `_delete` marshaling, dump, and diff.
 *
 * Banks two JSON ARRAY columns (`types` — enum strings; `componentItemIds`), two
 * boolean columns (`isDefault`/`replace` -> 0/1), a nullable soft-delete
 * TIMESTAMP column (`archivedAt` — 'date' affinity -> TEXT, exercised null and
 * set-to-non-null on update via the `archive` shape), and several nullable
 * string/UUID columns. No conflict detection / guard, so no expectThrow/Noop.
 *
 * Flow (mirrors conversation-annotations-tier2.ts):
 *   1. copy the SEED-ONLY fixture to a fresh working copy;
 *   2. open the copy through v4's real `initializeDatabase()` and run the op
 *      sequence from `wardrobe-tier2.json` via WardrobeSqlRepo (base CRUD);
 *   3. close, then dump the table canonically (RAW on-disk cells via `rawQuery`:
 *      booleans as 0/1, the two arrays as their JSON text, archivedAt null/text)
 *      and emit it as one NDJSON row.
 *
 * NORMALIZATION SPEC: none. Every id and timestamp is pinned on both sides.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`), AFTER
 * building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WARDROBE=/tmp/qt-wardrobe-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/wardrobe-tier2.ts \
 *     > /tmp/oracle-wardrobe.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  copyFileSync,
  existsSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind: 'create' | 'update' | 'delete';
  id?: string;
  data?: Record<string, unknown>;
  options?: { id: string; createdAt: string; updatedAt: string };
}

interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'wardrobe-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_WARDROBE;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_WARDROBE must point at the seed fixture from build-wardrobe-fixture.ts'
    );
  }

  // Work on a fresh copy so the shared seed fixture stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-wardrobe-oracle-'));
  // v4 nests working files under `<dataDir>/data/` (instance lock, sibling DBs).
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'wardrobe-work.db');
  copyFileSync(fixture, work);

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE; // writable path uses journal_mode = TRUNCATE
  // Keep stdout clean for the NDJSON: v4's console logger sends INFO to stdout.
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { AbstractBaseRepository } = await import(
    '@/lib/database/repositories/base.repository'
  );
  const { WardrobeItemSchema } = await import('@/lib/schemas/wardrobe.types');

  // Subclass the abstract base directly to drive v4's REAL base-repository SQL
  // CRUD against `wardrobe_items` (the public WardrobeRepository is vault-only).
  class WardrobeSqlRepo extends AbstractBaseRepository<any> {
    constructor() {
      super('wardrobe_items', WardrobeItemSchema);
    }
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
  const repo = new WardrobeSqlRepo();

  for (const op of spec.ops) {
    if (op.kind === 'create') {
      await repo.create(op.data as never, op.options);
    } else if (op.kind === 'update') {
      await repo.update(op.id as string, op.data as never);
    } else {
      await repo.delete(op.id as string);
    }
  }

  // Read RAW on-disk state through v4's own connected backend. table_info gives
  // schema column order; SELECT * gives the persisted rows (booleans as 0/1, the
  // two arrays as their JSON text, archivedAt null or the ISO timestamp).
  const columns = (
    (await rawQuery('PRAGMA table_info(wardrobe_items)')) as Array<{
      name: string;
    }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM wardrobe_items')) as Array<
    Record<string, unknown>
  >;

  await closeDatabase();

  const dump = canonicalizeRows({
    table: 'wardrobe_items',
    columns,
    rawRows,
    orderBy: 'id',
  });

  process.stdout.write(
    JSON.stringify({ case: 'wardrobe-tier2', ...dump }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`wardrobe-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
