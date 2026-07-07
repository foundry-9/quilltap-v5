/**
 * Tier-2 fixture builder — materializes the shared starting DB for the
 * `api_keys` repo (W4.7d, the last unported repo) from the committed spec
 * (`api-keys-tier2.json`).
 *
 * The `api_keys` table is created by v4's OWN
 * `ensureCollection('api_keys', ApiKeySchema)` (as `getApiKeysCollection` does),
 * so the DDL is identical to production by construction. The seed inserts ONE
 * **malformed** raw row (an empty `provider`, which `ProviderEnum =
 * z.string().min(1)` rejects) directly through the collection — bypassing the Zod
 * validation the repo would apply — so the op sequence can exercise
 * `getApiKeysByUserId`'s per-row `safeParse` DROP. All VALID keys are created by
 * the op sequence in `cases/api-keys.ts` (`createApiKey` mints ids/timestamps).
 *
 * SAFETY: every `key_value` is a SYNTHETIC placeholder — never a real API key.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-api-keys-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-api-keys-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userA: string;
  malformed: {
    id: string;
    label: string;
    provider: string;
    keyValue: string;
    createdAt: string;
    updatedAt: string;
  };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'api-keys-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the fixture .db to write');
  }

  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-api-keys-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, rawQuery, closeDatabase } =
    await import('@/lib/database/manager');
  const { ApiKeySchema } = await import('@/lib/schemas/profile.types');

  await initializeDatabase();
  await ensureCollection('api_keys', ApiKeySchema);

  // Seed the malformed row with a raw INSERT (bypassing the Zod validation the
  // repo would apply) so it lands on disk but fails the repo's per-row safeParse.
  await rawQuery(
    'INSERT INTO api_keys (id, userId, label, provider, key_value, isActive, lastUsed, createdAt, updatedAt) ' +
      'VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)',
    [
      spec.malformed.id,
      spec.userA,
      spec.malformed.label,
      spec.malformed.provider,
      spec.malformed.keyValue,
      1,
      null,
      spec.malformed.createdAt,
      spec.malformed.updatedAt,
    ]
  );

  await closeDatabase();

  process.stderr.write(`built api_keys fixture: ${out} (1 malformed seed row)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
