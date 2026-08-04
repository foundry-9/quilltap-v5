/**
 * The committed STORE-DELETE fixture builder (P4.31) — the substrate for
 * `store_delete_equivalence`, the differential behind dogfood finding #58's
 * root cause (a `doc_mount_points` delete that does not take its children).
 *
 * It is a NEW family rather than an extension of `groups-projects-*` because
 * the delete cascade needs shapes that fixture cannot carry without disturbing
 * six other differentials: a store that is BOTH group-linked and project-linked,
 * a file hard-linked ACROSS two stores (so the delete must collect one file and
 * spare the other), and — the point of the whole lane — rows that are ALREADY
 * orphaned when the fixture is opened.
 *
 * Bakes, via v4's REAL repositories + `storeMountFile` / `linkFile` (ids and
 * timestamps pinned wherever the differential names them):
 *
 *   - MP_TARGET "Target Store" — the delete target. Two native-text documents
 *     (`notes/intro.md`, `notes/appendix.md` → `doc_mount_documents` rows), one
 *     binary blob (`images/logo.png` → a `doc_mount_blobs` row), the `notes` +
 *     `images` folders, two chunks, and BOTH a `group_doc_mount_links` and a
 *     `project_doc_mount_links` row.
 *   - MP_KEEPER "Keeper Store" — the healthy remainder, diffed row-for-row
 *     after the delete. Carries its own `own/readme.md` PLUS a hard link to
 *     MP_TARGET's `notes/appendix.md` (v4's real `linkFile`, so both links
 *     share one `doc_mount_files` row under one `linkGroupId`). Deleting
 *     MP_TARGET must therefore collect the intro + logo content and leave the
 *     appendix file, document row and keeper link standing — the refcount arm.
 *   - The PLANTED ORPHANS. A third store, MP_GHOST, is built exactly like the
 *     others and then has its `doc_mount_points` row deleted with a bare
 *     `DELETE` on the mount-index connection — which is how the real ones arose
 *     (`build-restore-archive-orphan-links.test.ts` uses the same recipe, and
 *     the measured instance carried 43 links + 118 folders across 21 vanished
 *     vaults). Nothing stops it: a generateDDL `doc_mount_file_links` declares
 *     no foreign keys at all. What survives is the reaper's target — 2 links,
 *     3 folders, 1 chunk, and the 2 files / 1 document / 1 blob reachable only
 *     through those links.
 *
 * NB the existing `sweep_orphaned_link_content` boot pass CANNOT absorb these:
 * it collects content rows with no LINK, and every planted file still has one.
 * The orphan is one level up — a link whose mount point is gone.
 *
 * Regenerate (Node 24, from the v4 checkout) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SD_MAIN=$W/crates/quilltap-web/tests/fixtures/store-delete-main.db \
 *   QT_FIXTURE_SD_MOUNT=$W/crates/quilltap-web/tests/fixtures/store-delete-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-store-delete-fixture.ts
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

// ── Pinned ids (shared verbatim with the Rust differential) ─────────────────
const MP_TARGET = 'b9000000-0000-4000-8000-000000000001';
const MP_KEEPER = 'b9000000-0000-4000-8000-000000000002';
/** Built like any other store, then stripped of its `doc_mount_points` row. */
const MP_GHOST = 'b9000000-0000-4000-8000-0000000000ee';

const GRP = 'a2000000-0000-4000-8000-000000000001';
const PRJ = 'a3000000-0000-4000-8000-000000000001';

