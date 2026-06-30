/**
 * Tier-2 oracle case — the `characters` SLIM ROW (Phase-2, main DB, the
 * store-backed capstone sub-unit 2).
 *
 * Proves what state v4 leaves the `characters` table in after a fixed create /
 * update / delete sequence, so the Rust port can be diffed against it (structural
 * DB diff).
 *
 * WHY THE PROTECTED SLIM-ROW INTERNALS, NOT THE PUBLIC create/update: v4's
 * `CharactersRepository.create` provisions and projects the character vault
 * (ensureCharacterVault + writeCharacterVaultManagedFields), and `.update` routes
 * managed fields to the vault. Those orchestrations are a later sub-unit. This
 * case drives v4's REAL protected `_create`/`_update`/`_delete` via a thin
 * subclass (`CharactersSqlRepo`) — the vault-aware overrides that strip
 * MANAGED_FIELDS before the SQL write, leaving the non-managed "slim row" this
 * differential checks. v4 stays the oracle of truth.
 *
 * Banks seven nullable boolean columns, two boolean-default columns, a typed
 * JSON-object column (defaultTimestampConfig), an open JSON column
 * (sillyTavernData), two typed-struct array columns (partnerLinks /
 * avatarOverrides), a string-array column (tags), an enum TEXT column
 * (controlledBy), and many nullable UUID columns. The managed columns exist in
 * the fixture table (ensureCollection generates them) but both sides omit them
 * from every write, so they sit at their DDL defaults identically.
 *
 * Flow (mirrors wardrobe-tier2.ts):
 *   1. copy the SEED-ONLY fixture to a fresh working copy;
 *   2. open the copy through v4's real `initializeDatabase()` and run the op
 *      sequence from `characters-slim-tier2.json` via CharactersSqlRepo;
 *   3. close, then dump the table canonically (RAW on-disk cells via `rawQuery`:
 *      booleans as 0/1, the JSON columns as their text, nullables null/text) and
 *      emit it as one NDJSON row.
 *
 * NORMALIZATION SPEC: none. Every id and timestamp is pinned on both sides.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`), AFTER
 * building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARACTERS_SLIM=/tmp/qt-characters-slim-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/characters-slim.ts \
 *     > /tmp/oracle-characters-slim.ndjson
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
  const specPath = join(here, '..', 'fixtures', 'characters-slim-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_CHARACTERS_SLIM;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_CHARACTERS_SLIM must point at the seed fixture from build-characters-slim-fixture.ts'
    );
  }

  // Work on a fresh copy so the shared seed fixture stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-characters-slim-oracle-'));
  // v4 nests working files under `<dataDir>/data/` (instance lock, sibling DBs).
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'characters-slim-work.db');
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
  const { CharactersRepository } = await import(
    '@/lib/database/repositories/characters.repository'
  );

  // Subclass the real repo to drive its protected slim-row internals (the public
  // create/update orchestrate the vault). `_create`/`_update` are the vault-aware
  // overrides that strip MANAGED_FIELDS before the SQL write.
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
  const repo = new CharactersSqlRepo();

  for (const op of spec.ops) {
    if (op.kind === 'create') {
      await repo.createSlim(op.data, op.options);
    } else if (op.kind === 'update') {
      await repo.updateSlim(op.id as string, op.data);
    } else {
      await repo.deleteSlim(op.id as string);
    }
  }

  // Read RAW on-disk state through v4's own connected backend. table_info gives
  // schema column order; SELECT * gives the persisted rows (booleans as 0/1, the
  // JSON columns as their text, nullables null or their value).
  const columns = (
    (await rawQuery('PRAGMA table_info(characters)')) as Array<{
      name: string;
    }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM characters')) as Array<
    Record<string, unknown>
  >;

  await closeDatabase();

  const dump = canonicalizeRows({
    table: 'characters',
    columns,
    rawRows,
    orderBy: 'id',
  });

  process.stdout.write(
    JSON.stringify({ case: 'characters-slim-tier2', ...dump }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`characters-slim-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
