/**
 * Read-differential oracle — the character vault read overlay.
 *
 * Opens the pre-seeded mount-index fixture and drives v4's REAL
 * `applyDocumentStoreOverlay` (lib/database/repositories/vault-overlay/read-overlay)
 * over the spec's input characters, emitting the resulting (hydrated / dropped)
 * character list. The Rust port (db::vault_read_overlay::apply_document_store_overlay)
 * reads the same fixture and must produce the same list — exactly, except the
 * physicalDescription mint branch whose createdAt/updatedAt are placeholdered.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_VAULT_READ_OVERLAY=/tmp/qt-vault-read-overlay-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-read-overlay.ts \
 *     > /tmp/oracle-vault-read-overlay.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync, existsSync, mkdtempSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  characters: Array<Record<string, unknown>>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'vault-read-overlay-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_VAULT_READ_OVERLAY;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_VAULT_READ_OVERLAY must point at the seeded fixture .db');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-vault-read-overlay-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'vault-read-overlay-mount-index-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { applyDocumentStoreOverlay } = await import(
    '@/lib/database/repositories/vault-overlay/read-overlay'
  );

  await initializeDatabase();

  const result = await applyDocumentStoreOverlay(spec.characters as never);

  await closeDatabase();

  process.stdout.write(JSON.stringify({ characters: result }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
