/**
 * Tier-2 oracle case — the `provider_models` UPSERT (minted-values / remap) path,
 * the deferred `upsertModel` method.
 *
 * Proves what state v4 leaves the database in after a fixed `upsertModel`
 * sequence that pins NOTHING, so v4 mints its own random UUIDs (on create) and
 * wall-clock timestamps (createdAt on create, updatedAt on every upsert). The
 * Rust port mints its own (different) values; the two dumps are reconciled by the
 * harness's normalization (first-seen id remap in natural-key order + timestamp
 * placeholder) — see
 * crates/quilltap-harness/tests/provider_models_upsert_tier2_equivalence.rs.
 *
 * This case therefore emits a RAW dump (no remap, no placeholder), sorted by the
 * natural key `modelId` (an input — each final row's modelId is distinct). The
 * harness applies the SAME normalization to this dump and to the Rust dump, then
 * diffs, so the normalization is provably consistent.
 *
 * The predicate under test: `upsertModel` finds an existing row via
 * `findByProviderAndModelId(provider, modelId, modelType ?? 'chat', baseUrl ??
 * undefined)` — which constrains baseUrl ONLY when baseUrl is truthy. Op 1
 * upserts a null-baseUrl payload against a null-baseUrl seed row and MUST UPDATE
 * it (not create a duplicate). If the predicate were wrong (e.g. `baseUrl IS
 * NULL`-keyed or always-constrained), op 1 would create a duplicate gpt-4o and
 * the row count / dump would diverge.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`), AFTER
 * building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_PROVIDER_MODELS_UPSERT=/tmp/qt-pm-upsert-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/provider-models-upsert-tier2.ts \
 *     > /tmp/oracle-pm-upsert.ndjson
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
  data: Record<string, unknown>;
}

interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'provider-models-upsert-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_PROVIDER_MODELS_UPSERT;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_PROVIDER_MODELS_UPSERT must point at the seed fixture from build-provider-models-upsert-fixture.ts'
    );
  }

  // Work on a fresh copy so the shared seed fixture stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-pm-upsert-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'pm-upsert-work.db');
  copyFileSync(fixture, work);

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE; // writable path uses journal_mode = TRUNCATE
  process.env.LOG_LEVEL = 'error'; // keep stdout clean for the NDJSON

  const { initializeDatabase, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { ProviderModelsRepository } = await import(
    '@/lib/database/repositories/provider-models.repository'
  );

  await initializeDatabase();
  const repo = new ProviderModelsRepository();

  for (const op of spec.ops) {
    // No options -> v4 upsertModel mints id (on create) + updatedAt (always).
    await repo.upsertModel(op.data as never);
  }

  // Read RAW on-disk state through v4's own connected backend. table_info gives
  // schema column order; SELECT * gives the persisted rows (booleans as 0/1, an
  // integer-valued REAL as the integer).
  const columns = (
    (await rawQuery('PRAGMA table_info(provider_models)')) as Array<{
      name: string;
    }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM provider_models')) as Array<
    Record<string, unknown>
  >;

  await closeDatabase();

  // RAW dump, sorted by the natural key `modelId` (NOT the random id) so both
  // sides line up row-for-row before the harness remaps ids / placeholders ts.
  const dump = canonicalizeRows({
    table: 'provider_models',
    columns,
    rawRows,
    orderBy: 'modelId',
  });

  process.stdout.write(
    JSON.stringify({ case: 'provider-models-upsert-tier2', ...dump }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(
    `provider-models-upsert-tier2 oracle failed: ${err?.stack ?? err}\n`
  );
  process.exit(1);
});
