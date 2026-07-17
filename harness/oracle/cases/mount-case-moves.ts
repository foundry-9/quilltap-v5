/**
 * Differential oracle — the database-store case-only move/rename changes of v4
 * `0a0419f5`: `moveDatabaseDocument` (a case-only document rename is allowed —
 * the case-insensitive lookup would otherwise report a bogus conflict) and
 * `moveDatabaseFolder` (the dest-conflict allows the source itself, and the
 * destination is canonicalised under the destination PARENT's stored casing;
 * descendants + links are prefix-rewritten from the source's stored path).
 *
 * Drives v4's REAL `moveDatabaseDocument` / `moveDatabaseFolder` over a fresh
 * copy of the doc-mount-file-links tier-2 fixture (the seeded database store),
 * seeding documents via the real `linkDocumentContent`, then dumps folder paths
 * + link relativePaths (deterministic — minted ids/timestamps dropped).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the shared fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-dmfl-fixture.db \
 *     $N/node --import tsx <W>/harness/oracle/fixtures/build-doc-mount-file-links-fixture.ts
 *   QT_FIXTURE_MOUNT_CASE_MOVES=/tmp/qt-dmfl-fixture.db \
 *     $N/node --import tsx <W>/harness/oracle/cases/mount-case-moves.ts \
 *     > /tmp/oracle-mount-case-moves.ndjson
 */

import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const STORE_ID = '5700000e-0000-4000-8000-0000000000d5';
const TEST_PEPPER = 'ZpjI5jcj5CYsyBA6zPH90G4frQEbv2WsAhERvEKrjJk=';

function detectFileType(rel: string): 'markdown' | 'txt' | 'json' | 'jsonl' {
  const dot = rel.lastIndexOf('.');
  const ext = dot >= 0 ? rel.slice(dot).toLowerCase() : '';
  if (ext === '.md' || ext === '.markdown') return 'markdown';
  if (ext === '.txt') return 'txt';
  if (ext === '.json') return 'json';
  return 'jsonl';
}

async function main(): Promise<void> {
  const fixture = process.env.QT_FIXTURE_MOUNT_CASE_MOVES;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_MOUNT_CASE_MOVES must point at the doc-mount-file-links fixture db');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mount-case-moves-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'mount-index-work.db');
  copyFileSync(fixture, work);

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
  const { DocMountFileLinksRepository } = await import(
    '@/lib/database/repositories/doc-mount-file-links.repository'
  );
  const { sha256OfString } = await import('@/lib/utils/sha256');
  const { moveDatabaseDocument, moveDatabaseFolder } = await import(
    '@/lib/mount-index/database-store'
  );
  const { copyFile, moveFile } = await import('@/lib/mount-index/file-ops');

  await initializeDatabase();
  const repo = new DocMountFileLinksRepository();

  const seed = async (rel: string, content: string) => {
    await repo.linkDocumentContent({
      mountPointId: STORE_ID,
      relativePath: rel,
      fileName: rel.includes('/') ? rel.slice(rel.lastIndexOf('/') + 1) : rel,
      folderId: null,
      fileType: detectFileType(rel),
      content,
      contentSha256: sha256OfString(content),
      plainTextLength: content.length,
      fileSizeBytes: Buffer.byteLength(content, 'utf-8'),
    });
  };
  await seed('notes.md', 'root note');
  await seed('Lore/entry.md', 'lore entry');
  await seed('Lore/sub/deep.md', 'deep lore');
  await seed('Archive/keep.md', 'archived');
  await seed('guard.md', 'do not delete me');

  // 0a. copyFile same-path guard: force-copying guard.md onto GUARD.md (a
  //     case-variant of itself) must be REJECTED before the force-overwrite
  //     branch, so the source is NOT deleted. Capture the error.
  let guard: { error: string; code: string | undefined } | null = null;
  try {
    await copyFile({
      sourceMountPointId: STORE_ID,
      sourcePath: 'guard.md',
      destMountPointId: STORE_ID,
      destPath: 'GUARD.md',
      force: true,
    });
  } catch (err) {
    guard = {
      error: err instanceof Error ? err.message : String(err),
      code: (err as { code?: string })?.code,
    };
  }
  // 0b. moveFile case-only rename (guard.md → Guard.md on the same mount): legal.
  await moveFile({
    sourceMountPointId: STORE_ID,
    sourcePath: 'guard.md',
    destMountPointId: STORE_ID,
    destPath: 'Guard.md',
  });

  // 1. case-only DOCUMENT rename (notes.md → Notes.md): allowed, no bogus conflict.
  await moveDatabaseDocument(STORE_ID, 'notes.md', 'Notes.md');
  // 2. move Lore UNDER Archive, addressing the parent in a DIFFERENT casing
  //    (archive) — the destination canonicalises under Archive's stored path.
  await moveDatabaseFolder(STORE_ID, 'Lore', 'archive/Lore');
  // 3. case-only rename of Archive (now containing Lore): the dest-conflict finds
  //    the source itself and allows it; descendants + links rewrite under ARCHIVE.
  await moveDatabaseFolder(STORE_ID, 'Archive', 'ARCHIVE');

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');

  const folders = (
    midb
      .prepare(`SELECT path FROM doc_mount_folders WHERE mountPointId = ? ORDER BY path`)
      .all(STORE_ID) as Array<{ path: string }>
  ).map((r) => ({ path: r.path }));
  const links = (
    midb
      .prepare(
        `SELECT relativePath, fileName FROM doc_mount_file_links
           WHERE mountPointId = ? ORDER BY relativePath`,
      )
      .all(STORE_ID) as Array<{ relativePath: string; fileName: string }>
  ).map((r) => ({ relativePath: r.relativePath, fileName: r.fileName }));

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stdout.write(JSON.stringify({ guard, folders, links }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`mount-case-moves oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
