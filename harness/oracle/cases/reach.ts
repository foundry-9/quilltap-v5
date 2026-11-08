/**
 * P4.D376 — the REACH differential ORACLE: drives v4's REAL
 * `lib/mount-index/reach.ts` (NEW in `0c47f4539`; PIN REQUIRED at
 * `624d0d25c`) over its OWN copies of the tiered-mount-pool baked pair (the
 * shared builder `fixtures/build-tiered-mount-pool-fixture.ts`), after the
 * `fixtures/reach.json` plants (applied pre-init through the cipher driver;
 * `@vault:<characterId>` params resolved off the main copy first). See the
 * spec's `$comment` for the arms; `crates/quilltap-harness/tests/
 * reach_equivalence.rs` mirrors this file.
 *
 * Every row is `{ id, kind, ... , logs }`; `logs` holds every `[Reach]` line
 * (and, on the read-way arms, the repository's two lines) as `{ level,
 * message, fields }`, `error` omitted (each driver's own sentence).
 *
 * Run (Node 24, from a v4 checkout pinned at `624d0d25c`), after building the
 * tiered-mount-pool pair:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TMP_MAIN=/tmp/qt-p4d376-tmp-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-p4d376-tmp-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-tiered-mount-pool-fixture.ts
 *   QT_FIXTURE_TMP_MAIN=/tmp/qt-p4d376-tmp-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-p4d376-tmp-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/cases/reach.ts > /tmp/oracle-p4d376-reach.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync, existsSync, mkdtempSync, rmSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

type Sql = { db: 'main' | 'mount'; sql: string; params?: Array<string | null> };

interface Spec {
  plants: Sql[];
  pure: {
    orderArms: Array<{ id: string; pool: unknown; includeParticipants: boolean }>;
    bucketArms: Array<{ id: string; pool: unknown; scope?: string; participantsAsCharacter?: boolean }>;
    originArms: Array<{ id: string; groups: unknown[] }>;
  };
  ownerArms: Array<{
    id: string;
    scope: string;
    ownerId: string | null;
    userId: string;
    archived: 'allow' | 'refuse';
    ensure?: { folder: string; onFolderFailure: 'swallow' | 'throw' };
    context?: string;
    before?: Sql[];
    after?: Sql[];
  }>;
  readWayArms: Array<{ id: string; collection: 'projects' | 'groups'; ownerId: string; way: 'raw' | 'hydrated' }>;
  classifyArms: Array<{ id: string; mountPointId: string }>;
  castGroupArms: Array<{ id: string; characterIds: string[]; groupIds: string[] }>;
  castPoolArms: Array<{
    id: string;
    userId: string;
    projectId: string | null;
    characterIds: string[];
    groupIds: string[];
    before?: Sql[];
    after?: Sql[];
  }>;
}

const CHARACTER_KEYS = ['id', 'name', 'userId', 'archivedAt', 'characterDocumentMountPointId', 'defaultScenarioId'];

function projectCharacter(row: Record<string, unknown> | undefined): unknown {
  if (!row) return undefined;
  const out: Record<string, unknown> = {};
  for (const k of CHARACTER_KEYS) out[k] = row[k] ?? null;
  return out;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const tiered = JSON.parse(readFileSync(join(here, '..', 'fixtures', 'tiered-mount-pool.json'), 'utf8')) as {
    testPepperBase64: string;
  };
  const spec = JSON.parse(readFileSync(join(here, '..', 'fixtures', 'reach.json'), 'utf8')) as Spec;

  const mainFixture = process.env.QT_FIXTURE_TMP_MAIN;
  const mountFixture = process.env.QT_FIXTURE_TMP_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_TMP_MAIN and QT_FIXTURE_TMP_MOUNT must point at the seeded fixtures');
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-reach-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'reach-main-work.db');
  const mountWork = join(scratch, 'reach-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  // Pre-init plants, `@vault:<id>` resolved off the main copy.
  {
    const { createRequire } = await import('node:module');
    const nodeRequire = createRequire(join(process.cwd(), 'noop.js'));
    const Database = nodeRequire(
      join(process.cwd(), 'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'),
    );
    const hex = Buffer.from(tiered.testPepperBase64, 'base64').toString('hex');
    const open = (path: string) => {
      const conn = new Database(path);
      conn.pragma(`key = "x'${hex}'"`);
      return conn;
    };
    const mainConn = open(mainWork);
    const resolve = (p: string | null) => {
      if (typeof p === 'string' && p.startsWith('@vault:')) {
        const row = mainConn
          .prepare('SELECT "characterDocumentMountPointId" AS v FROM "characters" WHERE "id" = ?')
          .get(p.slice(7)) as { v: string } | undefined;
        if (!row?.v) throw new Error(`unresolvable ${p}`);
        return row.v;
      }
      return p;
    };
    const resolved = spec.plants.map((p) => ({ ...p, params: (p.params ?? []).map(resolve) }));
    for (const p of resolved.filter((x) => x.db === 'main')) mainConn.prepare(p.sql).run(...p.params);
    mainConn.close();
    const mountConn = open(mountWork);
    for (const p of resolved.filter((x) => x.db === 'mount')) mountConn.prepare(p.sql).run(...p.params);
    mountConn.close();
  }

  process.env.ENCRYPTION_MASTER_PEPPER = tiered.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const reach = (await import('@/lib/mount-index/reach')) as Record<string, (...a: never[]) => unknown>;
  const { getRepositories } = await import('@/lib/repositories/factory');
  await initializeDatabase();
  const repos = getRepositories();
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite');
  const { getRawMountIndexDatabase } = await import('@/lib/database/backends/sqlite/mount-index-client');
  const rawMain = getRawDatabase()!;
  const rawMount = getRawMountIndexDatabase()!;
  const run = (sqls: Sql[] | undefined) => {
    for (const s of sqls ?? []) (s.db === 'main' ? rawMain : rawMount).exec(s.sql);
  };

  // The `[Reach]` (and, per arm, repository) recorder.
  const { Logger } = await import('@/lib/logger');
  type Log = { level: string; message: string; fields: Array<[string, string]> };
  let logs: Log[] | null = null;
  let extra: Set<string> = new Set();
  for (const level of ['error', 'warn', 'info', 'debug'] as const) {
    const original = Logger.prototype[level];
    Logger.prototype[level] = function (
      this: unknown,
      message: string,
      context?: Record<string, unknown>,
      ...rest: unknown[]
    ) {
      if (logs && (message.startsWith('[Reach]') || extra.has(message))) {
        const fields: Array<[string, string]> = [];
        for (const [k, v] of Object.entries(context ?? {})) {
          if (k === 'error' || v === undefined) continue;
          fields.push([k, v === null ? 'null' : String(v)]);
        }
        logs.push({ level, message, fields });
      }
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }
  const arm = async <T>(f: () => Promise<T>, more: string[] = []): Promise<{ value?: T; threw?: boolean; logs: Log[] }> => {
    logs = [];
    extra = new Set(more);
    let value: T | undefined;
    let threw = false;
    try {
      value = await f();
    } catch {
      threw = true;
    }
    const out = { ...(threw ? { threw: true } : { value }), logs };
    logs = null;
    return out;
  };

  const rows: unknown[] = [];
  // ── Tier 1 (pure) ────────────────────────────────────────────────────────
  for (const a of spec.pure.orderArms) {
    rows.push({ id: a.id, kind: 'order', value: reach.orderMounts(a.pool as never, { includeParticipants: a.includeParticipants } as never) });
  }
  for (const a of spec.pure.bucketArms) {
    const opts: Record<string, unknown> = {};
    if (a.scope) opts.scope = a.scope;
    if (a.participantsAsCharacter !== undefined) opts.participantsAsCharacter = a.participantsAsCharacter;
    rows.push({ id: a.id, kind: 'bucket', value: reach.bucketMounts(a.pool as never, opts as never) });
  }
  for (const a of spec.pure.originArms) {
    const map = reach.groupOriginMap(a.groups as never) as Map<string, unknown>;
    rows.push({ id: a.id, kind: 'origin', value: [...map.entries()] });
  }

  // ── Tier 2 ───────────────────────────────────────────────────────────────
  const preexisting = new Set(
    (rawMount.prepare('SELECT "id" FROM "doc_mount_points"').all() as Array<{ id: string }>).map((r) => r.id),
  );
  const mask = (id: unknown) => (typeof id === 'string' && !preexisting.has(id) ? '<minted>' : id);

  for (const a of spec.ownerArms) {
    run(a.before);
    const opts: Record<string, unknown> = { archived: a.archived };
    if (a.ensure) opts.ensure = a.ensure;
    if (a.context) opts.context = a.context;
    const r = await arm(async () =>
      reach.resolveOwnerStore(a.scope as never, a.ownerId as never, repos as never, a.userId as never, opts as never) as Promise<
        Record<string, unknown> | null
      >,
    );
    run(a.after);
    let value: unknown = r.value;
    let dump: unknown = undefined;
    if (r.value) {
      const store = r.value as Record<string, unknown>;
      const id = store.mountPointId as string;
      const owner = store.owner as { id: string | null; name: string };
      value = {
        scope: store.scope,
        mountPointId: mask(id),
        owner,
        ...(store.character ? { character: projectCharacter(store.character as Record<string, unknown>) } : {}),
      };
      const mp = rawMount
        .prepare('SELECT "name", "storeType", "mountType", "enabled" FROM "doc_mount_points" WHERE "id" = ?')
        .get(id);
      const folders = (
        rawMount.prepare('SELECT "path" FROM "doc_mount_folders" WHERE "mountPointId" = ? ORDER BY "path"').all(id) as Array<{
          path: string;
        }>
      ).map((f) => f.path);
      const table = a.scope === 'project' ? 'projects' : a.scope === 'group' ? 'groups' : null;
      const pointer = table && owner.id
        ? ((rawMain.prepare(`SELECT "officialMountPointId" AS p FROM "${table}" WHERE "id" = ?`).get(owner.id) as { p: string | null })?.p ?? null)
        : null;
      dump = { store: mp ?? null, folders, ownerPointsAtIt: pointer === id };
    }
    rows.push({ id: a.id, kind: 'owner', ...(r.threw ? { threw: true } : { value: value ?? null }), dump: dump ?? null, logs: r.logs });
  }

  for (const a of spec.readWayArms) {
    const repo = a.collection === 'projects' ? repos.projects : repos.groups;
    const r = await arm(
      async () => (a.way === 'raw' ? repo.findByIdRaw(a.ownerId) : repo.findById(a.ownerId)) as Promise<unknown>,
      ['Data validation failed', 'Error finding entity by ID'],
    );
    rows.push({ id: a.id, kind: 'readWay', ...(r.threw ? { threw: true } : { found: !!r.value }), logs: r.logs });
  }

  const resolveMount = (token: string): string => {
    if (token.startsWith('@vault:')) {
      return (rawMain.prepare('SELECT "characterDocumentMountPointId" AS v FROM "characters" WHERE "id" = ?').get(token.slice(7)) as { v: string }).v;
    }
    if (token.startsWith('@projectOfficial:')) {
      return (rawMain.prepare('SELECT "officialMountPointId" AS v FROM "projects" WHERE "id" = ?').get(token.slice(17)) as { v: string }).v;
    }
    return token;
  };
  for (const a of spec.classifyArms) {
    const r = await arm(async () => reach.classifyMount(resolveMount(a.mountPointId) as never) as Promise<Record<string, unknown> | null>);
    const v = r.value as Record<string, unknown> | null | undefined;
    rows.push({
      id: a.id,
      kind: 'classify',
      value: v
        ? { ...v, ...(v.character ? { character: projectCharacter(v.character as Record<string, unknown>) } : {}) }
        : null,
      logs: r.logs,
    });
  }

  for (const a of spec.castGroupArms) {
    const r = await arm(async () => reach.castGroupMounts(a.characterIds as never, a.groupIds as never) as Promise<string[]>);
    rows.push({ id: a.id, kind: 'castGroups', value: r.value, logs: r.logs });
  }

  for (const a of spec.castPoolArms) {
    run(a.before);
    const opts: Record<string, unknown> = { userId: a.userId, characterIds: a.characterIds, groupIds: a.groupIds };
    if (a.projectId !== null) opts.projectId = a.projectId;
    const r = await arm(async () => reach.resolveCastPool(opts as never) as Promise<unknown>);
    run(a.after);
    rows.push({ id: a.id, kind: 'castPool', value: r.value, logs: r.logs });
  }

  await closeDatabase();
  for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`reach oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
