/**
 * Tier-2 oracle case — the `projects` STORE-BACKED entity (document-store
 * overlay slice, build step 4).
 *
 * Mirrors cases/groups-tier2.ts (same two-DB store-backed machine), exercising
 * the larger 16-key `properties.json` bag and the character-roster operations.
 * Drives v4's REAL `repos.projects.create()` / `.update()` / `.addToRoster()` /
 * `.removeFromRoster()` / `.setAllowAnyCharacter()` end-to-end — no mocked storage
 * boundary, no QUILLTAP_JOB_CHILD; v4's post-write reindex runs (database-backed
 * stores chunk with no model). Since P4.6BK v5 chunks on write too, so the Rust
 * harness diffs the link `chunkCount` exactly (`doc_mount_chunks` excluded —
 * chunk rows are dumped in the groups family).
 *
 * Dumps the slim `projects` row (MAIN) + the store tables (`doc_mount_points`,
 * `_files`, `_documents`, `_file_links`, `_folders`) + `project_doc_mount_links`
 * (MOUNT-INDEX). Minted-values remap form; the Rust harness applies the shared
 * cross-db id-map.
 *
 * P4.D29 (v4 `dcd9440a`) added the three `readProperties` refusal/seed arms:
 * `plantProperties` writes raw bytes into a store's `properties.json` through the
 * REAL `writeDatabaseDocument`, `deleteProperties` removes it through the REAL
 * `deleteDatabaseDocument`, and `updateExpectError` records the thrown message
 * into a new `errors` array the harness diffs alongside the tables — with the
 * tables themselves as the "wrote nothing" proof.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixtures:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_PROJECTS_MAIN=/tmp/qt-projects-main.db \
 *   QT_FIXTURE_PROJECTS_MOUNT=/tmp/qt-projects-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/projects-tier2.ts \
 *     > /tmp/oracle-projects.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind:
    | 'create'
    | 'update'
    | 'addToRoster'
    | 'removeFromRoster'
    | 'setAllowAnyCharacter'
    // P4.D29 (v4 dcd9440a): the readProperties refusal/seed arms.
    | 'plantProperties'
    | 'deleteProperties'
    | 'updateExpectError';
  label: string;
  input?: Record<string, unknown>;
  patch?: Record<string, unknown>;
  characterId?: string;
  value?: boolean;
  /** `plantProperties`: the raw bytes to write into `properties.json`. */
  content?: string;
}

interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'projects-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainFixture = process.env.QT_FIXTURE_PROJECTS_MAIN;
  const mountFixture = process.env.QT_FIXTURE_PROJECTS_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_PROJECTS_MAIN and QT_FIXTURE_PROJECTS_MOUNT must point at the fixtures from build-projects-tier2-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-projects-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'projects-main-work.db');
  const mountWork = join(scratch, 'projects-mount-work.db');
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

  const idByLabel = new Map<string, string>();
  /** Thrown-error messages from the `updateExpectError` arms, in op order. */
  const errors: Array<{ label: string; message: string | null }> = [];
  for (const op of spec.ops) {
    const id = () => {
      const v = idByLabel.get(op.label);
      if (!v) throw new Error(`op references unknown label: ${op.label}`);
      return v;
    };
    /** The label's official store, read RAW (a broken overlay must not block it). */
    const mountPointId = async (): Promise<string> => {
      const raw = (await repos.projects.findByIdRaw(id())) as
        | { officialMountPointId?: string | null }
        | null;
      const mp = raw?.officialMountPointId;
      if (!mp) throw new Error(`project ${op.label} has no officialMountPointId`);
      return mp;
    };
    switch (op.kind) {
      case 'create': {
        const created = await repos.projects.create(op.input as never);
        idByLabel.set(op.label, created.id);
        break;
      }
      case 'update':
        await repos.projects.update(id(), op.patch as never);
        break;
      case 'plantProperties':
        await writeDatabaseDocument(await mountPointId(), 'properties.json', op.content as string);
        break;
      case 'deleteProperties':
        await deleteDatabaseDocument(await mountPointId(), 'properties.json');
        break;
      case 'updateExpectError': {
        let message: string | null = null;
        try {
          await repos.projects.update(id(), op.patch as never);
        } catch (err) {
          message = err instanceof Error ? err.message : String(err);
        }
        errors.push({ label: op.label, message });
        break;
      }
      case 'addToRoster':
        await repos.projects.addToRoster(id(), op.characterId as string);
        break;
      case 'removeFromRoster':
        await repos.projects.removeFromRoster(id(), op.characterId as string);
        break;
      case 'setAllowAnyCharacter':
        await repos.projects.setAllowAnyCharacter(id(), op.value as boolean);
        break;
    }
  }

  const projectsColumns = (
    (await rawQuery('PRAGMA table_info(projects)')) as Array<{ name: string }>
  ).map((c) => c.name);
  const projectsRows = (await rawQuery('SELECT * FROM projects')) as Array<
    Record<string, unknown>
  >;
  const projects = canonicalizeRows({
    table: 'projects',
    columns: projectsColumns,
    rawRows: projectsRows,
    orderBy: 'name',
  });

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
  const projectLinks = dumpTable('project_doc_mount_links', 'createdAt');

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stdout.write(
    JSON.stringify({
      case: 'projects-tier2',
      projects,
      points,
      files,
      documents,
      links,
      folders,
      projectLinks,
      errors,
    }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`projects-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
