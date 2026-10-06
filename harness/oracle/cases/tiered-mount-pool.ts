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
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TMP_MAIN=/tmp/qt-tmp-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-tmp-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/cases/tiered-mount-pool.ts > /tmp/oracle-tmp.ndjson
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
  const { resolveProjectMountPointIds, resolveProjectMountPointIdsForChat } = tieredModule as unknown as {
    resolveProjectMountPointIds: (projectId: string) => Promise<string[]>;
    resolveProjectMountPointIdsForChat: (chatId: string) => Promise<string[]>;
  };

  await initializeDatabase();

  const A = spec.charAId;
  const B = spec.charBId;
  const P = spec.projectId;

  const matrix: Array<{
    id: string;
    ctx: Record<string, unknown>;
    opts: Record<string, unknown>;
  }> = [
    { id: 'basic', ctx: { characterId: A, projectId: P }, opts: {} },
    { id: 'ownership_pass', ctx: { userId: spec.userId, characterId: A, projectId: P }, opts: { requireOwnership: true } },
    { id: 'ownership_fail', ctx: { userId: spec.wrongUserId, characterId: A, projectId: P }, opts: { requireOwnership: true } },
    { id: 'fast_path_wins', ctx: { characterMountPointId: spec.fakeMountPointId, characterId: A }, opts: {} },
    { id: 'participants', ctx: { characterId: A, characterIds: [B], projectId: P }, opts: { includeParticipants: true } },
    { id: 'participant_excludes_self', ctx: { characterId: A, characterIds: [A, B], projectId: P }, opts: { includeParticipants: true } },
    { id: 'no_character', ctx: { projectId: P }, opts: {} },
    { id: 'group_per_character_B', ctx: { characterId: B, projectId: P }, opts: {} },
    { id: 'participants_flag_off', ctx: { characterId: A, characterIds: [B] }, opts: {} },
  ];

  const rows: unknown[] = [];
  for (const c of matrix) {
    const pool = await resolveTieredMountPool(c.ctx as never, c.opts as never);
    rows.push({ id: c.id, pool });
  }
  if (resolveMountPointIdsForGroup) {
    for (const h of spec.helperArms) {
      rows.push({ id: h.id, helper: true, ids: await resolveMountPointIdsForGroup(h.groupId) });
    }
  }

  // P4.149 (items 6a/6b): the project-tier helpers, AFTER everything above
  // (each repository's lazy ensure has already run, so a rename trips only the
  // read under test), every ERROR/WARN recorded off the `Logger` prototype —
  // `{level, message, fields}`, `error` omitted (each driver's own sentence).
  {
    const { Logger } = await import('@/lib/logger');
    let armLogs: Array<{ level: string; message: string; fields: Array<[string, string]> }> | null =
      null;
    for (const level of ['error', 'warn'] as const) {
      const original = Logger.prototype[level];
      Logger.prototype[level] = function (
        this: unknown,
        message: string,
        context?: Record<string, unknown>,
        ...rest: unknown[]
      ) {
        if (armLogs) {
          const fields: Array<[string, string]> = [];
          for (const [k, v] of Object.entries(context ?? {})) {
            if (k === 'error') continue;
            fields.push([k, String(v)]);
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
