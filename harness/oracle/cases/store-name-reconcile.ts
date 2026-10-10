/**
 * Tier-2 oracle — v4's store-name reconcile (`lib/mount-index/reconcile-
 * store-names.ts`, `ed8b15b50`, bug 186) through its REAL code (P4.D274 unit
 * 4): the real repositories (`docMountPoints.findAll()`,
 * `characters.findAllRaw()` — the Zod-dropping `_findAll`), the real index, the
 * real two-phase transaction, the real lines.
 *
 * Per scenario of `fixtures/store-name-reconcile-spec.json`:
 *   1. copy the builder's EMPTY base pair (`build-store-name-reconcile-
 *      fixture.ts`) to the working paths, reset v4's repositories and re-init;
 *   2. plant the rows RAW (stores: id, name, storeType, createdAt = updatedAt;
 *      characters: id, userId, name, characterDocumentMountPointId, archivedAt,
 *      createdAt = updatedAt) and the named plant:
 *        - `characters-read-fails` / `stores-read-fails`: a VIRTUAL generated
 *          column `qt_boom AS (json('not json'))`, so v4's `SELECT *` THROWS
 *          (`malformed JSON`) instead of self-healing — v4's `_findAll`
 *          answers `[]` with `Error finding all entities` (R-A arm 2);
 *        - `mount-index-degraded`: the mount-index file is garbage, so v4's
 *          open ladder marks the partition DEGRADED (no mock);
 *   3. register the characters collection with the fresh backend
 *      (`ensureCollection('characters', CharacterSchema)` — v4's repositories
 *      are module singletons, so their own first-access init does not re-run
 *      after a re-init), prime both repositories OUTSIDE the capture, then
 *      capture while `reconcileStoreNames(trigger)` runs;
 *   4. dump `doc_mount_points` BEFORE/AFTER (`id, name, storeType,
 *      createdAt`, `updatedAt` → `<now>` iff it moved), the result (or the
 *      thrown message), and the captured lines in order;
 *   5. copy the AFTER pair to `QT_SNR_OUT_DIR/<scenario>-{main,mount}.db` —
 *      the cross-run's input for v5 (v4-reconciled → v5 must rename nothing).
 *
 * CROSS mode (`QT_SNR_CROSS_DIR` set): instead of planting, each scenario's
 * pair is copied from `QT_SNR_CROSS_DIR/<scenario>-{main,mount}.db` (v5's
 * AFTER pairs, written by the Rust test) and v4's reconcile runs over it once
 * more — the v5-then-v4 half of the ping-pong proof. Output lines are
 * `{case: "cross", scenario, …}`.
 *
 * The job host is parked; `QUILLTAP_JOB_CHILD` is unset. Writes ZERO
 * `llm_logs` rows. Run from the pinned v4 worktree under Node 24 (TZ=UTC):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SNR_DIR=/tmp/qt-store-name-reconcile $N/npx tsx $V5W/harness/oracle/fixtures/build-store-name-reconcile-fixture.ts
 *   TZ=UTC QT_FIXTURE_SNR_DIR=/tmp/qt-store-name-reconcile QT_SNR_OUT_DIR=/tmp/qt-store-name-reconcile/v4-after $N/npx tsx $V5W/harness/oracle/cases/store-name-reconcile.ts > /tmp/oracle-store-name-reconcile.ndjson
 * then the Rust test (stage 1, which writes v5's AFTER pairs to
 * `QT_SNR_V5_AFTER_DIR`), then CROSS mode over those pairs:
 *   TZ=UTC QT_FIXTURE_SNR_DIR=/tmp/qt-store-name-reconcile QT_SNR_CROSS_DIR=/tmp/qt-store-name-reconcile/v5-after $N/npx tsx $V5W/harness/oracle/cases/store-name-reconcile.ts > /tmp/oracle-store-name-reconcile-cross.ndjson
 */

(globalThis as Record<string, unknown>).__quilltapJobHost = {
  child: null,
  spawning: false,
  shuttingDown: true,
  childCrashed: false,
  restartTimestamps: [],
  signalHandlersInstalled: false,
  invalidationListeners: new Set(),
};

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface StoreRow {
  id: string;
  name: string;
  storeType: string | null;
  createdAt: string;
}
interface CharRow {
  id: string;
  name: string;
  characterDocumentMountPointId: string | null;
  createdAt: string;
  archivedAt?: string;
}
interface Scenario {
  name: string;
  trigger: string;
  plant?: 'characters-read-fails' | 'stores-read-fails' | 'mount-index-degraded';
  stores: StoreRow[];
  characters: CharRow[];
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  scenarios: Scenario[];
}

