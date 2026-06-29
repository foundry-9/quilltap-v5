/**
 * Tier-2 fixture builder — materializes the shared starting DB for the
 * `provider_models` UPSERT (minted-values / remap) case from the committed
 * plaintext spec (`provider-models-upsert-tier2.json`).
 *
 * Same shape as build-provider-models-fixture.ts: the `provider_models` table is
 * created by v4's OWN `ensureCollection('provider_models', ProviderModelSchema)`
 * so the DDL is identical to production, then the SEED rows are inserted via the
 * real `ProviderModelsRepository.create` with id + timestamps pinned
 * (CreateOptions) so the starting state is deterministic.
 *
 * The op sequence under test (the upserts) is applied later, by
 * `cases/provider-models-upsert-tier2.ts` (oracle) and the Rust harness, each on
 * its own fresh copy — those ops pin NOTHING (minted ids + timestamps).
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-pm-upsert-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-provider-models-upsert-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seed: Array<{
    id: string;
    provider: string;
    modelId: string;
    modelType: string;
    displayName: string;
    baseUrl: string | null;
    contextWindow: number | null;
    maxOutputTokens: number | null;
    deprecated: boolean;
    experimental: boolean;
    createdAt: string;
    updatedAt: string;
  }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'provider-models-upsert-tier2.json');
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
  const scratch = mkdtempSync(join(tmpdir(), 'qt-pm-upsert-fixture-build-'));
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
  const { ProviderModelsRepository } = await import(
    '@/lib/database/repositories/provider-models.repository'
  );
  const { ProviderModelSchema } = await import('@/lib/schemas/model.types');

  await initializeDatabase();
  await ensureCollection('provider_models', ProviderModelSchema);

  const repo = new ProviderModelsRepository();
  for (const row of spec.seed) {
    await repo.create(
      {
        provider: row.provider,
        modelId: row.modelId,
        modelType: row.modelType,
        displayName: row.displayName,
        baseUrl: row.baseUrl,
        contextWindow: row.contextWindow,
        maxOutputTokens: row.maxOutputTokens,
        deprecated: row.deprecated,
        experimental: row.experimental,
      } as never,
      { id: row.id, createdAt: row.createdAt, updatedAt: row.updatedAt }
    );
  }

  await closeDatabase();

  process.stderr.write(
    `built provider_models upsert fixture: ${out} (${spec.seed.length} seed rows)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
