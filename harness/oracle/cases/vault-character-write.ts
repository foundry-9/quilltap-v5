/**
 * Tier-2 oracle — the character vault MANAGED-FIELDS WRITE projection.
 *
 * Opens the pre-seeded mount-index fixture and drives v4's REAL
 * `writeCharacterVaultManagedFields`
 * (lib/database/repositories/vault-overlay/managed-fields) over each op's full
 * character, then dumps the doc-store tables. v4's post-write `reindexSingleFile`
 * runs (database-backed stores chunk with no model, deterministically); its only
 * divergence — the link `chunkCount` and the derived `doc_mount_chunks` rows — is
 * pinned/excluded by the Rust harness (exactly as the groups/projects/wardrobe
 * store-backed tests do).
 *
 * Dumps `doc_mount_points` / `_files` / `_documents` / `_file_links` / `_folders`
 * in the minted-values remap form; the Rust harness applies the shared cross-table
 * id-map (the store `mountPointId` is the one pinned id).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_VAULT_CHARACTER_WRITE=/tmp/qt-vault-character-write-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-character-write.ts \
 *     > /tmp/oracle-vault-character-write.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

import type { Character } from '@/lib/schemas/types';

interface Op {
  label: string;
  character: Record<string, unknown>;
}
interface Spec {
  testPepperBase64: string;
  store: { id: string };
  characterId: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'vault-character-write-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_VAULT_CHARACTER_WRITE;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_VAULT_CHARACTER_WRITE must point at the seeded fixture .db');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-vault-character-write-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'vault-character-write-mount-index-work.db');
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
  const { writeCharacterVaultManagedFields } = await import(
    '@/lib/database/repositories/vault-overlay/managed-fields'
  );

  await initializeDatabase();

  for (const op of spec.ops) {
    await writeCharacterVaultManagedFields(spec.store.id, {
      character: op.character as unknown as Character,
    });
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
    JSON.stringify({ case: 'vault-character-write', points, files, documents, links, folders }) +
      '\n',
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`vault-character-write oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
