/**
 * Tier-2 oracle case — the `character_plugin_data` repo's `upsert` method
 * (Phase-2, MINTED-VALUES / remap form).
 *
 * Proves what state v4 leaves the database in after a fixed sequence of
 * `upsert(characterId, pluginName, data)` calls. Unlike the pinned
 * character-plugin-data-tier2.ts case, `upsert` mints its own id (create branch)
 * and `now` (both branches) internally, so NOTHING can be pinned. This case
 * therefore emits a RAW dump (no remap, no placeholder), sorted by the natural
 * key `pluginName` — an input, never minted — and the harness applies the SAME
 * normalization (first-seen id remap + timestamp placeholder) to both this dump
 * and the Rust dump, then diffs. See
 * crates/quilltap-harness/tests/character_plugin_data_upsert_tier2_equivalence.rs.
 *
 * The corpus exercises BOTH upsert branches (see the fixture JSON): an UPDATE of
 * a seed row, a CREATE of a new pair, an UPDATE of a freshly-minted-id row, and a
 * second CREATE. Each final row has a distinct `pluginName` so the sort lines the
 * rows up before the id remap.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`), AFTER
 * building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARACTER_PLUGIN_DATA_UPSERT=/tmp/qt-cpd-upsert-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/character-plugin-data-upsert-tier2.ts \
 *     > /tmp/oracle-cpd-upsert.ndjson
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
  kind: 'upsert';
  characterId: string;
  pluginName: string;
  data: unknown;
}

interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(
    here,
    '..',
    'fixtures',
    'character-plugin-data-upsert-tier2.json'
  );
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_CHARACTER_PLUGIN_DATA_UPSERT;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_CHARACTER_PLUGIN_DATA_UPSERT must point at the seed fixture from build-character-plugin-data-upsert-fixture.ts'
    );
  }

  // Work on a fresh copy so the shared seed fixture stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-cpd-upsert-oracle-'));
  // v4 nests working files under `<dataDir>/data/` (instance lock, sibling DBs).
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'cpd-upsert-work.db');
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
  const { CharacterPluginDataRepository } = await import(
    '@/lib/database/repositories/character-plugin-data.repository'
  );

  await initializeDatabase();
  const repo = new CharacterPluginDataRepository();

  for (const op of spec.ops) {
    // No pinned options anywhere — v4 mints id / timestamps inside upsert.
    await repo.upsert(op.characterId, op.pluginName, op.data);
  }

  // Read RAW on-disk state through v4's own connected backend. table_info gives
  // schema column order; SELECT * gives the persisted rows (data as compact
  // JSON text).
  const columns = (
    (await rawQuery('PRAGMA table_info(character_plugin_data)')) as Array<{
      name: string;
    }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM character_plugin_data')) as Array<
    Record<string, unknown>
  >;

  await closeDatabase();

  // RAW dump, sorted by the natural key `pluginName` (NOT the random id) so both
  // sides line up row-for-row before the harness remaps ids / placeholders ts.
  const dump = canonicalizeRows({
    table: 'character_plugin_data',
    columns,
    rawRows,
    orderBy: 'pluginName',
  });

  process.stdout.write(
    JSON.stringify({ case: 'character-plugin-data-upsert-tier2', ...dump }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(
    `character-plugin-data-upsert-tier2 oracle failed: ${err?.stack ?? err}\n`
  );
  process.exit(1);
});
