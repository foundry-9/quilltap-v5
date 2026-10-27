/**
 * Oracle case (P4.D340, the Inventory title lint — v4 `7263105c6` at
 * `322a5488e`): ONE family over v4's REAL `lib/inventory/{lint-scope,
 * title-lint, title-lint-documents}.ts` and `lib/mount-index/database-store.ts`
 * (R-5 — two tiers inside one family). `inventory-title-lint.json` carries
 * every input; per row ONE NDJSON line:
 *
 *   - `path-<i>`  (tier 1, no DB): `isToolDefinitionPath` / `isTitleLintFolder`
 *     / `isTitleLintPath` over each of `paths`;
 *   - `norm-<i>`  (tier 1, no DB): `normalizeTitle` over each of `norms`;
 *   - `msg-<i>`   (tier 1, no DB): v4's REAL `InventoryTitleShadowError` /
 *     `InventoryTitleCheckUnavailableError` over each of `messages` —
 *     `{name, message, code, status, details}`;
 *   - every `ops` entry (DB substrate) in its OWN child process on a FRESH
 *     copy of the pair (`build-inventory-title-lint-fixture.ts`): `lint`
 *     calls `lintInventoryTitle` DIRECTLY with an injected census (R-12);
 *     `doc` calls `lintInventoryDocumentChange` with the REAL census and then
 *     its returned writer (a `marker` log entry between the two); `store`
 *     calls the REAL `writeDatabaseDocument` / `deleteDatabaseDocument` /
 *     `moveDatabaseDocument` / `moveDatabaseFolder`. Each child emits the
 *     outcome (`result` — `JSON.stringify` — or `error`), every line v4
 *     logged (recorded off the `Logger` prototype before the level check),
 *     and the mount tables' delta.
 *
 * Unless an op says `tools: true`, General's keyed `Tools/measure_twice.
 * tool.json` is deleted (linted — no line) before the op runs: v5's branch
 * cannot read a keyed gate (P4.D340 R-4), so only carved rows keep it.
 *
 * Run (Node 24, from the v4 checkout), AFTER the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   QT_FIXTURE_OUT=/tmp/qt-itl-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-itl-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-inventory-title-lint-fixture.ts
 *   LOG_LEVEL=debug QT_FIXTURE_ITL_MAIN=/tmp/qt-itl-main.db QT_FIXTURE_ITL_MOUNT=/tmp/qt-itl-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/cases/inventory-title-lint.ts \
 *     > /tmp/oracle-inventory-title-lint.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';

type Json = any; // eslint-disable-line @typescript-eslint/no-explicit-any

const here = dirname(fileURLToPath(import.meta.url));
const spec = JSON.parse(readFileSync(join(here, '..', 'fixtures', 'inventory-title-lint.json'), 'utf8')) as Json;

/** The mount tables a delta covers: [table, key column]. */
const TABLES: Array<[string, string]> = [
  ['doc_mount_documents', 'id'],
  ['doc_mount_files', 'id'],
  ['doc_mount_file_links', 'id'],
  ['doc_mount_folders', 'id'],
];

function errorJson(e: unknown): Json {
  const err = e as Json;
  return {
    name: err?.name ?? 'Error',
    message: err?.message ?? String(e),
    ...(err?.code !== undefined ? { code: err.code } : {}),
    ...(err?.status !== undefined ? { status: err.status } : {}),
    ...(err?.details !== undefined ? { details: err.details } : {}),
  };
}

async function pureRows(): Promise<void> {
  const scope = await import('@/lib/inventory/lint-scope');
  const { normalizeTitle } = await import('@/lib/inventory/title-lint');
  const errors = await import('@/lib/inventory/errors');
  spec.paths.forEach((p: string, i: number) => {
    process.stdout.write(
      JSON.stringify({
        op: `path-${i}`,
        path: p,
        tool: scope.isToolDefinitionPath(p),
        folder: scope.isTitleLintFolder(p),
        lint: scope.isTitleLintPath(p),
      }) + '\n',
    );
  });
  spec.norms.forEach((n: { v: string | null }, i: number) => {
    process.stdout.write(JSON.stringify({ op: `norm-${i}`, v: n.v, out: normalizeTitle(n.v) }) + '\n');
  });
  spec.messages.forEach((m: Json, i: number) => {
    const e =
      m.kind === 'shadow'
        ? new errors.InventoryTitleShadowError(m.title, m.collisions)
        : new errors.InventoryTitleCheckUnavailableError(m.title, m.unread);
    process.stdout.write(JSON.stringify({ op: `msg-${i}`, error: errorJson(e) }) + '\n');
  });
}

