/**
 * Tier-2 oracle case — v4's `CharactersRepository.update` (Phase-2, the
 * store-backed capstone sub-unit 4a — the update vault integration).
 *
 * Drives v4's REAL `repos.characters.update(id, patch)` over the baked fixture
 * character: `applyDocumentStoreWriteOverlay` routes the managed fields to the
 * vault (markdown writes, the properties.json read-modify-write, prompt/scenario
 * reprojection), the unmanaged remainder goes to the slim `_update` (skipped when
 * empty), and the overlay re-reads. We do NOT set QUILLTAP_JOB_CHILD, so
 * reindexSingleFile runs; since P4.6BK v5 chunks on write too, so the
 * differential diffs chunkCount exactly.
 *
 * The character id is read back from the fixture so both sides target the same
 * minted id. A character spans two DBs, so we dump BOTH the main slim `characters`
 * row and the mount-index store tables.
 *
 * P4.22 (dogfood finding #47) extended the case with the vault-properties arms,
 * in THREE phases:
 *   - `afterOps`: the spec's ops (now including `deleteProperties` + the
 *     absent-arm reseed — genuine absence seeds the empty-managed defaults on
 *     BOTH sides), dumped and diffed as before.
 *   - `postPlant`: `plantedProperties` (malformed bytes) written through the
 *     REAL `writeDatabaseDocument`; both sides dump and still match.
 *   - `corrupt`: `corruptPatch` applied EXPECTING THE SIDES TO DIVERGE. v4's
 *     `readCharacterVaultProperties` collapses the parse failure into `null`
 *     and the `??`-seed CLOBBERS the whole bag to defaults (this case records
 *     `message: null` + the clobbered `finalProperties`); v5 refuses and writes
 *     nothing. The Rust harness pins both directions, so this arm goes RED the
 *     moment v4 lands its own #47 fix — reclassify to a drift re-port then.
 *
 * NORMALIZATION (done identically on both dumps by the Rust harness): the
 * shared-id remap across all six tables (FKs verify by relationship) + timestamp
 * placeholders; chunkCount pinned. The DB-field change (isFavorite) and every
 * vault file's content diff exactly.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixtures:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARUPD_MAIN=/tmp/qt-charupd-main.db \
 *   QT_FIXTURE_CHARUPD_MOUNT=/tmp/qt-charupd-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/characters-update.ts \
 *     > /tmp/oracle-charupd.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind?: 'update' | 'deleteProperties';
  patch?: Record<string, unknown>;
}
interface Spec {
  testPepperBase64: string;
  ops: Op[];
  plantedProperties: string;
  corruptPatch: Record<string, unknown>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'characters-update-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainFixture = process.env.QT_FIXTURE_CHARUPD_MAIN;
  const mountFixture = process.env.QT_FIXTURE_CHARUPD_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_CHARUPD_MAIN and QT_FIXTURE_CHARUPD_MOUNT must point at the fixtures from build-characters-update-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-charupd-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'charupd-main-work.db');
  const mountWork = join(scratch, 'charupd-mount-work.db');
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
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { writeDatabaseDocument, deleteDatabaseDocument } = await import(
    '@/lib/mount-index/database-store'
  );

  await initializeDatabase();
  const repos = getRepositories();

  // Read the baked character id (both sides target the same one).
  const idRow = (await rawQuery('SELECT id FROM characters LIMIT 1')) as Array<{ id: string }>;
  if (idRow.length === 0) throw new Error('fixture has no character row');
  const characterId = idRow[0].id;

  // The vault FK, read RAW (the vault-properties arms need the mount point).
  const fkRow = (await rawQuery(
    'SELECT characterDocumentMountPointId AS mp FROM characters LIMIT 1'
  )) as Array<{ mp: string | null }>;
  const mountPointId = fkRow[0]?.mp;
  if (!mountPointId) throw new Error('fixture character has no vault FK');

  // Phase 1: the ops under test.
  for (const op of spec.ops) {
    if (op.kind === 'deleteProperties') {
      await deleteDatabaseDocument(mountPointId, 'properties.json');
    } else {
      await repos.characters.update(characterId, op.patch as never);
    }
  }

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable (degraded open?)');

  const dumpMountTable = (table: string, orderBy: string) => {
    const columns = (
      midb.pragma(`table_info(${table})`) as Array<{ name: string }>
    ).map((c) => c.name);
    const rawRows = midb
      .prepare(`SELECT * FROM ${table}`)
      .all() as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };
  const snapshot = async () => {
    const charColumns = (
      (await rawQuery('PRAGMA table_info(characters)')) as Array<{ name: string }>
    ).map((c) => c.name);
    const charRows = (await rawQuery('SELECT * FROM characters')) as Array<
      Record<string, unknown>
    >;
    return {
      characters: canonicalizeRows({
        table: 'characters',
        columns: charColumns,
        rawRows: charRows,
        orderBy: 'name',
      }),
      points: dumpMountTable('doc_mount_points', 'name'),
      folders: dumpMountTable('doc_mount_folders', 'path'),
      files: dumpMountTable('doc_mount_files', 'sha256'),
      documents: dumpMountTable('doc_mount_documents', 'contentSha256'),
      links: dumpMountTable('doc_mount_file_links', 'relativePath'),
    };
  };

  const afterOps = await snapshot();

  // Phase 2: plant the malformed bytes through the REAL writeDatabaseDocument.
  await writeDatabaseDocument(mountPointId, 'properties.json', spec.plantedProperties);
  const postPlant = await snapshot();

  // Phase 3: the corrupt-arm patch. v4 TODAY does not throw — the RMW seed
  // clobbers the bag (finding #47). Record whatever happens so the harness can
  // pin the divergence in both directions.
  let message: string | null = null;
  try {
    await repos.characters.update(characterId, spec.corruptPatch as never);
  } catch (err) {
    message = err instanceof Error ? err.message : String(err);
  }
  const propsRow = midb
    .prepare(
      `SELECT d.content AS content FROM doc_mount_file_links l
        JOIN doc_mount_documents d ON d.fileId = l.fileId
       WHERE l.mountPointId = ? AND l.relativePath = 'properties.json'`
    )
    .get(mountPointId) as { content: string } | undefined;
  const finalProperties = propsRow ? propsRow.content : null;

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stdout.write(
    JSON.stringify({
      case: 'characters-update-tier2',
      afterOps,
      postPlant,
      corrupt: { message, finalProperties },
    }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`characters-update-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
