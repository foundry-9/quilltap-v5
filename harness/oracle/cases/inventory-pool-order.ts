/**
 * Oracle case (P4.D378 item 7 + Tier 2 item 17 — v4 `0c47f4539` at
 * `624d0d25c`): the Inventory pool's within-tier order. Drives v4's REAL
 * `loadInventoryPool` (`lib/inventory/pool.ts`) over the
 * `build-inventory-pool-order-fixture.ts` pair — v4's `pool-order.test.ts`
 * vectors on v4's REAL repositories instead of its fake store — each row in
 * its OWN child process on a FRESH copy of the pair.
 *
 * Per row ONE NDJSON line:
 *   - `mounts`: the stores by label, read RAW off the pair before the row's
 *     plants (the Rust side runs the same SQL); `dangling*` are pinned ids no
 *     store row carries;
 *   - `ids`: the twins' object ids (minted by the builder), by label;
 *   - `get`: per probed twin, `JSON.stringify({name, origin, mount})` of
 *     `pool.get(id)` — `mount` the winning record's `mountPointId` as a label;
 *   - `consulted` / `unread`: `pool.consultedMounts` / `pool.unreadMounts`
 *     VERBATIM, every mount id replaced by its label (ONE remap; an id no
 *     label carries is `<unlabelled:id>`);
 *   - `logs`: every `[InventoryPool]` line v4 logged during the load,
 *     recorded off the `Logger` prototype before the level check.
 *
 * Run (Node 24, from the v4 checkout), AFTER the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-ipo-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-ipo-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-inventory-pool-order-fixture.ts
 *   LOG_LEVEL=debug QT_FIXTURE_IPO_MAIN=/tmp/qt-ipo-main.db QT_FIXTURE_IPO_MOUNT=/tmp/qt-ipo-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/cases/inventory-pool-order.ts \
 *     > /tmp/oracle-inventory-pool-order.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';

type Json = any; // eslint-disable-line @typescript-eslint/no-explicit-any

const here = dirname(fileURLToPath(import.meta.url));
const spec = JSON.parse(readFileSync(join(here, '..', 'fixtures', 'inventory-pool-order.json'), 'utf8')) as Json;

async function runChild(index: number): Promise<void> {
  const row = spec.rows[index];
  const fixtureMain = process.env.QT_FIXTURE_IPO_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_IPO_MOUNT;
  if (!fixtureMain || !fixtureMount || !existsSync(fixtureMain) || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_IPO_MAIN / QT_FIXTURE_IPO_MOUNT must point at the pool-order pair');
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-ipo-op-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'main.db');
  const mountWork = join(scratch, 'mount.db');
  copyFileSync(fixtureMain, mainWork);
  copyFileSync(fixtureMount, mountWork);
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;

  const { Logger } = await import('@/lib/logger');
  const logs: Array<{ level: string; message: string; fields: Array<[string, unknown]> }> = [];
  let recording = false;
  for (const level of ['error', 'warn', 'info', 'debug'] as const) {
    const original = Logger.prototype[level];
    Logger.prototype[level] = function (this: unknown, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
      if (recording && typeof message === 'string' && message.startsWith('[InventoryPool]')) {
        logs.push({ level, message, fields: Object.entries(context ?? {}).filter(([, v]) => v !== undefined) });
      }
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  await initializeDatabase();
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const repos = getRepositories();
  const rawMain = getRawDatabase() as Json;
  const rawMount = getRawMountIndexDatabase() as Json;
  const one = (sql: string, ...args: unknown[]) => {
    const r = rawMain.prepare(sql).get(...args) as Json;
    if (!r || typeof r.mp !== 'string') throw new Error(`no mount for ${sql}`);
    return r.mp as string;
  };
  // The stores by label, RAW off the pair, before any plant.
  const mounts: Record<string, string> = {
    own: one('SELECT characterDocumentMountPointId AS mp FROM characters WHERE id = ?', spec.character.id),
    first: one('SELECT officialMountPointId AS mp FROM groups WHERE id = ?', spec.groups[0].id),
    second: one('SELECT officialMountPointId AS mp FROM groups WHERE id = ?', spec.groups[1].id),
    project: one('SELECT officialMountPointId AS mp FROM projects WHERE id = ?', spec.project.id),
    annex: spec.linked.id,
    general: spec.generalMountPointId,
    ...spec.dangling,
  };
  const labelOf = new Map(Object.entries(mounts).map(([k, v]) => [v, k]));
  const label = (id: unknown) => (typeof id === 'string' ? (labelOf.get(id) ?? `<unlabelled:${id}>`) : id);
  const mp = (s: string) => s.replace(/\{\{mp:(\w+)\}\}/g, (_, k) => mounts[k]);

  // The twins' ids: the object named `names[0]` in the store it was created in.
  const { readInventoryRecords } = await import('@/lib/database/repositories/vault-overlay/inventory-readers');
  const ids: Record<string, string> = {};
  for (const t of spec.twins) {
    const hit = (await readInventoryRecords(mounts[t.fromMount])).filter((r: Json) => r.name === t.names[0]);
    if (hit.length !== 1) throw new Error(`twin ${t.label}: ${hit.length} records named ${t.names[0]}`);
    ids[t.label] = hit[0].id;
  }

  for (const step of row.steps ?? []) {
    if (step.do === 'sql') {
      (step.db === 'main' ? rawMain : rawMount).exec(mp(step.sql));
    } else {
      throw new Error(`unknown step ${step.do}`);
    }
  }

  const { loadInventoryPool } = await import('@/lib/inventory/pool');
  recording = true;
  const pool = await loadInventoryPool(repos as never, {
    characterId: spec.character.id,
    projectId: row.project ? spec.project.id : null,
  });
  recording = false;

  const get: Record<string, string | null> = {};
  for (const probe of row.probes) {
    const r = pool.get(ids[probe]) as Json;
    get[probe] = r ? JSON.stringify({ name: r.name, origin: r.origin, mount: label(r.mountPointId) }) : null;
  }
  const consulted = pool.consultedMounts.map(label);
  const unread = pool.unreadMounts.map((u: Json) => ({ mount: label(u.mountPointId), error: u.error }));
  const relabel = (v: unknown): unknown => (typeof v === 'string' && labelOf.has(v) ? `<mp:${labelOf.get(v)}>` : v);
  const lines = logs.map((l) => ({ ...l, fields: l.fields.map(([k, v]) => [k, relabel(v)]) }));

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stdout.write(JSON.stringify({ op: row.name, index, mounts, ids, get, consulted, unread, logs: lines }) + '\n');
  process.exit(0);
}

async function main(): Promise<void> {
  const only = process.env.QT_IPO_OP_INDEX;
  if (only !== undefined) return runChild(Number(only));
  for (let i = 0; i < spec.rows.length; i++) {
    const child = spawnSync(process.execPath, [...process.execArgv, fileURLToPath(import.meta.url)], {
      env: { ...process.env, QT_IPO_OP_INDEX: String(i) },
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
  process.stderr.write(`inventory-pool-order oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
