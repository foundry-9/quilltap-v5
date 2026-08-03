/**
 * Differential oracle — `linkFile` binds a hard-link group, `copyFile` does not
 * (v4 `40319484`, `lib/mount-index/file-ops.ts`).
 *
 * **This surface had NO oracle coverage at all before this case.** Nothing drove
 * v4's real `linkFile`, so the whole `link`-vs-`copy` distinction — the single
 * user-visible thing `40319484` is about — was unverified.
 *
 * Drives v4's REAL `copyFile` / `linkFile` / `writeDatabaseDocument` over a
 * fresh copy of the doc-mount-file-links tier-2 fixture, in both storage shapes:
 *
 *   - **DB → DB.** A copy and a link both end up sharing the source's content
 *     row (the store is content-addressed), and are indistinguishable until
 *     something is written. Then a write through any GROUP member repoints every
 *     member, while the copy forks onto its own content row. Linking a third
 *     location EXTENDS the group rather than minting a second.
 *   - **FS → FS.** The OS shares the bytes through the inode, so no content
 *     fan-out is needed — but the index rows are per-path, so `linkFile` records
 *     the group anyway and the sibling gets re-indexed. Two filesystem mounts
 *     are minted on a scratch directory and the source is indexed through v4's
 *     real `processMountFile` (an unindexed source has no link id to bind, which
 *     is the arm v4 warns about).
 *
 * Dumps `doc_mount_file_links` (relativePath / a stable group LABEL / a stable
 * content LABEL / description) plus a content-row census. Ids and timestamps are
 * minted internally and are never compared: group ids and file ids are replaced
 * by first-seen labels in dump order, so what is asserted is the RELATIONSHIP —
 * who shares a group, who shares content — not the values.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the shared fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-dmfl-fixture.db \
 *     $N/node --import tsx <W>/harness/oracle/fixtures/build-doc-mount-file-links-fixture.ts
 *   QT_FIXTURE_MOUNT_LINK_GROUPS=/tmp/qt-dmfl-fixture.db \
 *     $N/node --import tsx <W>/harness/oracle/cases/mount-link-groups.ts \
 *     > /tmp/oracle-mount-link-groups.ndjson
 */

