/**
 * Tier-2/3 oracle case (P4.D328 — v4 `e1872c94f` + `102a23648` at
 * `322a5488e`): the nine `inventory_*` tool handlers through v4's REAL
 * `INVENTORY_TOOL_TABLE`, the turn's pending map through v4's REAL
 * `flushPendingInventoryAnnouncements`, and the transfer module's two
 * route-facing error shapes (R-15) through v4's REAL `transferInventoryItem`.
 *
 * Every case of `inventory-tools.json` runs in its OWN child process on a
 * FRESH copy of the tools pair (`build-inventory-tools-fixture.ts`) — v4's
 * module singletons (the index cache, the realtime bus, the job host) never
 * leak between cases. A case is a list of steps sharing one state: a TOOL
 * step (`{tool, chatId, characterId, args, turn?}` — the steps naming the
 * same `turn` share ONE pending map, v4's `newWardrobeTurnContainers()`) or a
 * FLUSH step (`{flush: <turn>}`). Per case the child emits ONE line:
 *   - `steps`: per tool step the run's `{ success, result, error }` as
 *     `JSON.stringify` (KEY ORDER is the contract — `formattedText` first),
 *     and when the step has a turn, that turn's pending map AFTER it
 *     (`Array.from(map.entries())`);
 *   - `logs`: every line v4 logged during the case, recorded off the `Logger`
 *     prototype before the level check (`{level, message, fields}`);
 *   - `delta`: per dumped table, the rows that differ from the fixture's and
 *     the fixture ids now gone.
 * Then each `transfers` row runs the same way (`kind: 'transfer'`), its
 * thrown error as `{name, message, code?, status?, details?}`.
 *
 * `avatarGenerationEnabled` is off on every chat (the fixture's default), so
 * the avatar refresh is a no-op both sides; v4's job host is held off. v4's
 * real title lint runs (the lane's stub lifted at the union, P4.D340 landed).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the pair:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_IT_MAIN=/tmp/qt-it-main.db QT_FIXTURE_IT_MOUNT=/tmp/qt-it-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/cases/inventory-tools.ts \
 *     > /tmp/oracle-inventory-tools.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';

type Json = any; // eslint-disable-line @typescript-eslint/no-explicit-any

interface Step {
  tool?: string;
  chatId?: string;
  characterId?: string;
  args?: Json;
  turn?: string;
  flush?: string;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  cases: Array<{ name: string; steps: Step[] }>;
  transfers: Array<{ name: string; source: Json; destination: Json; itemId?: string; record?: Json }>;
}

/** The tables a delta covers: [db, table]. */
const TABLES: Array<[string, string]> = [
  ['mount', 'doc_mount_documents'],
  ['mount', 'doc_mount_files'],
  ['mount', 'doc_mount_file_links'],
  ['mount', 'doc_mount_folders'],
  ['main', 'chats'],
  ['main', 'chat_messages'],
  ['main', 'background_jobs'],
];

const here = dirname(fileURLToPath(import.meta.url));
const spec = JSON.parse(readFileSync(join(here, '..', 'fixtures', 'inventory-tools.json'), 'utf8')) as Spec;
const fixtureMain = process.env.QT_FIXTURE_IT_MAIN;
const fixtureMount = process.env.QT_FIXTURE_IT_MOUNT;
if (!fixtureMain || !fixtureMount || !existsSync(fixtureMain) || !existsSync(fixtureMount)) {
  throw new Error('QT_FIXTURE_IT_MAIN / QT_FIXTURE_IT_MOUNT must point at the tools pair');
}

function canon(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (Buffer.isBuffer(v) || v instanceof Uint8Array) return `<blob ${v.length}>`;
  return v;
}

function dumpAll(rawMain: Json, rawMount: Json): Record<string, Array<Record<string, unknown>>> {
  const out: Record<string, Array<Record<string, unknown>>> = {};
  for (const [db, table] of TABLES) {
    const raw = db === 'main' ? rawMain : rawMount;
    const cols = (raw.prepare(`PRAGMA table_info("${table}")`).all() as Array<{ name: string }>).map((c) => c.name);
    const rows = cols.length ? (raw.prepare(`SELECT * FROM "${table}"`).all() as Array<Record<string, unknown>>) : [];
    out[table] = rows.map((r) => {
      const o: Record<string, unknown> = {};
      for (const c of cols) o[c] = canon(r[c]);
      return o;
    });
  }
  return out;
}

function errorJson(e: Json): Json {
  return {
    name: e?.name ?? 'Error',
    message: e?.message ?? String(e),
    ...(e?.code !== undefined ? { code: e.code } : {}),
    ...(e?.status !== undefined ? { status: e.status } : {}),
    ...(e?.details !== undefined ? { details: e.details } : {}),
  };
}

