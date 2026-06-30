/**
 * Read-differential oracle — the character vault WARDROBE read overlay.
 *
 * Opens the pre-seeded mount-index fixture and drives v4's REAL
 * `readCharacterVaultWardrobe` (lib/database/repositories/vault-overlay/vault-readers)
 * over each spec case, emitting the resulting `{ items } | null`. The Rust port
 * (db::vault_read_overlay::read_character_vault_wardrobe) reads the same fixture
 * and must produce the same result, exactly (no normalization — there is no clock
 * mint on this path; ids/timestamps come from the shared fixture).
 *
 * The corpus keeps no General-Wardrobe mount provisioned, so v4's archetype
 * seeding (findArchetypes) returns [] and the seed is a verified no-op — matching
 * the Rust port, which defers archetype seeding (see read_character_vault_wardrobe).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_VAULT_WARDROBE_READ=/tmp/qt-vault-wardrobe-read-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-wardrobe-read.ts \
 *     > /tmp/oracle-vault-wardrobe-read.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync, existsSync, mkdtempSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Case {
  mountPointId: string;
  characterId: string;
}
interface Spec {
  testPepperBase64: string;
  cases: Case[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'vault-wardrobe-read-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_VAULT_WARDROBE_READ;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_VAULT_WARDROBE_READ must point at the seeded fixture .db');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-vault-wardrobe-read-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'vault-wardrobe-read-mount-index-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { readCharacterVaultWardrobe } = await import(
    '@/lib/database/repositories/vault-overlay/vault-readers'
  );

  await initializeDatabase();

  const results: Array<unknown> = [];
  for (const c of spec.cases) {
    results.push(await readCharacterVaultWardrobe(c.mountPointId, c.characterId));
  }

  await closeDatabase();

  process.stdout.write(JSON.stringify({ results }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