const CH_TARGET_1 = 'bb000000-0000-4000-8000-000000000001';
const CH_TARGET_2 = 'bb000000-0000-4000-8000-000000000002';
const CH_KEEPER_1 = 'bb000000-0000-4000-8000-000000000003';
const CH_GHOST_1 = 'bb000000-0000-4000-8000-000000000004';
const CH_GHOST_2 = 'bb000000-0000-4000-8000-000000000005';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'store-delete.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_SD_MAIN;
  const mountOut = process.env.QT_FIXTURE_SD_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_SD_MAIN and QT_FIXTURE_SD_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-store-delete-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const { GroupSchema } = await import('@/lib/schemas/group.types');
  const { ProjectSchema } = await import('@/lib/schemas/project.types');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    GroupCharacterMemberSchema,
    GroupDocMountLinkSchema,
    ProjectDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  await ensureCollection('groups', GroupSchema);
  await ensureCollection('projects', ProjectSchema);

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
    ['group_character_members', GroupCharacterMemberSchema],
    ['group_doc_mount_links', GroupDocMountLinkSchema],
    ['project_doc_mount_links', ProjectDocMountLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  // `doc_mount_blobs` has hand-written DDL (a BLOB column generateDDL can't
  // emit) — a harmless read triggers the repo's lazy table-init.
  const { DocMountBlobsRepository } = await import(
    '@/lib/database/repositories/doc-mount-blobs.repository'
  );
  await new DocMountBlobsRepository().findByFileId('00000000-0000-4000-8000-000000000000');

  const repos = getRepositories();
  const TS = spec.seedTimestamp;

  // A single user so the MAIN partition is a valid, non-empty encrypted DB.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );

  const mkMount = async (id: string, name: string) =>
    repos.docMountPoints.create(
      {
        name,
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
      { id, createdAt: TS, updatedAt: TS } as never,
    );

  await mkMount(MP_TARGET, 'Target Store');
  await mkMount(MP_KEEPER, 'Keeper Store');
  await mkMount(MP_GHOST, 'Ghost Store');

  const { storeMountFile } = await import('@/lib/mount-index/store-file');
  const text = async (mountPointId: string, relativePath: string, body: string) =>
    storeMountFile({
      mountPointId,
      relativePath,
      data: Buffer.from(body, 'utf8'),
      originalMimeType: 'text/markdown',
      transcodeImages: false,
      extractText: false,
      enqueueEmbedding: false,
    } as never);
  const blob = async (mountPointId: string, relativePath: string, bytes: number[]) =>
    storeMountFile({
      mountPointId,
      relativePath,
      data: Buffer.from(bytes),
      originalMimeType: 'image/png',
      transcodeImages: false,
      extractText: false,
      enqueueEmbedding: false,
    } as never);

  // MP_TARGET — the delete target.
  await text(MP_TARGET, 'notes/intro.md', '# Intro\n\nThe target store opens.\n');
  await text(MP_TARGET, 'notes/ledger.md', '# Ledger\n\nA second document nobody else links.\n');
  await text(MP_TARGET, 'notes/appendix.md', '# Appendix\n\nShared across two stores.\n');
  await blob(MP_TARGET, 'images/logo.png', [137, 80, 78, 71, 13, 10, 26, 10, 0, 1, 2, 3, 4, 5]);

  // MP_KEEPER — the survivor, plus the cross-store hard link. `linkFile` is v4's
  // REAL op: it reuses the source `doc_mount_files` row and stamps a shared
  // `linkGroupId` on both links, so the appendix file has a refcount of two.
  await text(MP_KEEPER, 'own/readme.md', '# Readme\n\nThe keeper store stays.\n');
  const { linkFile } = await import('@/lib/mount-index/file-ops');
  const linked = await linkFile({
    sourceMountPointId: MP_TARGET,
    sourcePath: 'notes/appendix.md',
    destMountPointId: MP_KEEPER,
    destPath: 'shared/appendix.md',
  });
  if (linked.strategy !== 'db-link') {
    throw new Error(`expected a db-link hard link, got ${linked.strategy}`);
  }

  // MP_GHOST — built whole, orphaned below.
  await text(MP_GHOST, 'ghost/notes.md', '# Ghost\n\nIts store is about to vanish.\n');
  await blob(MP_GHOST, 'ghost/pic.png', [137, 80, 78, 71, 13, 10, 26, 10, 9, 9, 9]);
  // Empty folders with no file in them, so the orphan counts come out ALL
  // DIFFERENT (2 links / 4 folders / 3 chunks). Three statements reap the three
  // tables; equal counts would let a mis-scoped one pass unnoticed.
  const { ensureFolderPath } = await import('@/lib/mount-index/folder-paths');
  await ensureFolderPath(MP_GHOST, 'ghost/archive/old');
  await ensureFolderPath(MP_GHOST, 'ghost/spare');

  // Extra chunks at pinned ids, NULL embeddings (this family never embeds).
  // `storeMountFile` already chunks every native-text document as it writes it
  // (P4.6BK chunk-on-write), so these ride on top of the automatic ones.
  const linkIdFor = (mountPointId: string, rel: string): string => {
    const row = midb
      .prepare(
        'SELECT id FROM doc_mount_file_links WHERE mountPointId = ? AND relativePath = ? LIMIT 1',
      )
      .get(mountPointId, rel) as { id: string } | undefined;
    if (!row) throw new Error(`link not found after storeMountFile: ${mountPointId} ${rel}`);
    return row.id;
  };
  const insertChunk = midb.prepare(
    'INSERT INTO doc_mount_chunks (id, linkId, mountPointId, chunkIndex, content, tokenCount, headingContext, embedding, createdAt, updatedAt) ' +
      'VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)',
  );
  insertChunk.run(CH_TARGET_1, linkIdFor(MP_TARGET, 'notes/intro.md'), MP_TARGET, 0, 'The target store opens.', 5, 'Intro', null, TS, TS);
  insertChunk.run(CH_TARGET_2, linkIdFor(MP_TARGET, 'notes/appendix.md'), MP_TARGET, 0, 'Shared across two stores.', 5, 'Appendix', null, TS, TS);
  insertChunk.run(CH_KEEPER_1, linkIdFor(MP_KEEPER, 'own/readme.md'), MP_KEEPER, 0, 'The keeper store stays.', 5, 'Readme', null, TS, TS);
  insertChunk.run(CH_GHOST_1, linkIdFor(MP_GHOST, 'ghost/notes.md'), MP_GHOST, 1, 'Its store is about to vanish.', 6, 'Ghost', null, TS, TS);
  insertChunk.run(CH_GHOST_2, linkIdFor(MP_GHOST, 'ghost/notes.md'), MP_GHOST, 2, 'And its rows will linger.', 5, 'Ghost', null, TS, TS);

  // A group and a project, both linked to MP_TARGET. Real rows in MAIN (each
  // mints its own official store, which is additional healthy remainder), with
  // the two join rows written at PINNED ids so the differential can name them.
  const group = await repos.groups.create(
    { name: 'Gamma', description: 'The linked group.', state: {} } as never,
    { id: GRP, createdAt: TS, updatedAt: TS } as never,
  );
  if (group.id !== GRP) throw new Error(`group id drift: ${group.id}`);
  const project = await repos.projects.create(
    { name: 'Iota', state: {} } as never,
    { id: PRJ, createdAt: TS, updatedAt: TS } as never,
  );
  if (project.id !== PRJ) throw new Error(`project id drift: ${project.id}`);
  midb
    .prepare(
      'INSERT INTO group_doc_mount_links (id, groupId, mountPointId, createdAt, updatedAt) VALUES (?, ?, ?, ?, ?)',
    )
    .run('ba000000-0000-4000-8000-000000000001', GRP, MP_TARGET, TS, TS);
  midb
    .prepare(
      'INSERT INTO project_doc_mount_links (id, projectId, mountPointId, createdAt, updatedAt) VALUES (?, ?, ?, ?, ?)',
    )
    .run('ba000000-0000-4000-8000-000000000002', PRJ, MP_TARGET, TS, TS);

  // ── The orphan-maker. Last, so nothing re-creates the row. ────────────────
  const ghostLinks = (
    midb
      .prepare('SELECT COUNT(*) AS n FROM doc_mount_file_links WHERE mountPointId = ?')
      .get(MP_GHOST) as { n: number }
  ).n;
  const ghostFolders = (
    midb
      .prepare('SELECT COUNT(*) AS n FROM doc_mount_folders WHERE mountPointId = ?')
      .get(MP_GHOST) as { n: number }
  ).n;
  const ghostChunks = (
    midb
      .prepare('SELECT COUNT(*) AS n FROM doc_mount_chunks WHERE mountPointId = ?')
      .get(MP_GHOST) as { n: number }
  ).n;
  if (ghostLinks !== 2 || ghostFolders !== 4 || ghostChunks !== 3) {
    throw new Error(
      `ghost store shape drifted (links=${ghostLinks} folders=${ghostFolders} chunks=${ghostChunks}); ` +
        'the reaper arm pins 2/4/3 — update both sides deliberately',
    );
  }
  midb.prepare('DELETE FROM doc_mount_points WHERE id = ?').run(MP_GHOST);
  const stillThere = midb
    .prepare('SELECT COUNT(*) AS n FROM doc_mount_file_links WHERE mountPointId = ?')
    .get(MP_GHOST) as { n: number };
  if (stillThere.n !== ghostLinks) {
    throw new Error(
      'the ghost links did NOT survive the point delete — the fixture schema grew a ' +
        'mountPointId foreign key and the planted-orphan recipe no longer works',
    );
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built store-delete fixture: main=${mainOut} mount=${mountOut} ` +
      `(target/keeper/ghost; planted orphans: ${ghostLinks} links, ${ghostFolders} folders, ${ghostChunks} chunk)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`store-delete fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
