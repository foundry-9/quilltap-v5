/**
 * Fixture builder for the PUBLIC wardrobe READ trio + shared-archetype tier.
 *
 * Bakes the shared read state across TWO databases (main + mount-index) so BOTH
 * v4 and the Rust port read it identically:
 *   1. A character baked via the REAL repos.characters.create (pinned id + ts),
 *      which mints a linked vault mount + scaffolds an empty Wardrobe/. We then
 *      read the character's minted characterDocumentMountPointId and seed its
 *      Wardrobe/*.md files there via the REAL linkDocumentContent.
 *   2. A provisioned "Quilltap General" store (doc_mount_points row, pinned id;
 *      storeType 'documents', mountType 'database' -- the schema has no 'general'
 *      storeType, the General store is an ordinary documents store whose id lives
 *      in instance_settings.generalMountPointId). Two archetype items seeded into
 *      General/Wardrobe/.
 *   3. The character's vault gets a plain item, a COMPOSITE referencing one
 *      archetype by slug + one by UUID (only resolvable via archetype seeding),
 *      and an ARCHIVED item (exercises the include-archived filter).
 *
 * All ids + timestamps are pinned so the read compares with ZERO normalization.
 * Files are seeded via linkDocumentContent directly (NOT writeDatabaseDocument,
 * whose QUILLTAP_JOB_CHILD=1 skip-switch breaks initializeDatabase); see
 * [[document-store-oracle-gotchas]].
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WPR_MAIN=/tmp/qt-wpr-main.db QT_FIXTURE_WPR_MOUNT=/tmp/qt-wpr-mount.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-wardrobe-public-read-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedFile {
  relativePath: string;
  content: string;
}
interface Spec {
  testPepperBase64: string;
  characterId: string;
  generalMountPointId: string;
  generalStore: { id: string; name: string };
  character: Record<string, unknown>;
  archetypeFiles: SeedFile[];
  characterFiles: SeedFile[];
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
  const specPath = join(here, 'wardrobe-public-read-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_WPR_MAIN;
  const mountOut = process.env.QT_FIXTURE_WPR_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_WPR_MAIN and QT_FIXTURE_WPR_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wpr-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
  } = await import('@/lib/schemas/mount-index.types');
  const { DocMountFileLinksRepository } = await import(
    '@/lib/database/repositories/doc-mount-file-links.repository'
  );
  const { sha256OfString } = await import('@/lib/utils/sha256');

  await initializeDatabase();

  // MAIN db: the slim `characters` table (vault-managed columns stay NULL/default).
  await ensureCollection('characters', CharacterSchema);

  // MOUNT-INDEX db: materialize every store table the create/provision/write path
  // touches, via v4's own generated DDL.
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) {
      midb.exec(sql);
    }
  }

  const repos = getRepositories();
  const PINNED_TS = '2026-02-01T00:00:00.000Z';

  // 1. Bake the character + its empty vault via the REAL create (mints the mount
  // point, scaffolds the preset folders incl. an empty Wardrobe/). Pin the id.
  await repos.characters.create(spec.character as never, {
    id: spec.characterId,
    createdAt: PINNED_TS,
    updatedAt: PINNED_TS,
  } as never);

  // Read back the character's minted vault mount id (create mints it fresh).
  const character = await repos.characters.findByIdRaw(spec.characterId);
  if (!character) throw new Error('baked character not found after create');
  const characterMountId = character.characterDocumentMountPointId as string | null;
  if (!characterMountId) throw new Error('baked character has no linked vault mount');

  // 2. Provision the "Quilltap General" store (ordinary documents store; the
  // General-ness is the instance_settings key, not a storeType). Pin its id.
  await repos.docMountPoints.create(
    {
      name: spec.generalStore.name,
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
    { id: spec.generalMountPointId, createdAt: PINNED_TS, updatedAt: PINNED_TS },
  );

  // Persist the General mount id into the MAIN db instance_settings (the key the
  // archetype tier reads via getGeneralMountPointId).
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    spec.generalMountPointId,
  ]);

  // 3. Seed the corpus files via real linkDocumentContent (folderId:null lets
  // ensureLinkFolderId create the Wardrobe/ folder row).
  const links = new DocMountFileLinksRepository();
  const seed = async (mountPointId: string, files: SeedFile[]): Promise<void> => {
    for (const f of files) {
      const rel = f.relativePath;
      await links.linkDocumentContent({
        mountPointId,
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
  };
  await seed(spec.generalMountPointId, spec.archetypeFiles);
  await seed(characterMountId, spec.characterFiles);

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built wardrobe-public-read fixtures: main=${mainOut} mount=${mountOut} charMount=${characterMountId}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`wardrobe-public-read fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
