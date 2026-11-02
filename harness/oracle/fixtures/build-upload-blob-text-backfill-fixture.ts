/**
 * Fixture builder for the uploaded-blob text backfill differential (P4.D354,
 * v4 `40fab1c54`, bugs 260 / 261): the PRE-bug-260 upload state.
 *
 * Copies the committed mounts pair (`crates/quilltap-web/tests/fixtures/
 * mounts-{main,mount}.db`, located from this file's own path — read-only; no
 * env var points into the repo) and plants uploads through v4's REAL
 * `storeMountFile` with the bridges' OLD options (`extractText: false` — the
 * pin's bridges already extract, so the plant calls `storeMountFile`
 * directly), so every planted row is v4-made and `extractionStatus 'none'`:
 *
 *   MP_DB  uploads/CHANGELOG.md   native text            → extracted
 *   MP_DB  uploads/notes.TXT      native text (UPPER ext) → extracted
 *   MP_DB  uploads/data.json      native text            → extracted
 *   MP_DB  uploads/blank.txt      whitespace only        → the `failed` arm
 *   MP_DB  uploads/Report.PDF     garbage bytes          → R-1's carve
 *   MP_DB  uploads/photo.png      not extractable        → never listed
 *   MP_DB  uploads/gone.md        `data` NULLed          → bytes missing
 *   MP_DB  uploads/done.md        stored WITH extraction → not on the worklist
 *   MP_EMPTY inbox/second.md      native text, 2nd store → storesTouched 2
 *
 * `doc_mount_blobs.data` is `NOT NULL` in v4's hand DDL, so the copy rebuilds
 * the table with a nullable `data` column (same columns, same order, same
 * unique index) before the plant — the one way to reach v4's `readData`
 * `null` arm for a row the worklist JOIN still finds.
 *
 * MINTED per run (fresh UUIDs) — NOT committed; rebuild, regenerate, then
 * `cargo test` against that SAME build, in that order.
 *
 * Run (Node 24, from the v4 checkout or a pin worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_UBTB_MAIN=/tmp/qt-ubtb-main.db QT_FIXTURE_UBTB_MOUNT=/tmp/qt-ubtb-mount.db \
 *     $N/node --import tsx <v5>/harness/oracle/fixtures/build-upload-blob-text-backfill-fixture.ts
 */

