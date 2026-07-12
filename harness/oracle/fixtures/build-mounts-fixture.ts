/**
 * The committed mount-index file-ops server-surface fixture builder (P4.6v) —
 * the shared test-pepper substrate for the mount-read / mount-list differentials
 * (`mount_read_equivalence`) and later mount-ops units.
 *
 * Bakes, via v4's REAL repos + `storeMountFile` (all ids/timestamps pinned so
 * both differential sides read identical rows — reads diff with no remap):
 *   - MP_DB  — a database/documents mount: two text docs (`notes/intro.md`,
 *              `reference.txt`) + one blob with a description (`images/logo.png`)
 *              + one embedded chunk (raw insert), plus a `doc_mount_folders` row,
 *   - MP_EMPTY — an empty database mount (no files/folders),
 *   - MP_FS  — a filesystem mount, excludePatterns ['node_modules','*.tmp'],
 *   - MP_OBS — an obsidian mount, excludePatterns [].
 * MP_FS / MP_OBS store the SENTINEL basePath `__MOUNTS_FS_TREE__`; the oracle and
 * the Rust test each rewrite it to their per-side temp COPY of the committed
 * `mounts-fs-tree/` before reading (nothing writes the committed tree).
 *
 * Regenerate (Node 24, from the v4 checkout) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MOUNTS_MAIN=$W/crates/quilltap-web/tests/fixtures/mounts-main.db \
 *   QT_FIXTURE_MOUNTS_MOUNT=$W/crates/quilltap-web/tests/fixtures/mounts-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-mounts-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
}

// ── Pinned mount-point ids (shared verbatim with the Rust differential) ──────
const MP_DB = 'b6000000-0000-4000-8000-000000000001';
const MP_EMPTY = 'b6000000-0000-4000-8000-000000000002';
const MP_FS = 'b6000000-0000-4000-8000-000000000003';
const MP_OBS = 'b6000000-0000-4000-8000-000000000004';
const IDX_CHUNK = 'b7000000-0000-4000-8000-000000000001';

// The sentinel the test sides rewrite to their own fs-tree copy.
const FS_TREE_SENTINEL = '__MOUNTS_FS_TREE__';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'mounts.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_MOUNTS_MAIN;
  const mountOut = process.env.QT_FIXTURE_MOUNTS_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_MOUNTS_MAIN and QT_FIXTURE_MOUNTS_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mounts-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();

  // Materialize the mount-index schema on the fresh fixture. The `data BLOB`
  // table (`doc_mount_blobs`) can't be produced by generateDDL — storeMountFile's
  // blob branch triggers the repo's hand-written DDL lazily.
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
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  // Materialize doc_mount_blobs via the repo's hand-written DDL (a harmless read
  // triggers its lazy table-init) so storeMountFile's blob branch can insert.
  const { DocMountBlobsRepository } = await import(
    '@/lib/database/repositories/doc-mount-blobs.repository'
  );
  await new DocMountBlobsRepository().findByFileId('00000000-0000-4000-8000-000000000000');

  const repos = getRepositories();
  const TS = spec.seedTimestamp;

  // A single user so the MAIN partition is a valid (non-empty) encrypted DB the
  // Db runtime can open — the mount-index read path never touches it otherwise.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );

  const mkMount = async (
    id: string,
    name: string,
    mountType: string,
    storeType: string,
    basePath: string,
    excludePatterns: string[],
  ) =>
    repos.docMountPoints.create(
      {
        name,
        basePath,
        mountType,
        storeType,
        includePatterns: [],
        excludePatterns,
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
      { id, createdAt: TS, updatedAt: TS } as never,
    );

  await mkMount(MP_DB, 'DB Store', 'database', 'documents', '', []);
  await mkMount(MP_EMPTY, 'Empty Store', 'database', 'documents', '', []);
  await mkMount(MP_FS, 'FS Mount', 'filesystem', 'documents', FS_TREE_SENTINEL, [
    'node_modules',
    '*.tmp',
  ]);
  await mkMount(MP_OBS, 'Obsidian Vault', 'obsidian', 'documents', FS_TREE_SENTINEL, []);

  const { storeMountFile } = await import('@/lib/mount-index/store-file');

  // MP_DB text docs.
  await storeMountFile({
    mountPointId: MP_DB,
    relativePath: 'notes/intro.md',
    data: Buffer.from('# Intro\n\nFirst body line.\nSecond body line.\nThird body line.\n', 'utf8'),
    originalMimeType: 'text/markdown',
    transcodeImages: false,
    extractText: false,
    enqueueEmbedding: false,
  } as never);
  await storeMountFile({
    mountPointId: MP_DB,
    relativePath: 'reference.txt',
    data: Buffer.from('reference alpha\nreference beta\n', 'utf8'),
    originalMimeType: 'text/plain',
    transcodeImages: false,
    extractText: false,
    enqueueEmbedding: false,
  } as never);
  // MP_DB blob (with a description).
  await storeMountFile({
    mountPointId: MP_DB,
    relativePath: 'images/logo.png',
    data: Buffer.from([137, 80, 78, 71, 13, 10, 26, 10, 0, 1, 2, 3, 4, 5, 6, 7]),
    originalMimeType: 'image/png',
    description: 'The company logo.',
    transcodeImages: false,
    extractText: false,
    enqueueEmbedding: false,
  } as never);

  // One embedded chunk on the intro doc's link (proves the chunk path is intact;
  // not read by the read/list differential but part of the full fixture spec).
  const introLinkId = (
    midb
      .prepare(
        'SELECT id FROM doc_mount_file_links WHERE mountPointId = ? AND relativePath = ? LIMIT 1',
      )
      .get(MP_DB, 'notes/intro.md') as { id: string } | undefined
  )?.id;
  if (!introLinkId) throw new Error('intro link not found after storeMountFile');
  const embedding = Buffer.from(new Uint8Array(new Float32Array([1]).buffer));
  midb
    .prepare(
      'INSERT INTO doc_mount_chunks (id, linkId, mountPointId, chunkIndex, content, tokenCount, headingContext, embedding, createdAt, updatedAt) ' +
        'VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)',
    )
    .run(IDX_CHUNK, introLinkId, MP_DB, 0, 'First body line.', 4, 'Intro', embedding, TS, TS);

  // (storeMountFile's ensureFolderPath already created the `notes` + `images`
  // doc_mount_folders rows on MP_DB — the list route merges those.)

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(`built mounts fixture: main=${mainOut} mount=${mountOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`mounts fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
