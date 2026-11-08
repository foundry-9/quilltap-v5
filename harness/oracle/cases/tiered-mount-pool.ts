/**
 * Read-differential ORACLE for the tiered mount pool (W4.1d batch 3a).
 *
 * Opens the pre-seeded main + mount-index fixtures and drives v4's REAL
 * `resolveTieredMountPool` (lib/mount-index/tiered-mount-pool.ts) over a
 * resolution matrix, emitting each resolved `TieredMountPool`. The Rust port
 * (db::tiered_mount_pool::resolve_tiered_mount_pool) reads the SAME fixtures and
 * must produce the same pool exactly (every id pinned/shared — zero normalization).
 *
 * The matrix (see the harness test for the mirror):
 *   basic / ownership_pass / ownership_fail / fast_path_wins / participants /
 *   participant_excludes_self / no_character / group_per_character_B /
 *   participants_flag_off.
 *
 * P4.D231 (v4 `08c49319d`): the NEW `resolveMountPointIdsForGroup` is driven
 * directly by `helperArms`, over `helperPlants` applied to BOTH work copies
 * through the cipher driver BEFORE the database initializes (new groups with no
 * members, so the matrix above is untouched — the neutrality leg). Each helper
 * row is `{ id, helper: true, ids }`. At a pin older than `08c49319d` the
 * export does not exist and the helper rows are simply absent.
 *
 * P4.D256 (v4 `cc80dc89d`): the NEW `resolveGroupMountsForCharacter` is driven
 * by `groupedArms` (rows `{ id, grouped: true, groups, flat }`, the flat
 * resolver beside it), over the `p4d256` plants in `helperPlants`. PIN
 * REQUIRED at `cc80dc89d` or later.
 *
 * P4.D376 (v4 `0c47f4539`, PIN REQUIRED at `624d0d25c`): the matrix moved into
 * the spec (`matrix`), every row stating `roster` / `enabled` EXPLICITLY. The
 * nine pre-existing rows say `open` / `ignore` — what v4's `{}` meant at
 * `236c04e35`; at the pin an unchanged `{}` row leaves both `undefined`, which
 * is neither `'gated'` nor `'require'`, so its output equals the explicit
 * `open` / `ignore` row's. A row may carry `before` / `after` SQL on the mount
 * index (run around it on the raw handle), and records `logs`: every line whose
 * message is in `POOL_MESSAGES` (the pool's own lines), `{level, message,
 * fields}` with `error` omitted. Grouped arms may pass `includeEmpty`.
 *
 * Run (Node 24, from the v4 checkout — a pin at `624d0d25c`):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TMP_MAIN=/tmp/qt-p4d376-tmp-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-p4d376-tmp-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/cases/tiered-mount-pool.ts > /tmp/oracle-p4d376-tmp.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync, existsSync, mkdtempSync, rmSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  wrongUserId: string;
  charAId: string;
  charBId: string;
  generalMountPointId: string;
  groupId: string;
  groupOfficialMountPointId: string;
  groupLinkedMountPointId: string;
  projectId: string;
  projectStoreA: string;
  projectStoreB: string;
  fakeMountPointId: string;
  helperPlants: Array<{ db: 'main' | 'mount'; sql: string; params: Array<string | null> }>;
  helperArms: Array<{ id: string; groupId: string }>;
  /** P4.D256 (v4 `cc80dc89d`): the grouped resolver's arms (P4.D376: `includeEmpty`). */
  groupedArms: Array<{ id: string; characterId: string; includeEmpty?: boolean }>;
  /** P4.D376: the resolution matrix, `$A`/`$B`/`$C`/`$P`/`$U`/`$WU`/`$FAKE` substituted. */
  matrix: Array<{
    id: string;
    ctx: Record<string, unknown>;
    roster: 'gated' | 'open';
    enabled: 'require' | 'ignore';
    requireOwnership?: boolean;
    includeParticipants?: boolean;
    before?: string[];
    after?: string[];
  }>;
  charCId: string;
  /** P4.149: the two project-tier helpers, each arm optionally over a RENAME. */
  projectTierArms: Array<{
    id: string;
    helper: 'project' | 'chat';
    arg: string;
    plant?: { db: 'main' | 'mount'; table: string; from: string; to: string };
  }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'tiered-mount-pool.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_TMP_MAIN;
  const mountFixture = process.env.QT_FIXTURE_TMP_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_TMP_MAIN and QT_FIXTURE_TMP_MOUNT must point at the seeded fixtures');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-tmp-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'tmp-main-work.db');
  const mountWork = join(scratch, 'tmp-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  // P4.D231: plant the helper arms' rows on both work copies, pre-init.
  {
    const { createRequire } = await import('node:module');
    const nodeRequire = createRequire(join(process.cwd(), 'noop.js'));
    const Database = nodeRequire(
      join(process.cwd(), 'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'),
    );
    const hex = Buffer.from(spec.testPepperBase64, 'base64').toString('hex');
    for (const [db, path] of [['main', mainWork], ['mount', mountWork]] as const) {
      const conn = new Database(path);
      conn.pragma(`key = "x'${hex}'"`);
      for (const p of spec.helperPlants.filter((x) => x.db === db)) conn.prepare(p.sql).run(...p.params);
      conn.close();
    }
  }

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const tieredModule = (await import('@/lib/mount-index/tiered-mount-pool')) as Record<string, unknown>;
  const { resolveTieredMountPool } = tieredModule as unknown as {
    resolveTieredMountPool: (ctx: never, opts: never) => Promise<unknown>;
  };
  const resolveMountPointIdsForGroup = tieredModule.resolveMountPointIdsForGroup as
    | ((groupId: string) => Promise<string[]>)
    | undefined;
  const resolveGroupMountsForCharacter = tieredModule.resolveGroupMountsForCharacter as
    | ((characterId: string, opts?: { includeEmpty?: boolean }) => Promise<unknown[]>)
    | undefined;
  const resolveGroupMountPointIdsForCharacter =
    tieredModule.resolveGroupMountPointIdsForCharacter as (characterId: string) => Promise<string[]>;
  const { resolveProjectMountPointIds, resolveProjectMountPointIdsForChat } = tieredModule as unknown as {
    resolveProjectMountPointIds: (projectId: string) => Promise<string[]>;
    resolveProjectMountPointIdsForChat: (chatId: string) => Promise<string[]>;
  };

  await initializeDatabase();

  // P4.D376: one recorder over the Logger prototype for the whole run; each
  // section arms it with its own message filter.
  const { Logger } = await import('@/lib/logger');
  type ArmLog = { level: string; message: string; fields: Array<[string, string]> };
  let armLogs: ArmLog[] | null = null;
  let armFilter: (level: string, message: string) => boolean = () => false;
  for (const level of ['error', 'warn', 'debug'] as const) {
    const original = Logger.prototype[level];
    Logger.prototype[level] = function (
      this: unknown,
      message: string,
      context?: Record<string, unknown>,
      ...rest: unknown[]
    ) {
      if (armLogs && armFilter(level, message)) {
        const fields: Array<[string, string]> = [];
        for (const [k, v] of Object.entries(context ?? {})) {
          if (k === 'error') continue;
          fields.push([k, v === null ? 'null' : String(v)]);
        }
        armLogs.push({ level, message, fields });
      }
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite');
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );

  /** The pool's own lines (P4.D376) — the dead WARNs included, so a row would show them. */
  const POOL_MESSAGES = new Set([
    'Character off project roster — project tier withheld',
    'Disabled stores left out of the pool',
    'Mount lookup failed; store left out of the pool',
    'Project official store lookup failed',
    'Project mount lookup failed',
  ]);
  const subst: Record<string, unknown> = {
    $A: spec.charAId,
    $B: spec.charBId,
    $C: spec.charCId,
    $P: spec.projectId,
    $U: spec.userId,
    $WU: spec.wrongUserId,
    $FAKE: spec.fakeMountPointId,
  };
  const sub = (v: unknown): unknown =>
    Array.isArray(v) ? v.map(sub) : typeof v === 'string' && v in subst ? subst[v] : v;

  const rows: unknown[] = [];
  for (const c of spec.matrix) {
    const ctx: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(c.ctx)) ctx[k] = sub(v);
    const opts: Record<string, unknown> = { roster: c.roster, enabled: c.enabled };
    if (c.requireOwnership) opts.requireOwnership = true;
    if (c.includeParticipants) opts.includeParticipants = true;
    const raw = getRawMountIndexDatabase();
    for (const sql of c.before ?? []) raw!.exec(sql);
    armLogs = [];
    armFilter = (_level, message) => POOL_MESSAGES.has(message);
    const pool = await resolveTieredMountPool(ctx as never, opts as never);
    rows.push({ id: c.id, pool, logs: armLogs });
    armLogs = null;
    for (const sql of c.after ?? []) raw!.exec(sql);
  }
  if (resolveMountPointIdsForGroup) {
    for (const h of spec.helperArms) {
      rows.push({ id: h.id, helper: true, ids: await resolveMountPointIdsForGroup(h.groupId) });
    }
  }
  // P4.D256 (v4 `cc80dc89d`): the grouped resolver beside the flat one — the
  // flatten-equals-flat claim is a comparand. Absent at a pin older than
  // `cc80dc89d` (the export does not exist there).
  if (resolveGroupMountsForCharacter) {
    for (const g of spec.groupedArms) {
      rows.push({
        id: g.id,
        grouped: true,
        groups: await resolveGroupMountsForCharacter(
          g.characterId,
          g.includeEmpty ? { includeEmpty: true } : undefined,
        ),
        flat: await resolveGroupMountPointIdsForCharacter(g.characterId),
      });
    }
  }

  // P4.149 (items 6a/6b): the project-tier helpers, AFTER everything above
  // (each repository's lazy ensure has already run, so a rename trips only the
  // read under test), every ERROR/WARN recorded off the `Logger` prototype —
  // `{level, message, fields}`, `error` omitted (each driver's own sentence).
  {
    // The fixture carries no `chats` table: v4's chats repository creates it
    // lazily (`ensureCollection`, generateDDL) on its first access — a miss read
    // here, BEFORE the spy arms, so the chat arm's rename trips only its read.
    // The Rust side runs the same table's statements from the D23 dump.
    const { getRepositories } = await import('@/lib/repositories/factory');
    await getRepositories().chats.findById('00000000-0000-4000-8000-000000000000');
    for (const arm of spec.projectTierArms) {
      const raw = arm.plant ? (arm.plant.db === 'main' ? getRawDatabase() : getRawMountIndexDatabase()) : null;
      if (arm.plant) {
        if (!raw) throw new Error(`raw ${arm.plant.db} handle unavailable`);
        raw.exec(`ALTER TABLE "${arm.plant.table}" RENAME COLUMN "${arm.plant.from}" TO "${arm.plant.to}"`);
      }
      armLogs = [];
      armFilter = (level) => level === 'error' || level === 'warn';
      const ids =
        arm.helper === 'project'
          ? await resolveProjectMountPointIds(arm.arg)
          : await resolveProjectMountPointIdsForChat(arm.arg);
      rows.push({ id: arm.id, projectTier: true, ids, logs: armLogs });
      armLogs = null;
      if (arm.plant && raw) {
        raw.exec(`ALTER TABLE "${arm.plant.table}" RENAME COLUMN "${arm.plant.to}" TO "${arm.plant.from}"`);
      }
    }
  }

  await closeDatabase();

  for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`tiered-mount-pool oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
