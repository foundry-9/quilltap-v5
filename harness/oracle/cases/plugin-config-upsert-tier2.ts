/**
 * Tier-2 oracle case — the `plugin_config` repo's `upsertForUserPlugin` method
 * (Phase-2), in the MINTED-VALUES (remap) form.
 *
 * Proves what state v4 leaves the database in after a fixed sequence of
 * `upsertForUserPlugin(userId, pluginName, config)` calls, so the Rust port can
 * be diffed against it. Unlike the pinned `plugin-config-tier2.ts`, the upsert
 * ops pin NOTHING: v4 mints its own random UUIDs (on the create path) and
 * wall-clock timestamps (createdAt on create, updatedAt on every op). The Rust
 * port mints its own (different) values; the two dumps are reconciled by the
 * harness's normalization (first-seen id remap in natural-key `pluginName` order
 * + createdAt/updatedAt placeholder) — see
 * crates/quilltap-harness/tests/plugin_config_upsert_tier2_equivalence.rs.
 *
 * v4 `upsertForUserPlugin`: find existing by (userId, pluginName); if found ->
 * MERGE `{ ...existing.config, ...config }` then `update(existing.id, {config:
 * merged})` (id + createdAt preserved, updatedAt minted); else ->
 * `create({userId, pluginName, config})` (mints id + both timestamps; `enabled`
 * never set -> SQL NULL).
 *
 * OPEN-JSON MERGE CONSTRAINT (tracked deferred seam #5): every stored config —
 * INCLUDING every MERGE result — is {} or a SINGLE-KEY object, so v4's
 * insertion-order JSON.stringify and Rust's key-sorting serde_json::Value
 * coincide. The merge ops overwrite the value under the SAME single key (or
 * merge an empty existing with a single-key new), never producing a 2+-key
 * object.
 *
 * This case emits a RAW dump (no remap, no placeholder), sorted by the natural
 * key `pluginName` (each final row's pluginName is distinct) — the harness
 * applies the SAME normalization to this dump and to the Rust dump, then diffs,
 * so the normalization is provably consistent.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`), AFTER
 * building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_PLUGIN_CONFIG_UPSERT=/tmp/qt-pc-upsert-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/plugin-config-upsert-tier2.ts \
 *     > /tmp/oracle-pc-upsert.ndjson
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
  userId: string;
  pluginName: string;
  config: Record<string, unknown>;
}

interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'plugin-config-upsert-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_PLUGIN_CONFIG_UPSERT;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_PLUGIN_CONFIG_UPSERT must point at the seed fixture from build-plugin-config-upsert-fixture.ts'
    );
  }

  // Work on a fresh copy so the shared seed fixture stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-pc-upsert-oracle-'));
  // v4 nests working files under `<dataDir>/data/` (instance lock, sibling DBs).
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'pc-upsert-work.db');
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
  const { PluginConfigRepository } = await import(
    '@/lib/database/repositories/plugin-config.repository'
  );

  await initializeDatabase();
  const repo = new PluginConfigRepository();

  for (const op of spec.ops) {
    // No options -> v4 mints id (create path) + timestamps. The MERGE happens
    // inside upsertForUserPlugin on the update path.
    await repo.upsertForUserPlugin(op.userId, op.pluginName, op.config);
  }

  // Read RAW on-disk state through v4's own connected backend. table_info gives
  // schema column order; SELECT * gives the persisted rows (enabled as 0/1 or
  // NULL, config as the compact JSON text).
  const columns = (
    (await rawQuery('PRAGMA table_info(plugin_configs)')) as Array<{
      name: string;
    }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM plugin_configs')) as Array<
    Record<string, unknown>
  >;

  await closeDatabase();

  // RAW dump, sorted by the natural key `pluginName` (NOT the random id) so both
  // sides line up row-for-row before the harness remaps ids / placeholders ts.
  const dump = canonicalizeRows({
    table: 'plugin_configs',
    columns,
    rawRows,
    orderBy: 'pluginName',
  });

  process.stdout.write(
    JSON.stringify({ case: 'plugin-config-upsert-tier2', ...dump }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`plugin-config-upsert-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
