/**
 * Differential oracle — v4's REAL `ensureCharacterMetadataFile`
 * (`lib/mount-index/character-scaffold.ts`).
 *
 * The lazy seed the (deferred) startup backfill reaches for: it checks whether a
 * vault carries `metadata.json` (existence via `findByMountPointAndPath`, NEVER
 * parsing) and seeds an empty `{}` sheet if not. Existence-not-parse is the whole
 * point: a sheet the user has fat-fingered into invalid JSON is still theirs and
 * must not be "healed" into an empty one.
 *
 * Drives it over the SHARED vault-read-overlay fixture (its stores already hold
 * the three states — absent / valid / invalid metadata.json) and, per store,
 * emits `{ created, fileContent }`. The Rust port
 * (`db::character_vault::ensure_character_metadata_file`) must match.
 *
 * Run (Node 24, from the v4 checkout / pinned worktree), AFTER building the
 * vault-read-overlay fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   QT_FIXTURE_VAULT_READ_OVERLAY=/tmp/qt-vault-read-overlay-fixture.db \
 *     $N/node --import tsx <V5>/harness/oracle/cases/ensure-metadata-file.ts \
 *     > /tmp/oracle-ensure-metadata-file.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

const METADATA_JSON_PATH = 'metadata.json';

interface StoreCase {
  label: string;
  mountPointId: string;
}
interface Spec {
  testPepperBase64: string;
  stores: StoreCase[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'ensure-metadata-file.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_VAULT_READ_OVERLAY;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_VAULT_READ_OVERLAY must point at the seeded fixture .db');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ensure-metadata-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'ensure-metadata-mount-index-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { ensureCharacterMetadataFile } = await import('@/lib/mount-index/character-scaffold');

  await initializeDatabase();
  const repos = getRepositories();

  const rows: Array<{ label: string; created: boolean; fileContent: string | null }> = [];
  for (const store of spec.stores) {
    const created = await ensureCharacterMetadataFile(store.mountPointId);
    const doc = await repos.docMountDocuments.findByMountPointAndPath(
      store.mountPointId,
      METADATA_JSON_PATH,
    );
    rows.push({ label: store.label, created, fileContent: doc ? doc.content : null });
  }

  await closeDatabase();

  process.stdout.write(JSON.stringify({ case: 'ensure-metadata-file', rows }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`ensure-metadata-file oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