const CAPTURED = new Set([
  'Store-name reconcile planned',
  'Renamed document store to its rightful name',
  'Store-name reconcile skipped: mount-index database unavailable',
  'Store-name reconcile after a character change failed',
  'Data validation failed',
  'Safe validation failed',
  'Error finding all entities',
  'Mount-index case repair: renamed document store whose name collided with a peer except for casing',
  'Mount-index: created the unique document-store name index',
]);

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'store-name-reconcile-spec.json'), 'utf8'),
  ) as Spec;
  const baseDir = process.env.QT_FIXTURE_SNR_DIR;
  if (!baseDir) throw new Error('QT_FIXTURE_SNR_DIR is required (see header)');
  const crossDir = process.env.QT_SNR_CROSS_DIR;
  const outDir = process.env.QT_SNR_OUT_DIR;
  if (!crossDir && !outDir) throw new Error('QT_SNR_OUT_DIR is required outside CROSS mode');
  if (outDir) mkdirSync(outDir, { recursive: true });

  const scratch = mkdtempSync(join(tmpdir(), 'qt-snr-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainPath = join(scratch, 'data', 'main.db');
  const mountPath = join(scratch, 'data', 'mount.db');

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainPath;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountPath;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  delete process.env.QUILLTAP_JOB_CHILD;
  process.env.LOG_LEVEL = 'debug';

  const { logger } = await import('@/lib/logger');
  let captured: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  let capturing = false;
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown>; error?: { message: string } };
      if (!capturing || !CAPTURED.has(data.message)) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      if (data.error) fields.error = data.error.message;
      captured.push({ level: data.level, message: data.message, fields });
    },
  });

  const { initializeDatabase, closeDatabase, ensureCollection } = await import('@/lib/database/manager');
  const { CharacterSchema } = await import('@/lib/schemas/character.types');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const mic = await import('@/lib/database/backends/sqlite/mount-index-client');
  const { getRepositories, resetRepositories } = await import('@/lib/repositories/factory');
  const { reconcileStoreNames } = await import('@/lib/mount-index/reconcile-store-names');

  const dumpStores = (planted: Map<string, string>): unknown[] => {
    const db = mic.getRawMountIndexDatabase();
    if (!db) return [];
    const rows = db
      .prepare('SELECT id, name, storeType, createdAt, updatedAt FROM doc_mount_points ORDER BY id')
      .all() as Array<Record<string, unknown>>;
    return rows.map((r) => ({
      id: r.id,
      name: r.name,
      storeType: r.storeType,
      createdAt: r.createdAt,
      updatedAt: r.updatedAt === planted.get(r.id as string) ? r.updatedAt : '<now>',
    }));
  };

  for (const sc of spec.scenarios) {
    mic.closeMountIndexSQLiteClient();
    await closeDatabase();
    resetRepositories();
    for (const p of [mainPath, mountPath]) {
      for (const suffix of ['', '-journal', '-wal', '-shm']) if (existsSync(p + suffix)) rmSync(p + suffix);
    }
    if (crossDir) {
      const m = join(crossDir, `${sc.name}-main.db`);
      if (!existsSync(m)) continue; // the degraded scenario has no pair
      copyFileSync(m, mainPath);
      copyFileSync(join(crossDir, `${sc.name}-mount.db`), mountPath);
    } else {
      copyFileSync(join(baseDir, 'base-main.db'), mainPath);
      if (sc.plant === 'mount-index-degraded') writeFileSync(mountPath, 'this is not a database');
      else copyFileSync(join(baseDir, 'base-mount.db'), mountPath);
    }

    await initializeDatabase();
    // v4's repositories are module singletons whose `collectionInitialized`
    // outlives `resetRepositories()`, so after a re-init nothing registers
    // the characters table's JSON / boolean columns with the NEW backend and
    // every row would read its `[]` cells as strings and fail Zod — an oracle
    // artifact (one backend per process in production). Register them as
    // the character fixture builders do.
    await ensureCollection('characters', CharacterSchema);
    const main = getRawDatabase()!;
    const planted = new Map<string, string>();
    if (!crossDir) {
      const mount = mic.getRawMountIndexDatabase();
      for (const s of sc.stores) {
        mount!
          .prepare('INSERT INTO doc_mount_points (id, name, storeType, createdAt, updatedAt) VALUES (?, ?, ?, ?, ?)')
          .run(s.id, s.name, s.storeType, s.createdAt, s.createdAt);
        planted.set(s.id, s.createdAt);
      }
      for (const c of sc.characters) {
        main
          .prepare(
            'INSERT INTO characters (id, userId, name, characterDocumentMountPointId, archivedAt, createdAt, updatedAt) VALUES (?, ?, ?, ?, ?, ?, ?)',
          )
          .run(c.id, spec.userId, c.name, c.characterDocumentMountPointId, c.archivedAt ?? null, c.createdAt, c.createdAt);
      }
      if (sc.plant === 'characters-read-fails') {
        main.exec(`ALTER TABLE characters ADD COLUMN qt_boom TEXT GENERATED ALWAYS AS (json('not json')) VIRTUAL`);
      }
      if (sc.plant === 'stores-read-fails') {
        mount!.exec(`ALTER TABLE doc_mount_points ADD COLUMN qt_boom TEXT GENERATED ALWAYS AS (json('not json')) VIRTUAL`);
      }
    } else {
      const mount = mic.getRawMountIndexDatabase();
      for (const r of mount!.prepare('SELECT id, updatedAt FROM doc_mount_points').all() as Array<{ id: string; updatedAt: string }>) {
        planted.set(r.id, r.updatedAt);
      }
    }
    // Prime both repositories' table init outside the capture (a missing id
    // reads nothing and logs nothing).
    const repos = getRepositories();
    await repos.docMountPoints.findById('00000000-0000-4000-8000-00000000ffff').catch(() => null);
    await repos.characters.findByIdRaw('00000000-0000-4000-8000-00000000ffff').catch(() => null);

    const before = dumpStores(planted);
    captured = [];
    capturing = true;
    let result: unknown = null;
    let error: string | null = null;
    try {
      result = await reconcileStoreNames(sc.trigger);
    } catch (e) {
      error = (e as Error).message;
    }
    capturing = false;
    const after = dumpStores(planted);

    process.stdout.write(
      JSON.stringify({
        case: crossDir ? 'cross' : 'scenario',
        scenario: sc.name,
        before,
        after,
        result,
        error,
        lines: captured,
      }) + '\n',
    );

    mic.closeMountIndexSQLiteClient();
    await closeDatabase();
    if (outDir && sc.plant !== 'mount-index-degraded') {
      copyFileSync(mainPath, join(outDir, `${sc.name}-main.db`));
      copyFileSync(mountPath, join(outDir, `${sc.name}-mount.db`));
    }
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`store-name-reconcile oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