import { copyFileSync, existsSync, mkdtempSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const MP_DB = 'b6000000-0000-4000-8000-000000000001';
const MP_EMPTY = 'b6000000-0000-4000-8000-000000000002';

const PLANTS: Array<{ mp: string; path: string; mime: string; bytes: Buffer; extractText: boolean }> = [
  { mp: MP_DB, path: 'uploads/CHANGELOG.md', mime: 'text/markdown', extractText: false,
    bytes: Buffer.from('# Changelog\n\n## 1.0\n\nThe bees moved to the roof.\n', 'utf8') },
  { mp: MP_DB, path: 'uploads/notes.TXT', mime: 'text/plain', extractText: false,
    bytes: Buffer.from('plain text upload\nsecond line\n', 'utf8') },
  { mp: MP_DB, path: 'uploads/data.json', mime: 'application/json', extractText: false,
    bytes: Buffer.from('{"kind":"upload","n":3}\n', 'utf8') },
  { mp: MP_DB, path: 'uploads/blank.txt', mime: 'text/plain', extractText: false,
    bytes: Buffer.from('  \n\t\n', 'utf8') },
  { mp: MP_DB, path: 'uploads/Report.PDF', mime: 'application/pdf', extractText: false,
    bytes: Buffer.from('definitely not a pdf either', 'utf8') },
  { mp: MP_DB, path: 'uploads/photo.png', mime: 'image/png', extractText: false,
    bytes: Buffer.from([137, 80, 78, 71, 13, 10, 26, 10, 3, 3, 3]) },
  { mp: MP_DB, path: 'uploads/gone.md', mime: 'text/markdown', extractText: false,
    bytes: Buffer.from('# Gone\n\nthese bytes go missing\n', 'utf8') },
  { mp: MP_DB, path: 'uploads/done.md', mime: 'text/markdown', extractText: true,
    bytes: Buffer.from('# Done\n\nalready extracted at ingest\n', 'utf8') },
  { mp: MP_EMPTY, path: 'inbox/second.md', mime: 'text/markdown', extractText: false,
    bytes: Buffer.from('# Second store\n\nanother pre-round upload\n', 'utf8') },
];

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'mounts.json'), 'utf8')) as {
    testPepperBase64: string;
  };
  const committed = join(here, '..', '..', '..', 'crates', 'quilltap-web', 'tests', 'fixtures');
  const srcMain = join(committed, 'mounts-main.db');
  const srcMount = join(committed, 'mounts-mount.db');
  const outMain = process.env.QT_FIXTURE_UBTB_MAIN;
  const outMount = process.env.QT_FIXTURE_UBTB_MOUNT;
  if (!outMain || !outMount) {
    throw new Error('QT_FIXTURE_UBTB_MAIN and QT_FIXTURE_UBTB_MOUNT are required');
  }
  if (!existsSync(srcMain) || !existsSync(srcMount)) {
    throw new Error(`the committed mounts pair is missing under ${committed}`);
  }
  for (const out of [outMain, outMount]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
  }
  copyFileSync(srcMain, outMain);
  copyFileSync(srcMount, outMount);

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ubtb-build-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';
  // Hold v4's job host off: nothing here enqueues, but the store path imports it.
  (globalThis as Record<string, unknown>).__quilltapJobHost = {
    child: null, spawning: false, shuttingDown: true, childCrashed: false,
    restartTimestamps: [], signalHandlersInstalled: false, invalidationListeners: new Set(),
  };

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();
  const midb = getRawMountIndexDatabase() as unknown as {
    exec: (s: string) => void;
    prepare: (s: string) => { run: (...a: unknown[]) => unknown };
  };

  // The nullable-`data` rebuild (header). Same columns and order as v4's DDL.
  midb.exec(`
    CREATE TABLE IF NOT EXISTS "doc_mount_blobs" (
      "id" TEXT PRIMARY KEY, "fileId" TEXT NOT NULL, "sha256" TEXT NOT NULL,
      "sizeBytes" INTEGER NOT NULL, "storedMimeType" TEXT NOT NULL, "data" BLOB NOT NULL,
      "createdAt" TEXT NOT NULL, "updatedAt" TEXT NOT NULL,
      FOREIGN KEY ("fileId") REFERENCES "doc_mount_files" ("id") ON DELETE CASCADE
    );
    CREATE TABLE "doc_mount_blobs_nullable" (
      "id" TEXT PRIMARY KEY, "fileId" TEXT NOT NULL, "sha256" TEXT NOT NULL,
      "sizeBytes" INTEGER NOT NULL, "storedMimeType" TEXT NOT NULL, "data" BLOB,
      "createdAt" TEXT NOT NULL, "updatedAt" TEXT NOT NULL,
      FOREIGN KEY ("fileId") REFERENCES "doc_mount_files" ("id") ON DELETE CASCADE
    );
    INSERT INTO "doc_mount_blobs_nullable" SELECT * FROM "doc_mount_blobs";
    DROP TABLE "doc_mount_blobs";
    ALTER TABLE "doc_mount_blobs_nullable" RENAME TO "doc_mount_blobs";
    CREATE UNIQUE INDEX IF NOT EXISTS "idx_doc_mount_blobs_fileId" ON "doc_mount_blobs" ("fileId");
  `);

  const { storeMountFile } = await import('@/lib/mount-index/store-file');
  for (const p of PLANTS) {
    await storeMountFile({
      mountPointId: p.mp,
      relativePath: p.path,
      data: p.bytes,
      originalMimeType: p.mime,
      originalFileName: p.path.split('/').pop(),
      treatNativeTextAsDocument: false,
      transcodeImages: true,
      extractText: p.extractText,
      enqueueEmbedding: false,
      collisionStrategy: 'unique-suffix',
      assetStorage: 'database',
    });
  }
  midb
    .prepare(
      `UPDATE doc_mount_blobs SET data = NULL WHERE fileId =
         (SELECT fileId FROM doc_mount_file_links WHERE mountPointId = ? AND relativePath = ?)`,
    )
    .run(MP_DB, 'uploads/gone.md');

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(`built upload-blob-text-backfill fixtures: main=${outMain} mount=${outMount}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`upload-blob-text-backfill fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
