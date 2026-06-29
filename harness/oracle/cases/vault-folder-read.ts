/**
 * Read-differential oracle — the vault read overlay's directory-listing load.
 *
 * Opens the pre-seeded mount-index fixture and drives v4's REAL
 * `repos.docMountDocuments.findManyByMountPointsInFolder` for each query in the
 * spec, emitting the returned rows (the subset the overlay consumes:
 * content, mountPointId, relativePath, fileName, createdAt, updatedAt). The Rust
 * port (DocMountDocumentsRepository::find_many_by_mount_points_in_folder) reads
 * the same fixture and must return the same rows (compared sorted by
 * (mountPointId, relativePath); the read itself has no defined order).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_VAULT_FOLDER_READ=/tmp/qt-vault-folder-read-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-folder-read.ts \
 *     > /tmp/oracle-vault-folder-read.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync, existsSync, mkdtempSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Query {
  label: string;
  mountPointIds: string[];
  folder: string;
  extension: string;
}
interface Spec {
  testPepperBase64: string;
  queries: Query[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'vault-folder-read-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_VAULT_FOLDER_READ;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_VAULT_FOLDER_READ must point at the seeded fixture .db');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-vault-folder-read-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'vault-folder-read-mount-index-work.db');
  copyFileSync(fixture, work);

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();
  const repos = getRepositories();

  const pick = (doc: Record<string, unknown>) => ({
    content: doc.content,
    mountPointId: doc.mountPointId,
    relativePath: doc.relativePath,
    fileName: doc.fileName,
    createdAt: doc.createdAt,
    updatedAt: doc.updatedAt,
  });

  for (const q of spec.queries) {
    const rows = await repos.docMountDocuments.findManyByMountPointsInFolder(
      q.mountPointIds,
      q.folder,
      q.extension,
    );
    process.stdout.write(
      JSON.stringify({ label: q.label, rows: rows.map((d) => pick(d as never)) }) + '\n',
    );
  }

  await closeDatabase();
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
