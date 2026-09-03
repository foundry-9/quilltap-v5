/**
 * Tier-2 oracle case — v4's `executeImport` over the committed
 * `lorian-and-riya.qtap` sample-content seed (P4.4u4).
 *
 * Drives v4's REAL `executeImport(SINGLE_USER_ID, data, { conflictStrategy:
 * 'skip', includeMemories: true, includeRelatedEntities: false })` end-to-end —
 * exactly the call `seedFromImports`/`handleResetBuiltins` make — reading the
 * SAME committed `.qtap` asset the Rust port embeds. We do NOT mock the storage
 * boundary and do NOT set QUILLTAP_JOB_CHILD, so reindexSingleFile runs (no model
 * for a database-backed store); since P4.6BK v5 chunks on write too, so the
 * differential diffs the resulting link chunkCount exactly.
 *
 * The import spans two DBs, so we dump BOTH:
 *   - MAIN: the slim `characters` rows + `wardrobe_items` + `memories`;
 *   - MOUNT-INDEX: the store tables (doc_mount_points / _folders / _files /
 *     _documents / _file_links) via the raw handle.
 *
 * THE BUG-117 LEG (v4 `0b0617fee`): a THIRD import, of a files-only `.qtap`
 * whose PNG row carries the PRE-TRANSCODE hash. The comparand is the WITHIN-TREE
 * boolean `files.sha256 == doc_mount_blobs.sha256` (the hash string can never be
 * compared across the trees — v4 stores real sharp WebP).
 *
 * NORMALIZATION (done identically on both dumps by the Rust harness): nothing is
 * pinned — every character / wardrobe / memory / mount-point / file / document /
 * link / folder id is minted (v4's `create` strips the source id), so the harness
 * remaps ids to first-seen tokens in natural-key order ACROSS all tables (FKs —
 * incl. each memory's remapped `characterId`/`aboutCharacterId` — verify by
 * relationship) and placeholders timestamps. chunkCount is pinned.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixtures:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_QTAPIMPORT_MAIN=/tmp/qt-qtapimport-main.db \
 *   QT_FIXTURE_QTAPIMPORT_MOUNT=/tmp/qt-qtapimport-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/qtap-import.ts \
 *     > /tmp/oracle-qtap-import.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Spec {
  testPepperBase64: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'qtap-import-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  // The committed sample-content seed (byte-identical to v4's first-startup copy).
  const qtapPath = join(
    here,
    '..',
    '..',
    '..',
    'assets',
    'first-startup',
    'imports',
    'lorian-and-riya.qtap'
  );
  const exportData = JSON.parse(readFileSync(qtapPath, 'utf8'));

  const mainFixture = process.env.QT_FIXTURE_QTAPIMPORT_MAIN;
  const mountFixture = process.env.QT_FIXTURE_QTAPIMPORT_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_QTAPIMPORT_MAIN and QT_FIXTURE_QTAPIMPORT_MOUNT must point at the fixtures from build-qtap-import-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-qtapimport-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'qtapimport-main-work.db');
  const mountWork = join(scratch, 'qtapimport-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { executeImport } = await import('@/lib/import/quilltap-import-service');
  const { SINGLE_USER_ID } = await import('@/lib/auth/single-user');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );

  await initializeDatabase();

  // The op under test: the exact seed-import call.
  const result = await executeImport(SINGLE_USER_ID, exportData, {
    conflictStrategy: 'skip',
    includeMemories: true,
    includeRelatedEntities: false,
  });

  // Bug-75 leg (v4 `40d507cc`): a SECOND import of the committed composite
  // fixture — a character whose wardrobe carries a depth-2 composite chain plus
  // one dangling component reference. Item ids are re-minted on import, so the
  // stored `componentItemIds` must be REMAPPED to the new sibling ids
  // (leaf-first), and the dangling reference must be dropped with a warning.
  // The size-multiset diff above is blind to this (a 36-char UUID remaps
  // size-neutrally), so the items are read back through v4's REAL vault-overlay
  // reader and row-diffed by relationship.
  const bug75Path = join(here, '..', 'fixtures', 'qtap-import-bug75.qtap');
  const bug75Data = JSON.parse(readFileSync(bug75Path, 'utf8'));
  const bug75Result = await executeImport(SINGLE_USER_ID, bug75Data, {
    conflictStrategy: 'skip',
    includeMemories: true,
    includeRelatedEntities: false,
  });
  if (!bug75Result.success || bug75Result.importedCharacterIds.length !== 1) {
    throw new Error(
      `bug75 import did not land one character: ${JSON.stringify(bug75Result)}`
    );
  }
  const { getRepositories } = await import('@/lib/repositories/factory');
  const bramItems = await getRepositories().wardrobe.findByCharacterId(
    bug75Result.importedCharacterIds[0],
    true
  );
  const bug75Items = bramItems
    .map((i) => ({
      id: i.id,
      title: i.title,
      types: i.types,
      componentItemIds: i.componentItemIds ?? [],
      isDefault: i.isDefault ?? false,
      replace: i.replace ?? false,
    }))
    .sort((a, b) => (a.title < b.title ? -1 : a.title > b.title ? 1 : 0));

  // MAIN db: characters + wardrobe_items + memories, read RAW through v4's backend.
  const dumpMain = async (table: string, orderBy: string) => {
    const columns = (
      (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
    ).map((c) => c.name);
    const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<
      Record<string, unknown>
    >;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };
  const characters = await dumpMain('characters', 'name');
  const wardrobe = await dumpMain('wardrobe_items', 'title');
  const memories = await dumpMain('memories', 'content');

  // MOUNT-INDEX db: the store tables, read directly through the raw handle.
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable (degraded open?)');
  const dumpMount = (table: string, orderBy: string) => {
    const columns = (
      midb.pragma(`table_info(${table})`) as Array<{ name: string }>
    ).map((c) => c.name);
    const rawRows = midb
      .prepare(`SELECT * FROM ${table}`)
      .all() as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };

  const points = dumpMount('doc_mount_points', 'name');
  const folders = dumpMount('doc_mount_folders', 'path');
  const files = dumpMount('doc_mount_files', 'sha256');
  const documents = dumpMount('doc_mount_documents', 'contentSha256');
  const links = dumpMount('doc_mount_file_links', 'relativePath');

  closeMountIndexSQLiteClient();
  await closeDatabase();

  // ── P4.D152 / bug 117 (v4 `0b0617fee`) — an ISOLATED second pair ──────────
  //
  // A files-only `.qtap` whose PNG row carries the PRE-TRANSCODE hash, which is
  // exactly what a pre-4.9.0 exporting instance wrote. The importer's bridge
  // transcodes the bitmap, so a row that records the archive's `sha256` cannot be
  // joined to the mount blob it points at. The hash STRING can never be compared
  // across the trees (v4 stores real sharp WebP, the Rust harness a byte-changing
  // test codec), so the comparand is the WITHIN-TREE boolean
  // `files.sha256 == doc_mount_blobs.sha256`, joined through the parsed key.
  //
  // It runs on a SECOND copy of the same fixtures, not the pair above: writing
  // two blobs into the shared pair would change the `doc_mount_files`
  // `fileSizeBytes` multiset the main diff asserts, and those sizes legitimately
  // differ between sharp's WebP and the harness codec's bytes. Isolation keeps
  // the two questions apart.
  //
  // The uploads mount is PLANTED (identically on both sides — it is fixture
  // scaffolding, not ported code): the shared fixtures are empty, and
  // `writeUserUploadToMountStore` refuses without a `userUploadsMountPointId`
  // instance setting pointing at a database-backed store.
  const bug117 = await (async () => {
    const scratch2 = mkdtempSync(join(tmpdir(), 'qt-qtapimport-bug117-'));
    mkdirSync(join(scratch2, 'data'), { recursive: true });
    const main2 = join(scratch2, 'bug117-main.db');
    const mount2 = join(scratch2, 'bug117-mount.db');
    copyFileSync(mainFixture, main2);
    copyFileSync(mountFixture, mount2);
    process.env.SQLITE_PATH = main2;
    process.env.SQLITE_MOUNT_INDEX_PATH = mount2;
    process.env.QUILLTAP_DATA_DIR = scratch2;

    await initializeDatabase();
    const midb2 = getRawMountIndexDatabase();
    if (!midb2) throw new Error('mount-index DB handle unavailable (bug117 pair)');

    const UPLOADS_MP = 'aaaaaaaa-0000-4000-8000-00000000dd01';
    const TS = '2026-02-28T17:11:10.563Z';
    midb2
      .prepare(
        'INSERT INTO doc_mount_points (id, name, basePath, mountType, storeType, createdAt, updatedAt) ' +
          "VALUES (?, 'Quilltap Uploads', '', 'database', 'documents', ?, ?)"
      )
      .run(UPLOADS_MP, TS, TS);
    await rawQuery(
      'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)'
    );
    await rawQuery('INSERT INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
      'userUploadsMountPointId',
      UPLOADS_MP,
    ]);

    const data117 = JSON.parse(
      readFileSync(join(here, '..', 'fixtures', 'qtap-import-bug117.qtap'), 'utf8')
    );
    const r117 = await executeImport(SINGLE_USER_ID, data117, {
      conflictStrategy: 'skip',
      includeMemories: true,
      includeRelatedEntities: false,
    });

    const rows = (await rawQuery(
      'SELECT originalFilename, mimeType, category, isPlainText, storageKey, sha256 ' +
        "FROM files WHERE storageKey LIKE 'mount-blob:%' ORDER BY originalFilename"
    )) as Array<Record<string, unknown>>;
    const findBlob = midb2.prepare(
      'SELECT sha256, storedMimeType FROM doc_mount_blobs WHERE id = ?'
    );
    const shaJoin = rows.map((r) => {
      const key = String(r.storageKey);
      const rest = key.slice('mount-blob:'.length);
      const sep = rest.indexOf(':');
      const blobId = sep < 1 || sep === rest.length - 1 ? null : rest.slice(sep + 1);
      const blob = blobId
        ? (findBlob.get(blobId) as { sha256: string; storedMimeType: string } | undefined)
        : undefined;
      return {
        filename: r.originalFilename,
        mimeType: r.mimeType,
        category: r.category,
        isPlainText: r.isPlainText,
        blobFound: !!blob,
        storedMimeTypeInBlob: blob ? blob.storedMimeType : null,
        shaMatchesBlob: !!blob && blob.sha256 === r.sha256,
      };
    });

    closeMountIndexSQLiteClient();
    await closeDatabase();
    return {
      success: r117.success,
      warnings: r117.warnings,
      imported: r117.imported.files ?? null,
      shaJoin,
    };
  })();

  process.stdout.write(
    JSON.stringify({
      case: 'qtap-import-tier2',
      result: {
        success: result.success,
        imported: result.imported,
        skipped: result.skipped,
        warnings: result.warnings,
        importedCharacterIdCount: result.importedCharacterIds.length,
      },
      characters,
      wardrobe,
      memories,
      points,
      folders,
      files,
      documents,
      links,
      bug75: {
        warnings: bug75Result.warnings,
        items: bug75Items,
      },
      bug117,
    }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`qtap-import-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