async function runChild(index: number): Promise<void> {
  const op = spec.ops[index];
  const fixtureMain = process.env.QT_FIXTURE_ITL_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_ITL_MOUNT;
  if (!fixtureMain || !fixtureMount || !existsSync(fixtureMain) || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_ITL_MAIN / QT_FIXTURE_ITL_MOUNT must point at the title-lint pair');
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-itl-op-'));
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
      if (recording) {
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
  const { resolveInventoryLocation } = await import('@/lib/inventory/location');
  const { loadPoolForLocation } = await import('@/lib/inventory/pool');
  const { lintInventoryTitle } = await import('@/lib/inventory/title-lint');
  const { lintInventoryDocumentChange } = await import('@/lib/inventory/title-lint-documents');
  const { withInventoryWritesLinted } = await import('@/lib/inventory/lint-scope');
  const store = await import('@/lib/mount-index/database-store');

  const loc = async (at: Json) => {
    const l = await resolveInventoryLocation(at.scope, at.id ?? null, repos, spec.userId, { ensure: false });
    if (!l) throw new Error(`no location for ${at.scope}:${at.id}`);
    return l;
  };
  // `{{mp:<scope>}}` — a store's mount point, through the location resolver.
  const mounts: Record<string, string> = {
    general: (await loc({ scope: 'general' })).mountPointId,
    project: (await loc({ scope: 'project', id: spec.project.id })).mountPointId,
    group: (await loc({ scope: 'group', id: spec.group.id })).mountPointId,
    friday: (await loc({ scope: 'character', id: spec.friday.id })).mountPointId,
  };
  const mp = (s: string) => s.replace(/\{\{mp:(\w+)\}\}/g, (_, k) => mounts[k]);
  const sub = (v: Json): Json =>
    typeof v === 'string' ? mp(v) : Array.isArray(v) ? v.map(sub) : v && typeof v === 'object'
      ? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, sub(x)]))
      : v;

  // Setup, unrecorded.
  if (!op.tools) {
    const gone = await withInventoryWritesLinted(() =>
      store.deleteDatabaseDocument(mounts.general, 'Tools/measure_twice.tool.json'),
    );
    if (!gone) throw new Error('the keyed tool was not there to strip');
  }
  for (const step of op.steps ?? []) {
    if (step.do === 'sql') {
      (step.db === 'main' ? rawMain : rawMount).exec(step.sql);
    } else if (step.do === 'plant') {
      const l = await loc(step.at);
      await l.create(step.record);
      if (!(await l.findItem(step.record.id))) throw new Error(`plant ${step.record.id} did not land`);
    } else if (step.do === 'doc-plant') {
      await withInventoryWritesLinted(() => store.writeDatabaseDocument(mp(step.mp), step.path, step.content));
    } else {
      throw new Error(`unknown step ${step.do}`);
    }
  }

  const dump = () => {
    const out: Record<string, Array<Record<string, unknown>>> = {};
    for (const [table] of TABLES) {
      const cols = (rawMount.prepare(`PRAGMA table_info("${table}")`).all() as Array<{ name: string }>).map((c) => c.name);
      out[table] = (rawMount.prepare(`SELECT * FROM "${table}"`).all() as Array<Record<string, unknown>>).map((r) => {
        const o: Record<string, unknown> = {};
        for (const c of cols) {
          const v = r[c];
          o[c] = v === null || v === undefined ? null : Buffer.isBuffer(v) || v instanceof Uint8Array ? `<blob ${v.length}>` : v;
        }
        return o;
      });
    }
    return out;
  };
  const before = dump();

  const sideOf = async (l: Json, ref: Json) => {
    if (ref === null || ref === undefined) return null;
    if (ref.record) return ref.record;
    const found = await l.findItem(ref.id);
    if (!found) throw new Error(`no item ${ref.id}`);
    const out: Record<string, unknown> = { ...found, ...(ref.patch ?? {}) };
    for (const k of ref.remove ?? []) delete out[k];
    return out;
  };

  let result: unknown;
  let error: unknown;
  recording = true;
  try {
    if (op.kind === 'lint') {
      const l = await loc(op.at);
      const beforeRec = await sideOf(l, op.before);
      const afterRec = await sideOf(l, op.after);
      const census = op.census.throw
        ? async () => {
            throw new Error(op.census.throw);
          }
        : async () => JSON.parse(JSON.stringify(sub(op.census)));
      let pool: Json;
      if (op.poolUnread) {
        pool = await loadPoolForLocation(repos, l, op.context ?? {});
        for (const u of op.poolUnread) (pool.unreadMounts as Json[]).push(sub(u));
      }
      result = await lintInventoryTitle(
        repos,
        l,
        { door: op.door, before: beforeRec, after: afterRec },
        { census, ...(pool ? { pool } : {}), ...(op.context ? { context: op.context } : {}) },
      );
    } else if (op.kind === 'doc') {
      const change = sub(op.change);
      const run = () => lintInventoryDocumentChange(change);
      const finish = op.linted ? await withInventoryWritesLinted(run) : await run();
      logs.push({ level: 'marker', message: '--- after the change ---', fields: [] });
      finish();
      result = { written: true };
    } else if (op.kind === 'store') {
      const a = sub(op.args);
      const call = async () => {
        switch (op.fn) {
          case 'write':
            await store.writeDatabaseDocument(a.mountPointId, a.relativePath, a.content);
            return { written: true };
          case 'delete':
            return { deleted: await store.deleteDatabaseDocument(a.mountPointId, a.relativePath) };
          case 'move':
            await store.moveDatabaseDocument(a.mountPointId, a.fromPath, a.toPath);
            return { moved: true };
          case 'move-folder':
            await store.moveDatabaseFolder(a.mountPointId, a.fromPath, a.toPath);
            return { moved: true };
          default:
            throw new Error(`unknown store fn ${op.fn}`);
        }
      };
      result = op.linted ? await withInventoryWritesLinted(call) : await call();
    } else {
      throw new Error(`unknown op kind ${op.kind}`);
    }
  } catch (e) {
    error = errorJson(e);
  }
  recording = false;

  const after = dump();
  const delta: Record<string, { changed: unknown[]; removed: string[] }> = {};
  for (const [table, key] of TABLES) {
    const was = new Map(before[table].map((r) => [String(r[key]), JSON.stringify(r)]));
    const now = new Set(after[table].map((r) => String(r[key])));
    delta[table] = {
      changed: after[table].filter((r) => was.get(String(r[key])) !== JSON.stringify(r)),
      removed: Array.from(was.keys()).filter((k) => !now.has(k)),
    };
  }
  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stdout.write(
    JSON.stringify({
      op: op.name,
      index,
      mounts,
      ...(error ? { error } : { result: JSON.stringify(result) }),
      logs,
      delta,
    }) + '\n',
  );
  process.exit(0);
}

async function main(): Promise<void> {
  const only = process.env.QT_ITL_OP_INDEX;
  if (only !== undefined) return runChild(Number(only));
  await pureRows();
  for (let i = 0; i < spec.ops.length; i++) {
    const child = spawnSync(process.execPath, [...process.execArgv, fileURLToPath(import.meta.url)], {
      env: { ...process.env, QT_ITL_OP_INDEX: String(i) },
      encoding: 'utf8',
      maxBuffer: 256 * 1024 * 1024,
    });
    if (child.status !== 0 || !child.stdout.trim()) {
      process.stderr.write(`op #${i} ${spec.ops[i].name} failed:\n${child.stderr}\n`);
      process.exit(1);
    }
    process.stdout.write(child.stdout.trim().split('\n').pop() + '\n');
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`inventory-title-lint oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
