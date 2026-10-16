/**
 * Oracle case — v4's wardrobe FOLDER FILING (P4.D309, `bbbcd2388`): the REAL
 * `fileItemsToFolder` (`lib/wardrobe/folder-filing.ts`) over a REAL
 * `resolveWardrobeLocation(...)` and the real `getRepositories()`.
 *
 * Every CHAIN runs in its OWN child process against a FRESH copy of the baked
 * fixtures (build-folder-filing-fixture.ts) — v4's repositories and the mount
 * index client are process singletons, so a re-init inside one process would
 * leak state between chains. After EVERY op the child records `{ chain, op,
 * result | error: { name, message }, items: location.readItems(true), folders:
 * location.listFolders(), logs, tables }`:
 *   - `logs` — the `[FolderFiling]` / `[ItemMutations]` / `[FolderMutations]` /
 *     `[WardrobeImages]` lines and the bare `Wardrobe item created in folder`,
 *     `{ level, message, meta }` with the logger's child bindings (`service` /
 *     `module` / `environment`) dropped, captured by splicing a sink into the
 *     singleton logger's shared transports (children included);
 *   - `tables` — the six mount-index tables + MAIN `files` + MAIN
 *     `wardrobe_wear_stats`, `SELECT * … ORDER BY rowid` (BLOBs as hex).
 * The Rust port runs the same chains over copies of the same fixture and diffs
 * after normalising (see folder_filing_tier2_equivalence.rs's header).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixtures:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_FFL_MAIN=/tmp/qt-ffl-main.db QT_FIXTURE_FFL_MOUNT=/tmp/qt-ffl-mount.db \
 *     $N/node --import tsx <v5>/harness/oracle/cases/folder-filing-tier2.ts \
 *     > /tmp/oracle-folder-filing.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { spawnSync } from 'node:child_process';
import {
  mkdtempSync,
  rmSync,
  mkdirSync,
  readFileSync,
  copyFileSync,
  existsSync,
  writeSync,
  appendFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';

type Scope = 'character' | 'general' | 'project' | 'group' | 'archived';
interface Op {
  scope: Scope;
  action: 'move' | 'copy';
  itemIds: string[];
  folder: string;
  components?: 'move' | 'copy' | 'none';
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  characterId: string;
  archivedCharacterId: string;
  projectId: string;
  groupId: string;
  items: Record<string, { id: string }>;
  chains: Array<{ name: string; ops: Op[] }>;
}

const CAPTURED_PREFIXES = [
  '[FolderFiling]',
  '[ItemMutations]',
  '[FolderMutations]',
  '[WardrobeImages]',
  'Wardrobe item created in folder',
];
const MOUNT_TABLES: Array<[string, string]> = [
  ['points', 'doc_mount_points'],
  ['folders', 'doc_mount_folders'],
  ['links', 'doc_mount_file_links'],
  ['mountFiles', 'doc_mount_files'],
  ['documents', 'doc_mount_documents'],
  ['blobs', 'doc_mount_blobs'],
];

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

function sanitize(meta: unknown): unknown {
  // An `undefined` field is DROPPED, as winston drops it (§R.5).
  return JSON.parse(JSON.stringify(meta ?? {}, (_k, v) => (v instanceof Error ? v.message : v)));
}

async function runChain(spec: Spec, chainIndex: number): Promise<void> {
  const chain = spec.chains[chainIndex];
  const mainFixture = process.env.QT_FIXTURE_FFL_MAIN as string;
  const mountFixture = process.env.QT_FIXTURE_FFL_MOUNT as string;

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ffl-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'main.db');
  const mountWork = join(scratch, 'mount.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  // The compared lines are DEBUG — the logger must let them through.
  process.env.LOG_LEVEL = 'debug';

  const { logger } = await import('@/lib/logger');
  const logs: Array<{ level: string; message: string; meta: unknown }> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown> };
      if (!CAPTURED_PREFIXES.some((p) => data.message.startsWith(p))) return;
      const { service: _s, module: _m, environment: _e, ...rest } = (data.context ?? {}) as Record<
        string,
        unknown
      >;
      logs.push({ level: data.level, message: data.message, meta: sanitize(rest) });
    },
  });

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { resolveWardrobeLocation } = await import('@/lib/wardrobe/location');
  const { fileItemsToFolder } = await import('@/lib/wardrobe/folder-filing');

  await initializeDatabase();
  const repos = getRepositories();
  const raw = getRawDatabase();
  const midb = getRawMountIndexDatabase();
  if (!raw || !midb) throw new Error('DB handles unavailable');

  const dump = (db: { prepare: (s: string) => { all: () => unknown[] } }, table: string) => {
    const columns = (db.prepare(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>).map(
      (c) => c.name,
    );
    const rows = (db.prepare(`SELECT * FROM ${table} ORDER BY rowid`).all() as Array<
      Record<string, unknown>
    >).map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
      return out;
    });
    return { table, columns, rows };
  };

  const locationFor = async (scope: Scope) => {
    const [s, id] =
      scope === 'character'
        ? ['character', spec.characterId]
        : scope === 'archived'
          ? ['character', spec.archivedCharacterId]
          : scope === 'project'
            ? ['project', spec.projectId]
            : scope === 'group'
              ? ['group', spec.groupId]
              : ['general', null];
    // The route resolves without provisioning (`resolveLocation(ctx, params, false)`).
    const location = await resolveWardrobeLocation(s as never, id, repos, spec.userId, {});
    if (!location) throw new Error(`no ${scope} location`);
    return location;
  };
  const idOf = (ref: string) => (ref.startsWith('raw:') ? ref.slice(4) : spec.items[ref].id);

  for (let i = 0; i < chain.ops.length; i++) {
    const op = chain.ops[i];
    const location = await locationFor(op.scope);
    logs.length = 0;
    let result: unknown;
    try {
      const out = await fileItemsToFolder(repos, spec.userId, location, {
        action: op.action,
        itemIds: op.itemIds.map(idOf),
        folder: op.folder,
        ...(op.components ? { components: op.components } : {}),
      });
      result = { kind: 'ok', items: out.items, componentsCarried: out.componentsCarried };
    } catch (err) {
      result = {
        kind: 'error',
        name: err instanceof Error ? err.name : 'unknown',
        message: err instanceof Error ? err.message : String(err),
      };
    }
    const items = await location.readItems(true);
    const folders = await location.listFolders();
    const tables: Record<string, unknown> = {
      files: dump(raw as never, 'files'),
      wearStats: dump(raw as never, 'wardrobe_wear_stats'),
    };
    for (const [key, table] of MOUNT_TABLES) tables[key] = dump(midb as never, table);
    // Into the parent's per-chain file (a piped stdout is asynchronous and
    // the child exits right after the chain).
    appendFileSync(
      process.env.QT_FFL_OUT as string,
      `${JSON.stringify({ chain: chain.name, op: i, result, items, folders, logs: [...logs], tables })}\n`,
    );
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'folder-filing-tier2.json'), 'utf8'),
  ) as Spec;
  const mainFixture = process.env.QT_FIXTURE_FFL_MAIN;
  const mountFixture = process.env.QT_FIXTURE_FFL_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_FFL_MAIN and QT_FIXTURE_FFL_MOUNT must point at the fixtures from build-folder-filing-fixture.ts',
    );
  }

  const child = process.env.QT_FFL_CHAIN;
  if (child !== undefined) {
    await runChain(spec, Number(child));
    process.exit(0);
  }

  // The parent: one fresh process per chain, outputs concatenated in order.
  const outDir = mkdtempSync(join(tmpdir(), 'qt-ffl-oracle-out-'));
  process.on('exit', () => rmSync(outDir, { recursive: true, force: true }));
  for (let c = 0; c < spec.chains.length; c++) {
    const out = join(outDir, `chain-${c}.ndjson`);
    const run = spawnSync(process.execPath, [...process.execArgv, fileURLToPath(import.meta.url)], {
      env: { ...process.env, QT_FFL_CHAIN: String(c), QT_FFL_OUT: out },
      encoding: 'utf8',
    });
    if (run.status !== 0 || !existsSync(out)) {
      throw new Error(`chain ${spec.chains[c].name} failed (${run.status}): ${run.stderr}`);
    }
    const bytes = readFileSync(out);
    let off = 0;
    while (off < bytes.length) off += writeSync(1, bytes, off, bytes.length - off);
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`folder-filing-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
