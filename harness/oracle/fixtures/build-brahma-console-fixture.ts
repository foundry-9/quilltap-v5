/**
 * Tier-3 SEED fixture builder for the Brahma one-shot console (W4.5b; v4
 * `lib/services/brahma-console/one-shot.service.ts` `runBrahmaQuery`).
 *
 * Bakes the SEED both differential sides start from (MAIN db only — the console
 * never persists, and the corpus's `run_sql` case reads `connection_profiles`, so
 * no mount store / chat is needed). Via the REAL repos:
 *   - userA: three connection profiles (one `isDefault`, carrying a valid — SYNTHETIC
 *     — api key) so the profile chain + api-key step resolve, and `run_sql` has
 *     rows to read.
 *   - userB: no profiles → the no-profile branch.
 *   - userC: a default profile with NO `apiKeyId` → 'no API key configured…'.
 *   - userD: a default profile whose `apiKeyId` points at a missing key → 'API key
 *     not found'.
 *
 * All providers are ANTHROPIC with a fictional model NOT in FALLBACK_PRICING, so
 * v4's `checkModelSupportsTools` returns `true` deterministically (no pricing
 * fetch) — matching the Rust injected `model_supports_native_tools = true`.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-brahma-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-brahma-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-brahma-console-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface ProfileSpec {
  id: string;
  name: string;
  isDefault: boolean;
}
interface Spec {
  testPepperBase64: string;
  userA: string;
  userB: string;
  userC: string;
  userD: string;
  provider: string;
  modelName: string;
  syntheticKey: string;
  missingApiKeyId: string;
  profilesA: ProfileSpec[];
  profileC: ProfileSpec;
  profileD: ProfileSpec;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'brahma-console-tier3.json'), 'utf8')) as Spec;

  const outMain = process.env.QT_FIXTURE_OUT;
  const outMount = process.env.QT_FIXTURE_MOUNT_OUT;
  if (!outMain || !outMount) {
    throw new Error('QT_FIXTURE_OUT and QT_FIXTURE_MOUNT_OUT must point at the fixture .db files');
  }
  for (const base of [outMain, outMount]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = base + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-brahma-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();
  const repos = getRepositories();

  // userA's SYNTHETIC api key (its minted id is stable across both fixture copies).
  const apiKey = await repos.connections.createApiKey({
    userId: spec.userA,
    label: 'Brahma test key',
    provider: spec.provider,
    key_value: spec.syntheticKey,
    isActive: true,
  } as never);

  // userA profiles — the default carries the valid api key.
  for (const p of spec.profilesA) {
    await repos.connections.create(
      {
        userId: spec.userA,
        name: p.name,
        provider: spec.provider,
        modelName: spec.modelName,
        isDefault: p.isDefault,
        apiKeyId: p.isDefault ? apiKey.id : null,
      } as never,
      { id: p.id }
    );
  }

  // userC — default profile, NO api key configured.
  await repos.connections.create(
    {
      userId: spec.userC,
      name: spec.profileC.name,
      provider: spec.provider,
      modelName: spec.modelName,
      isDefault: spec.profileC.isDefault,
      apiKeyId: null,
    } as never,
    { id: spec.profileC.id }
  );

  // userD — default profile, api key id points at a missing key.
  await repos.connections.create(
    {
      userId: spec.userD,
      name: spec.profileD.name,
      provider: spec.provider,
      modelName: spec.modelName,
      isDefault: spec.profileD.isDefault,
      apiKeyId: spec.missingApiKeyId,
    } as never,
    { id: spec.profileD.id }
  );

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built brahma-console fixture: ${outMain} (${spec.profilesA.length + 2} profiles, 1 api key)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`brahma-console fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