import { mkdtempSync, mkdirSync, copyFileSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const STORE_ID = '5700000e-0000-4000-8000-0000000000d5';
const TEST_PEPPER = 'ZpjI5jcj5CYsyBA6zPH90G4frQEbv2WsAhERvEKrjJk=';

/** Pinned so both sides address the same filesystem mounts. */
const FS_SOURCE_MOUNT_ID = '5700000e-0000-4000-8000-0000000000f1';
const FS_DEST_MOUNT_ID = '5700000e-0000-4000-8000-0000000000f2';

async function main(): Promise<void> {
  const fixture = process.env.QT_FIXTURE_MOUNT_LINK_GROUPS;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_MOUNT_LINK_GROUPS must point at the doc-mount-file-links fixture db');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mount-link-groups-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'mount-index-work.db');
  copyFileSync(fixture, work);

  // The two filesystem mounts live under the scratch root. Their basePath is
  // therefore per-run, which is why nothing in the dump may mention it.
  const fsSourceBase = join(scratch, 'fs-source');
  const fsDestBase = join(scratch, 'fs-dest');
  mkdirSync(fsSourceBase, { recursive: true });
  mkdirSync(fsDestBase, { recursive: true });
  writeFileSync(join(fsSourceBase, 'shared.md'), '# Shared\n\nOne inode, two paths.\n', 'utf8');

  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = work;
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
  const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
  const { copyFile, linkFile } = await import('@/lib/mount-index/file-ops');
  const { processMountFile } = await import('@/lib/mount-index/scanner');

  await initializeDatabase();

  // ---------------------------------------------------------------------
  // DB → DB
  // ---------------------------------------------------------------------
  await writeDatabaseDocument(STORE_ID, 'origin.md', '# Origin\n\nThe first revision.');

  // A copy shares the deduped content row but joins no group.
  await copyFile({
    sourceMountPointId: STORE_ID,
    sourcePath: 'origin.md',
    destMountPointId: STORE_ID,
    destPath: 'copy-of-origin.md',
    force: false,
  });
  // A link joins one…
  await linkFile({
    sourceMountPointId: STORE_ID,
    sourcePath: 'origin.md',
    destMountPointId: STORE_ID,
    destPath: 'linked-origin.md',
  });
  // …and linking a third location EXTENDS it rather than minting a second.
  await linkFile({
    sourceMountPointId: STORE_ID,
    sourcePath: 'origin.md',
    destMountPointId: STORE_ID,
    destPath: 'third-origin.md',
  });

  // The distinction, made visible: write through a NON-anchor group member. The
  // two other members follow; the copy forks onto its own (unchanged) content.
  await writeDatabaseDocument(STORE_ID, 'linked-origin.md', '# Origin\n\nThe second revision.');

  // ---------------------------------------------------------------------
  // FS → FS
  // ---------------------------------------------------------------------
  const points = new DocMountPointsRepository();
  const fsMount = (id: string, name: string, basePath: string) =>
    points.create(
      {
        name,
        basePath,
        mountType: 'filesystem',
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
      { id, createdAt: '2026-02-01T00:00:00.000Z', updatedAt: '2026-02-01T00:00:00.000Z' }
    );
  const sourceMount = await fsMount(FS_SOURCE_MOUNT_ID, 'FS Source', fsSourceBase);
  await fsMount(FS_DEST_MOUNT_ID, 'FS Dest', fsDestBase);

  // Index the source: an UNINDEXED filesystem source has no link id to bind, and
  // v4 only warns in that case. Binding is what this leg is here to check.
  await processMountFile(sourceMount as never, join(fsSourceBase, 'shared.md'), 'shared.md');

  await linkFile({
    sourceMountPointId: FS_SOURCE_MOUNT_ID,
    sourcePath: 'shared.md',
    destMountPointId: FS_DEST_MOUNT_ID,
    destPath: 'mirror.md',
  });

  // ---------------------------------------------------------------------
  // Dump
  // ---------------------------------------------------------------------
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');

  const rows = midb
    .prepare(
      `SELECT l.mountPointId, l.relativePath, l.linkGroupId, l.fileId, l.description
         FROM doc_mount_file_links l
        ORDER BY l.mountPointId, l.relativePath`
    )
    .all() as Array<{
    mountPointId: string;
    relativePath: string;
    linkGroupId: string | null;
    fileId: string;
    description: string | null;
  }>;

  // First-seen labels in dump order: the VALUES are minted, the relationships
  // are the contract.
  const labels = new Map<string, string>();
  const label = (prefix: string, raw: string | null): string | null => {
    if (raw === null) return null;
    if (!labels.has(raw)) labels.set(raw, `${prefix}${labels.size}`);
    return labels.get(raw)!;
  };
  // Mount ids are pinned on both sides, so they stay literal.
  const links = rows.map((r) => ({
    mountPointId: r.mountPointId,
    relativePath: r.relativePath,
    linkGroupId: label('G', r.linkGroupId),
    fileId: label('F', r.fileId),
    description: r.description ?? '',
  }));

  // The content census: how many links point at each content row, and (for
  // database-backed documents) what those bytes now are.
  const contents = (
    midb
      .prepare(
        `SELECT f.id AS fileId, COUNT(l.id) AS linkCount, d.content AS content
           FROM doc_mount_files f
           LEFT JOIN doc_mount_file_links l ON l.fileId = f.id
           LEFT JOIN doc_mount_documents d ON d.fileId = f.id
          GROUP BY f.id
          ORDER BY d.content, f.id`
      )
      .all() as Array<{ fileId: string; linkCount: number; content: string | null }>
  )
    .filter((c) => c.linkCount > 0)
    .map((c) => ({
      fileId: label('F', c.fileId),
      linkCount: c.linkCount,
      content: c.content,
    }));

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stdout.write(
    JSON.stringify({ case: 'mount-link-groups', links, contents }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`mount-link-groups oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
