/**
 * Tier-2 oracle — the character vault WARDROBE WRITE projection.
 *
 * Opens the pre-seeded mount-index fixture and drives v4's REAL
 * `projectVaultWardrobe` (lib/database/repositories/vault-overlay/wardrobe-sync)
 * over each op's full item list, then dumps the doc-store tables. v4's post-write
 * `reindexSingleFile` runs (database-backed stores chunk with no model,
 * deterministically); its only divergence — the link `chunkCount` and the derived
 * `doc_mount_chunks` rows — is pinned/excluded by the Rust harness (exactly as the
 * groups/projects store-backed tests do).
 *
 * Dumps `doc_mount_points` / `_files` / `_documents` / `_file_links` / `_folders`
 * in the minted-values remap form; the Rust harness applies the shared cross-table
 * id-map (the store `mountPointId` is the one pinned id).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_VAULT_WARDROBE_WRITE=/tmp/qt-vault-wardrobe-write-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-wardrobe-write.ts \
 *     > /tmp/oracle-vault-wardrobe-write.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

import type { WardrobeItem } from '@/lib/schemas/wardrobe.types';

interface Op {
  label: string;
  items: WardrobeItem[];
}
interface Spec {
  testPepperBase64: string;
  store: { id: string };
  characterId: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'vault-wardrobe-write-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_VAULT_WARDROBE_WRITE;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_VAULT_WARDROBE_WRITE must point at the seeded fixture .db');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-vault-wardrobe-write-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'vault-wardrobe-write-mount-index-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { projectVaultWardrobe } = await import(
    '@/lib/database/repositories/vault-overlay/wardrobe-sync'
  );

  await initializeDatabase();

  for (const op of spec.ops) {
    await projectVaultWardrobe(spec.store.id, spec.characterId, op.items);
  }

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable (degraded open?)');
  const dumpTable = (table: string, orderBy: string) => {
    const columns = (midb.pragma(`table_info(${table})`) as Array<{ name: string }>).map(
      (c) => c.name,
    );
    const rawRows = midb.prepare(`SELECT * FROM ${table}`).all() as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };

  const points = dumpTable('doc_mount_points', 'name');
  const files = dumpTable('doc_mount_files', 'sha256');
  const documents = dumpTable('doc_mount_documents', 'contentSha256');
  const links = dumpTable('doc_mount_file_links', 'relativePath');
  const folders = dumpTable('doc_mount_folders', 'path');

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stdout.write(
    JSON.stringify({ case: 'vault-wardrobe-write', points, files, documents, links, folders }) +
      '\n',
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`vault-wardrobe-write oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