async function runChild(kind: string, index: number): Promise<void> {
  const scratch = mkdtempSync(join(tmpdir(), 'qt-it-op-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'main.db');
  const mountWork = join(scratch, 'mount.db');
  copyFileSync(fixtureMain!, mainWork);
  copyFileSync(fixtureMount!, mountWork);
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';
  (globalThis as Record<string, unknown>).__quilltapJobHost = { shuttingDown: true };

  const { Logger } = await import('@/lib/logger');
  const logs: Array<{ level: string; message: string; fields: Array<[string, unknown]> }> = [];
  let recording = false;
  for (const level of ['error', 'warn', 'info', 'debug'] as const) {
    const original = Logger.prototype[level];
    Logger.prototype[level] = function (this: unknown, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
      if (recording) {
        logs.push({ level, message, fields: Object.entries(context ?? {}).filter(([, v]) => v !== undefined) });
      }
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }

  // The Inventory title lint runs on both sides since the union (P4.D340 landed).
  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  await initializeDatabase();
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const repos = getRepositories();
  const userId = spec.userId;
  const rawMain = getRawDatabase();
  const rawMount = getRawMountIndexDatabase();
  const before = dumpAll(rawMain, rawMount);

  const { INVENTORY_TOOL_TABLE } = await import('@/lib/tools/handlers/inventory-tool-table');
  const { flushPendingInventoryAnnouncements } = await import('@/lib/inventory/equip-effects');
  const { newWardrobeTurnContainers } = await import('@/lib/wardrobe/turn-containers');
  const location = await import('@/lib/inventory/location');
  const transfers = await import('@/lib/inventory/transfers');

  recording = true;
  const steps: unknown[] = [];
  let transferOutcome: Json = null;
  const turns = new Map<string, ReturnType<typeof newWardrobeTurnContainers>>();
  const turnOf = (id: string) => {
    let t = turns.get(id);
    if (!t) {
      t = newWardrobeTurnContainers();
      turns.set(id, t);
    }
    return t;
  };
  if (kind === 'case') {
    for (const s of spec.cases[index].steps) {
      if (s.flush) {
        await flushPendingInventoryAnnouncements(turnOf(s.flush), repos);
        steps.push({ flush: s.flush });
        continue;
      }
      const run = INVENTORY_TOOL_TABLE[s.tool!];
      const turn = s.turn ? turnOf(s.turn) : null;
      const r = await run(s.args, {
        userId,
        chatId: s.chatId!,
        characterId: s.characterId!,
        ...(turn ? { pendingInventoryAnnouncements: turn.pendingInventoryAnnouncements } : {}),
      });
      steps.push({
        tool: s.tool,
        run: JSON.stringify(r),
        ...(turn ? { pending: JSON.stringify(Array.from(turn.pendingInventoryAnnouncements.entries())) } : {}),
      });
    }
  } else {
    const t = spec.transfers[index];
    try {
      const source = await location.resolveInventoryLocation(t.source.scope, t.source.id, repos, userId);
      const destination = await location.resolveInventoryLocation(t.destination.scope, t.destination.id, repos, userId, {
        ensure: true,
      });
      if (!source || !destination) throw new Error('no location');
      const record = t.record ?? (await source.findItem(t.itemId!));
      if (!record) throw new Error('no source record');
      const r = await transfers.transferInventoryItem(repos, { action: 'move', source, destination, record, userId });
      transferOutcome = { result: JSON.stringify(r) };
    } catch (e) {
      transferOutcome = { error: errorJson(e) };
    }
  }
  recording = false;

  const after = dumpAll(rawMain, rawMount);
  const delta: Record<string, { changed: unknown[]; removed: string[] }> = {};
  for (const [, table] of TABLES) {
    const was = new Map(before[table].map((r) => [String(r.id), JSON.stringify(r)]));
    const now = new Set(after[table].map((r) => String(r.id)));
    delta[table] = {
      changed: after[table].filter((r) => was.get(String(r.id)) !== JSON.stringify(r)),
      removed: Array.from(was.keys()).filter((k) => !now.has(k)),
    };
  }
  closeMountIndexSQLiteClient();
  await closeDatabase();
  const name = kind === 'case' ? spec.cases[index].name : spec.transfers[index].name;
  process.stdout.write(
    JSON.stringify({
      kind,
      index,
      name,
      ...(kind === 'case' ? { steps } : transferOutcome),
      logs,
      delta,
    }) + '\n',
  );
  process.exit(0);
}

async function main(): Promise<void> {
  const only = process.env.QT_IT_CHILD;
  if (only !== undefined) {
    const [kind, i] = only.split(':');
    return runChild(kind, Number(i));
  }
  const jobs: Array<[string, number, string]> = [
    ...spec.cases.map((c, i) => ['case', i, c.name] as [string, number, string]),
    ...spec.transfers.map((t, i) => ['transfer', i, t.name] as [string, number, string]),
  ];
  for (const [kind, i, name] of jobs) {
    const child = spawnSync(process.execPath, [...process.execArgv, fileURLToPath(import.meta.url)], {
      env: { ...process.env, QT_IT_CHILD: `${kind}:${i}` },
      encoding: 'utf8',
      maxBuffer: 256 * 1024 * 1024,
    });
    if (child.status !== 0 || !child.stdout.trim()) {
      process.stderr.write(`${kind} #${i} ${name} failed:\n${child.stderr}\n`);
      process.exit(1);
    }
    process.stdout.write(child.stdout.trim().split('\n').pop() + '\n');
  }
  process.stderr.write(`wrote ${jobs.length} inventory-tools rows\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`inventory-tools oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
