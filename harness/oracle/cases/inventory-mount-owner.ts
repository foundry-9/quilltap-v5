/**
 * Oracle case (P4.D378 item 9 — v4 `0c47f4539` at `624d0d25c`): the
 * Inventory owner resolver over the shared `classifyMount`. Drives v4's REAL
 * `mountOwnerResolver()(mp)` and `mountScopeOf(mp)` (`lib/inventory/
 * dependents.ts`) over the `build-inventory-title-lint-fixture.ts` pair,
 * widened PER ROW by in-case plants (`inventory-mount-owner.json`), each row
 * in its OWN child process on a FRESH copy of the pair (v4's repository
 * singletons and the General-id read never leak between rows).
 *
 * Per row ONE NDJSON line:
 *   - `mounts`: the four owner stores, read RAW off the pair (the same SQL the
 *     Rust side runs), so `{{mp:<label>}}` resolves identically;
 *   - `owner`: `JSON.stringify(await mountOwnerResolver()(probe))` — KEY ORDER
 *     is the contract (`scope, ownerId, ownerName, ownerArchived`);
 *   - `scope`: `JSON.stringify(await mountScopeOf(probe))` (a fresh resolver);
 *   - `hydrated` (the R-F rows only, round ruling "read project / group owners
 *     raw"): proof the plant FAILS v4's HYDRATED read and leaves the RAW read
 *     whole — `findAll` must DROP the owner (v4's batched overlay logs and
 *     drops), `findById` must THROW, `findByOfficialMountPointIdRaw` must
 *     still name it. A plant that does not do all three FAILS the case.
 *
 * Plants (`steps`): `sql` on either DB (`{{mp:<label>}}` substituted), and
 * `doc-plant` through v4's REAL `writeDatabaseDocument` (inside
 * `withInventoryWritesLinted` — the bytes `{` into an owner store's
 * `properties.json` make the store unreadable to the overlay).
 *
 * Run (Node 24, from the v4 checkout), AFTER the title-lint fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-itl-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-itl-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-inventory-title-lint-fixture.ts
 *   QT_FIXTURE_ITL_MAIN=/tmp/qt-itl-main.db QT_FIXTURE_ITL_MOUNT=/tmp/qt-itl-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/cases/inventory-mount-owner.ts \
 *     > /tmp/oracle-inventory-mount-owner.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';

type Json = any; // eslint-disable-line @typescript-eslint/no-explicit-any

const here = dirname(fileURLToPath(import.meta.url));
const pairSpec = JSON.parse(readFileSync(join(here, '..', 'fixtures', 'inventory-title-lint.json'), 'utf8')) as Json;
const spec = JSON.parse(readFileSync(join(here, '..', 'fixtures', 'inventory-mount-owner.json'), 'utf8')) as Json;

async function runChild(index: number): Promise<void> {
  const row = spec.rows[index];
  const fixtureMain = process.env.QT_FIXTURE_ITL_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_ITL_MOUNT;
  if (!fixtureMain || !fixtureMount || !existsSync(fixtureMain) || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_ITL_MAIN / QT_FIXTURE_ITL_MOUNT must point at the title-lint pair');
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-imo-op-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'main.db');
  const mountWork = join(scratch, 'mount.db');
  copyFileSync(fixtureMain, mainWork);
  copyFileSync(fixtureMount, mountWork);
  process.env.ENCRYPTION_MASTER_PEPPER = pairSpec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  await initializeDatabase();
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const rawMain = getRawDatabase() as Json;
  const rawMount = getRawMountIndexDatabase() as Json;
  const one = (sql: string, ...args: unknown[]) => {
    const r = rawMain.prepare(sql).get(...args) as Json;
    if (!r || typeof r.mp !== 'string') throw new Error(`no mount for ${sql}`);
    return r.mp as string;
  };
  // The four owner stores, RAW off the pair (the Rust side runs the same SQL).
  const mounts: Record<string, string> = {
    general: pairSpec.generalMountPointId,
    friday: one('SELECT characterDocumentMountPointId AS mp FROM characters WHERE id = ?', pairSpec.friday.id),
    project: one('SELECT officialMountPointId AS mp FROM projects WHERE id = ?', pairSpec.project.id),
    group: one('SELECT officialMountPointId AS mp FROM groups WHERE id = ?', pairSpec.group.id),
  };
  const mp = (s: string) => s.replace(/\{\{mp:(\w+)\}\}/g, (_, k) => mounts[k]);

  const { withInventoryWritesLinted } = await import('@/lib/inventory/lint-scope');
  const store = await import('@/lib/mount-index/database-store');
  for (const step of row.steps ?? []) {
    if (step.do === 'sql') {
      (step.db === 'main' ? rawMain : rawMount).exec(mp(step.sql));
    } else if (step.do === 'doc-plant') {
      await withInventoryWritesLinted(() => store.writeDatabaseDocument(mp(step.mp), step.path, step.content));
      const back = await store.readDatabaseDocument(mp(step.mp), step.path);
      if (back.content !== step.content) throw new Error(`plant ${step.path} did not land`);
    } else {
      throw new Error(`unknown step ${step.do}`);
    }
  }

  const { mountOwnerResolver, mountScopeOf } = await import('@/lib/inventory/dependents');
  const probe = mp(row.probe);
  const outcome = async (f: () => Promise<unknown>) => {
    try {
      return { result: JSON.stringify(await f()) };
    } catch (e) {
      return { error: e instanceof Error ? e.message : String(e) };
    }
  };
  const owner = await outcome(() => mountOwnerResolver()(probe));
  const scope = await outcome(() => mountScopeOf(probe));

  // R-F self-check: the plant must fail the HYDRATED read and leave the RAW one whole.
  let hydrated: Json;
  if (row.hydrated) {
    const repo = (getRepositories() as Json)[row.hydrated.entity];
    const all = await repo.findAll();
    const findAll = (all as Json[]).some((e) => e.id === row.hydrated.id) ? 'present' : 'dropped';
    let findById: string;
    try {
      await repo.findById(row.hydrated.id);
      findById = 'ok';
    } catch (e) {
      findById = `throws: ${(e as Json)?.name}`;
    }
    const raw = await repo.findByOfficialMountPointIdRaw(probe);
    hydrated = { findAll, findById, rawName: raw?.name ?? null };
    if (findAll !== 'dropped' || !findById.startsWith('throws') || hydrated.rawName === null) {
      throw new Error(`R-F plant did not fail the hydrated read / keep the raw one: ${JSON.stringify(hydrated)}`);
    }
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stdout.write(
    JSON.stringify({ op: row.name, index, mounts, probe, owner, scope, ...(hydrated ? { hydrated } : {}) }) + '\n',
  );
  process.exit(0);
}

async function main(): Promise<void> {
  const only = process.env.QT_IMO_OP_INDEX;
  if (only !== undefined) return runChild(Number(only));
  for (let i = 0; i < spec.rows.length; i++) {
    const child = spawnSync(process.execPath, [...process.execArgv, fileURLToPath(import.meta.url)], {
      env: { ...process.env, QT_IMO_OP_INDEX: String(i) },
      encoding: 'utf8',
      maxBuffer: 64 * 1024 * 1024,
    });
    if (child.status !== 0 || !child.stdout.trim()) {
      process.stderr.write(`row #${i} ${spec.rows[i].name} failed:\n${child.stderr}\n`);
      process.exit(1);
    }
    process.stdout.write(child.stdout.trim().split('\n').pop() + '\n');
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`inventory-mount-owner oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
