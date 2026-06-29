/**
 * Tier-2 oracle case — the `groups` STORE-BACKED pilot (document-store overlay
 * slice, build steps 2-3).
 *
 * Drives v4's REAL `repos.groups.create()` / `.update()` end-to-end — the whole
 * store-backed machine: the slim-row `_create`, `ensureGroupOfficialStore`
 * (provision a `Group Files: <name>` mount point + link + FK), the group
 * write-overlay (`writeGroupStoreManagedFields` → `writeDatabaseDocument` →
 * `linkDocumentContent`), and the closing overlay re-read. We do NOT mock the
 * storage boundary (that would defeat the purpose) and we do NOT set
 * QUILLTAP_JOB_CHILD (it reroutes `getRepositories()` through the forked-child
 * write proxy). So v4's post-write `reindexSingleFile` chunk pass DOES run — but
 * for database-backed stores it calls no model (deterministic text chunking),
 * and its only persisted divergence from the Rust storage primitive is the link
 * `chunkCount` (0 → N) + the `doc_mount_chunks` rows. The differential pins
 * `chunkCount` and excludes `doc_mount_chunks`; every other field matches.
 *
 * A store-backed entity spans two DBs, so we dump BOTH:
 *   - the MAIN slim `groups` row (via `rawQuery`);
 *   - the MOUNT-INDEX store tables (`doc_mount_points`, `_files`, `_documents`,
 *     `_file_links`, `_folders`) + `group_doc_mount_links` (via the raw handle).
 *
 * NORMALIZATION (done identically on both dumps by the Rust harness): every id is
 * minted internally (group ids, the mount-point id, file/document/link ids) and
 * every timestamp is minted, so NOTHING is pinnable. The harness remaps ids to
 * first-seen tokens in natural-key order ACROSS all tables (so the cross-table
 * FKs — `groups.officialMountPointId` → `doc_mount_points.id`, `link.fileId` →
 * `file.id`, `link.mountPointId` → the store, etc. — verify by relationship) and
 * placeholders timestamps. `chunkCount` is pinned (reindex artifact).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixtures:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_GROUPS_MAIN=/tmp/qt-groups-main.db \
 *   QT_FIXTURE_GROUPS_MOUNT=/tmp/qt-groups-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/groups-tier2.ts \
 *     > /tmp/oracle-groups.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface CreateOp {
  kind: 'create';
  label: string;
  input: Record<string, unknown>;
}
interface UpdateOp {
  kind: 'update';
  label: string;
  patch: Record<string, unknown>;
}
type Op = CreateOp | UpdateOp;

interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'groups-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainFixture = process.env.QT_FIXTURE_GROUPS_MAIN;
  const mountFixture = process.env.QT_FIXTURE_GROUPS_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_GROUPS_MAIN and QT_FIXTURE_GROUPS_MOUNT must point at the fixtures from build-groups-tier2-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-groups-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'groups-main-work.db');
  const mountWork = join(scratch, 'groups-mount-work.db');
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

  await initializeDatabase();
  const repos = getRepositories();

  // Run the op sequence, tracking minted ids by label (both sides do the same).
  const idByLabel = new Map<string, string>();
  for (const op of spec.ops) {
    if (op.kind === 'create') {
      const created = await repos.groups.create(op.input as never);
      idByLabel.set(op.label, created.id);
    } else {
      const id = idByLabel.get(op.label);
      if (!id) throw new Error(`update references unknown label: ${op.label}`);
      await repos.groups.update(id, op.patch as never);
    }
  }

  // MAIN db: the slim groups row, read RAW through v4's own backend.
  const groupsColumns = (
    (await rawQuery('PRAGMA table_info(groups)')) as Array<{ name: string }>
  ).map((c) => c.name);
  const groupsRows = (await rawQuery('SELECT * FROM groups')) as Array<
    Record<string, unknown>
  >;
  const groups = canonicalizeRows({
    table: 'groups',
    columns: groupsColumns,
    rawRows: groupsRows,
    orderBy: 'name',
  });

  // MOUNT-INDEX db: the store tables, read directly through the raw handle.
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable (degraded open?)');
  const dumpTable = (table: string, orderBy: string) => {
    const columns = (
      midb.pragma(`table_info(${table})`) as Array<{ name: string }>
    ).map((c) => c.name);
    const rawRows = midb
      .prepare(`SELECT * FROM ${table}`)
      .all() as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };

  const points = dumpTable('doc_mount_points', 'name');
  const files = dumpTable('doc_mount_files', 'sha256');
  const documents = dumpTable('doc_mount_documents', 'contentSha256');
  const links = dumpTable('doc_mount_file_links', 'relativePath');
  const folders = dumpTable('doc_mount_folders', 'path');
  const groupLinks = dumpTable('group_doc_mount_links', 'createdAt');

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stdout.write(
    JSON.stringify({
      case: 'groups-tier2',
      groups,
      points,
      files,
      documents,
      links,
      folders,
      groupLinks,
    }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`groups-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
