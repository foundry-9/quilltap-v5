/**
 * Tier-2 oracle — v4's PHASE 3.4 / 3.4b scenario heals (P4.171 C-19):
 * `ensureProjectScenariosForAllProjects` (`lib/startup/ensure-project-scenarios.ts`)
 * and `ensureGroupScenariosForAllGroups` (`lib/startup/ensure-group-scenarios.ts`),
 * v4 `322a5488e`.
 *
 * Opens a COPY of the scenario-location fixture pair (built by
 * `harness/oracle/fixtures/build-scenario-location-fixture.ts` — its storeless,
 * hand-linked, stale-pointer and nameless owners are this family's substrate)
 * and replays ONE op list against v4's REAL loops, in order, on that copy.
 * Each row records the op, the loop's `{ total, provisioned, healed, failed }`,
 * every owner's `officialMountPointId` (a mount minted during the run is
 * `<minted>`), the `doc_mount_points` count, the mounts holding a `Scenarios/`
 * or `Knowledge/` folder, and the captured lines (`LEVEL message k=v`;
 * objects / arrays as `kJson`). The two loops' `error` context string renders
 * as `errorJson=<JSON string>` — the v5 lines carry it under the `…Json`
 * convention (§R.5); the location's own line keeps its `error=` field.
 *
 * Run (Node 24, from the round's pin), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   LANG=en_US.UTF-8 TZ=UTC QT_FIXTURE_SS_MAIN=/tmp/qt-startup-scenarios-main.db \
 *   QT_FIXTURE_SS_MOUNT=/tmp/qt-startup-scenarios-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/cases/startup-scenarios.ts \
 *     > /tmp/oracle-startup-scenarios.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync, existsSync, mkdtempSync, rmSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  ids: Record<string, string>;
}

/** The lines this family pins (v4's exact wording). */
const PINNED_MESSAGES = [
  'ensure-project-scenarios: failed for project; continuing',
  'ensure-project-scenarios: complete',
  'ensure-group-scenarios: failed for group; continuing',
  'ensure-group-scenarios: complete',
  '[ScenarioLocation] Failed to ensure Scenarios folder',
  'Project officialMountPointId points to a missing mount point; will heal',
  'Group officialMountPointId points to a missing mount point; will heal',
  'Adopted existing linked store as project official',
  'Adopted existing linked store as group official',
  'Created project-official document store',
  'Created group-official document store',
];

type Op = { name: string; op: 'projects' | 'groups' | 'breakFolders' };

const OPS: Op[] = [
  { name: 'projects_first_boot', op: 'projects' },
  { name: 'groups_first_boot', op: 'groups' },
  { name: 'projects_second_boot_idempotent', op: 'projects' },
  { name: 'groups_second_boot_idempotent', op: 'groups' },
  { name: 'break_folders', op: 'breakFolders' },
  { name: 'projects_folder_failure_counted', op: 'projects' },
  { name: 'groups_folder_failure_counted', op: 'groups' },
];

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, '..', 'fixtures', 'scenario-location.json'), 'utf8')) as Spec;
  const mainFixture = process.env.QT_FIXTURE_SS_MAIN;
  const mountFixture = process.env.QT_FIXTURE_SS_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_SS_MAIN and QT_FIXTURE_SS_MOUNT must point at the built fixture .db files');
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-ss-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'ss-main.db');
  const mountWork = join(scratch, 'ss-mount.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'debug';

  const { logger } = await import('@/lib/logger');
  let captured: string[] = [];
  let remap = (s: string) => s;
  const render = (message: string, k: string, v: unknown): string => {
    if (k === 'error' && message.startsWith('ensure-')) return ` errorJson=${remap(JSON.stringify(v))}`;
    if (v === null || v === undefined) return ` ${k}=null`;
    if (typeof v === 'object') return ` ${k}Json=${remap(JSON.stringify(v))}`;
    return ` ${k}=${remap(String(v))}`;
  };
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown> };
      if (!PINNED_MESSAGES.includes(data.message)) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      const rendered = Object.entries(fields).map(([k, v]) => render(data.message, k, v)).join('');
      captured.push(`${data.level.toUpperCase()} ${data.message}${rendered}`);
    },
  });

  const { initializeDatabase } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase } = await import('@/lib/database/backends/sqlite/mount-index-client');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { ensureProjectScenariosForAllProjects } = await import('@/lib/startup/ensure-project-scenarios');
  const { ensureGroupScenariosForAllGroups } = await import('@/lib/startup/ensure-group-scenarios');

  await initializeDatabase();
  const midb = getRawMountIndexDatabase();
  const maindb = getRawDatabase();
  if (!midb || !maindb) throw new Error('DB handles unavailable');

  const knownMounts = new Set(
    (midb.prepare('SELECT id FROM doc_mount_points').all() as Array<{ id: string }>).map((r) => r.id),
  );
  remap = (s: string) => {
    let out = s;
    for (const m of midb.prepare('SELECT id FROM doc_mount_points').all() as Array<{ id: string }>) {
      if (!knownMounts.has(m.id)) out = out.split(m.id).join('<minted>');
    }
    return out;
  };
  const fks = (table: 'projects' | 'groups') =>
    (maindb.prepare(`SELECT id, officialMountPointId AS mp FROM "${table}" ORDER BY id`).all() as Array<{ id: string; mp: string | null }>)
      .map((r) => [r.id, r.mp === null ? null : remap(r.mp)]);
  const pointCount = () => (midb.prepare('SELECT COUNT(*) AS n FROM doc_mount_points').get() as { n: number }).n;
  let foldersBroken = false;
  const folders = (): string[] => {
    if (foldersBroken) return ['<broken>'];
    return (midb.prepare("SELECT mountPointId AS m, path AS p FROM doc_mount_folders WHERE LOWER(path) IN ('scenarios', 'knowledge')").all() as Array<{ m: string; p: string }>)
      .map((r) => `${remap(r.m)} ${r.p}`)
      .sort();
  };

  for (const o of OPS) {
    captured = [];
    let result: unknown = null;
    switch (o.op) {
      case 'projects':
        result = await ensureProjectScenariosForAllProjects();
        break;
      case 'groups':
        result = await ensureGroupScenariosForAllGroups();
        break;
      case 'breakFolders':
        midb.exec('DROP TABLE doc_mount_folders');
        foldersBroken = true;
        break;
    }
    process.stdout.write(
      `${JSON.stringify({
        name: o.name,
        op: o.op,
        result,
        projects: fks('projects'),
        groups: fks('groups'),
        points: pointCount(),
        folders: folders(),
        lines: captured,
      })}\n`,
    );
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`startup-scenarios oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
