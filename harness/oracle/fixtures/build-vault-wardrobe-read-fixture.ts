/**
 * Read-differential fixture builder — the shared mount-index sibling DB for the
 * character vault WARDROBE read overlay (readCharacterVaultWardrobe).
 *
 * Identical machinery to build-vault-read-overlay-fixture.ts: seed the pinned
 * document stores, materialize the four content tables, then write the corpus
 * files by driving v4's real `linkDocumentContent` directly (NOT
 * writeDatabaseDocument — its reindex pass + QUILLTAP_JOB_CHILD skip-switch are
 * unusable here). Pre-seeding lets both the oracle and the Rust port READ the
 * same rows so minted ids/timestamps match and items compare exactly.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-vault-wardrobe-read-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-vault-wardrobe-read-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Store {
  id: string;
  name: string;
}
interface SeedFile {
  mountPointId: string;
  relativePath: string;
  content: string;
}
interface Spec {
  testPepperBase64: string;
  stores: Store[];
  files: SeedFile[];
}

function detectFileType(rel: string): 'markdown' | 'txt' | 'json' | 'jsonl' {
  const dot = rel.lastIndexOf('.');
  const ext = dot >= 0 ? rel.slice(dot).toLowerCase() : '';
  switch (ext) {
    case '.md':
    case '.markdown':
      return 'markdown';
    case '.txt':
      return 'txt';
    case '.json':
      return 'json';
    case '.jsonl':
      return 'jsonl';
    default:
      return 'markdown';
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'vault-wardrobe-read-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the mount-index fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-vault-wardrobe-read-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { DocMountPointsRepository } = await import(
    '@/lib/database/repositories/doc-mount-points.repository'
  );
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');
  const { DocMountFileLinksRepository } = await import(
    '@/lib/database/repositories/doc-mount-file-links.repository'
  );
  const { sha256OfString } = await import('@/lib/utils/sha256');

  await initializeDatabase();

  // Seed the pinned store rows (also creates the doc_mount_points table). Defaults
  // mirror an official documents store.
  const points = new DocMountPointsRepository();
  const PINNED_TS = '2026-02-01T00:00:00.000Z';
  for (const store of spec.stores) {
    await points.create(
      {
        name: store.name,
        basePath: '',
        mountType: 'database',
        storeType: 'documents',
        includePatterns: [],
        excludePatterns: [],
        enabled: true,
        lastScannedAt: null,
        scanStatus: 'idle',
        lastScanError: null,
        conversionStatus: 'idle',
        conversionError: null,
        fileCount: 0,
        chunkCount: 0,
        totalSizeBytes: 0,
      } as never,
      { id: store.id, createdAt: PINNED_TS, updatedAt: PINNED_TS },
    );
  }

  // Materialize the four content tables.
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) {
      midb.exec(sql);
    }
  }

  // Seed the corpus files via real linkDocumentContent (folderId:null lets
  // ensureLinkFolderId create the Wardrobe folder row).
  const links = new DocMountFileLinksRepository();
  for (const f of spec.files) {
    const rel = f.relativePath;
    await links.linkDocumentContent({
      mountPointId: f.mountPointId,
      relativePath: rel,
      fileName: rel.includes('/') ? rel.slice(rel.lastIndexOf('/') + 1) : rel,
      folderId: null,
      fileType: detectFileType(rel),
      content: f.content,
      contentSha256: sha256OfString(f.content),
      plainTextLength: f.content.length,
      fileSizeBytes: Buffer.byteLength(f.content, 'utf-8'),
    });
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built vault-wardrobe-read fixture: ${out} (${spec.files.length} files, ${spec.stores.length} stores)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
